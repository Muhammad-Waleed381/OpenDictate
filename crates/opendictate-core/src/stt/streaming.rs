use std::path::Path;

use sherpa_onnx::{
    OnlineModelConfig, OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
    OnlineTransducerModelConfig,
};

use crate::error::{CoreError, Result};

const MAX_STT_THREADS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingKind {
    /// Parakeet Unified buffered streaming (NeMo encoder metadata dispatch).
    NemoTransducer,
    /// Classic icefall streaming zipformer.
    Zipformer,
}

/// Streaming (online) recognizer backed by sherpa-onnx `OnlineRecognizer`.
///
/// The sherpa-onnx crate declares `OnlineRecognizer`/`OnlineStream` as
/// `Send + Sync`; access to a session is serialized through a mutex in the
/// app.
pub struct StreamingRecognizer {
    recognizer: OnlineRecognizer,
    kind: StreamingKind,
}

pub struct StreamingSession {
    stream: OnlineStream,
    /// When the current utterance started (for duration reporting).
    pub started_at: std::time::Instant,
}

impl StreamingRecognizer {
    pub fn new(model_dir: &Path, kind: StreamingKind) -> Result<Self> {
        if !model_dir.exists() {
            return Err(CoreError::Transcription(format!(
                "streaming model directory not found at {}",
                model_dir.display()
            )));
        }

        let n_threads = std::thread::available_parallelism()
            .map(|p| p.get().clamp(1, MAX_STT_THREADS) as i32)
            .unwrap_or(2);

        let find = |needle: &str| -> Result<String> {
        let mut matches: Vec<_> = std::fs::read_dir(model_dir)
            .map_err(|e| {
                CoreError::Transcription(format!(
                    "failed to read {}: {e}",
                    model_dir.display()
                ))
            })?
            .flatten()
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.ends_with(".onnx") && n.contains(needle)
            })
            .map(|e| e.path())
            .collect();
        matches.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
        matches
            .first()
                .map(|p| p.to_string_lossy().into_owned())
                .ok_or_else(|| {
                    CoreError::Transcription(format!(
                        "streaming model missing a {needle} onnx file in {}",
                        model_dir.display()
                    ))
                })
        };

        let encoder = find("encoder")?;
        let decoder = find("decoder")?;
        let joiner = find("joiner")?;
        let tokens = model_dir.join("tokens.txt");
        if !tokens.exists() {
            return Err(CoreError::Transcription(format!(
                "tokens.txt not found in {}",
                model_dir.display()
            )));
        }

        let model_type = match kind {
            StreamingKind::NemoTransducer => Some("nemo_transducer".to_string()),
            StreamingKind::Zipformer => None,
        };

        let config = OnlineRecognizerConfig {
            model_config: OnlineModelConfig {
                transducer: OnlineTransducerModelConfig {
                    encoder: Some(encoder),
                    decoder: Some(decoder),
                    joiner: Some(joiner),
                },
                tokens: Some(tokens.to_string_lossy().to_string()),
                num_threads: n_threads,
                provider: Some("cpu".to_string()),
                model_type,
                ..Default::default()
            },
            decoding_method: Some("greedy_search".to_string()),
            enable_endpoint: true,
            rule1_min_trailing_silence: 2.4,
            rule2_min_trailing_silence: 1.2,
            rule3_min_utterance_length: 20.0,
            hotwords_score: 1.5,
            ..Default::default()
        };

        let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| {
            CoreError::Transcription(
                "failed to create streaming recognizer; check model files".to_string(),
            )
        })?;

        log::info!(
            "streaming engine loaded from {} ({n_threads} threads, {kind:?})",
            model_dir.display()
        );

        Ok(Self {
            recognizer,
            kind,
        })
    }

    pub fn create_session(&self, hotwords: Option<&str>) -> StreamingSession {
        let stream = match hotwords.filter(|h| !h.trim().is_empty()) {
            Some(words) => self.recognizer.create_stream_with_hotwords(words),
            None => self.recognizer.create_stream(),
        };
        StreamingSession {
            stream,
            started_at: std::time::Instant::now(),
        }
    }

    /// Feeds a chunk of waveform samples into the session. The caller is
    /// responsible for tracking the capture-buffer watermark.
    pub fn accept(&self, session: &StreamingSession, samples: &[f32]) {
        if !samples.is_empty() {
            session.stream.accept_waveform(16000, samples);
            self.recognizer.decode(&session.stream);
        }
    }

    pub fn decode(&self, session: &StreamingSession) {
        self.recognizer.decode(&session.stream);
    }

    pub fn is_ready(&self, session: &StreamingSession) -> bool {
        self.recognizer.is_ready(&session.stream)
    }

    pub fn is_endpoint(&self, session: &StreamingSession) -> bool {
        self.recognizer.is_endpoint(&session.stream)
    }

    pub fn result(&self, session: &StreamingSession) -> String {
        let text = self
            .recognizer
            .get_result(&session.stream)
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default();
        match self.kind {
            // icefall librispeech zipformer emits uppercase; normalize for
            // captions/dictation.
            StreamingKind::Zipformer => text.to_lowercase(),
            StreamingKind::NemoTransducer => text,
        }
    }

    /// Starts a fresh utterance: resets the internal stream state and the
    /// utterance clock. The capture watermark is owned by the caller and is
    /// NOT reset here.
    pub fn reset(&self, session: &mut StreamingSession) {
        self.recognizer.reset(&session.stream);
        session.started_at = std::time::Instant::now();
    }
