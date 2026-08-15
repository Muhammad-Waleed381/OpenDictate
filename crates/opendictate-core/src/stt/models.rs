use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use tar::Archive;

use crate::error::{CoreError, Result};

pub const DEFAULT_MODEL: &str = "parakeet-tdt-ctc-110m";
pub const VAD_FILENAME: &str = "silero_vad.onnx";

const PARAKEET_INT8_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000-int8.tar.bz2";
const PARAKEET_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000.tar.bz2";
const SILERO_VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";

const MODEL_MIN_BYTES: u64 = 1_000_000;
const TOKENS_MIN_BYTES: u64 = 100;
const VAD_MIN_BYTES: u64 = 100_000;

fn model_archive_urls() -> [&'static str; 2] {
    [PARAKEET_INT8_ARCHIVE, PARAKEET_ARCHIVE]
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
    let model = valid_file_size(&dir.join("model.int8.onnx"), MODEL_MIN_BYTES)
        .or_else(|| valid_file_size(&dir.join("model.onnx"), MODEL_MIN_BYTES));
    model.is_some() && valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
}

pub fn is_vad_ready() -> bool {
    valid_file_size(&vad_model_path(), VAD_MIN_BYTES).is_some()
}

pub fn download_to(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(CoreError::Io)?;
    }

    let response = ureq::get(url)
        .call()
        .map_err(|e| CoreError::Download(format!("failed to fetch {url}: {e}")))?;

    let mut body = response.into_body().into_reader();

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

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files(&path, out);
            } else {
                out.push(path);
            }
        }
    }
}

fn extract_archive(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive_path).map_err(CoreError::Io)?;
    let mut archive = Archive::new(BzDecoder::new(file));
    archive
        .unpack(dest)
        .map_err(|e| CoreError::Download(format!("failed to extract {}: {e}", archive_path.display())))
}

fn install_stt_model() -> Result<bool> {
    let base = models_dir();
    std::fs::create_dir_all(&base).map_err(CoreError::Io)?;
    let archive_path = base.join("parakeet-tdt-ctc-110m.tar.bz2");
    let extract_dir = base.join(".extract");

    for url in model_archive_urls() {
        log::info!("downloading {url} -> {}", archive_path.display());
        if download_to(url, &archive_path).is_err() {
            log::warn!("download failed for {url}, trying next archive");
            continue;
        }
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
        }
        if let Err(e) = extract_archive(&archive_path, &extract_dir) {
            log::warn!("extraction failed: {e}");
            continue;
        }

        let mut files = Vec::new();
        collect_files(&extract_dir, &mut files);
        let model_file = files.iter().find(|p| {
            matches!(
                p.file_name().and_then(|n| n.to_str()),
                Some("model.int8.onnx") | Some("model.onnx")
            )
        });
        let tokens_file = files.iter().find(|p| p.file_name().is_some_and(|n| n == "tokens.txt"));

        if let (Some(model), Some(tokens)) = (model_file, tokens_file) {
            let model_dir = stt_model_dir();
            std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
            let model_name = model.file_name().unwrap_or_default();
            std::fs::copy(model, model_dir.join(model_name)).map_err(CoreError::Io)?;
            std::fs::copy(tokens, model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
            log::info!("installed {} + tokens.txt -> {}", model_name.to_string_lossy(), model_dir.display());
            let _ = std::fs::remove_file(&archive_path);
            let _ = std::fs::remove_dir_all(&extract_dir);
            return Ok(true);
        }
        log::warn!("archive {} has no recognizer files, trying next", url);
    }

    Ok(false)
}

pub fn ensure_models() -> Result<()> {
    if !is_stt_model_ready() && !install_stt_model()? {
        return Err(CoreError::Download(
            "failed to download a usable STT model from any archive".to_string(),
        ));
    }

    if !is_vad_ready() {
        log::info!("downloading silero VAD -> {}", vad_model_path().display());
        download_to(SILERO_VAD_URL, &vad_model_path())?;
        if !is_vad_ready() {
            return Err(CoreError::Download(format!(
                "downloaded VAD file is too small: {}",
                vad_model_path().display()
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ready_requires_files() {
        assert!(!is_stt_model_ready());
    }

    #[test]
    fn model_archive_urls_are_ordered() {
        let urls = model_archive_urls();
        assert!(urls[0].contains("int8"));
    }
}