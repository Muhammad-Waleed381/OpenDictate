use std::path::{Path, PathBuf};

use sherpa_onnx::{
    OfflineCohereTranscribeModelConfig, OfflineModelConfig, OfflineNemoEncDecCtcModelConfig,
    OfflineOmnilingualAsrCtcModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineTransducerModelConfig, OfflineWhisperModelConfig,
};

use super::provider::Provider;
use crate::error::{CoreError, Result};

const MIN_AUDIO_SAMPLES: usize = 3_200;
/// Whisper models require audio chunks strictly under 30 seconds.
const WHISPER_CHUNK_SAMPLES: usize = 16_000 * 29;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Whisper,
    NemoCtc,
    NemoTransducer,
    OmnilingualCtc,
    CohereTranscribe,
}

pub struct SttEngine {
    recognizer: OfflineRecognizer,
    kind: ModelKind,
    hotword_tokenizer: Option<super::hotwords::HotwordTokenizer>,
    /// Provider the engine actually came up on (after fallback).
    pub provider: &'static str,
}

// SAFETY: OfflineRecognizer wraps the ONNX Runtime C API, which is documented
// thread-safe for inference. The engine is shared behind a mutex in the app.
unsafe impl Send for SttEngine {}
unsafe impl Sync for SttEngine {}

impl SttEngine {
    pub fn new(model_dir: &Path, kind: ModelKind, language: Option<String>) -> Result<Self> {
        Self::new_with_provider(model_dir, kind, language, Provider::Cpu)
    }

    /// [`new`] with an execution-provider request. GPU providers that fail
    /// to create fall back to CPU transparently.
    pub fn new_with_provider(
        model_dir: &Path,
        kind: ModelKind,
        language: Option<String>,
        requested: Provider,
    ) -> Result<Self> {
        match Self::build(model_dir, kind, language.clone(), requested) {
            Ok(e) => Ok(e),
            Err(err) if requested.is_gpu() => {
                log::warn!(
                    "provider '{}' unavailable ({err}); falling back to cpu",
                    requested.as_str()
                );
                Self::build(model_dir, kind, language.clone(), Provider::Cpu)
            }
            Err(err) => Err(err),
        }
    }

