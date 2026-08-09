use std::path::Path;

use sherpa_onnx::{
    OfflineModelConfig, OfflineNemoEncDecCtcModelConfig, OfflineRecognizer,
    OfflineRecognizerConfig,
};

use crate::error::{CoreError, Result};

const MIN_AUDIO_SAMPLES: usize = 3_200;
const MAX_STT_THREADS: usize = 4;

pub struct SttEngine {
    recognizer: OfflineRecognizer,
}

// SAFETY: OfflineRecognizer wraps the ONNX Runtime C API, which is documented
// thread-safe for inference. The engine is shared behind a mutex in the app.
unsafe impl Send for SttEngine {}
unsafe impl Sync for SttEngine {}

impl SttEngine {
    pub fn new(model_dir: &Path) -> Result<Self> {
        if !model_dir.exists() {
            return Err(CoreError::Transcription(format!(
                "STT model directory not found at {}",
                model_dir.display()
            )));
        }

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

        let n_threads = std::thread::available_parallelism()
            .map(|p| p.get().clamp(1, MAX_STT_THREADS) as i32)
            .unwrap_or(2);

        let config = OfflineRecognizerConfig {
            model_config: OfflineModelConfig {
                nemo_ctc: OfflineNemoEncDecCtcModelConfig {
                    model: Some(model_file.to_string_lossy().to_string()),
                    ..Default::default()
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
        };

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            CoreError::Transcription("failed to create STT recognizer; check model files".to_string())
        })?;

        log::info!(
            "STT engine loaded from {} ({n_threads} threads)",
            model_dir.display()
        );

        Ok(Self { recognizer })
    }

    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Ok(String::new());
        }

        let stream = self.recognizer.create_stream();
        stream.accept_waveform(16000, audio);
        self.recognizer.decode(&stream);

        let result = stream.get_result().ok_or_else(|| {
            CoreError::Transcription("no result from STT recognizer".to_string())
        })?;

        Ok(result.text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_requires_existing_model_dir() {
        let missing = std::env::temp_dir().join("opendictate-no-such-dir");
        assert!(SttEngine::new(&missing).is_err());
    }
}
