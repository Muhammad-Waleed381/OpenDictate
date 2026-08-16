use std::path::Path;

use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use crate::error::{CoreError, Result};

#[derive(Clone, Debug)]
pub struct VadResult {
    pub trimmed_audio: Vec<f32>,
    pub speech_duration_ms: u64,
    pub has_speech: bool,
}

pub struct SileroVad {
    config: VadModelConfig,
}

// SAFETY: VadModelConfig holds plain data; the stateful detector is created
// fresh per process() call, so sharing the config across threads is safe.
unsafe impl Send for SileroVad {}
unsafe impl Sync for SileroVad {}

impl SileroVad {
    pub fn new(model_path: &Path) -> Result<Self> {
        if !model_path.exists() {
            return Err(CoreError::Audio(format!(
                "silero VAD model not found at {}",
                model_path.display()
            )));
        }

        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model_path.to_string_lossy().to_string()),
                threshold: 0.5,
                min_silence_duration: 0.5,
                min_speech_duration: 0.1,
                max_speech_duration: 120.0,
                ..Default::default()
            },
            sample_rate: 16000,
            num_threads: 1,
            provider: Some("cpu".to_string()),
            debug: false,
            ..Default::default()
        };

        VoiceActivityDetector::create(&config, 0.5)
            .ok_or_else(|| CoreError::Audio("failed to initialize silero VAD".to_string()))?;

        log::info!("silero VAD loaded from {}", model_path.display());
        Ok(Self { config })
    }

    pub fn process(&self, audio: &[f32]) -> VadResult {
        if audio.is_empty() {
            return VadResult {
                trimmed_audio: Vec::new(),
                speech_duration_ms: 0,
                has_speech: false,
            };
        }

        let detector = match VoiceActivityDetector::create(&self.config, 0.5) {
            Some(d) => d,
            None => {
                log::warn!("VAD: detector creation failed, falling back to energy");
                return apply_energy_vad(audio);
            }
        };

        let chunk_size = 512;
        for chunk in audio.chunks(chunk_size) {
            if chunk.len() == chunk_size {
                detector.accept_waveform(chunk);
            } else {
                let mut padded = vec![0.0f32; chunk_size];
                padded[..chunk.len()].copy_from_slice(chunk);
                detector.accept_waveform(&padded);
            }
        }
        detector.flush();

        let mut speech_samples: Vec<f32> = Vec::new();
        while !detector.is_empty() {
            if let Some(segment) = detector.front() {
                speech_samples.extend_from_slice(segment.samples());
            }
            detector.pop();
        }

        if speech_samples.is_empty() {
            return VadResult {
                trimmed_audio: Vec::new(),
                speech_duration_ms: 0,
                has_speech: false,
            };
        }

        VadResult {
            speech_duration_ms: (speech_samples.len() as u64 * 1000) / 16_000,
            trimmed_audio: speech_samples,
            has_speech: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VadConfig {
    pub energy_threshold: f32,
    pub frame_size: usize,
    pub min_speech_frames: usize,
    pub hangover_frames: usize,
}

pub fn default_config() -> VadConfig {
    VadConfig {
        energy_threshold: 0.01,
        frame_size: 480,
        min_speech_frames: 3,
        hangover_frames: 10,
    }
}

pub fn compute_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = frame.iter().map(|&s| s * s).sum();
    (sum_sq / frame.len() as f32).sqrt()
}

pub fn classify_frames(audio: &[f32], config: &VadConfig) -> Vec<bool> {
    audio
        .chunks(config.frame_size)
        .map(|frame| compute_rms(frame) >= config.energy_threshold)
        .collect()
}

pub fn apply_energy_vad(audio: &[f32]) -> VadResult {
    let config = default_config();

    if audio.is_empty() {
        return VadResult {
            trimmed_audio: Vec::new(),
            speech_duration_ms: 0,
            has_speech: false,
        };
    }

    let raw_flags = classify_frames(audio, &config);
    let speech_frames = raw_flags.iter().filter(|&&f| f).count();

    if speech_frames < config.min_speech_frames {
        return VadResult {
            trimmed_audio: Vec::new(),
            speech_duration_ms: 0,
            has_speech: false,
        };
    }

    let mut smoothed = raw_flags.clone();
    let mut hangover_remaining = 0usize;
    for flag in smoothed.iter_mut() {
        if *flag {
            hangover_remaining = config.hangover_frames;
        } else if hangover_remaining > 0 {
            *flag = true;
            hangover_remaining -= 1;
        }
    }

    let first_speech = smoothed.iter().position(|&f| f).unwrap_or(0);
    let last_speech = smoothed
        .len()
        .saturating_sub(1)
        .saturating_sub(smoothed.iter().rev().position(|&f| f).unwrap_or(0));

    let start_sample = first_speech * config.frame_size;
    let end_sample = ((last_speech + 1) * config.frame_size).min(audio.len());
    let trimmed_audio = audio[start_sample..end_sample].to_vec();

    VadResult {
        speech_duration_ms: (trimmed_audio.len() as u64 * 1000) / 16_000,
        trimmed_audio,
        has_speech: true,
    }
}

pub fn apply_vad(audio: &[f32], silero: Option<&SileroVad>) -> VadResult {
    match silero {
        Some(vad) => vad.process(audio),
        None => apply_energy_vad(audio),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(compute_rms(&[0.0f32; 480]), 0.0);
    }

    #[test]
    fn rms_of_signal_is_nonzero() {
        let signal: Vec<f32> = (0..480).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        assert!(compute_rms(&signal) > 0.0);
    }

    #[test]
    fn energy_vad_on_silence_returns_no_speech() {
        let result = apply_energy_vad(&[0.0f32; 16_000]);
        assert!(!result.has_speech);
        assert!(result.trimmed_audio.is_empty());
    }

    #[test]
    fn energy_vad_on_speech_returns_speech() {
        let mut audio = vec![0.0f32; 8_000];
        let speech: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        audio.extend_from_slice(&speech);
        audio.extend_from_slice(&vec![0.0f32; 8_000]);

        let result = apply_energy_vad(&audio);
        assert!(result.has_speech);
        assert!(result.speech_duration_ms > 0);
        assert!(result.trimmed_audio.len() < audio.len());
        assert!(result.trimmed_audio.len() >= 16_000);
    }

    #[test]
    fn energy_vad_on_empty_returns_no_speech() {
        let result = apply_energy_vad(&[]);
        assert!(!result.has_speech);
    }

    #[test]
    fn apply_vad_without_silero_uses_energy() {
        let result = apply_vad(&[0.0f32; 16_000], None);
        assert!(!result.has_speech);
    }
}
