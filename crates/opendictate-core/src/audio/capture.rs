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

/// Shared capture buffer with a monotonic base offset.
///
/// `base` counts the samples that have been dropped from the front of
/// `samples` — either drained to enforce `MAX_RECORDING_SAMPLES` or discarded
/// by `clear_buffer()`. Consumers track an *absolute* watermark (total samples
/// ever appended) which maps to a buffer index as `watermark - base`, so
/// watermarks stay valid across front-drains and buffer clears.
#[derive(Default)]
pub(crate) struct SharedBuffer {
    pub(crate) samples: Vec<f32>,
    pub(crate) base: u64,
}

impl SharedBuffer {
    /// Appends samples, draining from the front when the recording cap would
    /// be exceeded. Advances `base` by the number of drained samples.
    pub(crate) fn push(&mut self, new_samples: &[f32]) {
        let to_add = new_samples.len();
        if self.samples.len() + to_add > MAX_RECORDING_SAMPLES {
            let excess = (self.samples.len() + to_add).saturating_sub(MAX_RECORDING_SAMPLES);
            let drain_len = excess.min(self.samples.len());
            self.samples.drain(0..drain_len);
            self.base += drain_len as u64;
        }
        self.samples.extend_from_slice(new_samples);
    }

    /// Drops all samples (advancing `base` so existing watermarks stay valid)
    /// and resets the base to zero — valid because the buffer is empty
    /// afterwards, so any watermark resolves to index 0.
    pub(crate) fn clear(&mut self) {
        self.base += self.samples.len() as u64;
        self.samples.clear();
        self.base = 0;
    }
}

pub struct AudioRecorder {
    state: Arc<AtomicU8>,
    buffer: Arc<Mutex<SharedBuffer>>,
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
            buffer: Arc::new(Mutex::new(SharedBuffer::default())),
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
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();

