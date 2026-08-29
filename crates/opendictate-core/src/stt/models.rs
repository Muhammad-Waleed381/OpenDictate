use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use serde::Serialize;
use tar::Archive;

use crate::error::{CoreError, Result};

pub const DEFAULT_MODEL: &str = "parakeet-tdt-ctc-110m";
pub const VAD_FILENAME: &str = "silero_vad.onnx";

pub const STT_MODEL_ID: &str = "parakeet-tdt-ctc-110m-int8";
pub const VAD_MODEL_ID: &str = "silero-vad-v4";
pub const PARAKEET_TDT_06B_MODEL_ID: &str = "parakeet-tdt-0.6b-v3";
pub const PARAKEET_UNIFIED_EN_MODEL_ID: &str = "parakeet-unified-en-0.6b";
pub const PARAKEET_STREAMING_MODEL_ID: &str = "parakeet-unified-en-0.6b-int8-streaming-560ms";
pub const FASTCONFORMER_STREAMING_80MS_MODEL_ID: &str = "nemo-streaming-fastconformer-ctc-en-80ms";
pub const WHISPER_TINY_MODEL_ID: &str = "whisper-tiny-en";
pub const WHISPER_BASE_MODEL_ID: &str = "whisper-base-en";
pub const WHISPER_SMALL_MODEL_ID: &str = "whisper-small-en";
pub const WHISPER_TURBO_MODEL_ID: &str = "whisper-turbo-en";
pub const WHISPER_MEDIUM_MODEL_ID: &str = "whisper-medium-en";

const PARAKEET_INT8_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000-int8.tar.bz2";
const PARAKEET_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000.tar.bz2";
const PARAKEET_TDT_06B_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";
const PARAKEET_UNIFIED_EN_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-non-streaming.tar.bz2";
const PARAKEET_STREAMING_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-streaming-560ms.tar.bz2";
const FASTCONFORMER_STREAMING_80MS_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-streaming-fast-conformer-ctc-en-80ms.tar.bz2";
/// Internal live-caption engine: a 20M-param zipformer transducer that decodes
/// faster than real time even on low-power CPUs. Not user-selectable; it only
/// ever powers partial captions while an accuracy model produces the final
/// transcript.
pub const CAPTION_MODEL_ID: &str = "streaming-zipformer-en-20m-2023-02-17";
const CAPTION_MODEL_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17.tar.bz2";
pub const KWS_MODEL_ID: &str = "kws-zipformer-gigaspeech-3.3m-2024-01-01";
const KWS_MODEL_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2";
const SILERO_VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
const WHISPER_TINY_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-tiny.en.tar.bz2";
const WHISPER_BASE_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-base.en.tar.bz2";
const WHISPER_SMALL_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-small.en.tar.bz2";
const WHISPER_TURBO_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-turbo.tar.bz2";
const WHISPER_MEDIUM_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-medium.en.tar.bz2";

const MODEL_MIN_BYTES: u64 = 1_000_000;
/// Zipformer streaming decoders/joiners are genuinely small (a few hundred
/// KB), so the part minimum must be lower than the encoder's.
const STREAMING_PART_MIN_BYTES: u64 = 100_000;
const TOKENS_MIN_BYTES: u64 = 100;
/// The silero VAD onnx is ~1.7 MB; the old 100 KB threshold accepted
/// truncated/garbage downloads as "installed".
const VAD_MIN_BYTES: u64 = 1_000_000;
const WHISPER_PART_MIN_BYTES: u64 = 5_000_000;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub engine_key: Option<String>,
    pub size_bytes: u64,
    pub disk_bytes: u64,
    pub installed: bool,
    pub available: bool,
    pub streaming: bool,
}

struct ModelDef {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    engine_key: Option<&'static str>,
    size_bytes: u64,
    url: &'static str,
    available: bool,
    streaming: bool,
}

