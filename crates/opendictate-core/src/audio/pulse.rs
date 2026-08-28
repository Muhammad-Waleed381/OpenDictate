//! PulseAudio source enumeration and capture (Linux only).
//!
//! On PipeWire/PulseAudio systems, external microphones (wired USB headsets,
//! Bluetooth earbuds, ...) are not ALSA PCM devices — they exist only as
//! PulseAudio sources. cpal (the ALSA backend) cannot see or capture them, so
//! this module bridges the gap using `libpulse-sys` (the C API directly).
//! When the PulseAudio server is unreachable, callers fall back to the
//! cpal/ALSA path.
//!
//! Capture uses a threaded mainloop exactly like `pa_simple`: an internal
//! thread continuously polls the pulse mainloop and a small read callback
//! merely signals that data is ready. The capture thread holds the mainloop
//! lock to drain samples with `pa_stream_peek`/`pa_stream_drop`. This is the
//! most robust arrangement — a caller-driven mainloop that is only polled on
//! demand can miss the shared-ring-buffer wakeup and never receive audio.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use libpulse_sys as capi;

use crate::audio::capture::{SAMPLE_RATE, SharedBuffer};
use crate::error::CoreError;
use crate::Result;

/// Prefix stored in settings for a PulseAudio-selected microphone.
pub const PULSE_PREFIX: &str = "pulse:";

/// A microphone visible to PulseAudio.
#[derive(Debug, Clone)]
pub struct PulseSource {
    pub name: String,
    pub description: String,
}

/// Extracts the PulseAudio source name from a stored mic id (`pulse:<name>`).
pub fn pulse_source_name(id: &str) -> Option<&str> {
    id.strip_prefix(PULSE_PREFIX)
}

unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
}

struct ListState {
    tx: mpsc::Sender<PulseSource>,
    done: Arc<AtomicBool>,
}

extern "C" fn source_info_cb(
    _context: *mut capi::pa_context,
    info: *const capi::pa_source_info,
    eol: i32,
    userdata: *mut c_void,
) {
    let state = unsafe { &mut *(userdata as *mut ListState) };
    if eol != 0 {
        state.done.store(true, Ordering::SeqCst);
        return;
    }
    if info.is_null() {
        return;
    }
    let info = unsafe { &*info };
    // Monitor sources (the "What you hear" echo of a sink) are not microphones.
    if info.monitor_of_sink != capi::PA_INVALID_INDEX {
        return;
    }
    let Some(name) = (unsafe { cstr_to_string(info.name) }) else {
        return;
    };
    if name.is_empty() {
        return;
    }
    let description = unsafe { cstr_to_string(info.description) }
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| name.clone());
    let _ = state.tx.send(PulseSource { name, description });
}

/// Lists every non-monitor input source the PulseAudio server exposes.
/// Returns `None` when the server is unreachable.
pub fn list_sources() -> Option<Vec<PulseSource>> {
    unsafe {
        let mainloop = capi::pa_mainloop_new();
        if mainloop.is_null() {
            return None;
        }
        let api = capi::pa_mainloop_get_api(mainloop);
        let name = CString::new("opendictate-list").ok()?;
        let context = capi::pa_context_new(api, name.as_ptr());
        if context.is_null() {
            capi::pa_mainloop_free(mainloop);
            return None;
        }
        if capi::pa_context_connect(context, std::ptr::null(), 0, std::ptr::null()) < 0 {
            capi::pa_context_unref(context);
            capi::pa_mainloop_free(mainloop);
            return None;
        }

        let mut rv: i32 = 0;
        let deadline = Instant::now() + Duration::from_secs(5);
        while capi::pa_context_get_state(context) != capi::pa_context_state_t::Ready {
            if matches!(
                capi::pa_context_get_state(context),
                capi::pa_context_state_t::Failed | capi::pa_context_state_t::Terminated
            ) {
                capi::pa_context_disconnect(context);
                capi::pa_context_unref(context);
                capi::pa_mainloop_free(mainloop);
                return None;
            }
            if Instant::now() > deadline {
                capi::pa_context_disconnect(context);
                capi::pa_context_unref(context);
                capi::pa_mainloop_free(mainloop);
                return None;
            }
            capi::pa_mainloop_iterate(mainloop, 1, &mut rv);
        }

        let (tx, rx) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let list_state = Box::into_raw(Box::new(ListState {
            tx,
            done: Arc::clone(&done),
        }));
        let operation = capi::pa_context_get_source_info_list(
            context,
            Some(source_info_cb),
            list_state as *mut c_void,
        );
        if operation.is_null() {
            drop(Box::from_raw(list_state));
            capi::pa_context_disconnect(context);
            capi::pa_context_unref(context);
            capi::pa_mainloop_free(mainloop);
            return None;
        }

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut sources = Vec::new();
        while !done.load(Ordering::SeqCst) && Instant::now() < deadline {
            capi::pa_mainloop_iterate(mainloop, 1, &mut rv);
            while let Ok(source) = rx.try_recv() {
                sources.push(source);
            }
        }
        capi::pa_operation_unref(operation);
        drop(Box::from_raw(list_state));
        capi::pa_context_disconnect(context);
        capi::pa_context_unref(context);
        capi::pa_mainloop_free(mainloop);
        Some(sources)
    }
}

