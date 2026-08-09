use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::{CoreError, Result};

pub const SAMPLE_RATE: u32 = 16_000;
pub const MAX_RECORDING_SECS: u64 = 120;
pub const MAX_RECORDING_SAMPLES: usize = (SAMPLE_RATE * MAX_RECORDING_SECS as u32) as usize;

const STATE_IDLE: u8 = 0;
const STATE_RECORDING: u8 = 1;

struct StreamHandle(Option<cpal::Stream>);

// SAFETY: cpal::Stream is !Send+!Sync as a conservative platform marker. The
// underlying capture handles (ALSA, PulseAudio, WASAPI, CoreAudio) are safe to
// move between threads when access is serialized through a Mutex, and the
// stream is only ever dropped from the thread that holds the lock.
unsafe impl Send for StreamHandle {}
unsafe impl Sync for StreamHandle {}

pub struct AudioRecorder {
    state: Arc<AtomicU8>,
    buffer: Arc<Mutex<Vec<f32>>>,
    stop_signal: Arc<AtomicBool>,
    stream: Arc<Mutex<StreamHandle>>,
    started_at: Arc<Mutex<Option<Instant>>>,
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(STATE_IDLE)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            stop_signal: Arc::new(AtomicBool::new(false)),
            stream: Arc::new(Mutex::new(StreamHandle(None))),
            started_at: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self) -> Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| {
            CoreError::Audio("no microphone found; connect one and try again".to_string())
        })?;
        self.start_with_device(device)
    }

    pub fn list_input_devices() -> Vec<String> {
        cpal::default_host()
            .input_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| d.name().ok())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default()
    }

    fn start_with_device(&self, device: cpal::Device) -> Result<()> {
        if self.state.load(Ordering::SeqCst) != STATE_IDLE {
            return Err(CoreError::Audio("recording already in progress".to_string()));
        }

        self.stop_signal.store(false, Ordering::SeqCst);
        self.buffer.lock().map(|mut b| b.clear()).unwrap();

        let default_config = device
            .default_input_config()
            .map_err(|e| CoreError::Audio(format!("failed to read input config: {e}")))?;
        let native_rate = default_config.sample_rate().0;
        let native_channels = default_config.channels();
        let config = cpal::StreamConfig {
            channels: native_channels,
            sample_rate: cpal::SampleRate(native_rate),
            buffer_size: cpal::BufferSize::Default,
        };
        log::info!(
            "capture: native {native_rate} Hz / {native_channels} ch, target {SAMPLE_RATE} Hz mono"
        );

        let buffer = Arc::clone(&self.buffer);
        let stop_signal = Arc::clone(&self.stop_signal);
        let err_stop_signal = Arc::clone(&self.stop_signal);
        let started_at = Arc::clone(&self.started_at);
        let resample_pos = Arc::new(Mutex::new(0.0_f64));
        let resample_ratio = SAMPLE_RATE as f64 / native_rate as f64;

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stop_signal.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(guard) = started_at.lock() {
                        if let Some(start) = *guard {
                            if start.elapsed().as_secs() >= MAX_RECORDING_SECS {
                                stop_signal.store(true, Ordering::SeqCst);
                                return;
                            }
                        }
                    }

                    let ch = native_channels as usize;
                    let mono: Vec<f32> = data
                        .chunks_exact(ch)
                        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                        .collect();

                    let resampled = if native_rate == SAMPLE_RATE {
                        mono
                    } else {
                        let mut out = Vec::with_capacity(
                            (mono.len() as f64 * resample_ratio) as usize + 1,
                        );
                        let mut pos = resample_pos.lock().unwrap_or_else(|e| e.into_inner());
                        while (pos.floor() as usize) < mono.len().saturating_sub(1) {
                            let idx = pos.floor() as usize;
                            let frac = (*pos - idx as f64) as f32;
                            let sample = mono[idx] * (1.0 - frac) + mono[idx + 1] * frac;
                            out.push(sample);
                            *pos += 1.0 / resample_ratio;
                        }
                        *pos -= mono.len() as f64;
                        if *pos < 0.0 {
                            *pos = 0.0;
                        }
                        out
                    };

                    if let Ok(mut buf) = buffer.try_lock() {
                        let remaining = MAX_RECORDING_SAMPLES.saturating_sub(buf.len());
                        let to_copy = resampled.len().min(remaining);
                        buf.extend_from_slice(&resampled[..to_copy]);
                        if remaining == 0 {
                            stop_signal.store(true, Ordering::SeqCst);
                        }
                    }
                },
                move |err| {
                    log::error!("audio stream error: {err}");
                    err_stop_signal.store(true, Ordering::SeqCst);
                },
                None,
            )
            .map_err(|e| CoreError::Audio(format!("failed to build input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| CoreError::Audio(format!("failed to start stream: {e}")))?;

        self.stream
            .lock()
            .map(|mut s| s.0 = Some(stream))
            .unwrap();
        self.started_at
            .lock()
            .map(|mut t| *t = Some(Instant::now()))
            .unwrap();
        self.state.store(STATE_RECORDING, Ordering::SeqCst);
        log::info!("recording started");
        Ok(())
    }

    pub fn stop(&self) -> Result<Vec<f32>> {
        if self.state.load(Ordering::SeqCst) != STATE_RECORDING {
            return Err(CoreError::Audio("no recording in progress".to_string()));
        }
        self.stop_signal.store(true, Ordering::SeqCst);
        self.stream
            .lock()
            .map(|mut s| s.0 = None)
            .unwrap();
        let audio = self
            .buffer
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default();
        self.started_at
            .lock()
            .map(|mut t| *t = None)
            .unwrap();
        self.state.store(STATE_IDLE, Ordering::SeqCst);
        log::info!("recording stopped: {} samples", audio.len());
        Ok(audio)
    }

    pub fn cancel(&self) -> Result<()> {
        if self.state.load(Ordering::SeqCst) != STATE_RECORDING {
            return Err(CoreError::Audio("no recording in progress".to_string()));
        }
        self.stop_signal.store(true, Ordering::SeqCst);
        self.stream
            .lock()
            .map(|mut s| s.0 = None)
            .unwrap();
        self.buffer
            .lock()
            .map(|mut b| *b = Vec::new())
            .unwrap();
        self.started_at
            .lock()
            .map(|mut t| *t = None)
            .unwrap();
        self.state.store(STATE_IDLE, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_recording(&self) -> bool {
        self.state.load(Ordering::SeqCst) == STATE_RECORDING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_recorder_is_idle() {
        let recorder = AudioRecorder::new();
        assert!(!recorder.is_recording());
        assert!(recorder.stop().is_err());
        assert!(recorder.cancel().is_err());
    }

    #[test]
    fn stop_without_start_errors() {
        let recorder = AudioRecorder::new();
        assert!(recorder.stop().is_err());
    }
}