const MODELS: &[ModelDef] = &[
    ModelDef {
        id: STT_MODEL_ID,
        name: "Parakeet TDT 110M (int8)",
        kind: "stt",
        engine_key: Some("parakeet"),
        size_bytes: 104_337_827,
        url: PARAKEET_INT8_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: CAPTION_MODEL_ID,
        name: "Zipformer EN 20M (captions)",
        kind: "caption",
        engine_key: None,
        size_bytes: 29_000_000,
        url: CAPTION_MODEL_ARCHIVE,
        available: true,
        // Not a user-selectable STT model; `streaming` flags selectable
        // engines only. This one powers live captions internally.
        streaming: false,
    },
    ModelDef {
        id: VAD_MODEL_ID,
        name: "Silero VAD v4",
        kind: "vad",
        engine_key: None,
        size_bytes: 1_700_000,
        url: SILERO_VAD_URL,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: KWS_MODEL_ID,
        name: "Zipformer 3.3M (Wake Word / KWS)",
        kind: "kws",
        engine_key: None,
        size_bytes: 15_800_000,
        url: KWS_MODEL_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: PARAKEET_STREAMING_MODEL_ID,
        name: "Parakeet Unified 0.6B (Streaming)",
        kind: "stt",
        engine_key: Some("parakeet-streaming"),
        size_bytes: 501_360_769,
        url: PARAKEET_STREAMING_ARCHIVE,
        available: true,
        streaming: true,
    },
    ModelDef {
        id: FASTCONFORMER_STREAMING_80MS_MODEL_ID,
        name: "FastConformer 80ms (Streaming)",
        kind: "stt",
        engine_key: Some("fastconformer-streaming"),
        size_bytes: 125_000_000,
        url: FASTCONFORMER_STREAMING_80MS_ARCHIVE,
        available: true,
        streaming: true,
    },
    ModelDef {
        id: WHISPER_TINY_MODEL_ID,
        name: "Whisper Tiny (en)",
        kind: "stt",
        engine_key: Some("whisper"),
        size_bytes: 118_071_777,
        url: WHISPER_TINY_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: WHISPER_BASE_MODEL_ID,
        name: "Whisper Base (en)",
        kind: "stt",
        engine_key: Some("whisper"),
        size_bytes: 208_576_005,
        url: WHISPER_BASE_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: PARAKEET_TDT_06B_MODEL_ID,
        name: "Parakeet TDT 0.6B (Multilingual)",
        kind: "stt",
        engine_key: Some("parakeet"),
        size_bytes: 487_170_055,
        url: PARAKEET_TDT_06B_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: PARAKEET_UNIFIED_EN_MODEL_ID,
        name: "Parakeet Unified En 0.6B",
        kind: "stt",
        engine_key: Some("parakeet"),
        size_bytes: 501_350_460,
        url: PARAKEET_UNIFIED_EN_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: WHISPER_TURBO_MODEL_ID,
        name: "Whisper Turbo (Large v3)",
        kind: "stt",
        engine_key: Some("whisper"),
        size_bytes: 563_790_207,
        url: WHISPER_TURBO_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: WHISPER_SMALL_MODEL_ID,
        name: "Whisper Small (en)",
        kind: "stt",
        engine_key: Some("whisper"),
        size_bytes: 635_693_775,
        url: WHISPER_SMALL_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: WHISPER_MEDIUM_MODEL_ID,
        name: "Whisper Medium (en)",
        kind: "stt",
        engine_key: Some("whisper"),
        size_bytes: 1_905_872_689,
        url: WHISPER_MEDIUM_ARCHIVE,
        available: true,
        streaming: false,
    },
];

fn model_def(id: &str) -> Option<&'static ModelDef> {
    MODELS.iter().find(|m| m.id == id)
}

