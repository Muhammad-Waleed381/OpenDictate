# OpenDictate — Multi-Mic Support via PulseAudio

Date: 2026-08-20
Status: Approved
Scope: Linux multi-mic enumeration + capture. Windows/macOS keep the existing
cpal path (they already enumerate all devices).

## 1. Problem

The app captures audio through cpal, which on Linux reads ALSA PCM names
directly. On PipeWire-based systems, external microphones (wired USB headsets,
Bluetooth earbuds) are not ALSA PCM devices — they live in PipeWire/PulseAudio
as *sources*. Verified on this machine: cpal sees only `default` as usable.
Therefore only the built-in mic is selectable today, and external/wireless mics
are invisible to the app.

## 2. Backend: `pulse` module (core crate, `target_os = "linux"`)

New `crates/opendictate-core/src/audio/pulse.rs` using `libpulse-binding`:

- **Enumeration** — connect a PulseAudio context, `get_source_info_list()`,
  filter out monitors (`monitor_of` non-empty) and non-input sources, return
  `Vec<PulseSource { name, description }>`. Also fetch the server's default
  source name for labeling.
- **Capture** — PulseAudio *stream* API (not `pa_simple`): a `pa_stream` with a
  16 kHz mono `f32le` sample spec whose read callback (`pa_stream_peek`/`drop`)
  appends samples to the same shared `Arc<Mutex<Vec<f32>>>` buffer the cpal
  path uses. Runs on a `pa_threaded_mainloop` in a dedicated thread. Stopping
  disconnects the stream and joins the thread — safe from another thread.
- Because RMS, streaming `take_since`, VAD, and continuous mode all read the
  shared buffer, they work unchanged for PulseAudio capture.

## 3. Mic identity and resolution

- `list_mics()` returns `Vec<MicDevice>` where `MicDevice { id, label }`.
  IDs: `"default"` (system default), `"pulse:<source-name>"`, or a legacy raw
  ALSA/cpal name.
- `AudioRecorder::start_with_name(id)` dispatches:
  - `default` / empty → cpal default input.
  - `pulse:*` → PulseAudio capture from that exact source.
  - anything else → try cpal by name, else try a pulse source match, else fall
    back to the cpal default.
- No PulseAudio server reachable → enumeration falls back to cpal/ALSA devices
  (today's behavior) and capture falls back to the cpal default. Existing saved
  `"default"` settings keep working.

## 4. Frontend

- `list_mics` returns `MicDevice[]`. Settings + Home ready-strip dropdowns
  render friendly `label`s and persist `id`. "System default" stays the top
  option. `set_mic`/`get_mic` unchanged.

## 5. Testing

- Pure-function unit tests: monitor filtering, id parsing/resolution.
- One `#[ignore]` integration test that captures briefly from the running
  PulseAudio server (skipped when no server).
- Debug example printing detected sources on this machine.

## 6. Verification

- `cargo test` (core), clippy `-D warnings`, `npm run build`, release build,
  restart app.
- Manual: this machine has only the built-in mic — enumeration must show it;
  the Bluetooth/USB path requires the user to plug in a second mic.

## 7. Notes

- `libpulse`/`libpulse-simple` dev headers are present on the build machine, so
  no system package install is needed. The deb will link `libpulse0` at runtime
  (already a common base dependency).
- Dependency is Linux-gated with `cfg(target_os = "linux")` so other targets
  are unaffected.