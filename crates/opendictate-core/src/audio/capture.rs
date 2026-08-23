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

/// Which capture backend is currently driving the shared buffer.
enum CaptureHandle {
    /// cpal/ALSA capture stream (kept alive for its Drop impl).
    Alsa(#[allow(dead_code)] cpal::Stream),
    /// PulseAudio capture thread (Linux only).
    #[cfg(target_os = "linux")]
    Pulse(std::thread::JoinHandle<()>),
    None,
}

// SAFETY: cpal::Stream is !Send+!Sync as a conservative platform marker. The
// underlying capture handles are safe to move between threads when access is
// serialized through the Mutex, and the stream is only ever dropped from the
// thread that holds the lock.
#[repr(transparent)]
struct SharedCaptureHandle(Mutex<CaptureHandle>);
unsafe impl Send for SharedCaptureHandle {}
unsafe impl Sync for SharedCaptureHandle {}

pub struct AudioRecorder {
    state: Arc<AtomicU8>,
    buffer: Arc<Mutex<Vec<f32>>>,
    stop_signal: Arc<AtomicBool>,
    stream: Arc<SharedCaptureHandle>,
    started_at: Arc<Mutex<Option<Instant>>>,
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// ALSA plugin/remap PCM names that cpal exposes as "input devices" but that
/// are not real microphones (rate converters, mixers, routing endpoints, ...).
const ALSA_PLUGIN_NAMES: &[&str] = &[
    "a52",
    "adsrs",
    "alaw",
    "asym",
    "autoconvert",
    "ctl",
    "dav",
    "dboss",
    "dshare",
    "dsnoop",
    "dsp",
    "dtable",
    "dup",
    "empty",
    "extplug",
    "file",
    "files",
    "hooks",
    "hw",
    "ioplug",
    "jack",
    "ladspa",
    "lfloat",
    "linear",
    "loop",
    "maflo",
    "mchmap",
    "mix",
    "mulaw",
    "multi",
    "null",
    "oss",
    "pipewire",
    "plug",
    "pulse",
    "rate",
    "route",
    "share",
    "shm",
    "softvol",
    "speex",
    "speexrate",
    "samplerate",
    "lavrate",
    "tee",
    "upmix",
    "usbstream",
    "vdownmix",
    "dmix",
    "sysdefault",
];

const NON_INPUT_PREFIXES: &[&str] = &[
    "dsnoop",
    "surround",
    "front",
    "rear",
    "center_lfe",
    "side",
    "dmix",
    "hw:",
    "plughw",
    "iec958",
    "null",
    "jack",
    "multi",
    "usbstream",
    "sysdefault",
];

pub fn is_usable_mic_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && !n.contains(':')
        && !n.contains('=')
        && !ALSA_PLUGIN_NAMES.contains(&n)
        && !NON_INPUT_PREFIXES.iter().any(|p| n.starts_with(p))
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(STATE_IDLE)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            stop_signal: Arc::new(AtomicBool::new(false)),
            stream: Arc::new(SharedCaptureHandle(Mutex::new(CaptureHandle::None))),
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

    /// Starts recording from a stored mic id.
    ///
    ///   - `"default"` / empty → the system default input (cpal/ALSA).
    ///   - `"pulse:<source>"` → that exact PulseAudio source (Linux).
    ///   - anything else → a cpal device name, falling back to the default.
    pub fn start_with_name(&self, id: &str) -> Result<()> {
        let trimmed = id.trim();
        if trimmed.is_empty() || trimmed == "default" {
            return self.start();
        }
        #[cfg(target_os = "linux")]
        if let Some(source) = crate::audio::pulse::pulse_source_name(trimmed) {
            if !source.is_empty() {
                return self.start_pulse(source);
            }
        }
        self.start_with_cpal_name(trimmed)
    }

    /// PulseAudio capture from a named source (Linux).
    #[cfg(target_os = "linux")]
    pub fn start_pulse(&self, source: &str) -> Result<()> {
        if self.state.load(Ordering::SeqCst) != STATE_IDLE {
            return Err(CoreError::Audio(
                "recording already in progress".to_string(),
            ));
        }
        self.stop_signal.store(false, Ordering::SeqCst);
        self.buffer.lock().map(|mut b| b.clear()).unwrap();

        let handle = crate::audio::pulse::spawn_capture(
            source,
            Arc::clone(&self.stop_signal),
            Arc::clone(&self.buffer),
            Arc::clone(&self.started_at),
        )?;
        self.stream.0.lock().map(|mut s| *s = CaptureHandle::Pulse(handle)).unwrap();
        self.state.store(STATE_RECORDING, Ordering::SeqCst);
        log::info!("pulse recording started: {source}");
        Ok(())
    }