/// Platform-appropriate model storage. Never panics: a machine with no
/// discernible home falls back to the temp dir rather than aborting a
/// command/background thread mid-flight.
pub fn models_dir() -> PathBuf {
    // XDG override first (Linux convention, also used by tests).
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("opendictate").join("models");
        }
    }

    #[cfg(target_os = "windows")]
    {
        // HOME is not set on stock Windows; LOCALAPPDATA is the canonical
        // location for per-user app data.
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            if !la.is_empty() {
                return PathBuf::from(la).join("opendictate").join("models");
            }
        }
        if let Ok(up) = std::env::var("USERPROFILE") {
            return PathBuf::from(up)
                .join("AppData")
                .join("Local")
                .join("opendictate")
                .join("models");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                let mac_path = PathBuf::from(&home)
                    .join("Library")
                    .join("Application Support")
                    .join("opendictate")
                    .join("models");
                let legacy_path = PathBuf::from(&home)
                    .join(".local")
                    .join("share")
                    .join("opendictate")
                    .join("models");
                if legacy_path.exists() {
                    return legacy_path;
                }
                return mac_path;
            }
        }
    }

    // Linux and legacy setups keep ~/.local/share location — existing
    // installs already have models there and must not be orphaned.
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".local/share").join("opendictate").join("models");
        }
    }

    log::warn!("no home directory resolvable; models fall back to temp dir");
    std::env::temp_dir().join("opendictate").join("models")
}

pub fn stt_model_dir() -> PathBuf {
    models_dir().join(DEFAULT_MODEL)
}

pub fn vad_model_path() -> PathBuf {
    models_dir().join(VAD_FILENAME)
}

pub fn model_dir_for(id: &str) -> PathBuf {
    if id == STT_MODEL_ID {
        stt_model_dir()
    } else {
        models_dir().join(id)
    }
}

pub fn is_whisper_model(id: &str) -> bool {
    matches!(
        id,
        WHISPER_TINY_MODEL_ID
            | WHISPER_BASE_MODEL_ID
            | WHISPER_SMALL_MODEL_ID
            | WHISPER_TURBO_MODEL_ID
            | WHISPER_MEDIUM_MODEL_ID
    )
}

pub fn caption_model_dir() -> PathBuf {
    models_dir().join(CAPTION_MODEL_ID)
}

pub fn is_caption_model_ready() -> bool {
    let dir = caption_model_dir();
    valid_file_size(&dir.join("encoder.onnx"), STREAMING_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("decoder.onnx"), STREAMING_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("joiner.onnx"), STREAMING_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
}

pub fn is_streaming_model(id: &str) -> bool {
    matches!(
        id,
        PARAKEET_STREAMING_MODEL_ID | FASTCONFORMER_STREAMING_80MS_MODEL_ID
    )
}

pub fn is_transducer_model(id: &str) -> bool {
    matches!(
        id,
        PARAKEET_TDT_06B_MODEL_ID | PARAKEET_UNIFIED_EN_MODEL_ID
    )
}

fn valid_file_size(path: &Path, min_bytes: u64) -> Option<u64> {
    let size = std::fs::metadata(path).ok()?.len();
    (size >= min_bytes).then_some(size)
}

fn is_nemo_ctc_ready(id: &str) -> bool {
    let dir = model_dir_for(id);
    let model = valid_file_size(&dir.join("model.int8.onnx"), MODEL_MIN_BYTES)
        .or_else(|| valid_file_size(&dir.join("model.onnx"), MODEL_MIN_BYTES));
    model.is_some() && valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
}

fn is_parakeet_ready() -> bool {
    let dir = stt_model_dir();
    let model = valid_file_size(&dir.join("model.int8.onnx"), MODEL_MIN_BYTES)
        .or_else(|| valid_file_size(&dir.join("model.onnx"), MODEL_MIN_BYTES));
    model.is_some() && valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
}

fn is_whisper_ready(id: &str) -> bool {
    let dir = model_dir_for(id);
    valid_file_size(&dir.join("encoder.onnx"), WHISPER_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("decoder.onnx"), WHISPER_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
}

fn is_transducer_ready(id: &str) -> bool {
    let dir = model_dir_for(id);
    valid_file_size(&dir.join("encoder.onnx"), MODEL_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("decoder.onnx"), STREAMING_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("joiner.onnx"), STREAMING_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
}

pub fn kws_model_dir() -> PathBuf {
    models_dir().join(KWS_MODEL_ID)
}

pub fn is_kws_ready() -> bool {
    let dir = kws_model_dir();
    if !dir.exists() {
        return false;
    }
    let find = |needle: &str| -> bool {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            entries.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.ends_with(".onnx") && n.contains(needle)
            })
        } else {
            false
        }
    };
    valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
        && (valid_file_size(&dir.join("encoder.onnx"), STREAMING_PART_MIN_BYTES).is_some()
            || find("encoder"))
        && (valid_file_size(&dir.join("decoder.onnx"), STREAMING_PART_MIN_BYTES).is_some()
            || find("decoder"))
        && (valid_file_size(&dir.join("joiner.onnx"), STREAMING_PART_MIN_BYTES).is_some()
            || find("joiner"))
}