    fn build(
        model_dir: &Path,
        kind: ModelKind,
        language: Option<String>,
        provider: Provider,
    ) -> Result<Self> {
        if !model_dir.exists() {
            return Err(CoreError::Transcription(format!(
                "STT model directory not found at {}",
                model_dir.display()
            )));
        }

        let n_threads = super::cpu::optimal_stt_threads();

        let whisper_language = language
            .as_ref()
            .filter(|l| !l.is_empty() && l.as_str() != "auto")
            .cloned();

        let hotword_tokenizer = (kind == ModelKind::NemoTransducer)
            .then(|| super::hotwords::HotwordTokenizer::load(model_dir))
            .flatten();
        let mut config = match kind {
            ModelKind::Whisper => {
                let find_part = |needle: &str| -> Option<PathBuf> {
                    let direct = model_dir.join(format!("{needle}.onnx"));
                    if direct.exists() {
                        return Some(direct);
                    }
                    let mut hits: Vec<PathBuf> = std::fs::read_dir(&model_dir)
                        .ok()?
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                                let lower = n.to_lowercase();
                                lower.ends_with(".onnx") && lower.contains(needle)
                            })
                        })
                        .collect();
                    hits.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
                    hits.first().cloned()
                };
                let find_tokens = || -> Option<PathBuf> {
                    let direct = model_dir.join("tokens.txt");
                    if direct.exists() {
                        return Some(direct);
                    }
                    std::fs::read_dir(&model_dir)
                        .ok()?
                        .flatten()
                        .map(|e| e.path())
                        .find(|p| {
                            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                                let lower = n.to_lowercase();
                                lower.contains("tokens") && lower.ends_with(".txt")
                            })
                        })
                };

                let (Some(encoder), Some(decoder), Some(tokens)) =
                    (find_part("encoder"), find_part("decoder"), find_tokens())
                else {
                    return Err(CoreError::Transcription(format!(
                    "whisper model incomplete in {} (need encoder.onnx, decoder.onnx, tokens.txt)",
                    model_dir.display()
                )));
                };
                OfflineRecognizerConfig {
                    model_config: OfflineModelConfig {
                        whisper: OfflineWhisperModelConfig {
                            encoder: Some(encoder.to_string_lossy().to_string()),
                            decoder: Some(decoder.to_string_lossy().to_string()),
                            language: whisper_language,
                            task: Some("transcribe".to_string()),
                            tail_paddings: -1,
                            ..Default::default()
                        },
                        tokens: Some(tokens.to_string_lossy().to_string()),
                        num_threads: n_threads,
                        provider: Some(provider.as_str().to_string()),
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
                        provider: Some(provider.as_str().to_string()),
                        model_type: Some("nemo_transducer".to_string()),
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
                        provider: Some(provider.as_str().to_string()),
                        debug: false,
                        ..Default::default()
                    },
                    decoding_method: Some("greedy_search".to_string()),
                    blank_penalty: 1.2,
                    ..Default::default()
                }
            }
            ModelKind::OmnilingualCtc => {
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
                        omnilingual: OfflineOmnilingualAsrCtcModelConfig {
                            model: Some(model_file.to_string_lossy().to_string()),
                        },
                        tokens: Some(tokens_file.to_string_lossy().to_string()),
                        num_threads: n_threads,
                        provider: Some(provider.as_str().to_string()),
                        debug: false,
                        ..Default::default()
                    },
                    decoding_method: Some("greedy_search".to_string()),
                    blank_penalty: 1.2,
                    ..Default::default()
                }
            }
            ModelKind::CohereTranscribe => {
                let find_part = |needle: &str| {
                    if model_dir.join(format!("{needle}.int8.onnx")).exists() {
                        return Some(model_dir.join(format!("{needle}.int8.onnx")));
                    }
                    if model_dir.join(format!("{needle}.onnx")).exists() {
                        return Some(model_dir.join(format!("{needle}.onnx")));
                    }
                    if let Ok(entries) = std::fs::read_dir(model_dir) {
                        for entry in entries.flatten() {
                            let n = entry.file_name().to_string_lossy().to_lowercase();
                            if n.ends_with(".onnx") && n.contains(needle) {
                                return Some(entry.path());
                            }
                        }
                    }
                    None
                };
                let (Some(encoder), Some(decoder)) = (find_part("encoder"), find_part("decoder")) else {
                    return Err(CoreError::Transcription(format!(
                        "cohere model incomplete in {} (need encoder.onnx, decoder.onnx)",
                        model_dir.display()
                    )));
                };
                let tokens_file = model_dir.join("tokens.txt");
                let tokens_opt = if tokens_file.exists() {
                    Some(tokens_file.to_string_lossy().to_string())
                } else {
                    None
                };

                let cohere_language = language.clone().or_else(|| Some("en".to_string()));

                OfflineRecognizerConfig {
                    model_config: OfflineModelConfig {
                        cohere_transcribe: OfflineCohereTranscribeModelConfig {
                            encoder: Some(encoder.to_string_lossy().to_string()),
                            decoder: Some(decoder.to_string_lossy().to_string()),
                            language: cohere_language,
                            use_punct: true,
                            use_itn: true,
                        },
                        tokens: tokens_opt,
                        num_threads: n_threads,
                        provider: Some(provider.as_str().to_string()),
                        debug: false,
                        ..Default::default()
                    },
                    decoding_method: Some("greedy_search".to_string()),
                    ..Default::default()
                }
            }
        };

        // Only NeMo transducers implement this beam decoder and hotword stream.
        // CTC/Whisper continue with greedy decoding and dictionary text repair.
        if let Some(tokenizer) = &hotword_tokenizer {
            config.decoding_method = Some("modified_beam_search".into());
            config.model_config.modeling_unit = Some("bpe".into());
            config.model_config.bpe_vocab = Some(tokenizer.path.to_string_lossy().into_owned());
            config.hotwords_score = 3.0;
            config.max_active_paths = 4;
        }

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            CoreError::Transcription(format!(
                "failed to create STT recognizer on provider '{}'; check model files",
                provider.as_str()
            ))
        })?;

        // If a GPU provider was requested, test forward execution with a micro-probe
        // to catch runtime driver/DLL/architecture incompatibilities (e.g. missing cuDNN,
        // unsupported Blackwell sm_120 architecture, etc.) and trigger clean CPU fallback.
        if provider.is_gpu() {
            let probe_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let stream = recognizer.create_stream();
                let silence = vec![0.0f32; 1600]; // 0.1s dummy buffer
                stream.accept_waveform(16000, &silence);
                recognizer.decode(&stream);
                let _ = stream.get_result();
            }));
            if probe_result.is_err() {
                return Err(CoreError::Transcription(format!(
                    "GPU provider '{}' failed runtime inference probe; driver or hardware incompatible",
                    provider.as_str()
                )));
            }
        }

        log::info!(
            "STT engine loaded from {} ({n_threads} threads, {:?}, provider {})",
            model_dir.display(),
            kind,
            provider.as_str()
        );

        Ok(Self {
            recognizer,
            kind,
            hotword_tokenizer,
            provider: provider.as_str(),
        })
    }

    pub fn transcribe(&self, audio: &[f32], hotwords: Option<&str>) -> Result<String> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Ok(String::new());
        }

        // Whisper models have a hard 30-second context window.
        // Longer audio must be chunked or sherpa-onnx silently truncates.
        //
        // The NeMo transducer/CTC (Parakeet) offline models do NOT need this:
        // they accept arbitrarily long audio in a single pass (verified
        // empirically; sherpa-onnx only *advises* <=30s for offline models).
        // Streaming models feed samples incrementally and are unaffected too.
        if self.kind == ModelKind::Whisper && audio.len() > WHISPER_CHUNK_SAMPLES {
            let chunks: Vec<&[f32]> = audio
                .chunks(WHISPER_CHUNK_SAMPLES)
                .filter(|c| c.len() >= MIN_AUDIO_SAMPLES)
                .collect();
            let results = self.transcribe_batch_chunks(&chunks, hotwords)?;
            return Ok(results.join(" "));
        }

        self.transcribe_single(audio, hotwords)
    }

    /// Decodes multiple audio chunks in a single batched pass using
    /// `decode_multiple_streams`. This allows ONNX Runtime to execute GEMM / encoder-decoder
    /// passes concurrently across available CPU threads instead of sequentially.
    pub fn transcribe_batch_chunks(
        &self,
        chunks: &[&[f32]],
        hotwords: Option<&str>,
    ) -> Result<Vec<String>> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        if chunks.len() == 1 {
            let text = self.transcribe_single(chunks[0], hotwords)?;
            return Ok(if text.is_empty() { vec![] } else { vec![text] });
        }

        let native_hotwords = self
            .hotword_tokenizer
            .as_ref()
            .and_then(|tokenizer| hotwords.and_then(|words| tokenizer.prepare(words)));

        let streams: Vec<_> = chunks
            .iter()
            .map(|chunk| {
                let stream = match native_hotwords.as_deref() {
                    Some(words) => self.recognizer.create_stream_with_hotwords(words),
                    None => self.recognizer.create_stream(),
                };
                stream.accept_waveform(16000, chunk);
                stream
            })
            .collect();

        let stream_refs: Vec<&_> = streams.iter().collect();
        self.recognizer.decode_multiple_streams(&stream_refs);

        let mut results = Vec::with_capacity(streams.len());
        for stream in &streams {
            if let Some(res) = stream.get_result() {
                let trimmed = res.text.trim();
                if !trimmed.is_empty() {
                    results.push(trimmed.to_string());
                }
            }
        }

        Ok(results)
    }

    fn transcribe_single(&self, audio: &[f32], hotwords: Option<&str>) -> Result<String> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Ok(String::new());
        }

        let native_hotwords = self
            .hotword_tokenizer
            .as_ref()
            .and_then(|tokenizer| hotwords.and_then(|words| tokenizer.prepare(words)));
        let stream = match native_hotwords.as_deref() {
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
    #[ignore = "requires installed NeMo model assets"]
    fn installed_offline_nemo_hotwords_are_native_safe() {
        for id in [
            crate::stt::models::PARAKEET_TDT_06B_MODEL_ID,
            crate::stt::models::PARAKEET_UNIFIED_EN_MODEL_ID,
        ] {
            let dir = crate::stt::models::model_dir_for(id);
            if !crate::stt::models::is_model_installed(id) {
                continue;
            }
            let engine = SttEngine::new(&dir, ModelKind::NemoTransducer, None).unwrap();
            let tokenizer = engine
                .hotword_tokenizer
                .as_ref()
                .expect("installed BPE vocab must enable supported decoder");
            assert!(tokenizer.prepare("open dictate").is_some());
            assert!(engine
                .transcribe(&vec![0.0; 6400], Some("open dictate"))
                .is_ok());
        }
    }

    #[test]
    fn engine_requires_existing_model_dir() {
        let missing = std::env::temp_dir().join("opendictate-no-such-dir");
        assert!(SttEngine::new(&missing, ModelKind::NemoCtc, None).is_err());
        assert!(SttEngine::new(&missing, ModelKind::Whisper, None).is_err());
        assert!(SttEngine::new(&missing, ModelKind::NemoTransducer, None).is_err());
        assert!(SttEngine::new(&missing, ModelKind::OmnilingualCtc, None).is_err());
        assert!(SttEngine::new(&missing, ModelKind::CohereTranscribe, None).is_err());
    }

    #[test]
    fn engine_loads_installed_whisper_turbo_if_present() {
        let dir = crate::stt::models::model_dir_for(crate::stt::models::WHISPER_TURBO_MODEL_ID);
        if crate::stt::models::is_model_installed(crate::stt::models::WHISPER_TURBO_MODEL_ID) {
            let engine = SttEngine::new(&dir, ModelKind::Whisper, None);
            assert!(
                engine.is_ok(),
                "failed to load whisper-turbo engine: {:?}",
                engine.err()
            );
            // Verify inference on 1 second of silence
            let silence = vec![0.0f32; 16000];
            let res = engine.unwrap().transcribe(&silence, None);
            assert!(res.is_ok(), "transcribe error: {:?}", res.err());
        }
    }

    #[test]
    fn engine_whisper_multi_chunk_batch_decoding() {
        let dir = crate::stt::models::model_dir_for(crate::stt::models::WHISPER_TURBO_MODEL_ID);
        if crate::stt::models::is_model_installed(crate::stt::models::WHISPER_TURBO_MODEL_ID) {
            let engine = SttEngine::new(&dir, ModelKind::Whisper, None).unwrap();
            // 31 seconds of audio triggers chunking (> WHISPER_CHUNK_SAMPLES)
            let multi_chunk = vec![0.0f32; WHISPER_CHUNK_SAMPLES + 16000];
            let res = engine.transcribe_batch_chunks(&[&multi_chunk[..WHISPER_CHUNK_SAMPLES], &multi_chunk[WHISPER_CHUNK_SAMPLES..]], None);
            assert!(res.is_ok(), "transcribe batch error: {:?}", res.err());
        }
    }
}
