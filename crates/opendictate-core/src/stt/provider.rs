//! Execution-provider selection shared by the offline and streaming engines.
//!
//! A `Provider` only ever *requests* an ONNX Runtime EP; construction sites
//! must fall back to [`Provider::Cpu`] when creation fails, because GPU
//! providers require matching shared libraries and drivers that most
//! installs do not have.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Cpu,
    /// NVIDIA CUDA EP. Requires a CUDA-enabled sherpa-onnx shared library at
    /// link time and working NVIDIA drivers at runtime.
    Cuda,
    /// Apple CoreML/ANE EP. Experimental: int8 transducer support in ORT's
    /// CoreML EP has been historically uneven, so this is opt-in only.
    CoreMl,
}

impl Provider {
    /// Parses an explicit user request (`settings.gpu`). Unknown values are
    /// treated as CPU rather than erroring.
    pub fn from_request(requested: &str) -> Provider {
        match requested.trim().to_lowercase().as_str() {
            "cuda" => Provider::Cuda,
            "coreml" => Provider::CoreMl,
            _ => Provider::Cpu,
        }
    }

    /// What `gpu = "auto"` resolves to on this platform. Conservative by
    /// design: CoreML stays opt-in until it has been validated on real
    /// Apple Silicon, so auto only ever attempts CUDA.
    pub fn auto() -> Provider {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            Provider::Cuda
        }
        #[cfg(target_os = "macos")]
        {
            Provider::Cpu
        }
    }

    /// Whether execution providers are compiled into the linked onnxruntime
    /// at all.
    ///
    /// The default `cpu-static` link is a CPU-only build: sherpa-onnx accepts
    /// the provider string, finds no such EP, logs "Fallback to cpu!" and
    /// runs on CPU. That fallback is invisible to us, so without this check a
    /// GPU provider is reported as active while every frame executes on CPU.
    /// Only the `gpu-shared` link can carry EPs — see docs/GPU.md.
    const fn gpu_providers_linked() -> bool {
        cfg!(feature = "gpu-shared")
    }

    /// Whether this machine plausibly has the hardware/driver for the
    /// provider, **and** the binary was linked in a way that can use it.
    /// sherpa-onnx also degrades gracefully internally, but its fallback is
    /// invisible to us — the recognizer reports the *requested* provider even
    /// while executing on CPU. Gating here keeps our UI truthful and skips
    /// pointless GPU session attempts entirely.
    pub fn hardware_available(self) -> bool {
        if self.is_gpu() && !Self::gpu_providers_linked() {
            return false;
        }
        match self {
            Provider::Cpu => true,
            Provider::Cuda => {
                #[cfg(target_os = "linux")]
                {
                    std::path::Path::new("/proc/driver/nvidia").exists()
                        || which("nvidia-smi")
                }
                #[cfg(target_os = "windows")]
                {
                    std::path::Path::new(r"C:\Windows\System32\nvapi64.dll").exists()
                        || which("nvidia-smi")
                }
                #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                {
                    false
                }
            }
            Provider::CoreMl => cfg!(target_os = "macos"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Cpu => "cpu",
            Provider::Cuda => "cuda",
            Provider::CoreMl => "coreml",
        }
    }

    pub fn is_gpu(self) -> bool {
        !matches!(self, Provider::Cpu)
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p).any(|dir| dir.join(bin).is_file())
        })
        .unwrap_or(false)
}

/// Resolves the provider to attempt for a given `settings.gpu` value:
/// `"auto"` defers to [`Provider::auto`], anything else parses explicitly,
/// and `"off"` / unknown fall back to CPU.
pub fn resolve(mode: &str) -> Provider {
    let requested = match mode.trim().to_lowercase().as_str() {
        "auto" => Provider::auto(),
        other => Provider::from_request(other),
    };
    // Downgrade before construction so the reported provider reflects
    // reality and we never spin up a doomed CUDA session.
    if requested.is_gpu() && !requested.hardware_available() {
        let reason = if Provider::gpu_providers_linked() {
            "hardware unavailable"
        } else {
            "this build links a CPU-only onnxruntime"
        };
        log::info!(
            "gpu mode '{}' requested but {reason}; using cpu",
            requested.as_str()
        );
        Provider::Cpu
    } else {
        requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_requests_parse() {
        assert_eq!(Provider::from_request("cuda"), Provider::Cuda);
        assert_eq!(Provider::from_request("CUDA "), Provider::Cuda);
        assert_eq!(Provider::from_request("coreml"), Provider::CoreMl);
        assert_eq!(Provider::from_request("off"), Provider::Cpu);
        assert_eq!(Provider::from_request("nonsense"), Provider::Cpu);
    }

    #[test]
    fn resolve_maps_auto_and_off() {
        // auto must never resolve to CoreMl until it is validated.
        assert_ne!(Provider::auto(), Provider::CoreMl);
        assert_eq!(resolve("off"), Provider::Cpu);
        assert_eq!(resolve(""), Provider::Cpu);
        // Explicit cuda resolves through the hardware gate: on GPU-less CI
        // it must come back as CPU.
        if !Provider::Cuda.hardware_available() {
            assert_eq!(resolve("cuda"), Provider::Cpu);
        } else {
            assert_eq!(resolve("cuda"), Provider::Cuda);
        }
    }

    /// The regression this guards: on a `cpu-static` build, requesting CoreML
    /// on macOS passed the old gate (which only asked "is this a Mac?"), so
    /// the engine reported `coreml` and the UI showed a "GPU ✓ coreml" chip
    /// while sherpa-onnx had already fallen back to CPU internally.
    #[test]
    fn gpu_never_reported_when_no_providers_are_linked() {
        if Provider::gpu_providers_linked() {
            return;
        }
        assert_eq!(resolve("coreml"), Provider::Cpu);
        assert_eq!(resolve("cuda"), Provider::Cpu);
        assert!(!Provider::CoreMl.hardware_available());
        assert!(!Provider::Cuda.hardware_available());
    }

    #[test]
    fn as_str_roundtrips() {
        for p in [Provider::Cpu, Provider::Cuda, Provider::CoreMl] {
            assert_eq!(Provider::from_request(p.as_str()), p);
        }
    }
}