    fn start_with_cpal_name(&self, name: &str) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .input_devices()
            .map_err(|e| CoreError::Audio(format!("failed to list input devices: {e}")))?
            .find(|d| d.name().is_ok_and(|n| n == name));
        match device {
            Some(device) => match self.start_with_device(device) {
                Ok(()) => Ok(()),
                Err(e) => {
                    log::warn!(
                        "mic '{name}' failed to start ({e}), falling back to default device"
                    );
                    self.start()
                }
            },
            None => {
                log::warn!("mic '{name}' not found, falling back to default device");
                self.start()
            }
        }
    }

    pub fn list_input_devices() -> Vec<String> {
        cpal::default_host()
            .input_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| d.name().ok())
                    .filter(|name| is_usable_mic_name(name))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default()
    }

    pub fn current_rms(&self) -> f32 {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        if buffer.is_empty() {
            return 0.0;
        }
        let start = buffer.len().saturating_sub(SAMPLE_RATE as usize / 10);
        let window = &buffer[start..];
        let sum_sq: f32 = window.iter().map(|&s| s * s).sum();
        (sum_sq / window.len() as f32).sqrt()
    }

    /// Returns every sample appended since the given watermark and advances
    /// the watermark to the end of the buffer. Used for streaming ASR.
    pub fn take_since(&self, watermark: &mut usize) -> Vec<f32> {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        let start = (*watermark).min(buffer.len());
        let out = buffer[start..].to_vec();
        *watermark = buffer.len();
        out
    }

    fn start_with_device(&self, device: cpal::Device) -> Result<()> {
        if self.state.load(Ordering::SeqCst) != STATE_IDLE {
            return Err(CoreError::Audio(
                "recording already in progress".to_string(),
            ));
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
        // Log which device actually got opened. Several inputs can share a rate
        // and channel count (a USB headset and the built-in mic are both 48 kHz
        // mono here), so without the name the log cannot tell them apart — and
        // "recording from the wrong mic" looks identical to "the model is
        // broken": audio is captured, then the VAD reports no speech.
        let device_name = device
            .name()
            .unwrap_or_else(|_| "<unknown device>".to_string());
        log::info!(
            "capture: '{device_name}' native {native_rate} Hz / {native_channels} ch, target {SAMPLE_RATE} Hz mono"
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
                        let mut out =
                            Vec::with_capacity((mono.len() as f64 * resample_ratio) as usize + 1);
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

        self.stream.0.lock().map(|mut s| *s = CaptureHandle::Alsa(stream)).unwrap();
        self.started_at
            .lock()
            .map(|mut t| *t = Some(Instant::now()))
            .unwrap();
        self.state.store(STATE_RECORDING, Ordering::SeqCst);
        log::info!("recording started");
        Ok(())
    }

    /// Stops the active capture backend: drops the cpal stream or joins the
    /// PulseAudio thread. Must only be called after `stop_signal` is set.
    fn release_stream(&self) {
        let mut guard = self.stream.0.lock().unwrap_or_else(|e| e.into_inner());
        match std::mem::replace(&mut *guard, CaptureHandle::None) {
            CaptureHandle::Alsa(_) => {}
            #[cfg(target_os = "linux")]
            CaptureHandle::Pulse(handle) => {
                let _ = handle.join();
            }
            CaptureHandle::None => {}
        }
    }

    pub fn stop(&self) -> Result<Vec<f32>> {
        if self.state.load(Ordering::SeqCst) != STATE_RECORDING {
            return Err(CoreError::Audio("no recording in progress".to_string()));
        }
        self.stop_signal.store(true, Ordering::SeqCst);
        self.release_stream();
        let audio = self
            .buffer
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default();
        self.started_at.lock().map(|mut t| *t = None).unwrap();
        self.state.store(STATE_IDLE, Ordering::SeqCst);
        log::info!("recording stopped: {} samples", audio.len());
        Ok(audio)
    }

    pub fn cancel(&self) -> Result<()> {
        if self.state.load(Ordering::SeqCst) != STATE_RECORDING {
            return Err(CoreError::Audio("no recording in progress".to_string()));
        }
        self.stop_signal.store(true, Ordering::SeqCst);
        self.release_stream();
        self.buffer.lock().map(|mut b| *b = Vec::new()).unwrap();
        self.started_at.lock().map(|mut t| *t = None).unwrap();
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

    #[test]
    fn rejects_alsa_plugin_junk() {
        for junk in [
            "lavrate",
            "samplerate",
            "speexrate",
            "pipewire",
            "pulse",
            "speex",
            "upmix",
            "vdownmix",
            "dsnoop:CARD=PCH,DEV=0",
            "hw:0,0",
            "surround21",
            "dmix",
            "sysdefault",
            "jack",
            "multi",
            "usbstream",
            "plughw:0",
        ] {
            assert!(!is_usable_mic_name(junk), "{junk} should be rejected");
        }
    }

    #[test]
    fn accepts_real_mics_and_default() {
        for mic in [
            "default",
            "HDA Intel PCH",
            "USB Audio",
            "HD Webcam C270",
            "Jabra Evolve 20",
            "sof-hda-dsp",
            "Built-in Microphone",
        ] {
            assert!(is_usable_mic_name(mic), "{mic} should be accepted");
        }
    }
}
