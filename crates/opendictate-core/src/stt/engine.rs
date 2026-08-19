use std::path::Path;

use sherpa_onnx::{
    OfflineModelConfig, OfflineNemoEncDecCtcModelConfig, OfflineRecognizer,
    OfflineRecognizerConfig, OfflineTransducerModelConfig, OfflineWhisperModelConfig,
};

use crate::error::{CoreError, Result};

const MIN_AUDIO_SAMPLES: usize = 3_200;
const MAX_STT_THREADS: usize = 8;
/// Whisper models have a hard 30-second context window.
const WHISPER_CHUNK_SAMPLES: usize = 16_000 * 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Whisper,
    NemoCtc,
    NemoTransducer,
}

pub struct SttEngine {
    recognizer: OfflineRecognizer,
    kind: ModelKind,
}

// SAFETY: OfflineRecognizer wraps the ONNX Runtime C API, which is documented
// thread-safe for inference. The engine is shared behind a mutex in the app.
unsafe impl Send for SttEngine {}
unsafe impl Sync for SttEngine {}

impl SttEngine {
    pub fn new(model_dir: &Path, kind: ModelKind, language: Option<String>) -> Result<Self> {
        if !model_dir.exists() {
            return Err(CoreError::Transcription(format!(
                "STT model directory not found at {}",
                model_dir.display()
            )));
        }

        let n_threads = std::thread::available_parallelism()
            .map(|p| p.get().clamp(1, MAX_STT_THREADS) as i32)
            .unwrap_or(2);

        let whisper_language = language
            .filter(|l| !l.is_empty() && l != "auto")
            .map(|l| l.to_string());

        let config = match kind {
            ModelKind::Whisper => {
            let encoder = model_dir.join("encoder.onnx");
            let decoder = model_dir.join("decoder.onnx");
            let tokens = model_dir.join("tokens.txt");
            if !encoder.exists() || !decoder.exists() || !tokens.exists() {
                return Err(CoreError::Transcription(format!(
                    "whisper model incomplete in {} (need encoder.onnx, decoder.onnx, tokens.txt)",
                    model_dir.display()
                )));
            }
            OfflineRecognizerConfig {
                model_config: OfflineModelConfig {
                    whisper: OfflineWhisperModelConfig {
                        encoder: Some(encoder.to_string_lossy().to_string()),
                        decoder: Some(decoder.to_string_lossy().to_string()),
                        language: whisper_language,
                        task: Some("transcribe".to_string()),
                        tail_paddings: 50,
                        ..Default::default()
                    },
                    tokens: Some(tokens.to_string_lossy().to_string()),
                    num_threads: n_threads,
                    provider: Some("cpu".to_string()),
                    model_type: Some("nemo_transducer".to_string()),
                    debug: false,
                    ..Default::default()
                },
                decoding_method: Some("greedy_search".to_string()),
                ..Default::default()
            }
        }
        ModelKind::NemoTransducer => {
            let encoder = model_dir.join("encoder.onnx");
            let decoder = model_dir.join("decoder.onnx");
            let joiner = model_dir.join("joiner.onnx");
            let tokens = model_dir.join("tokens.txt");
            if !encoder.exists() || !decoder.exists() || !joiner.exists() || !tokens.exists() {
                return Err(CoreError::Transcription(format!(
                    "transducer model incomplete in {} (need encoder.onnx, decoder.onnx, joiner.onnx, tokens.txt)",
                    model_dir.display()
                )));
            }
            OfflineRecognizerConfig {
                model_config: OfflineModelConfig {
                    transducer: OfflineTransducerModelConfig {
                        encoder: Some(encoder.to_string_lossy().to_string()),
                        decoder: Some(decoder.to_string_lossy().to_string()),
                        joiner: Some(joiner.to_string_lossy().to_string()),
                    },
                    tokens: Some(tokens.to_string_lossy().to_string()),
                    num_threads: n_threads,
                    provider: Some("cpu".to_string()),
                    debug: false,
                    ..Default::default()
                },
                decoding_method: Some("greedy_search".to_string()),
                ..Default::default()
            }
        }
        ModelKind::NemoCtc => {
            let model_file = if model_dir.join("model.int8.onnx").exists() {
                model_dir.join("model.int8.onnx")
            } else if model_dir.join("model.onnx").exists() {
                model_dir.join("model.onnx")
            } else {
                return Err(CoreError::Transcription(format!(
                    "no ONNX model file found in {}",
                    model_dir.display()
                )));
            };

            let tokens_file = model_dir.join("tokens.txt");
            if !tokens_file.exists() {
                return Err(CoreError::Transcription(format!(
                    "tokens.txt not found in {}",
                    model_dir.display()
                )));
            }

            OfflineRecognizerConfig {
                model_config: OfflineModelConfig {
                    nemo_ctc: OfflineNemoEncDecCtcModelConfig {
                        model: Some(model_file.to_string_lossy().to_string()),
                    },
                    tokens: Some(tokens_file.to_string_lossy().to_string()),
                    num_threads: n_threads,
                    provider: Some("cpu".to_string()),
                    debug: false,
                    ..Default::default()
                },
                decoding_method: Some("greedy_search".to_string()),
                blank_penalty: 1.2,
                ..Default::default()
            }
        }
        };

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            CoreError::Transcription(
                "failed to create STT recognizer; check model files".to_string(),
            )
        })?;

        log::info!(
            "STT engine loaded from {} ({n_threads} threads, {:?})",
            model_dir.display(),
            kind
        );

        Ok(Self { recognizer, kind })
    }

    pub fn transcribe(&self, audio: &[f32], hotwords: Option<&str>) -> Result<String> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Ok(String::new());
        }

        // Whisper models have a hard 30-second context window.
        // Longer audio must be chunked or sherpa-onnx silently truncates.
        if self.kind == ModelKind::Whisper && audio.len() > WHISPER_CHUNK_SAMPLES {
            let mut results = Vec::new();
            for chunk in audio.chunks(WHISPER_CHUNK_SAMPLES) {
                let text = self.transcribe_single(chunk, hotwords)?;
                if !text.is_empty() {
                    results.push(text);
                }
            }
            return Ok(results.join(" "));
        }

        self.transcribe_single(audio, hotwords)
    }

    fn transcribe_single(&self, audio: &[f32], hotwords: Option<&str>) -> Result<String> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Ok(String::new());
        }

        let stream = match hotwords.filter(|h| !h.trim().is_empty()) {
            Some(words) => self.recognizer.create_stream_with_hotwords(words),
            None => self.recognizer.create_stream(),
        };
        stream.accept_waveform(16000, audio);
        self.recognizer.decode(&stream);

        let result = stream
            .get_result()
            .ok_or_else(|| CoreError::Transcription("no result from STT recognizer".to_string()))?;

        Ok(result.text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_requires_existing_model_dir() {
        let missing = std::env::temp_dir().join("opendictate-no-such-dir");
        assert!(SttEngine::new(&missing, ModelKind::NemoCtc, None).is_err());
        assert!(SttEngine::new(&missing, ModelKind::Whisper, None).is_err());
        assert!(SttEngine::new(&missing, ModelKind::NemoTransducer, None).is_err());
    }
}