pub fn is_vad_ready() -> bool {
    valid_file_size(&vad_model_path(), VAD_MIN_BYTES).is_some()
}

pub fn is_stt_model_ready() -> bool {
    MODELS.iter().any(|m| m.kind == "stt" && is_model_installed(m.id))
}

pub fn is_model_installed(id: &str) -> bool {
    match id {
        STT_MODEL_ID => is_parakeet_ready(),
        VAD_MODEL_ID => is_vad_ready(),
        KWS_MODEL_ID => is_kws_ready(),
        FASTCONFORMER_STREAMING_80MS_MODEL_ID => is_nemo_ctc_ready(id),
        PARAKEET_TDT_06B_MODEL_ID | PARAKEET_UNIFIED_EN_MODEL_ID => is_transducer_ready(id),
        PARAKEET_STREAMING_MODEL_ID | CAPTION_MODEL_ID => is_transducer_ready(id),
        WHISPER_TINY_MODEL_ID
        | WHISPER_BASE_MODEL_ID
        | WHISPER_SMALL_MODEL_ID
        | WHISPER_TURBO_MODEL_ID
        | WHISPER_MEDIUM_MODEL_ID => is_whisper_ready(id),
        _ => false,
    }
}

fn dir_size(path: &Path, out: &mut u64) {
    if let Ok(rd) = std::fs::read_dir(path) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                dir_size(&p, out);
            } else if let Ok(meta) = std::fs::metadata(&p) {
                *out += meta.len();
            }
        }
    }
}

pub fn model_disk_bytes(id: &str) -> u64 {
    let path = if id == VAD_MODEL_ID {
        vad_model_path()
    } else {
        model_dir_for(id)
    };
    if path.is_file() {
        valid_file_size(&path, 0).unwrap_or(0)
    } else if path.is_dir() {
        let mut total = 0u64;
        dir_size(&path, &mut total);
        total
    } else {
        0
    }
}

pub fn catalog() -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|def| ModelInfo {
            id: def.id.to_string(),
            name: def.name.to_string(),
            kind: def.kind.to_string(),
            engine_key: def.engine_key.map(|s| s.to_string()),
            size_bytes: def.size_bytes,
            disk_bytes: model_disk_bytes(def.id),
            installed: is_model_installed(def.id),
            available: def.available,
            streaming: def.streaming,
        })
        .collect()
}

pub fn download_to(url: &str, dest: &Path) -> Result<()> {
    download_to_with_progress(url, dest, &mut |_, _| {})
}

pub fn download_to_with_progress(
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    download_to_with_progress_cancel(url, dest, on_progress, &|| false)
}

