use std::path::Path;

use crate::audio::capture::SAMPLE_RATE;

use sherpa_onnx::{
    OnlineModelConfig, OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
    OnlineTransducerModelConfig,
};

use super::provider::Provider;
use crate::error::{CoreError, Result};

const MAX_STT_THREADS: usize = 8;

/// Streaming (online) recognizer backed by sherpa-onnx `OnlineRecognizer`
/// (NeMo/parakeet unified transducer models).
///
/// The sherpa-onnx crate declares `OnlineRecognizer`/`OnlineStream` as
/// `Send + Sync`; access to a session is serialized through a mutex in the
/// app.
pub struct StreamingRecognizer {
    recognizer: OnlineRecognizer,
    /// Provider the recognizer actually came up on (after fallback).
    pub provider: &'static str,
}

pub struct StreamingSession {
    stream: OnlineStream,
    /// When the current utterance started (for duration reporting).
    pub started_at: std::time::Instant,
}

impl StreamingRecognizer {
    pub fn new(model_dir: &Path) -> Result<Self> {
        Self::new_for(model_dir, None)
    }

    /// [`new_for`] with an execution-provider request. A GPU provider that
    /// fails to create (missing libs, no drivers, unsupported model) falls
    /// back to CPU transparently; `provider` reports what actually ran.
    pub fn new_with_provider(
        model_dir: &Path,
        model_type: Option<&str>,
        requested: Provider,
    ) -> Result<Self> {
        match Self::build(model_dir, model_type, requested) {
            Ok(r) => Ok(r),
            Err(e) if requested.is_gpu() => {
                log::warn!(
                    "provider '{}' unavailable ({e}); falling back to cpu",
                    requested.as_str()
                );
                Self::build(model_dir, model_type, Provider::Cpu)
            }
            Err(e) => Err(e),
        }
    }

    /// Same as [`new`] but with an explicit sherpa `model_type`. Pass `None`
    /// to let sherpa auto-detect (required for non-NeMo engines like
    /// zipformer, where a wrong hint aborts feature setup).
    pub fn new_for(model_dir: &Path, model_type: Option<&str>) -> Result<Self> {
        Self::build(model_dir, model_type, Provider::Cpu)
    }

