# OpenDictate

**Local-first voice dictation for your desktop. Press a hotkey, speak, and your words are typed into whatever app has focus — with zero cloud, zero telemetry, and zero cost per word.**

<!-- TODO: demo video — drop an mp4/gif here -->

<p align="center">
  <em>Demo coming soon.</em>
</p>

[![License: MIT](https://img.shields.io/badge/License-MIT-black.svg)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/Built%20with-Tauri%202-blue)](https://tauri.app)

---

## Why

Cloud dictation is fast to build but slow to trust: your voice leaves the machine, you pay per minute, and nothing works offline. OpenDictate runs state-of-the-art speech models **entirely on your hardware** — after a one-time model download, dictation is instant, private, and free forever.

## Features

- **One hotkey, anywhere** — global shortcut starts/stops dictation; the transcript lands in whichever app has focus
- **Live captions while you speak** — a realtime zipformer engine streams partial text as you talk, independent of the model producing the final transcript
- **Multiple engines, one interface** — NVIDIA Parakeet, Whisper, and streaming models managed in-app (download, install-state, disk usage, removal)
- **Custom dictionary** — hotwords fed to the recognizer for names, jargon, and acronyms
- **Snippets** — say “insert snippet *signature*” to expand canned templates mid-dictation
- **Spoken punctuation** — say “comma”, “period”, “question mark” and get real punctuation
- **Insert modes** — auto / synthetic typing / clipboard paste
- **History & activity** — searchable local history, re-insert anything, GitHub-style yearly activity heatmap
- **Audio feedback** — synthesized start/success/error cues with volume control (no bundled assets)
- **100% offline** — no accounts, no telemetry, no network calls after model downloads

## Platform support

| Platform | Status | Notes |
|---|---|---|
| Linux | ✅ Supported | X11/Wayland (GNOME-tested); ydotool/xdotool injection; dock overlay |
| Windows | 🚧 Builds via CI | NSIS `.exe` + `.msi`; SendInput injection |
| macOS | 🚧 Builds via CI | universal `.dmg` (Apple Silicon + Intel); CGEvent injection |

## Install

Grab an installer from [Releases](https://github.com/Muhammad-Waleed381/OpenDictate/releases):

- **Linux**: `.deb` or `.AppImage`
- **Windows**: `.msi` or NSIS `.exe`
- **macOS**: universal `.dmg`

On first launch, pick a model in Settings → Models (the default Parakeet TDT 110M is ~100 MB) and press <kbd>Ctrl</kbd>+<kbd>K</kbd> to dictate.

> Models are downloaded once from sherpa-onnx/HuggingFace release mirrors and stored under your user data directory. Nothing else ever touches the network.

## Choosing a model

| Model | Size | Engine | Best for |
|---|---|---|---|
| Parakeet TDT 110M (int8) | ~104 MB | offline | Default — fast, accurate English on modest CPUs |
| Parakeet TDT 0.6B v3 | ~650 MB | offline | Highest accuracy; still single-pass at stop |
| Parakeet Unified EN 0.6B | ~650 MB | offline | Unified punctuation/casing out of the box |
| Whisper tiny/base/small/turbo/medium | varies | offline (chunked) | Multilingual + widely known behavior |
| Zipformer EN 20M | ~29 MB | internal | Live-captions engine only — auto-managed |
| Parakeet Unified EN 0.6B Streaming | ~650 MB | streaming | Realtime decoding on fast CPUs; benchmarked at startup and flagged if your CPU can't keep up |

## How it works

```
mic ──► capture (cpal / PulseAudio) ──► shared ring buffer
                                          │
              ┌───────────────────────────┼──────────────────────────┐
              ▼                           ▼                          ▼
      live captions                silence detection          audio level meter
   (zipformer streaming,        (Silero VAD / energy)            (dock UI)
    realtime, any model)               │
                                       ▼
                          accuracy engine (your selected model)
                                       │
                        transcript ──► cleanup ──► inject (typing/paste)
```

- **Captions** come from a small always-on streaming engine so they stay real-time even when the accuracy model decodes slower than you speak.
- **Final transcripts** come from the model you picked; long Whisper inputs are processed in 30-second chunks.
- Everything is wired through a small Rust core (`crates/opendictate-core`) shared by the Tauri shell.

## Building from source

Prerequisites: Node.js 20+, Rust stable, platform webview deps, and CMake + a C++ toolchain (sherpa-onnx builds native code).

```bash
npm install
npm run tauri dev     # develop
npm run tauri build   # produce installers
```

Linux system packages (Debian/Ubuntu):

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
                 patchelf libasound2-dev libpulse-dev cmake
```

Integration tests that need live services (PulseAudio, installed models) are marked `#[ignore]`:

```bash
cargo test -p opendictate-core -- --ignored
```

## Privacy

OpenDictate never sends your audio or text anywhere. The only network traffic is the one-time model download you explicitly trigger. History, dictionary, snippets, and stats live in a local SQLite database under your user data directory.

## License

[MIT](LICENSE) © Muhammad Waleed
