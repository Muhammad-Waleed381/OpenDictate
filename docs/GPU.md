# GPU acceleration (experimental)

OpenDictate runs speech models on CPU by default. There is early,
opt-in support for GPU execution providers via ONNX Runtime.

## How it works

- **Settings → GPU acceleration** (`settings.gpu`):
  - `off` *(default)* — CPU only, exactly as always
  - `auto` — attempts CUDA on Linux/Windows; macOS stays CPU until CoreML
    is validated
  - `cuda` / `coreml` — explicit requests
- Engine construction **always falls back to CPU** when a GPU provider
  cannot be created (no drivers, missing libraries, unsupported model).
  A GPU build on a machine without an NVIDIA GPU behaves identically to
  the standard build.
- The Models page shows a `GPU ✓ <provider>` chip whenever an engine is
  actually running off-CPU.
- Changing the setting drops cached engines, so it applies from the next
  dictation — no restart needed. The tiny live-caption zipformer stays on
  CPU deliberately.

## What a GPU build requires

The default build statically links a CPU-only sherpa-onnx. To get CUDA:

1. Obtain a sherpa-onnx shared-library archive built with
   `-DSHERPA_ONNX_ENABLE_CUDA=ON`, matching the version in
   `crates/opendictate-core/Cargo.toml`. k2-fsa publishes these on their
   GitHub releases (names like
   `sherpa-onnx-v<ver>-linux-x64-shared-cuda-lib.tar.bz2`).
2. Build normally with the libs on disk — the sys crate picks them up via
   an env override (works with the default static link mode):

   ```bash
   export SHERPA_ONNX_LIB_DIR=/path/to/extracted/libs
   npm run tauri build
   ```

3. Target machines need NVIDIA drivers; the CUDA provider libraries are
   loaded at runtime by ONNX Runtime.

`.github/workflows/gpu-build.yml` automates step 2–3 for Linux as a
dispatch-only job: supply `libs_url` and it uploads a `-cuda` .deb
artifact. It intentionally never runs for tagged releases.

## Status / verification matrix

| Path | Code | Fallback tested | Acceleration measured |
|---|---|---|---|
| CPU | ✅ | — | ✅ (RTF benchmark) |
| CUDA | ✅ | ✅ (fails → CPU on non-NVIDIA machines) | ⏳ pending hardware run |
| CoreML | ✅ plumbing only | ✅ same mechanism | ⏳ opt-in, untested |

When acceleration gets its first real measurement, record the RTF here.
