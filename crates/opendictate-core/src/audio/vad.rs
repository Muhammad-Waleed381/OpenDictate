use std::path::Path;
use std::sync::Mutex;

use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use crate::error::{CoreError, Result};

#[derive(Clone, Debug)]
pub struct VadResult {
    pub trimmed_audio: Vec<f32>,
    pub speech_duration_ms: u64,
    pub has_speech: bool,
}

pub struct SileroVad {
    detector: Mutex<VoiceActivityDetector>,
}

// SAFETY: VadModelConfig holds plain data; the stateful detector is created
// fresh per process() call, so sharing the config across threads is safe.
unsafe impl Send for SileroVad {}
unsafe impl Sync for SileroVad {}

impl SileroVad {
    pub fn new(model_path: &Path) -> Result<Self> {
        Self::with_threshold(model_path, 0.5)
    }

    pub fn with_threshold(model_path: &Path, threshold: f32) -> Result<Self> {
        if !model_path.exists() {
            return Err(CoreError::Audio(format!(
                "silero VAD model not found at {}",
                model_path.display()
            )));
        }

        let threshold = threshold.clamp(0.0, 1.0);
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model_path.to_string_lossy().to_string()),
                threshold,
                min_silence_duration: 0.25,
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

        let detector = VoiceActivityDetector::create(&config, 60.0)
            .ok_or_else(|| CoreError::Audio("failed to initialize silero VAD".to_string()))?;