pub fn download_to_with_progress_cancel(
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    if is_cancelled() {
        return Err(CoreError::Download("download cancelled by user".to_string()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(CoreError::Io)?;
    }

    let response = ureq::get(url)
        .call()
        .map_err(|e| CoreError::Download(format!("failed to fetch {url}: {e}")))?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut body = response.into_body().into_reader();

    let mut file = File::create(dest)?;
    let mut received = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if is_cancelled() {
            drop(file);
            let _ = std::fs::remove_file(dest);
            return Err(CoreError::Download("download cancelled by user".to_string()));
        }
        let n = std::io::Read::read(&mut body, &mut chunk)?;
        if n == 0 {
            break;
        }
        file.write_all(&chunk[..n])?;
        received += n as u64;
        on_progress(received, total);
        if received.is_multiple_of(4 * 1024 * 1024) {
            log::info!("downloaded {received} bytes -> {}", dest.display());
        }
    }
    if is_cancelled() {
        drop(file);
        let _ = std::fs::remove_file(dest);
        return Err(CoreError::Download("download cancelled by user".to_string()));
    }
    log::info!("downloaded {received} bytes -> {}", dest.display());
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
    archive.unpack(dest).map_err(|e| {
        CoreError::Download(format!("failed to extract {}: {e}", archive_path.display()))
    })
}

fn clean_after(archive_path: &Path, extract_dir: &Path) {
    let _ = std::fs::remove_file(archive_path);
    let _ = std::fs::remove_dir_all(extract_dir);
}

fn install_stt_model(
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<bool> {
    let base = models_dir();
    std::fs::create_dir_all(&base).map_err(CoreError::Io)?;
    let archive_path = base.join("parakeet-tdt-ctc-110m.tar.bz2");
    let extract_dir = base.join(".extract");

    for url in [PARAKEET_INT8_ARCHIVE, PARAKEET_ARCHIVE] {
        if is_cancelled() {
            clean_after(&archive_path, &extract_dir);
            return Err(CoreError::Download("download cancelled by user".to_string()));
        }
        log::info!("downloading {url} -> {}", archive_path.display());
        if download_to_with_progress_cancel(
            url,
            &archive_path,
            &mut |received, total| on_progress(STT_MODEL_ID, received, total),
            is_cancelled,
        )
        .is_err()
        {
            if is_cancelled() {
                clean_after(&archive_path, &extract_dir);
                return Err(CoreError::Download("download cancelled by user".to_string()));
            }
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
        // Prefer the int8 variant: the catalog advertises int8 sizes and the
        // runtime loads `model.int8.onnx` first (see `find_model_file`).
        // `collect_files` order is filesystem-dependent, so a plain `find`
        // could nondeterministically install the ~2x larger fp32 model.
        let model_file = files
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n == "model.int8.onnx"))
            .or_else(|| {
                files
                    .iter()
                    .find(|p| p.file_name().is_some_and(|n| n == "model.onnx"))
            });
        let tokens_file = files
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n == "tokens.txt"));

        if let (Some(model), Some(tokens)) = (model_file, tokens_file) {
            let model_dir = stt_model_dir();
            std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
            let model_name = model.file_name().unwrap_or_default();
            std::fs::copy(model, model_dir.join(model_name)).map_err(CoreError::Io)?;
            std::fs::copy(tokens, model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
            log::info!(
                "installed {} + tokens.txt -> {}",
                model_name.to_string_lossy(),
                model_dir.display()
            );
            clean_after(&archive_path, &extract_dir);
            return Ok(true);
        }
        log::warn!("archive {} has no recognizer files, trying next", url);
    }

    Ok(false)
}

fn install_whisper_model(
    id: &str,
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let def = model_def(id).ok_or_else(|| CoreError::Download(format!("unknown model '{id}'")))?;
    let base = models_dir();
    std::fs::create_dir_all(&base).map_err(CoreError::Io)?;
    let archive_path = base.join(format!("{id}.tar.bz2"));
    let extract_dir = base.join(format!(".extract-{id}"));

    log::info!("downloading {} -> {}", def.url, archive_path.display());
    download_to_with_progress_cancel(
        def.url,
        &archive_path,
        &mut |received, total| on_progress(id, received, total),
        is_cancelled,
    )?;
    if is_cancelled() {
        clean_after(&archive_path, &extract_dir);
        return Err(CoreError::Download("download cancelled by user".to_string()));
    }
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
    }
    extract_archive(&archive_path, &extract_dir)?;

    let mut files = Vec::new();
    collect_files(&extract_dir, &mut files);
    let encoder = files.iter().find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("encoder.onnx"))
    });
    let decoder = files.iter().find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("decoder.onnx"))
    });
    let tokens = files.iter().find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("tokens.txt"))
    });

    let (Some(encoder), Some(decoder), Some(tokens)) = (encoder, decoder, tokens) else {
        clean_after(&archive_path, &extract_dir);
        return Err(CoreError::Download(format!(
            "archive {id} has no encoder/decoder/tokens files"
        )));
    };

    let model_dir = model_dir_for(id);
    std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
    std::fs::copy(encoder, model_dir.join("encoder.onnx")).map_err(CoreError::Io)?;
    std::fs::copy(decoder, model_dir.join("decoder.onnx")).map_err(CoreError::Io)?;
    std::fs::copy(tokens, model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
    log::info!("installed {id} -> {}", model_dir.display());
    clean_after(&archive_path, &extract_dir);
    Ok(())
}

