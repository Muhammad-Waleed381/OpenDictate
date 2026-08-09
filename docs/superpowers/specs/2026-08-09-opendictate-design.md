# OpenDictate — Design Spec

Date: 2026-08-09
Status: Approved
License: MIT

## 1. Product

OpenDictate is a local-first voice dictation app for Linux: press a hotkey, speak, and the transcript is pasted at your cursor in any application. Free, open source (MIT), zero telemetry, no account.

Tagline: "Speak. Don't type. Your words stay on your machine."

### 1.1 Positioning

- Wedge 1 — Linux quality. Every serious OSS dictation app is macOS-first or treats Linux as an afterthought. Murmur is the only Linux-native competitor, and its README lists voice commands, custom dictionaries, and history as roadmap items — exactly what OpenDictate ships.
- Wedge 2 — Command Mode, local (v0.2). Wispr Flow paywalls voice text-commands at $144/yr; no OSS dictation app ships them. Local-first (or BYOK) command mode is the differentiating feature.
- Privacy with receipts. The May 2026 Wispr Flow screenshot scandal (Context Awareness capturing active windows without disclosure, banning the user who reported it) is fresh in users' minds. OpenDictate ships with a documented `PRIVACY.md` listing every network endpoint (zero outbound calls by default).

### 1.2 Target users

Developers, writers, students, professionals — anyone typing on Linux who wants hands-free text input. Accessibility users are a first-class audience.

### 1.3 Success metrics

- v0.1: daily dogfooding on the author's machine, >100 GitHub stars, ≥5 outside users, first external PR.
- v0.2: >500 stars, Command Mode demo clip, GitHub Sponsors live.

## 2. Architecture

```
React 19 + TypeScript + Tailwind v4 + shadcn/ui   (renderer, unchanged from original concept)
        | Tauri 2 IPC (commands + events)
Rust core (tauri crate):
├── cpal                    audio capture (16 kHz mono, RAM only, no disk writes)
├── silero-vad              voice activity detection, silence trimming
├── sherpa-onnx + Parakeet TDT   default ASR engine — CPU ~5× realtime
│       └── whisper-rs (whisper.cpp)  optional engine (languages, GPU)
├── enigo + xdotool/wtype   text injection: clipboard save → paste → restore
├── global hotkey           X11: XGrabKey via tauri-plugin-global-shortcut
│                           Wayland: setgid input helper (Murmur's proven pattern)
├── rusqlite                history + settings (single SQLite file in app-data dir)
├── command engine (v0.2)   pluggable: deterministic rules → llama.cpp (local) / OpenAI-compatible (BYOK)
└── tray app + small overlay window (listening / transcribing / done / error states)
```

### 2.1 Latency expectations (set in stone)

