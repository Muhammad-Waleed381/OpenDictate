# GPU acceleration

OpenDictate ships in two builds:

- **Standard build** (every platform): speech models run on the CPU. It links
  the static CPU build of sherpa-onnx and cannot use a GPU.
- **CUDA build** (Linux `.deb`, Windows setup): links the CUDA 12 / cuDNN 9
  build of sherpa-onnx, so models can run on an NVIDIA GPU. Release assets end
  in `_amd64-cuda.deb` and `_x64-cuda-setup.exe`.

The CUDA build bundles sherpa-onnx and ONNX Runtime with its CUDA provider.
The machine still needs the NVIDIA driver, the CUDA 12 runtime and cuDNN 9
installed; without them it falls back to the CPU.

There is no macOS GPU build. CoreML measured no speed-up (see below), and
Apple Silicon CPUs are already fast with these models.

## How it works

- **Settings → GPU acceleration** (`settings.gpu`), shown in the CUDA build:
  - `auto` *(default in the CUDA build)*: CUDA when an NVIDIA GPU is found
  - `cuda`: explicit request
  - `off` *(default in the standard build)*: CPU only
- The standard build shows a link to the CUDA download instead of the setting
  (on Windows and Linux; macOS shows nothing).
- Engine construction **always falls back to CPU** when a GPU provider cannot
  be created (no drivers, missing libraries, unsupported model).
- The Models page shows a `GPU: <provider>` chip whenever an engine is actually
  running off-CPU.
- Changing the setting drops cached engines, so it applies from the next
  dictation, with no restart. The live-caption zipformer stays on CPU.
- The in-app updater keeps each build on its own kind: the CUDA build only
  installs `-cuda` assets, and the standard build never does.

## How the CUDA build is made

The Release workflow builds it on every `v*` tag, next to the standard
installers. `.github/actions/cuda-libs` downloads the sherpa-onnx CUDA archive
that matches the `sherpa-onnx-sys` version in `Cargo.lock`, points
`SHERPA_ONNX_LIB_DIR` at it, and adds the runtime libraries to the installer:

- Linux: `scripts/inject-linux-libs.py` adds the `.so` files to the `.deb`
  under `/usr/lib/opendictate`, and `src-tauri/build.rs` gives the binary a
  matching RUNPATH (from `OPENDICTATE_LINUX_RPATH`).
- Windows: `scripts/inject-windows-libs.py` adds the DLLs to
  `bundle.resources`, which installs them next to the executable.

The TensorRT provider and the C++ API libraries are left out, since the app
never loads them. To check the packaging without publishing, run the
**CUDA build (dry run)** workflow (`gpu-build.yml`) on a branch; it attaches
both installers to the run.

Locally on Linux, with the CUDA archive extracted:

```bash
export SHERPA_ONNX_LIB_DIR=/path/to/extracted/lib
export OPENDICTATE_LINUX_RPATH=/usr/lib/opendictate
python3 scripts/inject-linux-libs.py
npm run tauri -- build --bundles deb --features gpu-shared -- --no-default-features
```

The inject scripts edit `src-tauri/tauri.conf.json`; don't commit that change.
Keep the CPU and shared features mutually exclusive: `--no-default-features`
is passed to Cargo to turn off the default `cpu-static` feature.

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