fn install_transducer_model(
    id: &str,
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let def = model_def(id).ok_or_else(|| CoreError::Download(format!("unknown model '{id}'")))?;
    let base = models_dir();
    std::fs::create_dir_all(&base).map_err(CoreError::Io)?;
    let archive_path = base.join(format!("{id}.tar.bz2"));
    let extract_dir = base.join(format!(".extract-{id}"));

    log::info!("downloading {} -> {}", def.url, archive_path.display());
    download_to_with_progress_cancel(
        def.url,
        &archive_path,
        &mut |received, total| on_progress(id, received, total),
        is_cancelled,
    )?;
    if is_cancelled() {
        clean_after(&archive_path, &extract_dir);
        return Err(CoreError::Download("download cancelled by user".to_string()));
    }
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
    }
    extract_archive(&archive_path, &extract_dir)?;

    let mut files = Vec::new();
    collect_files(&extract_dir, &mut files);
    let part = |needle: &str| {
        let mut hits: Vec<_> = files
            .iter()
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.ends_with(".onnx") && n.contains(needle)
                })
            })
            .cloned()
            .collect();
        hits.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
        hits.first().cloned()
    };
    let (Some(encoder), Some(decoder), Some(joiner), Some(tokens)) = (
        part("encoder"),
        part("decoder"),
        part("joiner"),
        files
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n == "tokens.txt"))
            .cloned(),
    ) else {
        clean_after(&archive_path, &extract_dir);
        return Err(CoreError::Download(format!(
            "archive {id} has no encoder/decoder/joiner/tokens files"
        )));
    };

    let model_dir = model_dir_for(id);
    std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
    std::fs::copy(encoder, model_dir.join("encoder.onnx")).map_err(CoreError::Io)?;
    std::fs::copy(decoder, model_dir.join("decoder.onnx")).map_err(CoreError::Io)?;
    std::fs::copy(joiner, model_dir.join("joiner.onnx")).map_err(CoreError::Io)?;
    std::fs::copy(tokens, model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
    log::info!("installed {id} -> {}", model_dir.display());
    clean_after(&archive_path, &extract_dir);
    Ok(())
}

fn install_nemo_ctc_model(
    id: &str,
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let def = model_def(id).ok_or_else(|| CoreError::Download(format!("unknown model '{id}'")))?;
    let base = models_dir();
    std::fs::create_dir_all(&base).map_err(CoreError::Io)?;
    let archive_path = base.join(format!("{id}.tar.bz2"));
    let extract_dir = base.join(format!(".extract-{id}"));

    log::info!("downloading {} -> {}", def.url, archive_path.display());
    download_to_with_progress_cancel(
        def.url,
        &archive_path,
        &mut |received, total| on_progress(id, received, total),
        is_cancelled,
    )?;
    if is_cancelled() {
        clean_after(&archive_path, &extract_dir);
        return Err(CoreError::Download("download cancelled by user".to_string()));
    }
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
    }
    extract_archive(&archive_path, &extract_dir)?;

    let mut files = Vec::new();
    collect_files(&extract_dir, &mut files);
    let mut hits: Vec<_> = files
        .iter()
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.ends_with(".onnx") && (n.starts_with("model") || n.contains("conformer"))
            })
        })
        .cloned()
        .collect();
    hits.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
    let model_file = hits.first().cloned();
    let tokens_file = files
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == "tokens.txt"))
        .cloned();

    let (Some(model), Some(tokens)) = (model_file, tokens_file) else {
        clean_after(&archive_path, &extract_dir);
        return Err(CoreError::Download(format!(
            "archive {id} has no model/tokens files"
        )));
    };

    let model_dir = model_dir_for(id);
    std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
    let model_name = model.file_name().unwrap_or_default().to_owned();
    std::fs::copy(&model, model_dir.join(model_name)).map_err(CoreError::Io)?;
    std::fs::copy(&tokens, model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
    log::info!("installed {id} -> {}", model_dir.display());
    clean_after(&archive_path, &extract_dir);
    Ok(())
}