- Push-to-talk stop → text in target app: ~1–2 s on CPU with Parakeet TDT. NOT streaming transcription — whisper.cpp continuous streaming is 5–7× slower than realtime on CPU (known trap, github #3567).
- Command Mode local LLM: seconds, with progress. BYOK: fast.

### 2.2 Engine selection

Default engine: sherpa-onnx + Parakeet TDT (CPU-optimized, ~5× realtime, auto language detection). Alternate: whisper-rs (99+ languages, GPU acceleration). Model licenses verified as a checklist item before bundling (Parakeet: NVIDIA community / CC-BY-4.0 depending on variant — attribution required; whisper.cpp + Whisper models + Silero VAD: MIT).

## 3. Scope

### v0.1 (weeks 1–3) — Linux, X11 first, Wayland as close second

- Push-to-talk global hotkey → cpal capture → Silero VAD trim → Parakeet TDT → paste at cursor (clipboard save/paste/restore)
- Tray app (no dock icon) + fixed-position overlay window
- Overlay states: listening (blue, waveform bars), transcribing (progress), inserted (green flash), error (red, actionable)
- Esc cancels; overlay never steals focus; auto-dismisses
- SQLite history + settings
- Custom dictionary / hotwords (the retention feature; fixes the #1 local-model complaint)
- Settings UI: hotkey, engine, model, language
- Packaging: deb + AppImage (AUR later)
- `PRIVACY.md` (every network endpoint), README, MIT LICENSE, ATTRIBUTIONS.md

### v0.2 (weeks 4–6) — Command Mode

- Deterministic commands first: filler removal, punctuation, capitalization (instant, zero LLM)
- Pluggable LLM backends: local llama.cpp (privacy) / BYOK Groq or OpenRouter (speed)
- Command grammar on selected text: "make formal", "fix grammar", "translate to Spanish", "make shorter", "summarize"
- Command overlay variant: purple accent, result preview with Apply/Discard
- Demo clip, GitHub Sponsors, launch follow-up

### Explicitly cut

- Context awareness (the Wispr-scandal feature; revisit only with explicit opt-in)
- Mobile, teams, plugins, sync
- Light mode in v0.1 (system theme if time allows)

## 4. UI/UX

Philosophy: the best dictation UI is invisible. 95% of usage happens in other apps; OpenDictate's UI is feedback, not interface.

### 4.1 Overlay (only thing visible while dictating)

Tiny always-on-top pill, focus-less, fixed position (top-center), appears on hotkey:

| State | Look | Behavior |
|---|---|---|
| Listening | Blue accent + live waveform bars (canvas RMS, ~30 fps) | Reassurance + record-level feedback |
| Transcribing | Subtle progress "Transcribing…" | Target ~1–2 s so barely visible |
| Inserted | Green flash "Inserted" | Text already pasted in target app |
| Error | Red, actionable (e.g. "Mic not found — check PipeWire") | Never a silent failure |
| Command (v0.2) | Purple accent + result preview | Apply / Discard |

No buttons needed during recording. Tray icon is the idle state.

### 4.2 Main window (tray → "Open OpenDictate")

- First-run onboarding: mic test, model download with progress, hotkey confirm
- Tabs: General (hotkey, engine, model, language) · Dictionary · History (searchable, copy, re-dictate) · Privacy (documented endpoints)
- Dark-first palette: slate `#0F172A` bg, blue `#3B82F6` listening, green `#10B981` success, purple commands, red errors, off-white `#F8FAFC` text, muted `#64748B` secondary

### 4.3 Core loop

Hotkey → speak → release → ~1–2 s → text pasted instantly. Corrections happen in the target app (no preview step in the default flow).

## 5. Dependencies

Rust: `cpal 0.15`, `sherpa-onnx` (+ Parakeet TDT models), `whisper-rs` (alt), `silero-vad` (onnx), `rusqlite`, `enigo 0.2`, `tauri-plugin-global-shortcut`, `clipboard-rs` (or enigo clipboard).
Frontend: React 19, TypeScript, Vite, Tailwind v4, shadcn/ui, lucide-react, Zustand/Jotai.
System (Linux): webkit2gtk-4.1, gtk3, libayatana-appindicator3, librsvg2, libasound2-dev, libpulse-dev, xdotool, wl-clipboard, wtype.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Wayland hotkey/injection (hardest part) | setgid input helper (Murmur's proven approach); X11 fully working first |
| Parakeet < cloud accuracy on jargon | custom dictionary in v0.1 |
| Local LLM command latency | small quantized models + streaming output + BYOK escape hatch |
| 3-week v0.1 tight | No invention: reference LocalYapper (MIT) / Dictus (MIT) / LocalVoice (MIT) for patterns; Murmur + JonaWhisper (GPL) read for approach only, never copied |

## 7. Licensing & attribution rules (standing)

- Reading any OSS code is always legal. Ideas/patterns are not copyrightable — reimplement in our own code.
- Verbatim code reuse ONLY from MIT / Apache-2.0 sources (LocalYapper, Dictus, LocalVoice, whisper.cpp, Silero VAD, sherpa-onnx), with copyright notices preserved in `ATTRIBUTIONS.md`.
- GPL-3.0 projects (Murmur, JonaWhisper) are reference-only: study the approach, write our own implementation.
- Model licenses verified at engine-selection time.
- Not legal advice; this is the standard practice for a project of this size.

## 8. Development workflow

1. Design doc (this file) approved — execution begins
2. Per-subsystem just-in-time reference fetches (raw.githubusercontent.com) before writing that subsystem
3. Scaffold Tauri 2 + React, verify `tauri dev` on the author's machine (Zorin 18.1 / Ubuntu 24.04 base, Wayland native)
4. Phase 0 (weeks 1): capture → VAD → ASR → injection → hotkey, end-to-end CLI test
5. Phase 1 (week 2): app shell — tray, overlay, IPC, waveform, SQLite
6. Phase 2 (week 3): dictionary, settings, packaging, privacy/readme/attributions, v0.1 launch
7. Phase 3 (weeks 4–6): Command Mode (deterministic rules → local/BYOK LLM), demo, sponsors
8. Verification per milestone: `cargo build` + `cargo clippy` + manual test script; never claim done without evidence
