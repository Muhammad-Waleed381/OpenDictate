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

## Building a GPU installer

Standard release installers use static CPU libraries. The dispatch-only
`.github/workflows/gpu-build.yml` builds an experimental Linux `.deb` from a
CUDA-enabled **shared** sherpa-onnx archive matching the pinned crate version.
Provide its URL as the workflow's `libs_url` input. The workflow accepts zip,
tar.gz and tar.bz2 content, packages runtime `.so` libraries under
`/usr/lib/opendictate`, and builds with the matching runtime search path.

Locally, from the repository root:

```bash
export SHERPA_ONNX_LIB_DIR=/path/to/extracted/shared-cuda-libs
export OPENDICTATE_LINUX_RPATH=/usr/lib/opendictate
python3 scripts/inject-linux-libs.py
npm run tauri -- build --bundles deb --features gpu-shared -- --no-default-features
```

`inject-linux-libs.py` updates `src-tauri/tauri.conf.json` for that local package;
review that generated mapping before committing configuration changes. Keep
CPU and shared features mutually exclusive. The explicit `--no-default-features`
argument is passed to Cargo, disabling the app's default `cpu-static` feature.

GPU archives can be large and must include the dependent runtime libraries.
Actual acceleration and installer startup still require validation on matching
GPU hardware; a successful package build does not establish either result.

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
