# Contributing to OpenDictate

Thank you for your interest in contributing to OpenDictate! 🎙️

OpenDictate is a free, open-source, local-first voice dictation application for desktop operating systems. Our goal is to provide instantaneous, private, and fully customizable speech-to-text with zero cloud dependency and zero telemetry.

We welcome contributions of all kinds: bug fixes, performance improvements, new speech models, voice actions, UI/UX polish, platform compatibility enhancements, and documentation updates.

---

## Table of Contents

- [Code of Conduct & Philosophy](#code-of-conduct--philosophy)
- [Architecture Overview](#architecture-overview)
- [Prerequisites & System Setup](#prerequisites--system-setup)
  - [Linux (Ubuntu / Debian / Fedora / Arch)](#linux-ubuntu--debian--fedora--arch)
  - [macOS](#macos)
  - [Windows](#windows)
- [Development Workflow](#development-workflow)
  - [Clone & Setup](#clone--setup)
  - [Running in Development Mode](#running-in-development-mode)
  - [Frontend-Only Development](#frontend-only-development)
  - [Building Release Binaries](#building-release-binaries)
- [Running Tests & Quality Checks](#running-tests--quality-checks)
- [Contribution Guides](#contribution-guides)
  - [1. Adding a New Speech Model](#1-adding-a-new-speech-model)
  - [2. Adding or Modifying Voice Actions](#2-adding-or-modifying-voice-actions)
  - [3. Enhancing Text Normalization & Formatting](#3-enhancing-text-normalization--formatting)
  - [4. Improving Frontend UI & Tabs](#4-improving-frontend-ui--tabs)
  - [5. Platform Input Injection & Hotkeys](#5-platform-input-injection--hotkeys)
- [Submitting a Pull Request](#submitting-a-pull-request)
- [Getting Help](#getting-help)

---

## Code of Conduct & Philosophy

OpenDictate is guided by three non-negotiable principles:

1. **100% Local-First & Private**: Raw microphone audio and user text must never be sent to external cloud servers. All inference, dictionary storage, and transcription history stay on the user's machine.
2. **Zero Telemetry**: No remote analytics, tracking pixels, or diagnostic phone-homes.
3. **Inclusive & Respectful Community**: We welcome contributors of all experience levels and backgrounds. Be considerate, constructive, and kind in code reviews and issue discussions.

---

## Architecture Overview

The repository is structured as a Cargo workspace with a Tauri 2 desktop shell and a React 19 frontend:

```text
OpenDictate/
├── Cargo.toml                    # Root workspace manifest
├── package.json                  # Node.js dependencies and scripts
│
├── crates/
│   └── opendictate-core/         # Pure Rust core speech & text library
│       └── src/
│           ├── audio/            # Audio capture (cpal, pulseaudio) & VAD (Silero)
│           ├── stt/              # Sherpa-ONNX engines, model catalog, downloaders
│           └── text/             # Text normalization, actions, casing, templates
│
├── src-tauri/                    # Tauri 2 native desktop backend
│   └── src/
│       ├── dictation.rs          # Dictation session state machine & audio pipeline
│       ├── inject.rs             # Platform synthetic keystroke injection
│       ├── hotkey.rs             # Global shortcut registration & double-tap
│       ├── dock.rs               # Floating dock positioning & window control
│       ├── db.rs                 # SQLite persistence (history, dictionary, snippets)
│       └── commands.rs           # Tauri IPC commands exposed to frontend
│
└── src/                          # React 19 + TypeScript + Tailwind CSS UI
    ├── components/
    │   ├── tabs/                 # Home, History, Snippets, Models, Settings, etc.
    │   └── ui/                   # Reusable UI primitives (shadcn / base-ui)
    ├── lib/
    │   ├── api.ts                # Strongly-typed wrappers for Tauri IPC commands
    │   └── store.ts              # Zustand store for app state & real-time events
    └── main.tsx                  # Frontend entry point
```

### The Dictation Pipeline

```text
[ Microphone Audio ] 
        │
        ▼ (cpal / PulseAudio ring buffer)
[ Voice Activity Detection (Silero VAD / Energy) ]
        │
        ▼ (Speech detected)
[ STT Engine (sherpa-onnx: FastConformer CTC / Parakeet TDT / Whisper) ]
        │
        ▼ (Raw text tokens)
[ Text Post-Processing & Normalization ]
  ├─ Dictionary / Hotword replacement
  ├─ Smart punctuation & formatting
  ├─ Voice action commands (e.g., "new line", "select all", "camel case")
  └─ Dynamic snippet expansion ({date}, {time}, {clipboard})
        │
        ▼ (Processed text or action keycode)
[ Synthetic Keyboard Injection ]
  ├─ Linux: /dev/uinput or enigo
  ├─ macOS: CGEvent
  └─ Windows: SendInput
```

---

## Prerequisites & System Setup

Before building OpenDictate, ensure you have the following installed:
- **Rust (1.80+)**: Install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Node.js (v20+) & npm**: Install via [Node.js](https://nodejs.org/) or `nvm`.
- **CMake (3.20+)** and a C/C++ compiler: Required for compiling native `sherpa-onnx` and ONNX Runtime bindings.

### Linux (Ubuntu / Debian / Fedora / Arch)

1. **System Dependencies (Ubuntu / Debian)**:
   ```bash
   sudo apt update
   sudo apt install -y \
     libwebkit2gtk-4.1-dev \
     libappindicator3-dev \
     librsvg2-dev \
     patchelf \
     libasound2-dev \
     libpulse-dev \
     libudev-dev \
     libgtk-3-dev \
     cmake \
     build-essential
   ```

2. **System Dependencies (Fedora)**:
   ```bash
   sudo dnf install -y \
     webkit2gtk4.1-devel \
     libappindicator-gtk3-devel \
     librsvg2-devel \
     alsa-lib-devel \
     pulseaudio-libs-devel \
     systemd-devel \
     gtk3-devel \
     cmake \
     gcc-c++
   ```

3. **System Dependencies (Arch Linux)**:
   ```bash
   sudo pacman -S --needed \
     webkit2gtk-4.1 \
     libappindicator-gtk3 \
     librsvg \
     alsa-lib \
     libpulse \
     systemd-libs \
     gtk3 \
     cmake \
     base-devel
   ```

4. **Linux Key Injection Permissions (`/dev/uinput`)**:
   OpenDictate injects keystrokes natively via `/dev/uinput`. To allow running without root:
   ```bash
   sudo usermod -aG input $USER
   echo 'KERNEL=="uinput", MODE="0660", GROUP="input", OPTIONS+="static_node=uinput"' | sudo tee /etc/udev/rules.d/99-input.rules
   sudo udevadm control --reload-rules && sudo udevadm trigger
   ```
   *(You may need to log out and log back in for group changes to take effect).*

### macOS

1. Install Apple Command Line Tools:
   ```bash
   xcode-select --install
   ```
2. Install CMake and Node using Homebrew:
   ```bash
   brew install cmake node
   ```
3. **Permissions**: When running OpenDictate on macOS, enable **Accessibility** and **Input Monitoring** permissions under `System Settings > Privacy & Security` to allow global hotkeys and text injection.

### Windows

1. Install [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (select "Desktop development with C++" and ensure CMake is checked).
2. Install Node.js (v20+) and Rust using `rustup-init.exe` targeting `x86_64-pc-windows-msvc`.

---

## Development Workflow

### Clone & Setup

```bash
# Clone the repository
git clone https://github.com/Muhammad-Waleed381/OpenDictate.git
cd OpenDictate

# Install frontend dependencies
npm install
```

### Running in Development Mode

To launch OpenDictate with hot-reloading for both the React frontend and Rust backend:

```bash
npm run tauri dev
```

### Frontend-Only Development

If you are styling or making quick visual changes to React components without needing active STT or native keyboard injection, you can run Vite directly in the browser:

```bash
npm run dev
```

### Building Release Binaries

To produce production-ready platform bundles (`.deb`, `.AppImage`, `.dmg`, or `.exe`):

```bash
npm run tauri build
```

The resulting packages will be located in `target/release/bundle/`.

---

## Running Tests & Quality Checks

Always ensure that tests pass and formatting is clean before submitting a pull request.

### 1. Rust Unit & Integration Tests

```bash
# Test opendictate-core
cargo test -p opendictate-core

# Test src-tauri backend
cargo test --manifest-path src-tauri/Cargo.toml
```

### 2. Rust Code Formatting & Linting

```bash
# Check formatting
cargo fmt --all --check

# Format code
cargo fmt --all

# Run Clippy checks
cargo clippy --workspace --all-targets -- -D warnings
```

### 3. Frontend Type Checking & Build Verification

```bash
# Verify TypeScript compile & Vite production build
npm run build
```

---

## Contribution Guides

### 1. Adding a New Speech Model

Models supported by OpenDictate are defined in [`crates/opendictate-core/src/stt/models.rs`](crates/opendictate-core/src/stt/models.rs).

To introduce a new model (e.g. from Hugging Face / sherpa-onnx):
1. Add an entry to the `ModelCatalog::built_in()` list in `crates/opendictate-core/src/stt/models.rs`.
2. Specify:
   - `id`: Unique identifier (e.g., `whisper-base-en`).
   - `name`: Human-readable label.
   - `engine`: Engine family (`StreamingCtc`, `StreamingTdt`, `OfflineWhisper`, etc.).
   - `download_urls`: Array of mirror URLs (usually GitHub releases or Hugging Face).
   - Expected unpacked file structure and checksum/size.
3. Configure the engine initialization in [`crates/opendictate-core/src/stt/engine.rs`](crates/opendictate-core/src/stt/engine.rs) to map model paths to the corresponding `sherpa-onnx` configuration.
4. Run `cargo test -p opendictate-core` to verify catalog consistency.

### 2. Adding or Modifying Voice Actions

Voice actions allow users to format text or control the OS by voice (e.g., saying *"capitalize that"*, *"new paragraph"*, *"press enter"*).

1. Voice action parsing lives in [`crates/opendictate-core/src/text/actions.rs`](crates/opendictate-core/src/text/actions.rs).
2. Add or update regex / token patterns in the `ActionParser`.
3. If the action produces text casing or punctuation, update `crates/opendictate-core/src/text.rs`.
4. If the action sends special key combinations, handle the action keycode in `src-tauri/src/inject.rs`.
5. Add unit tests in `crates/opendictate-core/src/text/actions.rs` validating spoken variations.

### 3. Enhancing Text Normalization & Formatting

Text cleanup and dynamic snippet expansions are located in `crates/opendictate-core/src/text.rs`:
- Smart punctuation and number formatting.
- Dictionary hotword replacements (boosted terms).
- Snippet variable expansion (`{date}`, `{time}`, `{datetime}`, `{clipboard}`).
- Keep all post-processing functions pure and deterministic, backed by comprehensive unit tests.

### 4. Improving Frontend UI & Tabs

- UI tabs are located in `src/components/tabs/`.
- UI primitives are based on Tailwind CSS and located in `src/components/ui/`.
- Frontend state is managed with [Zustand](https://github.com/pmndrs/zustand) in `src/lib/store.ts`.
- All backend communication occurs via typed wrappers in `src/lib/api.ts` which invoke Tauri IPC commands.

### 5. Platform Input Injection & Hotkeys

- Keystroke injection is handled in `src-tauri/src/inject.rs`:
  - Linux: uses `/dev/uinput` device simulation with fallback.
  - macOS: uses `core-graphics` (`CGEventCreateKeyboardEvent`).
  - Windows: uses Win32 `SendInput`.
- Hotkey handling is located in `src-tauri/src/hotkey.rs` and `src-tauri/src/doubletap.rs`.
- When modifying platform-specific code, use `#[cfg(target_os = "...")]` attributes and ensure conditional compilation succeeds across targets.

---

## Submitting a Pull Request

1. **Fork & Branch**:
   - Fork the repository on GitHub.
   - Create a feature or bugfix branch off `main`:
     ```bash
     git checkout -b feat/my-awesome-feature
     # or
     git checkout -b fix/issue-description
     ```

2. **Commit Messages**:
   Follow [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat: add whisper-tiny-multilingual model`
   - `fix: resolve uinput keycode mapping on wayland`
   - `docs: update linux setup instructions`
   - `refactor: clean up dictation state transition`

3. **Check Your Work**:
   - [ ] `cargo test --workspace` passes without errors.
   - [ ] `cargo fmt --all --check` reports no issues.
   - [ ] `npm run build` succeeds without TypeScript or Vite errors.
   - [ ] No unapproved dependencies or external network calls added.

4. **Open a PR**:
   - Push your branch to your fork and submit a PR to `main`.
   - Provide a clear description of the problem solved, what changes were made, and testing steps.

---

## Getting Help

- 💬 Have questions or feature proposals? Open a discussion or issue on [GitHub Issues](https://github.com/Muhammad-Waleed381/OpenDictate/issues).
- 🐛 Found a bug? Please include your OS version, hardware specs, model in use, and relevant terminal logs.

Thank you for helping make local, private voice dictation accessible to everyone! ❤️