// ---- capture ---------------------------------------------------------------

/// Shared state for the threaded capture; owned by the capture thread.
struct Capture {
    mainloop: *mut capi::pa_threaded_mainloop,
    context: *mut capi::pa_context,
    stream: *mut capi::pa_stream,
}

/// Wakes a thread blocked in `pa_threaded_mainloop_wait`.
extern "C" fn notify_cb(_: *mut capi::pa_context, userdata: *mut c_void) {
    let ml = userdata as *mut capi::pa_threaded_mainloop;
    unsafe { capi::pa_threaded_mainloop_signal(ml, 0) };
}

extern "C" fn stream_notify_cb(_: *mut capi::pa_stream, userdata: *mut c_void) {
    let ml = userdata as *mut capi::pa_threaded_mainloop;
    unsafe { capi::pa_threaded_mainloop_signal(ml, 0) };
}

/// Read callback that just wakes the drain loop. Data is pulled by the capture
/// thread with `pa_stream_peek` while holding the mainloop lock, matching
/// `pa_simple`'s usage.
extern "C" fn read_cb(_: *mut capi::pa_stream, _nbytes: usize, userdata: *mut c_void) {
    let ml = userdata as *mut capi::pa_threaded_mainloop;
    unsafe { capi::pa_threaded_mainloop_signal(ml, 0) };
}

