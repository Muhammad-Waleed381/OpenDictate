# Dock Record Widget — Design

> **Status:** Approved 2026-08-16

## Goal

Make OpenDictate a true background utility: closing the window keeps it running; a small persistent record button (waveform icon) sits docked at the top-right edge of the screen; clicking it — or pressing the global hotkey — starts/stops dictation from any app, and the STT result is auto-inserted into whatever app has focus.

## What already exists (no work)

- Close-to-tray: `on_window_event` prevents close + hides the main window (src-tauri/src/lib.rs:77-84).
- Tray menu: Open / Start-Stop Dictation / Quit (src-tauri/src/tray.rs).
- Global hotkey toggle (ctrl+k) that works regardless of focused app.
- VAD → STT → auto-insert pipeline (`dictation.rs` → `inject.rs` via wtype/xdotool).

## Design

### Architecture

The transient `overlay` window becomes a **persistent `dock` window** (top-right corner, **30×30 round dot**, always-on-top, skip-taskbar, **non-focusable** — `focusable: false` in the window config). It never hides. It is the record button. (Revision per user feedback: originally 140×40 pill; user wanted "round, very very small".)

- Rust: `overlay.rs` → `dock.rs`. `set_state` keeps emitting the `overlay-state` event (drives dock visuals) but no longer show/hides the window. `init()` positions the window at the top-right of the primary monitor at startup.
- Frontend: `OverlayPill.tsx` → `DockButton.tsx`. Rendered by `OverlayApp` (same window routing, `?window=dock`). Click = toggle record (same backend commands as the hotkey). Visuals driven by `overlay-state` + `audio-level` events:
  - *idle* — static waveform glyph in a white dot, subtle
  - *listening* — black dot, waveform bars animated live from real mic RMS; this state also marks the button as "recording" (works for both hotkey- and click-initiated recording, fixing the old store.recording desync for hotkey starts)
  - *transcribing* — black dot, three small bouncing dots
  - *inserted* — black dot with green ✓ flash (~1.4s, backend already emits "hidden" after 1200ms → dock returns to idle)
  - *error* — black dot with red ✕ flash (~2.4s), then idle
- Dead code removed: `show_overlay` command (never called from frontend), old `show/hide/center_top` in overlay.rs.

### Focus safety

The dock window is non-focusable (`focusable: false` in tauri.conf.json, already present on the overlay window) so clicking it never steals keyboard focus from the user's app; the auto-insert paste therefore lands in the app the user was using. Pointer clicks still reach the window (Wayland delivers pointer events to the surface under the cursor independently of keyboard focus).

### Data flow

Click / hotkey → `start_recording` / `stop_recording` (dictation.rs) → overlay-state events (listening/transcribing/inserted/error/hidden) + audio-level events (33ms RMS) → dock visuals. Stop → VAD → STT → inject into focused window → history row.

### Edge cases

- MicTest (onboarding) uses the same `audio-level` events and `start_recording("test")` — unaffected; dock visuals will mirror test-mode states (acceptable).
- `stop_recording` errors (e.g. "no recording in progress") → dock shows error flash; the `result?.text` guard prevents null crashes (hardening already applied to App.tsx, mirrored in dock click handler).
- Multi-monitor: position on the primary monitor; repositioned at startup only (kept simple per YAGNI).

## Removed

- `OverlayPill.tsx` (transient top-center pill; "dock only" decision)
- overlay window show/hide machinery in Rust
- `show_overlay` command + its frontend references (none)

## Files

- Modify: `src-tauri/tauri.conf.json` (overlay → dock window config)
- Rename/rewrite: `src-tauri/src/overlay.rs` → `src-tauri/src/dock.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/dictation.rs`, `src-tauri/src/commands.rs`
- Rewrite: `src/components/OverlayPill.tsx` → `src/components/DockButton.tsx`
- Modify: `src/App.tsx` (OverlayApp renders DockButton), `src/main.tsx` (window routing)
- Stub: `/tmp/opencode/stub-invoke.js` (unused outside tests)