pub fn ensure_model(id: &str, on_progress: &mut dyn FnMut(&str, u64, u64)) -> Result<()> {
    ensure_model_with_cancel(id, on_progress, &|| false)
}

pub fn ensure_model_with_cancel(
    id: &str,
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    if is_model_installed(id) {
        return Ok(());
    }
    if is_cancelled() {
        return Err(CoreError::Download("download cancelled by user".to_string()));
    }
    match id {
        STT_MODEL_ID => {
            if !install_stt_model(on_progress, is_cancelled)? {
                return Err(CoreError::Download(
                    "failed to download a usable STT model from any archive".to_string(),
                ));
            }
            Ok(())
        }
        FASTCONFORMER_STREAMING_80MS_MODEL_ID => {
            install_nemo_ctc_model(id, on_progress, is_cancelled)
        }
        PARAKEET_TDT_06B_MODEL_ID | PARAKEET_UNIFIED_EN_MODEL_ID => {
            install_transducer_model(id, on_progress, is_cancelled)
        }
        PARAKEET_STREAMING_MODEL_ID | CAPTION_MODEL_ID | KWS_MODEL_ID => {
            install_transducer_model(id, on_progress, is_cancelled)
        }
        VAD_MODEL_ID => {
            log::info!("downloading silero VAD -> {}", vad_model_path().display());
            download_to_with_progress_cancel(
                SILERO_VAD_URL,
                &vad_model_path(),
                &mut |received, total| on_progress(VAD_MODEL_ID, received, total),
                is_cancelled,
            )?;
            if is_cancelled() {
                let _ = std::fs::remove_file(vad_model_path());
                return Err(CoreError::Download("download cancelled by user".to_string()));
            }
            if !is_vad_ready() {
                return Err(CoreError::Download(format!(
                    "downloaded VAD file is too small: {}",
                    vad_model_path().display()
                )));
            }
            Ok(())
        }
        WHISPER_TINY_MODEL_ID
        | WHISPER_BASE_MODEL_ID
        | WHISPER_SMALL_MODEL_ID
        | WHISPER_TURBO_MODEL_ID
        | WHISPER_MEDIUM_MODEL_ID => {
            install_whisper_model(id, on_progress, is_cancelled)
        }
        other => Err(CoreError::Download(format!("unknown model '{other}'"))),
    }
}

pub fn remove_model(id: &str) -> Result<()> {
    let path = if id == VAD_MODEL_ID {
        vad_model_path()
    } else {
        model_dir_for(id)
    };
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(CoreError::Io)?;
    } else {
        std::fs::remove_file(&path).map_err(CoreError::Io)?;
    }
    log::info!("removed model {id} ({})", path.display());
    Ok(())
}

pub fn ensure_models() -> Result<()> {
    ensure_models_with_progress(&mut |_, _, _| {})
}