/// Minimum samples the caller must accumulate before `accept` for the
    /// model's decoder to make progress (icefall zipformer needs exact
    /// chunk+context frames; nemo/parakeet accepts any size).
    pub fn chunk_samples(&self) -> usize {
        match self.kind {
            StreamingKind::NemoTransducer => 0,
            // Empirically the en-20M zipformer needs >= 39 frames (~480 ms)
            // per decode; 6240 samples still under-feeds it (GetFrames abort).
            StreamingKind::Zipformer => 7680,
        }
    }

    #[cfg(test)]
    fn probe_chunk(&self, session: &StreamingSession, samples: &[f32]) {
        session.stream.accept_waveform(16000, samples);
        self.recognizer.decode(&session.stream);
        eprintln!("probe: accepted {} samples, decode ok", samples.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Abort-safe probe: accepts one chunk of PROBE_CHUNK samples and decodes.
    /// Used to find the minimum viable chunk size for icefall zipformer.
    #[test]
    #[ignore]
    fn zipformer_probe_chunk() {
        let home = std::env::var("HOME").expect("HOME set");
        let model_dir = std::env::var("OPENDICTATE_TEST_MODEL_DIR").unwrap_or_else(|_| {
            std::path::Path::new(&home)
                .join(".local/share/opendictate/models/streaming-zipformer-en-20m-2023-02-17")
                .to_string_lossy()
                .into_owned()
        });
        let n: usize = std::env::var("PROBE_CHUNK")
            .expect("PROBE_CHUNK env")
            .parse()
            .unwrap();
        let wav_path = std::path::Path::new(&home).join(".cache/opendictate-stream-test.wav");
        let wave = sherpa_onnx::Wave::read(&wav_path.to_string_lossy()).expect("wave readable");
        let recognizer = StreamingRecognizer::new(
            std::path::Path::new(&model_dir),
            StreamingKind::Zipformer,
        )
        .unwrap();
        let session = recognizer.create_session(None);
        recognizer.probe_chunk(&session, &wave.samples()[..n.min(wave.samples().len())]);
        let r = recognizer.result(&session);
        eprintln!("probe result: {:?}", r);
    }

    /// Runs the streaming recognizer over a wav file with chunked input,
    /// simulating a live capture loop. Requires an installed streaming model
    /// and a test wav; skipped in CI (`cargo test -- --ignored`).
    #[test]
    #[ignore]
    fn streaming_recognizes_chunked_wav() {
        let home = std::env::var("HOME").expect("HOME set");
        let model_dir = std::env::var("OPENDICTATE_TEST_MODEL_DIR").unwrap_or_else(|_| {
            std::path::Path::new(&home)
                .join(".local/share/opendictate/models/parakeet-unified-en-0.6b-int8-streaming-560ms")
                .to_string_lossy()
                .into_owned()
        });
        let wav_path = std::path::Path::new(&home)
            .join(".cache/opendictate-stream-test.wav");
        if !wav_path.exists() {
            eprintln!("skipping: test wav not found at {}", wav_path.display());
            return;
        }
        let kind = if model_dir.to_lowercase().contains("zipformer") {
            StreamingKind::Zipformer
        } else {
            StreamingKind::NemoTransducer
        };

        let recognizer =
            StreamingRecognizer::new(std::path::Path::new(&model_dir), kind).unwrap();
        let mut session = recognizer.create_session(None);

        let wave = sherpa_onnx::Wave::read(&wav_path.to_string_lossy()).expect("wave readable");
        assert_eq!(wave.sample_rate(), 16000);

        let chunk = match kind {
            StreamingKind::Zipformer => 7680usize,
            StreamingKind::NemoTransducer => 3200usize,
        }; // 480 ms / 200 ms
        let mut partials = Vec::new();
        let mut finalized = 0;
        for slice in wave.samples().chunks(chunk) {
            recognizer.accept(&session, slice);
            if recognizer.is_ready(&session) {
                let text = recognizer.result(&session);
                if !text.is_empty() {
                    partials.push(text.clone());
                }
            }
            if recognizer.is_endpoint(&session) {
                let text = recognizer.result(&session);
                if !text.is_empty() {
                    finalized += 1;
                }
                recognizer.reset(&mut session);
            }
        }

        assert!(!partials.is_empty(), "expected partial hypotheses");
        let final_text = recognizer.result(&session);
        assert!(!final_text.is_empty(), "expected a final hypothesis");
        eprintln!("partials={partials:?}");
        eprintln!("finalized utterances={finalized} final={final_text:?}");
    }
}