        let handle = crate::audio::pulse::spawn_capture(
            source,
            Arc::clone(&self.stop_signal),
            Arc::clone(&self.buffer),
            Arc::clone(&self.started_at),
        )?;
        *self.stream.0.lock().unwrap_or_else(|e| e.into_inner()) = CaptureHandle::Pulse(handle);
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
        let samples = &buffer.samples;
        if samples.is_empty() {
            return 0.0;
        }
        let start = samples.len().saturating_sub(SAMPLE_RATE as usize / 10);
        let window = &samples[start..];
        let sum_sq: f32 = window.iter().map(|&s| s * s).sum();
        (sum_sq / window.len() as f32).sqrt()
    }

    /// Returns every sample appended since the given watermark and advances
    /// the watermark to the absolute end of the buffer. Used for streaming ASR,
    /// live captions, and KWS.
    ///
    /// The watermark is an *absolute* sample count (total samples ever
    /// appended), not a buffer index: front-drains and `clear_buffer` shift
    /// the buffer contents, so only the monotonic `SharedBuffer::base` offset
    /// keeps watermarks meaningful. Samples dropped from the front before an
    /// outdated watermark are gone for good; the consumer simply receives
    /// everything still available.
    pub fn take_since(&self, watermark: &mut u64) -> Vec<f32> {
        let buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        let end = buffer.base + buffer.samples.len() as u64;
        let start = (*watermark).min(end).saturating_sub(buffer.base) as usize;
        let out = buffer.samples[start..].to_vec();
        *watermark = end;
        out
    }

    fn start_with_device(&self, device: cpal::Device) -> Result<()> {
        if self.state.load(Ordering::SeqCst) != STATE_IDLE {
            return Err(CoreError::Audio(
                "recording already in progress".to_string(),
            ));
        }

        self.stop_signal.store(false, Ordering::SeqCst);
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();

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
        let resample_pos = Arc::new(Mutex::new(0.0_f64));
        let resample_ratio = SAMPLE_RATE as f64 / native_rate as f64;

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stop_signal.load(Ordering::Relaxed) {
                        return;
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
                        buf.push(&resampled);
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

        *self.stream.0.lock().unwrap_or_else(|e| e.into_inner()) = CaptureHandle::Alsa(stream);
        *self.started_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
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
        let audio = {
            let mut buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut buf.samples)
        };
        *self.started_at.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *self.started_at.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.state.store(STATE_IDLE, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_recording(&self) -> bool {
        self.state.load(Ordering::SeqCst) == STATE_RECORDING
    }

    /// Clears the sample buffer without stopping the hardware stream.
    /// Used when handsfree mode holds the mic open but a new dictation session
    /// needs a clean starting point (discarding ambient audio captured so far).
    ///
    /// Existing watermarks stay valid: `SharedBuffer::clear` treats the dropped
    /// samples as front-drained, so consumers resume from the next appended
    /// sample instead of stalling until the buffer regrows past the old
    /// watermark.
    pub fn clear_buffer(&self) {
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
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
    fn take_since_tracks_appends() {
        let recorder = AudioRecorder::new();
        {
            let mut buf = recorder.buffer.lock().unwrap();
            buf.push(&[1.0, 2.0, 3.0]);
        }
        let mut watermark: u64 = 0;
        assert_eq!(recorder.take_since(&mut watermark), vec![1.0, 2.0, 3.0]);
        assert_eq!(watermark, 3);
        // Nothing new → empty.
        assert!(recorder.take_since(&mut watermark).is_empty());
        {
            let mut buf = recorder.buffer.lock().unwrap();
            buf.push(&[4.0, 5.0]);
        }
        assert_eq!(recorder.take_since(&mut watermark), vec![4.0, 5.0]);
    }

    #[test]
    fn take_since_survives_front_drain_at_cap() {
        let recorder = AudioRecorder::new();
        // Fill to the cap so the next push drains from the front.
        let filled: Vec<f32> = (0..MAX_RECORDING_SAMPLES).map(|i| i as f32).collect();
        {
            let mut buf = recorder.buffer.lock().unwrap();
            buf.push(&filled);
        }
        let mut watermark: u64 = MAX_RECORDING_SAMPLES as u64;
        // Buffer is at the cap; push a batch → drains exactly that many from
        // the front. The old (index-based) implementation returned empty
        // forever after this point.
        let batch: Vec<f32> = vec![9.0; 100];
        {
            let mut buf = recorder.buffer.lock().unwrap();
            buf.push(&batch);
        }
        let drained = recorder.take_since(&mut watermark);
        assert_eq!(drained.len(), 100);
        assert_eq!(drained[0], 9.0);
        // Watermark is absolute: cap drained + cap buffered + new batch.
        assert_eq!(watermark, MAX_RECORDING_SAMPLES as u64 + 100);
    }

    #[test]
    fn take_since_survives_clear_buffer() {
        let recorder = AudioRecorder::new();
        {
            let mut buf = recorder.buffer.lock().unwrap();
            buf.push(&[1.0, 2.0, 3.0, 4.0]);
        }
        let mut watermark: u64 = 2;
        // clear_buffer with a stale watermark used to stall consumers until
        // the buffer regrew past the old watermark, then skip fresh audio.
        recorder.clear_buffer();
        assert!(recorder.take_since(&mut watermark).is_empty());
        {
            let mut buf = recorder.buffer.lock().unwrap();
            buf.push(&[7.0, 8.0]);
        }
        assert_eq!(recorder.take_since(&mut watermark), vec![7.0, 8.0]);
        assert_eq!(watermark, 2);
    }

    #[test]
    fn take_since_with_future_watermark_returns_empty() {
        let recorder = AudioRecorder::new();
        {
            let mut buf = recorder.buffer.lock().unwrap();
            buf.push(&[1.0]);
        }
        let mut watermark: u64 = 1_000_000;
        assert!(recorder.take_since(&mut watermark).is_empty());
        assert_eq!(watermark, 1);
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