/// Creates a 16 kHz mono f32le record stream on `source` and blocks until it
/// is ready. Samples are drained by `capture_loop`; all objects are created
/// and used on the capture thread.
fn setup_capture(
    source: &str,
    _stop_signal: &Arc<AtomicBool>,
    buffer: &Arc<Mutex<SharedBuffer>>,
    started_at: &Arc<Mutex<Option<Instant>>>,
) -> std::result::Result<Capture, String> {
    unsafe {
        let mainloop = capi::pa_threaded_mainloop_new();
        if mainloop.is_null() {
            return Err("failed to create pulse mainloop".to_string());
        }
        let api = capi::pa_threaded_mainloop_get_api(mainloop);
        let ctx_name = CString::new("opendictate-capture").map_err(|_| {
            capi::pa_threaded_mainloop_free(mainloop);
            "invalid pulse context name".to_string()
        })?;
        let context = capi::pa_context_new(api, ctx_name.as_ptr());
        if context.is_null() {
            capi::pa_threaded_mainloop_free(mainloop);
            return Err("failed to create pulse context".to_string());
        }
        capi::pa_context_set_state_callback(context, Some(notify_cb), mainloop as *mut c_void);
        // Hold the lock across connect/start so the internal thread cannot
        // dispatch any callback until we first enter `wait`: dispatch always
        // runs with the mutex held, and `signal` is a bare broadcast with no
        // memory, so a callback fired outside our wait loops would be lost.
        capi::pa_threaded_mainloop_lock(mainloop);
        if capi::pa_context_connect(context, std::ptr::null(), 0, std::ptr::null()) < 0 {
            capi::pa_threaded_mainloop_unlock(mainloop);
            capi::pa_context_unref(context);
            capi::pa_threaded_mainloop_free(mainloop);
            return Err("failed to connect to pulse server".to_string());
        }
        if capi::pa_threaded_mainloop_start(mainloop) < 0 {
            capi::pa_threaded_mainloop_unlock(mainloop);
            capi::pa_context_unref(context);
            capi::pa_threaded_mainloop_free(mainloop);
            return Err("failed to start pulse mainloop".to_string());
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        while capi::pa_context_get_state(context) != capi::pa_context_state_t::Ready {
            if matches!(
                capi::pa_context_get_state(context),
                capi::pa_context_state_t::Failed | capi::pa_context_state_t::Terminated
            ) {
                capi::pa_threaded_mainloop_unlock(mainloop);
                capi::pa_threaded_mainloop_stop(mainloop);
                capi::pa_context_disconnect(context);
                capi::pa_context_unref(context);
                capi::pa_threaded_mainloop_free(mainloop);
                return Err("pulse server did not become ready".to_string());
            }
            if Instant::now() > deadline {
                capi::pa_threaded_mainloop_unlock(mainloop);
                capi::pa_threaded_mainloop_stop(mainloop);
                capi::pa_context_disconnect(context);
                capi::pa_context_unref(context);
                capi::pa_threaded_mainloop_free(mainloop);
                return Err("pulse server did not become ready (timeout)".to_string());
            }
            capi::pa_threaded_mainloop_wait(mainloop);
        }

        let spec = capi::pa_sample_spec {
            format: capi::pa_sample_format_t::F32le,
            rate: SAMPLE_RATE,
            channels: 1,
        };
        let stream_name = CString::new("opendictate-capture").map_err(|_| {
            capi::pa_threaded_mainloop_unlock(mainloop);
            capi::pa_threaded_mainloop_stop(mainloop);
            capi::pa_context_disconnect(context);
            capi::pa_context_unref(context);
            capi::pa_threaded_mainloop_free(mainloop);
            "invalid pulse stream name".to_string()
        })?;
        let stream = capi::pa_stream_new(context, stream_name.as_ptr(), &spec, std::ptr::null());
        if stream.is_null() {
            capi::pa_threaded_mainloop_unlock(mainloop);
            capi::pa_threaded_mainloop_stop(mainloop);
            capi::pa_context_disconnect(context);
            capi::pa_context_unref(context);
            capi::pa_threaded_mainloop_free(mainloop);
            return Err("failed to create pulse record stream".to_string());
        }
        capi::pa_stream_set_state_callback(stream, Some(stream_notify_cb), mainloop as *mut c_void);
        capi::pa_stream_set_read_callback(stream, Some(read_cb), mainloop as *mut c_void);

        let c_source = CString::new(source).map_err(|_| {
            capi::pa_threaded_mainloop_unlock(mainloop);
            capi::pa_threaded_mainloop_stop(mainloop);
            capi::pa_stream_unref(stream);
            capi::pa_context_disconnect(context);
            capi::pa_context_unref(context);
            capi::pa_threaded_mainloop_free(mainloop);
            "invalid pulse source name".to_string()
        })?;
        // Small fragsize so chunks arrive every few milliseconds; that keeps
        // the read callback (and thus our wakeup) flowing right up to the
        // moment an external stop is requested.
        let attr = capi::pa_buffer_attr {
            maxlength: u32::MAX,
            tlength: u32::MAX,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: 1024,
        };
        if capi::pa_stream_connect_record(stream, c_source.as_ptr(), &attr, 0) < 0 {
            capi::pa_threaded_mainloop_unlock(mainloop);
            capi::pa_threaded_mainloop_stop(mainloop);
            capi::pa_stream_unref(stream);
            capi::pa_context_disconnect(context);
            capi::pa_context_unref(context);
            capi::pa_threaded_mainloop_free(mainloop);
            return Err("failed to connect record stream".to_string());
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = capi::pa_stream_get_state(stream);
            if state == capi::pa_stream_state_t::Ready {
                break;
            }
            if matches!(state, capi::pa_stream_state_t::Failed | capi::pa_stream_state_t::Terminated)
            {
                capi::pa_threaded_mainloop_unlock(mainloop);
                capi::pa_threaded_mainloop_stop(mainloop);
                let _ = capi::pa_stream_disconnect(stream);
                capi::pa_stream_unref(stream);
                capi::pa_context_disconnect(context);
                capi::pa_context_unref(context);
                capi::pa_threaded_mainloop_free(mainloop);
                return Err("pulse record stream failed to start".to_string());
            }
            if Instant::now() > deadline {
                capi::pa_threaded_mainloop_unlock(mainloop);
                capi::pa_threaded_mainloop_stop(mainloop);
                let _ = capi::pa_stream_disconnect(stream);
                capi::pa_stream_unref(stream);
                capi::pa_context_disconnect(context);
                capi::pa_context_unref(context);
                capi::pa_threaded_mainloop_free(mainloop);
                return Err("pulse record stream timed out while starting".to_string());
            }
            capi::pa_threaded_mainloop_wait(mainloop);
        }
        capi::pa_threaded_mainloop_unlock(mainloop);

        // NOTE: `stop_signal` is deliberately NOT reset to `false` here. The
        // caller (`AudioRecorder::start_pulse`) already cleared it before
        // spawning this thread; resetting it here would re-open the race where
        // a timed-out `spawn_capture` sets stop=true to abort, and this thread
        // immediately un-sets it, wedging `join()` forever.
        *started_at.lock().map_err(|e| e.to_string())? = Some(Instant::now());
        buffer.lock().map_err(|e| e.to_string())?.clear();

        Ok(Capture {
            mainloop,
            context,
            stream,
        })
    }
}

fn append_samples(
    bytes: &[u8],
    buffer: &Arc<Mutex<SharedBuffer>>,
    _stop_signal: &Arc<AtomicBool>,
    _started_at: &Arc<Mutex<Option<Instant>>>,
) {
    let mut guard = match buffer.try_lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let new_samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    // `SharedBuffer::push` drains from the front when the recording cap is
    // hit, advancing `base` so consumer watermarks stay valid.
    guard.push(&new_samples);
}

/// Drains available samples into the shared buffer until `stop_signal` is set
/// or the stream dies. Polls with a short sleep rather than blocking in
/// `pa_threaded_mainloop_wait`, so an external stop always takes effect.
fn capture_loop(
    capture: &mut Capture,
    stop_signal: &Arc<AtomicBool>,
    buffer: &Arc<Mutex<SharedBuffer>>,
    started_at: &Arc<Mutex<Option<Instant>>>,
) {
    unsafe {
        // Wakes `pa_threaded_mainloop_wait` on an external stop even when the
        // server delivers no audio (e.g. the source was removed), so the join
        // in `Recorder::stop` always returns promptly.
        while !stop_signal.load(Ordering::Relaxed) {
            capi::pa_threaded_mainloop_lock(capture.mainloop);
            let mut sz = capi::pa_stream_readable_size(capture.stream);
            while sz == 0 && !stop_signal.load(Ordering::Relaxed) {
                capi::pa_threaded_mainloop_wait(capture.mainloop);
                sz = capi::pa_stream_readable_size(capture.stream);
            }
            loop {
                let size = capi::pa_stream_readable_size(capture.stream);
                if size == 0 || size == usize::MAX {
                    break;
                }
                let mut data: *const c_void = std::ptr::null();
                let mut nbytes: usize = 0;
                if capi::pa_stream_peek(capture.stream, &mut data, &mut nbytes) < 0 {
                    break;
                }
                if nbytes == 0 {
                    break;
                }
                if data.is_null() {
                    // A "hole" (dropped samples): discard it to advance the read index.
                    let _ = capi::pa_stream_drop(capture.stream);
                    continue;
                }
                let bytes = std::slice::from_raw_parts(data as *const u8, nbytes);
                append_samples(bytes, buffer, stop_signal, started_at);
                let _ = capi::pa_stream_drop(capture.stream);
            }
            let state = capi::pa_stream_get_state(capture.stream);
            capi::pa_threaded_mainloop_unlock(capture.mainloop);
            if matches!(state, capi::pa_stream_state_t::Failed | capi::pa_stream_state_t::Terminated)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        capi::pa_threaded_mainloop_stop(capture.mainloop);
        let _ = capi::pa_stream_disconnect(capture.stream);
        capi::pa_stream_unref(capture.stream);
        capi::pa_context_disconnect(capture.context);
        capi::pa_context_unref(capture.context);
        capi::pa_threaded_mainloop_free(capture.mainloop);
    }
}

/// Spawns a capture thread recording from `source`. Returns once the stream is
/// ready and buffering. The thread exits when `stop_signal` is set.
pub(crate) fn spawn_capture(
    source: &str,
    stop_signal: Arc<AtomicBool>,
    buffer: Arc<Mutex<SharedBuffer>>,
    started_at: Arc<Mutex<Option<Instant>>>,
) -> Result<std::thread::JoinHandle<()>> {
    let source = source.to_string();
    let (tx, rx) = mpsc::sync_channel(1);
    // Keep a handle to signal the thread on the timeout path below (the
    // original Arc is moved into the thread closure).
    let timeout_stop = Arc::clone(&stop_signal);
    let handle = std::thread::Builder::new()
        .name("opendictate-pulse".to_string())
        .spawn(move || {
            let setup = setup_capture(&source, &stop_signal, &buffer, &started_at);
            let _ = tx.send(setup.is_ok());
            if let Ok(mut capture) = setup {
                capture_loop(&mut capture, &stop_signal, &buffer, &started_at);
            }
        })
        .map_err(|e| CoreError::Audio(format!("failed to spawn pulse capture thread: {e}")))?;

    match rx.recv_timeout(Duration::from_secs(8)) {
        Ok(true) => Ok(handle),
        Ok(false) => {
            let _ = handle.join();
            Err(CoreError::Audio(
                "failed to start pulse capture (setup failed)".to_string(),
            ))
        }
        Err(_) => {
            // The thread may have completed setup just after the timeout and
            // be entering `capture_loop`. Signal it to stop before joining,
            // otherwise `join()` blocks indefinitely (`capture_loop` only
            // exits when `stop_signal` is set).
            timeout_stop.store(true, Ordering::SeqCst);
            let _ = handle.join();
            Err(CoreError::Audio(
                "pulse capture setup timed out".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pulse_ids() {
        assert_eq!(pulse_source_name("pulse:alsa_input.usb-hd.webcam.analog-stereo"),
            Some("alsa_input.usb-hd.webcam.analog-stereo"));
        assert_eq!(pulse_source_name("pulse:"), Some(""));
        assert_eq!(pulse_source_name("default"), None);
        assert_eq!(pulse_source_name("HDA Intel PCH"), None);
    }

    #[test]
    fn rejects_non_monitor_rule_is_filtered_in_callback() {
        let s = PulseSource {
            name: "alsa_input.pci-0000_00_1f.3.analog-stereo".to_string(),
            description: "Built-in Audio Analog Stereo".to_string(),
        };
        assert_eq!(pulse_source_name(&format!("{}{}", PULSE_PREFIX, s.name)), Some(s.name.as_str()));
        assert_eq!(PULSE_PREFIX, "pulse:");
    }

    #[test]
    #[ignore]
    fn lists_sources_against_running_server() {
        let sources = list_sources();
        assert!(sources.is_some(), "no PulseAudio server reachable");
        let sources = sources.unwrap();
        assert!(!sources.is_empty(), "expected at least one input source");
        for s in &sources {
            assert!(!s.name.is_empty());
            assert!(!s.description.is_empty());
        }
    }

    #[test]
    #[ignore]
    fn captures_from_running_server() {
        let Some(sources) = list_sources() else {
            panic!("no PulseAudio server reachable");
        };
        let source = sources.first().unwrap().name.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let buffer = Arc::new(Mutex::new(SharedBuffer::default()));
        let started_at = Arc::new(Mutex::new(None));
        let handle = spawn_capture(
            &source,
            Arc::clone(&stop),
            Arc::clone(&buffer),
            Arc::clone(&started_at),
        )
        .expect("capture should start");
        std::thread::sleep(Duration::from_millis(800));
        stop.store(true, Ordering::SeqCst);
        let _ = handle.join();
        let samples = buffer.lock().unwrap().samples.clone();
        assert!(!samples.is_empty(), "expected captured samples from {source}");
        let rms: f32 =
            (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        assert!(rms > 1e-5, "expected non-silent audio, rms={rms}");
    }
}
