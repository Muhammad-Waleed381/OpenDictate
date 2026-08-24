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

## Standard installers: CPU and GPU in one binary

Release builds can link against a **shared** sherpa-onnx/onnxruntime that
includes GPU execution providers. The resulting installer then supports
both worlds from one download:

- Machine has an NVIDIA GPU + drivers → `gpu = auto/cuda` engages CUDA.
- Anything else → provider creation fails and engines fall back to CPU,
  exactly like the classic build.

Set the repository **variable** `CUDA_LIBS_URL` to a k2-fsa shared-lib
archive built with `-DSHERPA_ONNX_ENABLE_CUDA=ON` (matching the
sherpa-onnx version in `crates/opendictate-core/Cargo.toml`, e.g. names
like `sherpa-onnx-v<ver>-linux-x64-shared-cuda-lib.tar.bz2`) and the next
tagged Linux/Windows release picks it up automatically.

Size note: those archives bundle CUDA runtime libraries, so GPU-enabled
installers grow substantially (~hundreds of MB). Leave the variable unset
for lean CPU-only releases.

### Locally

```bash
export SHERPA_ONNX_LIB_DIR=/path/to/extracted/shared-cuda-libs
npm run tauri build -- --features gpu-shared
```

`.github/workflows/gpu-build.yml` remains as a dispatch-only sandbox for
testing new lib archives without cutting a release.

## Status / verification matrix

| Path | Code | Fallback tested | Acceleration measured |
|---|---|---|---|
| CPU | ✅ | — | ✅ (RTF benchmark) |
| CUDA | ✅ | ✅ **live** — gpu-linked build on a GPU-less machine resolves to CPU and loads the real model (`cuda_request_falls_back_to_cpu_without_gpu`) | ⏳ pending hardware run |
| CoreML | ✅ plumbing only | ✅ hardware gate forces CPU off-macOS, and the link-mode gate forces CPU on `cpu-static` builds | ❌ **measured: no acceleration** (see below) |

### CoreML measurement, Apple M4 (10-core), macOS 26.5, `cpu-static` build

Parakeet TDT 110M int8, identical 30s of speech, best of 8 runs:

| Requested | Reported | Best | RTF | vs realtime |
|---|---|---|---|---|
| `cpu` | cpu | 0.527s | 0.0176 | 56.9x |
| `coreml` | coreml | 0.500s | 0.0167 | 60.0x |

The 5% gap is run-to-run noise, not acceleration — both ran on CPU.
sherpa-onnx says so itself on the CoreML attempt:

```
session.cc:GetSessionOptionsImpl:354
CoreML is for Apple only since onnxruntime>=1.15. Fallback to cpu!
```

The default `cpu-static` link has no execution providers compiled in, so the
CoreML request is discarded internally. Note the engine still **reported**
`coreml` before the link-mode gate was added — that is the false-positive
`GPU ✓` chip this gate exists to prevent.

Hardware gating: providers are only *requested* when the machine plausibly
has them (NVIDIA driver probe for CUDA), so reported providers are truthful.
sherpa-onnx additionally degrades gracefully internally as a second net.

When acceleration gets its first real measurement, record the RTF here.
