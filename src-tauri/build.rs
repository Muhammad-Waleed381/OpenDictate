fn main() {
    // Linux GPU builds: the shared sherpa-onnx/onnxruntime libraries install
    // to a fixed directory via bundle.linux.deb.files, and CI exports this
    // env var so the binary carries a matching rpath. Unset everywhere else
    // (default builds, other platforms), so this is a no-op for them.
    #[cfg(target_os = "linux")]
    {
        if let Ok(dir) = std::env::var("OPENDICTATE_LINUX_RPATH") {
            if !dir.is_empty() {
                // -bins targets the executable specifically; the plain
                // variant is kept for toolchain differences.
                println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{dir}");
                println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
            }
        }
    }
    tauri_build::build()
}
