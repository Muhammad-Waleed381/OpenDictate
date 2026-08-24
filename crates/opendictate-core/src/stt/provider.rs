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

/// Resolves the provider to attempt for a given `settings.gpu` value:
/// `"auto"` defers to [`Provider::auto`], anything else parses explicitly,
/// and `"off"` / unknown fall back to CPU.
pub fn resolve(mode: &str) -> Provider {
    match mode.trim().to_lowercase().as_str() {
        "auto" => Provider::auto(),
        other => Provider::from_request(other),
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
        assert_eq!(resolve("cuda"), Provider::Cuda);
    }

    #[test]
    fn as_str_roundtrips() {
        for p in [Provider::Cpu, Provider::Cuda, Provider::CoreMl] {
            assert_eq!(Provider::from_request(p.as_str()), p);
        }
    }
}
