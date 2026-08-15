use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

pub const DEFAULT_MODEL: &str = "parakeet-tdt-ctc-110m";
pub const VAD_FILENAME: &str = "silero_vad.onnx";

const PARAKEET_110M_REPO: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000/resolve/main";
const SILERO_VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";

#[derive(Clone, Debug)]
pub struct ModelFile {
    pub filename: &'static str,
    pub url: String,
    pub min_bytes: u64,
}

pub fn model_files() -> Vec<ModelFile> {
    vec![
        ModelFile {
            filename: "model.int8.onnx",
            url: format!("{PARAKEET_110M_REPO}/model.int8.onnx"),
            min_bytes: 1_000_000,
        },
        ModelFile {
            filename: "model.onnx",
            url: format!("{PARAKEET_110M_REPO}/model.onnx"),
            min_bytes: 1_000_000,
        },
        ModelFile {
            filename: "tokens.txt",
            url: format!("{PARAKEET_110M_REPO}/tokens.txt"),
            min_bytes: 100,
        },
    ]
}

pub fn models_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".local/share"))
                .expect("HOME is not set")
        });
    base.join("opendictate").join("models")
}

pub fn stt_model_dir() -> PathBuf {
    models_dir().join(DEFAULT_MODEL)
}

pub fn vad_model_path() -> PathBuf {
    models_dir().join(VAD_FILENAME)
}

fn valid_file_size(path: &Path, min_bytes: u64) -> Option<u64> {
    let size = std::fs::metadata(path).ok()?.len();
    (size >= min_bytes).then_some(size)
}

pub fn is_stt_model_ready() -> bool {
    let dir = stt_model_dir();
    dir.is_dir()
        && (valid_file_size(&dir.join("model.int8.onnx"), 1_000_000).is_some()
            || valid_file_size(&dir.join("model.onnx"), 1_000_000).is_some())
        && valid_file_size(&dir.join("tokens.txt"), 100).is_some()
}

pub fn is_vad_ready() -> bool {
    valid_file_size(&vad_model_path(), 100_000).is_some()
}

pub fn download_to(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(CoreError::Io)?;
    }

    let response = ureq::get(url)
        .call()
        .map_err(|e| CoreError::Download(format!("failed to fetch {url}: {e}")))?;

    let mut body = response
        .into_body()
        .into_reader();

    let mut file = File::create(dest)?;
    let mut total = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut body, &mut chunk)?;
        if n == 0 {
            break;
        }
        file.write_all(&chunk[..n])?;
        total += n as u64;
        if total.is_multiple_of(4 * 1024 * 1024) {
            log::info!("downloaded {total} bytes -> {}", dest.display());
        }
    }
    log::info!("downloaded {} bytes -> {}", dest.display(), total);
    Ok(())
}

pub fn ensure_models() -> Result<()> {
    for file in model_files() {
        let dest = stt_model_dir().join(file.filename);
        if valid_file_size(&dest, file.min_bytes).is_some() {
            log::info!("model file present: {}", dest.display());
            continue;
        }
        log::info!("downloading {} -> {}", file.url, dest.display());
        download_to(&file.url, &dest)?;
        if valid_file_size(&dest, file.min_bytes).is_none() {
            return Err(CoreError::Download(format!(
                "downloaded file is too small: {}",
                dest.display()
            )));
        }
    }

    let vad_dest = vad_model_path();
    if !is_vad_ready() {
        log::info!("downloading silero VAD -> {}", vad_dest.display());
        download_to(SILERO_VAD_URL, &vad_dest)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_files_contains_all_needed() {
        let files = model_files();
        assert!(files.iter().any(|f| f.filename == "model.int8.onnx"));
        assert!(files.iter().any(|f| f.filename == "tokens.txt"));
    }

    #[test]
    fn model_ready_requires_files() {
        assert!(!is_stt_model_ready());
    }
}
