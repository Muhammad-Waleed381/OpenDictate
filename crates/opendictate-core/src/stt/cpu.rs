use std::collections::HashSet;

pub const MAX_STT_THREADS: usize = 32;

/// Number of available logical hardware threads.
pub fn logical_cores() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

/// Parses /proc/cpuinfo text to count unique (physical_id, core_id) pairs.
pub fn parse_cpuinfo_physical_cores(cpuinfo: &str) -> Option<usize> {
    let mut cores = HashSet::new();
    let mut cur_phys_id: Option<String> = None;
    let mut cur_core_id: Option<String> = None;

    for line in cpuinfo.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let (Some(phys), Some(core)) = (cur_phys_id.take(), cur_core_id.take()) {
                cores.insert((phys, core));
            }
            continue;
        }

        if let Some((key, val)) = trimmed.split_once(':') {
            let key = key.trim().to_lowercase();
            let val = val.trim().to_string();
            if key == "physical id" {
                cur_phys_id = Some(val);
            } else if key == "core id" {
                cur_core_id = Some(val);
            }
        }
    }

    if let (Some(phys), Some(core)) = (cur_phys_id, cur_core_id) {
        cores.insert((phys, core));
    }

    if cores.is_empty() {
        None
    } else {
        Some(cores.len())
    }
}

/// Detects physical CPU cores on the host, avoiding logical hyperthreading
/// oversubscription that causes cache thrashing in ONNX Runtime inference.
pub fn physical_cores() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
            if let Some(count) = parse_cpuinfo_physical_cores(&content) {
                if count >= 1 {
                    return count;
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, sysctl -n hw.physicalcpu gives physical cores directly.
        if let Ok(output) = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("hw.physicalcpu")
            .output()
        {
            if output.status.success() {
                if let Ok(s) = std::str::from_utf8(&output.stdout) {
                    if let Ok(n) = s.trim().parse::<usize>() {
                        if n >= 1 {
                            return n;
                        }
                    }
                }
            }
        }
    }

    // Conservative cross-platform fallback: if logical count is >= 4 and even,
    // SMT/hyperthreading is common.
    let logical = logical_cores();
    if logical >= 4 && logical % 2 == 0 {
        logical / 2
    } else {
        logical
    }
}

/// Computes the optimal intra-op STT thread count based on physical cores,
/// scaling up to 32 threads on high-end hardware without SMT thrashing.
pub fn compute_optimal_stt_threads(physical: usize, _logical: usize) -> usize {
    physical.clamp(1, MAX_STT_THREADS)
}

/// Returns optimal thread count for offline STT models (Whisper, Parakeet CTC/TDT).
pub fn optimal_stt_threads() -> i32 {
    compute_optimal_stt_threads(physical_cores(), logical_cores()) as i32
}

/// Returns optimal thread count for streaming STT models.
/// Small audio chunks sequentially decoded suffer synchronization overhead beyond 6 threads.
pub fn optimal_streaming_threads() -> i32 {
    physical_cores().clamp(1, 6) as i32
}

/// Returns optimal thread count for keyword spotting (KWS).
pub fn optimal_kws_threads() -> i32 {
    physical_cores().clamp(1, 2) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physical_cores_bounds() {
        let physical = physical_cores();
        let logical = logical_cores();
        assert!(physical >= 1);
        assert!(logical >= physical);
    }

    #[test]
    fn test_parse_proc_cpuinfo_cores() {
        let fake_cpuinfo = "
processor	: 0
physical id	: 0
core id		: 0

processor	: 1
physical id	: 0
core id		: 1

processor	: 2
physical id	: 0
core id		: 0

processor	: 3
physical id	: 0
core id		: 1
";
        let count = parse_cpuinfo_physical_cores(fake_cpuinfo);
        assert_eq!(count, Some(2));
    }

    #[test]
    fn test_optimal_threads_scaling() {
        assert_eq!(compute_optimal_stt_threads(1, 1), 1);
        assert_eq!(compute_optimal_stt_threads(4, 8), 4);
        assert_eq!(compute_optimal_stt_threads(16, 32), 16);
        assert_eq!(compute_optimal_stt_threads(64, 128), 32); // capped at MAX_STT_THREADS (32)
    }

    #[test]
    fn test_optimal_streaming_and_kws_threads() {
        assert!(optimal_streaming_threads() >= 1 && optimal_streaming_threads() <= 6);
        assert!(optimal_kws_threads() >= 1 && optimal_kws_threads() <= 2);
    }
}