pub fn ensure_models_with_progress(on_progress: &mut dyn FnMut(&str, u64, u64)) -> Result<()> {
    ensure_model(STT_MODEL_ID, on_progress)?;
    ensure_model(VAD_MODEL_ID, on_progress)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ready_requires_files() {
        let tmp = std::env::temp_dir().join("opendictate-test-ready");
        std::env::set_var("XDG_DATA_HOME", &tmp);
        let result = is_stt_model_ready();
        std::env::remove_var("XDG_DATA_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(!result);
    }

    #[test]
    fn catalog_lists_known_models() {
        let catalog = catalog();
        for id in [
            STT_MODEL_ID,
            VAD_MODEL_ID,
            PARAKEET_TDT_06B_MODEL_ID,
            PARAKEET_UNIFIED_EN_MODEL_ID,
            PARAKEET_STREAMING_MODEL_ID,
            FASTCONFORMER_STREAMING_80MS_MODEL_ID,
            WHISPER_TINY_MODEL_ID,
            WHISPER_BASE_MODEL_ID,
            WHISPER_SMALL_MODEL_ID,
            WHISPER_TURBO_MODEL_ID,
            WHISPER_MEDIUM_MODEL_ID,
        ] {
            assert!(catalog.iter().any(|m| m.id == id), "{id} missing");
        }
        assert!(!catalog.iter().any(|m| m.id.is_empty()));
        assert_eq!(
            catalog
                .iter()
                .find(|m| m.id == STT_MODEL_ID)
                .and_then(|m| m.engine_key.as_deref()),
            Some("parakeet")
        );
        assert!(catalog.iter().all(|m| m.size_bytes > 0));
        assert!(
            catalog
                .iter()
                .filter(|m| m.streaming)
                .all(|m| is_streaming_model(&m.id))
        );
    }

    #[test]
    fn streaming_models_are_flagged() {
        assert!(is_streaming_model(PARAKEET_STREAMING_MODEL_ID));
        assert!(is_streaming_model(FASTCONFORMER_STREAMING_80MS_MODEL_ID));
        assert!(!is_streaming_model(STT_MODEL_ID));
        assert!(!is_streaming_model(VAD_MODEL_ID));
        assert!(!is_streaming_model("bogus"));
    }

    #[test]
    fn whisper_models_are_flagged() {
        for id in [
            WHISPER_TINY_MODEL_ID,
            WHISPER_BASE_MODEL_ID,
            WHISPER_SMALL_MODEL_ID,
            WHISPER_TURBO_MODEL_ID,
            WHISPER_MEDIUM_MODEL_ID,
        ] {
            assert!(is_whisper_model(id), "{id}");
        }
        assert!(!is_whisper_model(STT_MODEL_ID));
        assert!(!is_whisper_model("bogus"));
    }

    #[test]
    fn transducer_models_are_flagged() {
        for id in [PARAKEET_TDT_06B_MODEL_ID, PARAKEET_UNIFIED_EN_MODEL_ID] {
            assert!(is_transducer_model(id), "{id}");
            assert!(!is_whisper_model(id), "{id}");
        }
        assert!(!is_transducer_model(STT_MODEL_ID));
        assert!(!is_transducer_model(WHISPER_TINY_MODEL_ID));
        assert!(!is_transducer_model("bogus"));
    }

    #[test]
    fn transducer_install_state_detects_files() {
        let tmp = std::env::temp_dir().join("opendictate-test-transducer");
        let model_dir = tmp.join("opendictate").join("models").join(PARAKEET_TDT_06B_MODEL_ID);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("encoder.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("decoder.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("joiner.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("tokens.txt"), vec![0u8; 200]).unwrap();
        std::env::set_var("XDG_DATA_HOME", &tmp);
        let catalog = catalog();
        let tdt = catalog.iter().find(|m| m.id == PARAKEET_TDT_06B_MODEL_ID).unwrap();
        assert!(tdt.installed);
        assert_eq!(tdt.disk_bytes, 6_000_200);
        let unified = catalog
            .iter()
            .find(|m| m.id == PARAKEET_UNIFIED_EN_MODEL_ID)
            .unwrap();
        assert!(!unified.installed);
        std::env::remove_var("XDG_DATA_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn catalog_serializes_snake_case_keys() {
        let json = serde_json::to_string(&catalog()).unwrap();
        assert!(json.contains("\"size_bytes\""));
        assert!(json.contains("\"disk_bytes\""));
        assert!(json.contains("\"engine_key\""));
        assert!(!json.contains("sizeBytes"));
        assert!(!json.contains("diskBytes"));
        assert!(!json.contains("engineKey"));
    }

    #[test]
    fn installed_state_matches_disk() {
        let tmp = std::env::temp_dir().join("opendictate-test-installed");
        let model_dir = tmp.join("opendictate").join("models").join(DEFAULT_MODEL);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("model.int8.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("tokens.txt"), vec![0u8; 200]).unwrap();
        std::env::set_var("XDG_DATA_HOME", &tmp);
        let catalog = catalog();
        let parakeet = catalog.iter().find(|m| m.id == STT_MODEL_ID).unwrap();
        assert!(parakeet.installed);
        assert_eq!(parakeet.disk_bytes, 2_000_200);
        let whisper = catalog
            .iter()
            .find(|m| m.id == WHISPER_TINY_MODEL_ID)
            .unwrap();
        assert!(!whisper.installed);
        assert_eq!(whisper.disk_bytes, 0);
        std::env::remove_var("XDG_DATA_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