        log::info!("silero VAD loaded from {}", model_path.display());
        Ok(Self {
            detector: Mutex::new(detector),
        })
    }

    /// Feeds streaming samples into the persistent VAD detector and returns
    /// whether speech is currently detected. Maintains RNN state between frames.
    pub fn accept_streaming(&self, samples: &[f32]) -> bool {
        if samples.is_empty() {
            return false;
        }
        let detector = match self.detector.lock() {
            Ok(detector) => detector,
            Err(_) => return false,
        };
        let chunk_size = 512;
        for chunk in samples.chunks(chunk_size) {
            if chunk.len() == chunk_size {
                detector.accept_waveform(chunk);
            } else {
                let mut padded = vec![0.0f32; chunk_size];
                padded[..chunk.len()].copy_from_slice(chunk);
                detector.accept_waveform(&padded);
            }
        }
        let is_speech = detector.detected() || !detector.is_empty();
        detector.clear();
        is_speech
    }

    /// Resets the persistent streaming state of the VAD detector.
    pub fn reset(&self) {
        if let Ok(detector) = self.detector.lock() {
            detector.reset();
        }
    }

    pub fn process(&self, audio: &[f32]) -> VadResult {
        if audio.is_empty() {
            return VadResult {
                trimmed_audio: Vec::new(),
                speech_duration_ms: 0,
                has_speech: false,
            };
        }

        let detector = match self.detector.lock() {
            Ok(detector) => detector,
            Err(_) => {
                log::warn!("VAD: detector lock poisoned, falling back to energy");
                return apply_energy_vad(audio);
            }
        };
        detector.reset();

        let chunk_size = 512;
        let mut has_speech = false;
        let mut first_speech_idx: Option<usize> = None;
        let mut last_speech_idx: usize = 0;

        for chunk in audio.chunks(chunk_size) {
            let actual_len = chunk.len();
            if actual_len == chunk_size {
                detector.accept_waveform(chunk);
            } else {
                let mut padded = vec![0.0f32; chunk_size];
                padded[..actual_len].copy_from_slice(chunk);
                detector.accept_waveform(&padded);
            }

            while !detector.is_empty() {
                has_speech = true;
                if let Some(seg) = detector.front() {
                    let start = (seg.start().max(0) as usize).min(audio.len());
                    let end = (start + seg.samples().len()).min(audio.len());
                    if first_speech_idx.is_none() {
                        first_speech_idx = Some(start);
                    }
                    last_speech_idx = last_speech_idx.max(end);
                }
                detector.pop();
            }
        }

        detector.flush();
        while !detector.is_empty() {
            has_speech = true;
            if let Some(seg) = detector.front() {
                let start = (seg.start().max(0) as usize).min(audio.len());
                let end = (start + seg.samples().len()).min(audio.len());
                if first_speech_idx.is_none() {
                    first_speech_idx = Some(start);
                }
                last_speech_idx = last_speech_idx.max(end);
            }
            detector.pop();
        }

        if !has_speech {
            let energy_res = apply_energy_vad(audio);
            if energy_res.has_speech {
                return energy_res;
            }
            return VadResult {
                trimmed_audio: Vec::new(),
                speech_duration_ms: 0,
                has_speech: false,
            };
        }

        // Trim leading and trailing silence with 300ms padding, keeping continuous speech audio intact
        let pad_samples = (16_000 * 300) / 1000; // 300ms
        let start_sample = first_speech_idx.unwrap_or(0).saturating_sub(pad_samples);
        let end_sample = (last_speech_idx + pad_samples).min(audio.len());

        let trimmed_audio = if start_sample < end_sample {
            audio[start_sample..end_sample].to_vec()
        } else {
            audio.to_vec()
        };

        VadResult {
            speech_duration_ms: (trimmed_audio.len() as u64 * 1000) / 16_000,
            trimmed_audio,
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
        energy_threshold: 0.008,
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
    apply_energy_vad_with_config(audio, &default_config())
}

pub fn apply_energy_vad_with_config(audio: &[f32], config: &VadConfig) -> VadResult {
    if audio.is_empty() {
        return VadResult {
            trimmed_audio: Vec::new(),
            speech_duration_ms: 0,
            has_speech: false,
        };
    }

    let raw_flags = classify_frames(audio, config);
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

/// Map a 0..1 sensitivity knob to the energy VAD threshold. Lower sensitivity
/// (0.0) means louder speech is required; higher sensitivity (1.0) catches
/// quieter speech.
pub fn sensitivity_to_energy_threshold(sensitivity: f32) -> f32 {
    let s = sensitivity.clamp(0.0, 1.0);
    0.015 - 0.013 * s
}

/// Map a 0..1 sensitivity knob to the Silero speech-probability threshold.
/// Lower sensitivity = higher threshold (needs more confident speech).
pub fn sensitivity_to_silero_threshold(sensitivity: f32) -> f32 {
    let s = sensitivity.clamp(0.0, 1.0);
    0.7 - 0.4 * s
}

pub fn apply_vad(audio: &[f32], silero: Option<&SileroVad>, sensitivity: f32) -> VadResult {
    match silero {
        Some(vad) => vad.process(audio),
        None => {
            let mut config = default_config();
            config.energy_threshold = sensitivity_to_energy_threshold(sensitivity);
            apply_energy_vad_with_config(audio, &config)
        }
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
        let result = apply_vad(&[0.0f32; 16_000], None, 0.5);
        assert!(!result.has_speech);
    }

    #[test]
    fn sensitivity_mapping_is_monotonic() {
        let low = sensitivity_to_silero_threshold(0.0);
        let mid = sensitivity_to_silero_threshold(0.5);
        let high = sensitivity_to_silero_threshold(1.0);
        assert!(low > mid && mid > high, "silero thresholds must decrease with sensitivity");

        let elow = sensitivity_to_energy_threshold(0.0);
        let ehigh = sensitivity_to_energy_threshold(1.0);
        assert!(elow > ehigh, "energy thresholds must decrease with sensitivity");
    }

    #[test]
    fn energy_vad_respects_sensitivity() {
        let quiet: Vec<f32> = vec![0.0f32; 8_000];
        let speech: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.05).sin() * 0.08).collect();
        let mut audio = quiet.clone();
        audio.extend_from_slice(&speech);
        audio.extend_from_slice(&quiet);

        let insensitive = apply_vad(&audio, None, 0.0);
        let sensitive = apply_vad(&audio, None, 1.0);
        assert!(!insensitive.has_speech || sensitive.has_speech);
        assert!(sensitive.has_speech);
    }
}