    fn build(model_dir: &Path, model_type: Option<&str>, provider: Provider) -> Result<Self> {
        if !model_dir.exists() {
            return Err(CoreError::Transcription(format!(
                "streaming model directory not found at {}",
                model_dir.display()
            )));
        }

        // Online transducers decode many small chunks sequentially; beyond
        // ~4 intra-op threads the runtime thrashes hyperthreads and decode
        // throughput DROPS (measured: RTF 21 at 8 threads vs 15 at 4 on a
        // 4-core part), so cap well below logical parallelism.
        let n_threads = std::thread::available_parallelism()
            .map(|p| ((p.get() / 2).clamp(1, MAX_STT_THREADS)) as i32)
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

        let config = OnlineRecognizerConfig {
            model_config: OnlineModelConfig {
                transducer: OnlineTransducerModelConfig {
                    encoder: Some(encoder),
                    decoder: Some(decoder),
                    joiner: Some(joiner),
                },
                tokens: Some(tokens.to_string_lossy().to_string()),
                num_threads: n_threads,
                provider: Some(provider.as_str().to_string()),
                model_type: model_type.map(|s| s.to_string()),
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
            CoreError::Transcription(format!(
                "failed to create streaming recognizer on provider '{}'; check model files",
                provider.as_str()
            ))
        })?;

        log::info!(
            "streaming engine loaded from {} ({n_threads} threads, provider {})",
            model_dir.display(),
            provider.as_str()
        );

        Ok(Self {
            recognizer,
            provider: provider.as_str(),
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

    /// Feeds a chunk of waveform samples into the session. Does NOT decode:
    /// engines differ in how many feature frames a decode consumes
    /// (zipformer wants 39, NeMo accepts fewer), so decoding here would
    /// underflow the buffer on small feeds. Callers gate decoding through
    /// [`Self::is_ready`] / [`Self::drain`]. The capture-buffer watermark is
    /// owned by the caller.
    pub fn accept(&self, session: &StreamingSession, samples: &[f32]) {
        if !samples.is_empty() {
            session.stream.accept_waveform(16000, samples);
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
        self.recognizer
            .get_result(&session.stream)
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    }

    /// Decodes every buffered frame so `result` reflects all accepted audio.
    /// Call before reading the final hypothesis (the online decoder lags
    /// behind acceptance by design).
    pub fn drain(&self, session: &StreamingSession) {
        while self.recognizer.is_ready(&session.stream) {
            self.recognizer.decode(&session.stream);
        }
    }

    /// Measures this machine's decode speed against a fixed synthetic
    /// workload: feeds `BENCH_SECONDS` of speech-like noise through the model
    /// and returns the real-time factor (wall time / audio time). RTF < 1
    /// means the model can keep up with live audio on this CPU.
    pub fn benchmark_rtf(model_dir: &Path) -> Result<f32> {
        Self::benchmark_rtf_for(model_dir, Some("nemo_transducer"))
    }

    /// [`benchmark_rtf`] with an explicit sherpa `model_type` (see
    /// [`new_for`]).
    pub fn benchmark_rtf_for(model_dir: &Path, model_type: Option<&str>) -> Result<f32> {
        const BENCH_SECONDS: f32 = 3.0;
        const SAMPLES_PER_CHUNK: usize = 1600; // 100 ms
        let recognizer = Self::new_for(model_dir, model_type)?;
        let session = recognizer.create_session(None);

        // Deterministic pseudo-noise with speech-like energy; encoder cost is
        // amplitude-independent, so exact shape does not matter.
        let mut seed = 0x2545_F491_u32;
        let mut chunk = Vec::with_capacity(SAMPLES_PER_CHUNK);
        let mut make_chunk = |chunk: &mut Vec<f32>| {
            chunk.clear();
            for _ in 0..SAMPLES_PER_CHUNK {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                chunk.push(((seed >> 16) as f32 / u16::MAX as f32 - 0.5) * 0.3);
            }
        };

        let chunks = (BENCH_SECONDS * SAMPLE_RATE as f32) as usize / SAMPLES_PER_CHUNK;
        let started = std::time::Instant::now();
        for _ in 0..chunks {
            make_chunk(&mut chunk);
            recognizer.accept(&session, &chunk);
            recognizer.drain(&session);
        }
        let rtf = started.elapsed().as_secs_f32() / BENCH_SECONDS;
        log::info!("streaming benchmark: {model_dir:?} rtf={rtf:.2}");
        Ok(rtf)
    }

    /// Starts a fresh utterance: resets the internal stream state and the
    /// utterance clock. The capture watermark is owned by the caller and is
    /// NOT reset here.
    pub fn reset(&self, session: &mut StreamingSession) {
        self.recognizer.reset(&session.stream);
        session.started_at = std::time::Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let wav_path = std::path::Path::new(&home).join(".cache/opendictate-stream-test.wav");
        if !wav_path.exists() {
            eprintln!("skipping: test wav not found at {}", wav_path.display());
            return;
        }

        let recognizer = StreamingRecognizer::new(std::path::Path::new(&model_dir)).unwrap();
        let mut session = recognizer.create_session(None);

        let wave = sherpa_onnx::Wave::read(&wav_path.to_string_lossy()).expect("wave readable");
        assert_eq!(wave.sample_rate(), 16000);

        let chunk = 3200usize; // 200 ms
        let mut partials = Vec::new();
        let mut finalized = 0;
        for slice in wave.samples().chunks(chunk) {
            recognizer.accept(&session, slice);
            if recognizer.is_ready(&session) {
                // accept() only feeds; decoding is explicit and gated.
                recognizer.drain(&session);
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
        recognizer.drain(&session);
        let final_text = recognizer.result(&session);
        assert!(!final_text.is_empty(), "expected a final hypothesis");
        eprintln!("partials={partials:?}");
        eprintln!("finalized utterances={finalized} final={final_text:?}");
    }

    /// Requests the CUDA provider on whatever machine runs this test. On a
    /// GPU-less box (the common case) creation must fail and land on CPU,
    /// which is exactly the behavior every non-NVIDIA user depends on when
    /// running a GPU-linked build. Requires the caption model installed and
    /// skips silently otherwise; pair with `--features gpu-shared`.
    #[test]
    fn cuda_request_falls_back_to_cpu_without_gpu() {
        use super::super::provider::{Provider, resolve};
        let dir = crate::stt::models::caption_model_dir();
        if !crate::stt::models::is_caption_model_ready() {
            eprintln!("skipping: caption model not installed");
            return;
        }
        // resolve() applies the hardware gate; on NVIDIA boxes the request
        // legitimately survives and this test has nothing to prove.
        let prov = resolve("cuda");
        if prov == Provider::Cuda {
            eprintln!("NVIDIA hardware present; skipping fallback assertion");
            return;
        }
        let rec = StreamingRecognizer::new_with_provider(&dir, None, prov)
            .expect("construction must succeed");
        assert_eq!(rec.provider, "cpu");
    }
}
