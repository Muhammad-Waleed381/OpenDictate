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
pub const OMNILINGUAL_ASR_MODEL_ID: &str = "omnilingual-asr-1600-languages-300m-ctc-int8";
pub const COHERE_TRANSCRIBE_MODEL_ID: &str = "cohere-transcribe-14-lang-int8";

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
const OMNILINGUAL_ASR_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-omnilingual-asr-1600-languages-300M-ctc-int8-2025-11-12.tar.bz2";
const COHERE_TRANSCRIBE_ARCHIVE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-cohere-transcribe-14-lang-int8-2026-04-01.tar.bz2";

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
    pub company: Option<String>,
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
    company: Option<&'static str>,
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
        company: Some("NVIDIA"),
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
        company: Some("Next-gen Kaldi"),
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
        company: Some("Silero"),
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
        company: Some("Next-gen Kaldi"),
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
        company: Some("NVIDIA"),
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
        company: Some("NVIDIA"),
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
        company: Some("OpenAI"),
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
        company: Some("OpenAI"),
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
        company: Some("NVIDIA"),
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
        company: Some("NVIDIA"),
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
        company: Some("OpenAI"),
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
        company: Some("OpenAI"),
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
        company: Some("OpenAI"),
        size_bytes: 1_905_872_689,
        url: WHISPER_MEDIUM_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: OMNILINGUAL_ASR_MODEL_ID,
        name: "Omnilingual ASR 300M (1600+ Languages)",
        kind: "stt",
        engine_key: Some("omnilingual"),
        company: Some("Meta"),
        size_bytes: 292_571_207,
        url: OMNILINGUAL_ASR_ARCHIVE,
        available: true,
        streaming: false,
    },
    ModelDef {
        id: COHERE_TRANSCRIBE_MODEL_ID,
        name: "Cohere Transcribe 2B (int8)",
        kind: "stt",
        engine_key: Some("cohere"),
        company: Some("Cohere"),
        size_bytes: 1_699_791_751,
        url: COHERE_TRANSCRIBE_ARCHIVE,
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
    #[cfg(test)]
    if let Some(root) = TEST_MODEL_ROOT.with(|root| root.borrow().clone()) {
        return root;
    }

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
            return PathBuf::from(home)
                .join(".local/share")
                .join("opendictate")
                .join("models");
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
    matches!(id, PARAKEET_TDT_06B_MODEL_ID | PARAKEET_UNIFIED_EN_MODEL_ID)
}

pub fn is_omnilingual_model(id: &str) -> bool {
    id == OMNILINGUAL_ASR_MODEL_ID
}

pub fn is_cohere_model(id: &str) -> bool {
    id == COHERE_TRANSCRIBE_MODEL_ID
}

fn valid_file_size(path: &Path, min_bytes: u64) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    (metadata.is_file() && metadata.len() >= min_bytes).then_some(metadata.len())
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
    if !dir.exists() {
        return false;
    }
    let has_part = |needle: &str| -> bool {
        if valid_file_size(&dir.join(format!("{needle}.onnx")), WHISPER_PART_MIN_BYTES).is_some() {
            return true;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            entries.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.ends_with(".onnx")
                    && n.contains(needle)
                    && valid_file_size(&e.path(), WHISPER_PART_MIN_BYTES).is_some()
            })
        } else {
            false
        }
    };
    let has_tokens = valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
        || std::fs::read_dir(&dir).map_or(false, |entries| {
            entries.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.contains("tokens")
                    && n.ends_with(".txt")
                    && valid_file_size(&e.path(), TOKENS_MIN_BYTES).is_some()
            })
        });

    has_part("encoder") && has_part("decoder") && has_tokens
}

fn is_transducer_ready(id: &str) -> bool {
    let dir = model_dir_for(id);
    valid_file_size(&dir.join("encoder.onnx"), MODEL_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("decoder.onnx"), STREAMING_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("joiner.onnx"), STREAMING_PART_MIN_BYTES).is_some()
        && valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
}

pub fn is_omnilingual_ready(id: &str) -> bool {
    is_nemo_ctc_ready(id)
}

pub fn is_cohere_ready(id: &str) -> bool {
    let dir = model_dir_for(id);
    if !dir.exists() {
        return false;
    }
    let has_part = |needle: &str| -> bool {
        if valid_file_size(&dir.join(format!("{needle}.int8.onnx")), MODEL_MIN_BYTES).is_some()
            || valid_file_size(&dir.join(format!("{needle}.onnx")), MODEL_MIN_BYTES).is_some()
        {
            return true;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            entries.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.ends_with(".onnx")
                    && n.contains(needle)
                    && valid_file_size(&e.path(), MODEL_MIN_BYTES).is_some()
            })
        } else {
            false
        }
    };
    let has_tokens = valid_file_size(&dir.join("tokens.txt"), TOKENS_MIN_BYTES).is_some()
        || std::fs::read_dir(&dir).map_or(false, |entries| {
            entries.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.contains("tokens")
                    && n.ends_with(".txt")
                    && valid_file_size(&e.path(), TOKENS_MIN_BYTES).is_some()
            })
        });

    has_part("encoder") && has_part("decoder") && has_tokens
}

pub fn kws_model_dir() -> PathBuf {
    models_dir().join(KWS_MODEL_ID)
}

/// The transducer parts a keyword spotter needs. A file whose name mentions
/// more than one of them is ambiguous and must not be selected for either.
const KWS_PART_NAMES: [&str; 3] = ["encoder", "decoder", "joiner"];

pub(crate) fn kws_model_file(dir: &Path, needle: &str) -> Option<PathBuf> {
    let mut exact: Vec<PathBuf> = Vec::new();
    let mut partial: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let name = name.to_lowercase();
        if !name.ends_with(".onnx") || valid_file_size(&path, STREAMING_PART_MIN_BYTES).is_none() {
            continue;
        }
        if name == format!("{needle}.onnx") {
            exact.push(path);
        } else if name.contains(needle)
            && !KWS_PART_NAMES
                .iter()
                .any(|other| *other != needle && name.contains(other))
        {
            partial.push(path);
        }
    }
    let by_preference = |mut paths: Vec<PathBuf>| {
        paths.sort_by_key(|path| {
            (
                !path.to_string_lossy().to_lowercase().contains(".int8."),
                path.clone(),
            )
        });
        paths.into_iter().next()
    };
    // The canonical `<part>.onnx` name always wins over a substring match.
    by_preference(exact).or_else(|| by_preference(partial))
}

pub(crate) fn valid_tokens_file(path: &Path) -> bool {
    valid_file_size(path, TOKENS_MIN_BYTES).is_some()
}

pub fn is_kws_ready() -> bool {
    let dir = kws_model_dir();
    valid_tokens_file(&dir.join("tokens.txt"))
        && ["encoder", "decoder", "joiner"]
            .iter()
            .all(|part| kws_model_file(&dir, part).is_some())
}

pub fn is_vad_ready() -> bool {
    valid_file_size(&vad_model_path(), VAD_MIN_BYTES).is_some()
}

pub fn is_stt_model_ready() -> bool {
    MODELS
        .iter()
        .any(|m| m.kind == "stt" && is_model_installed(m.id))
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
        OMNILINGUAL_ASR_MODEL_ID => is_omnilingual_ready(id),
        COHERE_TRANSCRIBE_MODEL_ID => is_cohere_ready(id),
        _ => false,
    }
}

fn dir_size(path: &Path, out: &mut u64) {
    if let Ok(rd) = std::fs::read_dir(path) {
        for entry in rd.flatten() {
            let p = entry.path();
            // Never follow links: an archive may contain a link that points
            // outside the model directory.
            match std::fs::symlink_metadata(&p) {
                Ok(meta) if meta.is_dir() => dir_size(&p, out),
                Ok(meta) if meta.is_file() => {
                    if let Ok(meta) = std::fs::metadata(&p) {
                        *out += meta.len();
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn model_disk_bytes(id: &str) -> u64 {
    if model_def(id).is_none() {
        return 0;
    }
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
            company: def.company.map(|s| s.to_string()),
            size_bytes: def.size_bytes,
            disk_bytes: model_disk_bytes(def.id),
            installed: is_model_installed(def.id),
            available: def.available,
            streaming: def.streaming,
        })
        .collect()
}

/// Copies `src` to `dst` without ever writing through a link that already
/// exists at the destination. A planted symlink would otherwise turn the
/// install into an arbitrary-file overwrite.
fn copy_model_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    // `remove_file` unlinks the destination itself and never follows it.
    let _ = std::fs::remove_file(dst);
    let mut input = std::fs::File::open(src)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    std::io::copy(&mut input, &mut output)?;
    Ok(())
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
    download_with_timeout(
        url,
        dest,
        on_progress,
        is_cancelled,
        std::time::Duration::from_secs(5),
    )
}

fn download_with_timeout(
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64, u64),
    is_cancelled: &dyn Fn() -> bool,
    inactivity: std::time::Duration,
) -> Result<()> {
    if is_cancelled() {
        return Err(CoreError::Download(
            "download cancelled by user".to_string(),
        ));
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(CoreError::Io)?;
        }
    }

    let (agent, body_started) = super::download::agent_with_inactivity_timeout(inactivity);

    let user_agent = concat!(
        "OpenDictate/",
        env!("CARGO_PKG_VERSION"),
        " (Desktop; Tauri)"
    );
    let response = agent
        .get(url)
        .header("User-Agent", user_agent)
        .call()
        .map_err(|e| {
            if is_cancelled() {
                CoreError::Download("download cancelled by user".into())
            } else {
                CoreError::Download(format!("failed to fetch {url}: {e}"))
            }
        })?;
    body_started.store(true, std::sync::atomic::Ordering::Relaxed);

    if is_cancelled() {
        return Err(CoreError::Download(
            "download cancelled by user".to_string(),
        ));
    }

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Immediately emit initial progress so the UI progress bar appears right away.
    on_progress(0, total);

    if is_cancelled() {
        return Err(CoreError::Download(
            "download cancelled by user".to_string(),
        ));
    }

    let part_name = match dest.file_name() {
        Some(name) => format!("{}.part", name.to_string_lossy()),
        None => "download.part".to_string(),
    };
    let part_path = dest.with_file_name(part_name);

    let mut body = response.into_body().into_reader();
    // O_EXCL: never follow a link planted at the part path. A stale part file
    // from an earlier attempt is removed and the exclusive open is retried.
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(&part_path)?;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&part_path)?
        }
        Err(error) => return Err(error.into()),
    };
    let mut received = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    let mut last_progress_emit = std::time::Instant::now();
    let mut last_emitted_bytes = 0u64;

    let download_result = (|| -> Result<()> {
        loop {
            if is_cancelled() {
                return Err(CoreError::Download(
                    "download cancelled by user".to_string(),
                ));
            }
            let n = match std::io::Read::read(&mut body, &mut chunk) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    if is_cancelled() {
                        return Err(CoreError::Download(
                            "download cancelled by user".to_string(),
                        ));
                    }
                    return Err(CoreError::Io(e));
                }
            };
            if is_cancelled() {
                return Err(CoreError::Download(
                    "download cancelled by user".to_string(),
                ));
            }
            file.write_all(&chunk[..n])?;
            received += n as u64;

            let elapsed = last_progress_emit.elapsed();
            if elapsed.as_millis() >= 50
                || received - last_emitted_bytes >= 256 * 1024
                || (total > 0 && received >= total)
            {
                on_progress(received, total);
                last_progress_emit = std::time::Instant::now();
                last_emitted_bytes = received;
            }

            if received.is_multiple_of(4 * 1024 * 1024) {
                log::info!("downloaded {received} bytes -> {}", dest.display());
            }
        }
        file.flush()?;
        Ok(())
    })();

    drop(file);
    drop(body);

    if let Err(err) = download_result {
        let _ = std::fs::remove_file(&part_path);
        return Err(err);
    }

    if is_cancelled() {
        let _ = std::fs::remove_file(&part_path);
        return Err(CoreError::Download(
            "download cancelled by user".to_string(),
        ));
    }

    on_progress(received, total);

    if let Err(e) = std::fs::rename(&part_path, dest) {
        log::warn!(
            "rename {} to {} failed ({e}), falling back to copy",
            part_path.display(),
            dest.display()
        );
        if let Err(copy_err) = copy_model_file(&part_path, dest) {
            let _ = std::fs::remove_file(&part_path);
            return Err(CoreError::Io(copy_err));
        }
        let _ = std::fs::remove_file(&part_path);
    }

    log::info!("downloaded {received} bytes -> {}", dest.display());
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            // Never follow links: an archive may contain a link that points
            // outside the extraction root, and the install selectors must
            // only ever see regular files inside it.
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => collect_files(&path, out),
                Ok(meta) if meta.is_file() => out.push(path),
                _ => {}
            }
        }
    }
}

fn extract_archive(
    archive_path: &Path,
    dest: &Path,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    if is_cancelled() {
        return Err(CoreError::Download(
            "download cancelled by user".to_string(),
        ));
    }
    std::fs::create_dir_all(dest).map_err(CoreError::Io)?;
    let file = File::open(archive_path).map_err(CoreError::Io)?;
    let mut archive = Archive::new(BzDecoder::new(file));
    let entries = archive.entries().map_err(|e| {
        CoreError::Download(format!(
            "failed to read archive entries {}: {e}",
            archive_path.display()
        ))
    })?;
    for entry in entries {
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }
        let mut entry = entry.map_err(|e| {
            CoreError::Download(format!("corrupt entry in {}: {e}", archive_path.display()))
        })?;
        // Never materialise archive-controlled links. `unpack_in` validates the
        // entry path but not the link target, so a symlink entry could point
        // outside the extraction root and the post-extraction walkers would
        // follow it.
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            log::warn!(
                "skipping link entry in {}: {}",
                archive_path.display(),
                entry
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "<unknown>".to_string())
            );
            continue;
        }
        entry.unpack_in(dest).map_err(|e| {
            CoreError::Download(format!("failed to unpack entry in {}: {e}", dest.display()))
        })?;
    }
    if is_cancelled() {
        return Err(CoreError::Download(
            "download cancelled by user".to_string(),
        ));
    }
    Ok(())
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

    let res = (|| -> Result<bool> {
        for url in [PARAKEET_INT8_ARCHIVE, PARAKEET_ARCHIVE] {
            if is_cancelled() {
                return Err(CoreError::Download(
                    "download cancelled by user".to_string(),
                ));
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
                    return Err(CoreError::Download(
                        "download cancelled by user".to_string(),
                    ));
                }
                log::warn!("download failed for {url}, trying next archive");
                continue;
            }
            if extract_dir.exists() {
                std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
            }
            if let Err(e) = extract_archive(&archive_path, &extract_dir, is_cancelled) {
                if is_cancelled() {
                    return Err(CoreError::Download(
                        "download cancelled by user".to_string(),
                    ));
                }
                log::warn!("extraction failed: {e}");
                continue;
            }

            let mut files = Vec::new();
            collect_files(&extract_dir, &mut files);
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
                if is_cancelled() {
                    return Err(CoreError::Download(
                        "download cancelled by user".to_string(),
                    ));
                }
                let model_dir = stt_model_dir();
                std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
                let model_name = model.file_name().unwrap_or_default();
                copy_model_file(model, &model_dir.join(model_name)).map_err(CoreError::Io)?;
                copy_model_file(tokens, &model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
                log::info!(
                    "installed {} + tokens.txt -> {}",
                    model_name.to_string_lossy(),
                    model_dir.display()
                );
                return Ok(true);
            }
            log::warn!("archive {} has no recognizer files, trying next", url);
        }
        Ok(false)
    })();

    clean_after(&archive_path, &extract_dir);
    if res.is_err() && is_cancelled() {
        let _ = remove_model_unlocked(STT_MODEL_ID);
    }
    res
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

    let res = (|| -> Result<()> {
        log::info!("downloading {} -> {}", def.url, archive_path.display());
        download_to_with_progress_cancel(
            def.url,
            &archive_path,
            &mut |received, total| on_progress(id, received, total),
            is_cancelled,
        )?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
        }
        extract_archive(&archive_path, &extract_dir, is_cancelled)?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

        let mut files = Vec::new();
        collect_files(&extract_dir, &mut files);
        let find_part = |needle: &str| {
            let mut hits: Vec<_> = files
                .iter()
                .filter(|p| {
                    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                        let lower = n.to_lowercase();
                        lower.ends_with(".onnx")
                            && lower.contains(needle)
                            && valid_file_size(p, WHISPER_PART_MIN_BYTES).is_some()
                    })
                })
                .cloned()
                .collect();
            hits.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
            hits.first().cloned()
        };

        let encoder = find_part("encoder");
        let decoder = find_part("decoder");
        let tokens = files
            .iter()
            .find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    let lower = n.to_lowercase();
                    lower.contains("tokens") && lower.ends_with(".txt")
                })
            })
            .cloned();

        let (Some(encoder), Some(decoder), Some(tokens)) = (encoder, decoder, tokens) else {
            return Err(CoreError::Download(format!(
                "archive {id} has no encoder/decoder/tokens files"
            )));
        };

        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

        let model_dir = model_dir_for(id);
        std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
        let move_or_copy = |src: &Path, dst: &Path| -> std::io::Result<()> {
            if std::fs::rename(src, dst).is_err() {
                copy_model_file(src, dst)?;
            }
            Ok(())
        };
        move_or_copy(&encoder, &model_dir.join("encoder.onnx")).map_err(CoreError::Io)?;
        move_or_copy(&decoder, &model_dir.join("decoder.onnx")).map_err(CoreError::Io)?;
        move_or_copy(&tokens, &model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
        log::info!("installed {id} -> {}", model_dir.display());
        Ok(())
    })();

    clean_after(&archive_path, &extract_dir);
    if res.is_err() {
        let _ = remove_model_unlocked(id);
    }
    res
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

    let res = (|| -> Result<()> {
        log::info!("downloading {} -> {}", def.url, archive_path.display());
        download_to_with_progress_cancel(
            def.url,
            &archive_path,
            &mut |received, total| on_progress(id, received, total),
            is_cancelled,
        )?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
        }
        extract_archive(&archive_path, &extract_dir, is_cancelled)?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

        let mut files = Vec::new();
        collect_files(&extract_dir, &mut files);
        let part = |needle: &str| {
            let mut hits: Vec<_> = files
                .iter()
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".onnx") && n.contains(needle))
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
            return Err(CoreError::Download(format!(
                "archive {id} has no encoder/decoder/joiner/tokens files"
            )));
        };

        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

        let model_dir = model_dir_for(id);
        std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
        copy_model_file(&encoder, &model_dir.join("encoder.onnx")).map_err(CoreError::Io)?;
        copy_model_file(&decoder, &model_dir.join("decoder.onnx")).map_err(CoreError::Io)?;
        copy_model_file(&joiner, &model_dir.join("joiner.onnx")).map_err(CoreError::Io)?;
        copy_model_file(&tokens, &model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
        if let Some(vocab) = files
            .iter()
            .find(|p| p.file_name().is_some_and(|name| name == "bpe.vocab"))
        {
            copy_model_file(vocab, &model_dir.join("bpe.vocab")).map_err(CoreError::Io)?;
        }
        log::info!("installed {id} -> {}", model_dir.display());
        Ok(())
    })();

    clean_after(&archive_path, &extract_dir);
    if res.is_err() && is_cancelled() {
        let _ = remove_model_unlocked(id);
    }
    res
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

    let res = (|| -> Result<()> {
        log::info!("downloading {} -> {}", def.url, archive_path.display());
        download_to_with_progress_cancel(
            def.url,
            &archive_path,
            &mut |received, total| on_progress(id, received, total),
            is_cancelled,
        )?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
        }
        extract_archive(&archive_path, &extract_dir, is_cancelled)?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

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
            return Err(CoreError::Download(format!(
                "archive {id} has no model/tokens files"
            )));
        };

        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

        let model_dir = model_dir_for(id);
        std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
        let model_name = model.file_name().unwrap_or_default().to_owned();
        copy_model_file(&model, &model_dir.join(model_name)).map_err(CoreError::Io)?;
        copy_model_file(&tokens, &model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
        log::info!("installed {id} -> {}", model_dir.display());
        Ok(())
    })();

    clean_after(&archive_path, &extract_dir);
    if res.is_err() && is_cancelled() {
        let _ = remove_model_unlocked(id);
    }
    res
}

fn install_cohere_model(
    id: &str,
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let def = model_def(id).ok_or_else(|| CoreError::Download(format!("unknown model '{id}'")))?;
    let base = models_dir();
    std::fs::create_dir_all(&base).map_err(CoreError::Io)?;
    let archive_path = base.join(format!("{id}.tar.bz2"));
    let extract_dir = base.join(format!(".extract-{id}"));

    let res = (|| -> Result<()> {
        log::info!("downloading {} -> {}", def.url, archive_path.display());
        download_to_with_progress_cancel(
            def.url,
            &archive_path,
            &mut |received, total| on_progress(id, received, total),
            is_cancelled,
        )?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir).map_err(CoreError::Io)?;
        }
        extract_archive(&archive_path, &extract_dir, is_cancelled)?;
        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

        let mut files = Vec::new();
        collect_files(&extract_dir, &mut files);
        let find_part = |needle: &str| {
            let mut hits: Vec<_> = files
                .iter()
                .filter(|p| {
                    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                        let lower = n.to_lowercase();
                        lower.ends_with(".onnx")
                            && lower.contains(needle)
                            && valid_file_size(p, MODEL_MIN_BYTES).is_some()
                    })
                })
                .cloned()
                .collect();
            hits.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
            hits.first().cloned()
        };

        let encoder = find_part("encoder");
        let decoder = find_part("decoder");
        let tokens = files
            .iter()
            .find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    let lower = n.to_lowercase();
                    lower.contains("tokens") && lower.ends_with(".txt")
                })
            })
            .cloned();

        let (Some(encoder), Some(decoder), Some(tokens)) = (encoder, decoder, tokens) else {
            return Err(CoreError::Download(format!(
                "archive {id} has no encoder/decoder/tokens files"
            )));
        };

        if is_cancelled() {
            return Err(CoreError::Download(
                "download cancelled by user".to_string(),
            ));
        }

        let model_dir = model_dir_for(id);
        std::fs::create_dir_all(&model_dir).map_err(CoreError::Io)?;
        let move_or_copy = |src: &Path, dst: &Path| -> std::io::Result<()> {
            if std::fs::rename(src, dst).is_err() {
                copy_model_file(src, dst)?;
            }
            Ok(())
        };
        let enc_name = encoder.file_name().unwrap_or_default().to_owned();
        let dec_name = decoder.file_name().unwrap_or_default().to_owned();
        move_or_copy(&encoder, &model_dir.join(enc_name)).map_err(CoreError::Io)?;
        move_or_copy(&decoder, &model_dir.join(dec_name)).map_err(CoreError::Io)?;
        move_or_copy(&tokens, &model_dir.join("tokens.txt")).map_err(CoreError::Io)?;
        log::info!("installed {id} -> {}", model_dir.display());
        Ok(())
    })();

    clean_after(&archive_path, &extract_dir);
    if res.is_err() && is_cancelled() {
        let _ = remove_model_unlocked(id);
    }
    res
}

pub fn ensure_model(id: &str, on_progress: &mut dyn FnMut(&str, u64, u64)) -> Result<()> {
    ensure_model_with_cancel(id, on_progress, &|| false)
}

pub fn ensure_model_with_cancel(
    id: &str,
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    ensure_with_install(id, is_cancelled, || {
        ensure_model_unlocked(id, on_progress, is_cancelled)
    })
}

fn ensure_with_install(
    id: &str,
    is_cancelled: &dyn Fn() -> bool,
    install: impl FnOnce() -> Result<()>,
) -> Result<()> {
    with_model_operation(id, is_cancelled, || {
        if is_model_installed(id) {
            return Ok(());
        }
        install()
    })
}

#[derive(Default)]
struct ModelOperations {
    active: std::sync::Mutex<std::collections::HashSet<PathBuf>>,
    changed: std::sync::Condvar,
}
static MODEL_OPERATIONS: std::sync::OnceLock<ModelOperations> = std::sync::OnceLock::new();
struct ModelLease {
    path: PathBuf,
    ownership: &'static ModelOperations,
}
impl Drop for ModelLease {
    fn drop(&mut self) {
        self.ownership
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.path);
        self.ownership.changed.notify_all();
    }
}

fn checked_model_path(id: &str) -> Result<PathBuf> {
    model_def(id).ok_or_else(|| CoreError::Download(format!("unknown model '{id}'")))?;
    let base = models_dir();
    let path = if id == VAD_MODEL_ID {
        vad_model_path()
    } else {
        model_dir_for(id)
    };
    if path.parent() != Some(base.as_path()) {
        return Err(CoreError::Download(
            "model path is outside model storage".into(),
        ));
    }
    // Do not follow user-created replacement links during install or removal.
    let mut artifacts = vec![path.clone()];
    if id == VAD_MODEL_ID {
        artifacts.push(base.join(format!("{VAD_FILENAME}.part")));
    } else if id == STT_MODEL_ID {
        artifacts.extend([
            base.join("parakeet-tdt-ctc-110m.tar.bz2"),
            base.join("parakeet-tdt-ctc-110m.tar.bz2.part"),
            base.join(".extract"),
        ]);
    } else {
        artifacts.extend([
            base.join(format!("{id}.tar.bz2")),
            base.join(format!("{id}.tar.bz2.part")),
            base.join(format!(".extract-{id}")),
        ]);
    }
    for artifact in artifacts {
        if std::fs::symlink_metadata(&artifact).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(CoreError::Download(format!(
                "model path is a symbolic link: {}",
                artifact.display()
            )));
        }
    }
    if path.exists() {
        let root = std::fs::canonicalize(&base)?;
        if !std::fs::canonicalize(&path)?.starts_with(root) {
            return Err(CoreError::Download(
                "model path resolves outside model storage".into(),
            ));
        }
    }
    Ok(path)
}

/// How long a caller waits for another operation on the same model to finish
/// before giving up. Without a bound a stalled install would block a removal
/// forever and leak the blocking worker thread.
const MODEL_OPERATION_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

fn with_model_operation<T>(
    id: &str,
    is_cancelled: &dyn Fn() -> bool,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let path = checked_model_path(id)?;
    // The cancellation predicate is caller-supplied. Never run it while the
    // global ownership mutex is held.
    if is_cancelled() {
        return Err(CoreError::Download("download cancelled by user".into()));
    }
    let ownership = MODEL_OPERATIONS.get_or_init(Default::default);
    let deadline = std::time::Instant::now() + MODEL_OPERATION_WAIT_TIMEOUT;
    loop {
        let mut active = ownership.active.lock().unwrap_or_else(|e| e.into_inner());
        if active.insert(path.clone()) {
            drop(active);
            break;
        }
        // Wait for the current holder, then re-check cancellation with the
        // lock released.
        let (guard, _timed_out) = ownership
            .changed
            .wait_timeout(active, std::time::Duration::from_millis(50))
            .unwrap_or_else(|e| e.into_inner());
        drop(guard);
        if is_cancelled() {
            return Err(CoreError::Download("download cancelled by user".into()));
        }
        if std::time::Instant::now() >= deadline {
            return Err(CoreError::Download(format!(
                "another operation for '{id}' is still in progress"
            )));
        }
    }
    let _lease = ModelLease { path, ownership };
    // Another operation may have replaced the path while we waited.
    checked_model_path(id)?;
    operation()
}

fn ensure_model_unlocked(
    id: &str,
    on_progress: &mut dyn FnMut(&str, u64, u64),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    if is_cancelled() {
        return Err(CoreError::Download(
            "download cancelled by user".to_string(),
        ));
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
                return Err(CoreError::Download(
                    "download cancelled by user".to_string(),
                ));
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
        | WHISPER_MEDIUM_MODEL_ID => install_whisper_model(id, on_progress, is_cancelled),
        OMNILINGUAL_ASR_MODEL_ID => {
            install_nemo_ctc_model(id, on_progress, is_cancelled)
        }
        COHERE_TRANSCRIBE_MODEL_ID => {
            install_cohere_model(id, on_progress, is_cancelled)
        }
        other => Err(CoreError::Download(format!("unknown model '{other}'"))),
    }
}

pub fn remove_model(id: &str) -> Result<()> {
    remove_model_with_cancel(id, &|| false)
}

/// Removes a model, honouring a cancellation signal while waiting for another
/// operation on the same model to finish.
pub fn remove_model_with_cancel(id: &str, is_cancelled: &dyn Fn() -> bool) -> Result<()> {
    with_model_operation(id, is_cancelled, || remove_model_unlocked(id))
}

fn remove_model_unlocked(id: &str) -> Result<()> {
    let path = checked_model_path(id)?;
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
thread_local! {
    static TEST_MODEL_ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
struct TestModelRoot(Option<PathBuf>);
#[cfg(test)]
impl TestModelRoot {
    fn new(data_home: &Path) -> Self {
        Self(TEST_MODEL_ROOT.with(|root| root.replace(Some(data_home.join("opendictate/models")))))
    }
}
#[cfg(test)]
impl Drop for TestModelRoot {
    fn drop(&mut self) {
        TEST_MODEL_ROOT.with(|root| root.replace(self.0.take()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn fixture(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "opendictate-{name}-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ))
    }

    fn accept_fixture(listener: std::net::TcpListener) -> std::net::TcpStream {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    return stream;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(error) => panic!("HTTP fixture accept failed: {error}"),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn catalog_paths_cannot_follow_replacement_symlinks() {
        let tmp = fixture("model-symlink");
        let _root = TestModelRoot::new(&tmp);
        std::fs::create_dir_all(models_dir()).unwrap();
        let outside = tmp.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep"), "original").unwrap();
        std::os::unix::fs::symlink(&outside, model_dir_for(STT_MODEL_ID)).unwrap();
        assert!(remove_model(STT_MODEL_ID).is_err());
        assert!(ensure_model(STT_MODEL_ID, &mut |_, _, _| panic!(
            "must not download through a link"
        ))
        .is_err());
        assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"original");
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn failed_download_cleanup_cannot_remove_waiting_replacement() {
        let tmp = fixture("failed-install-retry");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut stream = accept_fixture(listener);
            let _ = std::io::Read::read(&mut stream, &mut [0; 2048]);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\npartial");
            std::thread::sleep(std::time::Duration::from_millis(400));
        });
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let owner_tmp = tmp.clone();
        let owner = std::thread::spawn(move || {
            let _root = TestModelRoot::new(&owner_tmp);
            let result = with_model_operation(VAD_MODEL_ID, &|| false, || {
                let result = download_with_timeout(
                    &url,
                    &vad_model_path(),
                    &mut |_, _| {
                        let _ = started_tx.send(());
                    },
                    &|| false,
                    std::time::Duration::from_millis(100),
                );
                if result.is_err() {
                    remove_model_unlocked(VAD_MODEL_ID)?;
                }
                result
            });
            assert!(result.is_err());
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let retry_tmp = tmp.clone();
        let retry = std::thread::spawn(move || {
            let _root = TestModelRoot::new(&retry_tmp);
            with_model_operation(VAD_MODEL_ID, &|| false, || {
                std::fs::create_dir_all(models_dir())?;
                File::create(vad_model_path())?.set_len(VAD_MIN_BYTES)?;
                Ok(())
            })
            .unwrap();
        });
        owner.join().unwrap();
        retry.join().unwrap();
        server.join().unwrap();
        let _root = TestModelRoot::new(&tmp);
        assert!(
            is_vad_ready(),
            "old failure cleanup must not remove the replacement"
        );
        assert!(!models_dir().join(format!("{VAD_FILENAME}.part")).exists());
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn waiting_ensure_rechecks_readiness_after_install_finishes() {
        let tmp = fixture("ensure-recheck");
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let owner_tmp = tmp.clone();
        let owner = std::thread::spawn(move || {
            let _root = TestModelRoot::new(&owner_tmp);
            with_model_operation(VAD_MODEL_ID, &|| false, || {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
                std::fs::create_dir_all(models_dir())?;
                File::create(vad_model_path())?.set_len(VAD_MIN_BYTES)?;
                Ok(())
            })
            .unwrap();
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let (waiting_tx, waiting_rx) = std::sync::mpsc::channel();
        let waiter_tmp = tmp.clone();
        let waiter = std::thread::spawn(move || {
            let _root = TestModelRoot::new(&waiter_tmp);
            ensure_with_install(
                VAD_MODEL_ID,
                &|| {
                    let _ = waiting_tx.send(());
                    false
                },
                || panic!("already-installed model must not be downloaded again"),
            )
            .unwrap();
        });
        waiting_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        release_tx.send(()).unwrap();
        owner.join().unwrap();
        waiter.join().unwrap();
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn progressing_download_can_outlast_inactivity_timeout() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut stream = accept_fixture(listener);
            let _ = std::io::Read::read(&mut stream, &mut [0; 2048]);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n")
                .unwrap();
            for _ in 0..8 {
                stream.write_all(b"x").unwrap();
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
        });
        let tmp = fixture("slow-success");
        let dest = tmp.join("model");
        download_with_timeout(
            &url,
            &dest,
            &mut |_, _| {},
            &|| false,
            std::time::Duration::from_millis(150),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), b"xxxxxxxx");
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn concurrent_fixture_roots_are_independent_and_restore() {
        let original = models_dir();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let tmp = fixture("parallel-root");
                    let _root = TestModelRoot::new(&tmp);
                    barrier.wait();
                    assert_eq!(models_dir(), tmp.join("opendictate/models"));
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(models_dir(), original);
    }

    #[test]
    fn core_removal_waits_for_inflight_install_ownership() {
        let tmp = fixture("install-lock");
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let owner_tmp = tmp.clone();
        let owner = std::thread::spawn(move || {
            let _root = TestModelRoot::new(&owner_tmp);
            with_model_operation(VAD_MODEL_ID, &|| false, || {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
                Ok(())
            })
            .unwrap();
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let remover = std::thread::spawn(move || {
            let _root = TestModelRoot::new(&tmp);
            done_tx.send(remove_model(VAD_MODEL_ID)).unwrap();
        });
        let early = done_rx
            .recv_timeout(std::time::Duration::from_millis(80))
            .is_ok();
        release_tx.send(()).unwrap();
        owner.join().unwrap();
        remover.join().unwrap();
        assert!(
            !early,
            "removal must not enter an active installation's directory"
        );
    }

    #[test]
    fn stalled_download_headers_and_body_have_bounded_waits() {
        for headers in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/model", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let mut stream = accept_fixture(listener);
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                    .unwrap();
                let _ = std::io::Read::read(&mut stream, &mut [0; 2048]);
                if headers {
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\n");
                }
                std::thread::sleep(std::time::Duration::from_millis(700));
            });
            let tmp = fixture("stall-download");
            let dest = tmp.join("model");
            let start = std::time::Instant::now();
            let result = download_with_timeout(
                &url,
                &dest,
                &mut |_, _| {},
                &|| false,
                std::time::Duration::from_millis(100),
            );
            let elapsed = start.elapsed();
            server.join().unwrap();
            let _ = std::fs::remove_dir_all(tmp);
            assert!(result.is_err());
            assert!(
                elapsed < std::time::Duration::from_millis(500),
                "headers={headers}, elapsed={elapsed:?}"
            );
        }
    }

    #[test]
    fn rejects_removal_outside_catalog_before_touching_files() {
        let outside = fixture("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep"), "untouched").unwrap();
        let result = remove_model(outside.to_str().unwrap());
        let preserved = outside.join("keep").exists();
        let _ = std::fs::remove_dir_all(&outside);
        assert!(
            result.is_err(),
            "non-catalog absolute paths must be rejected"
        );
        assert!(preserved, "external fixture must remain intact");
        assert!(remove_model("../outside").is_err());
        assert!(remove_model("").is_err());
    }

    #[test]
    fn kws_rejects_empty_versioned_and_canonical_candidates() {
        let tmp = fixture("kws-empty");
        let dir = tmp.join("opendictate/models").join(KWS_MODEL_ID);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tokens.txt"), vec![b'a'; 200]).unwrap();
        for name in ["encoder.onnx", "decoder.int8.onnx", "joiner-epoch-12.onnx"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        let _root = TestModelRoot::new(&tmp);
        let ready = is_kws_ready();
        let _ = std::fs::remove_dir_all(tmp);
        assert!(!ready, "zero-byte candidates must not report installed");
    }

    #[test]
    fn model_ready_requires_files() {
        let tmp = fixture("ready");
        let _root = TestModelRoot::new(&tmp);
        let result = is_stt_model_ready();
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
        assert!(catalog
            .iter()
            .filter(|m| m.streaming)
            .all(|m| is_streaming_model(&m.id)));
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
        let tmp = fixture("transducer");
        let model_dir = tmp
            .join("opendictate")
            .join("models")
            .join(PARAKEET_TDT_06B_MODEL_ID);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("encoder.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("decoder.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("joiner.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("tokens.txt"), vec![0u8; 200]).unwrap();
        let _root = TestModelRoot::new(&tmp);
        let catalog = catalog();
        let tdt = catalog
            .iter()
            .find(|m| m.id == PARAKEET_TDT_06B_MODEL_ID)
            .unwrap();
        assert!(tdt.installed);
        assert_eq!(tdt.disk_bytes, 6_000_200);
        let unified = catalog
            .iter()
            .find(|m| m.id == PARAKEET_UNIFIED_EN_MODEL_ID)
            .unwrap();
        assert!(!unified.installed);
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
    fn catalog_includes_omnilingual_and_cohere_with_company() {
        let catalog = catalog();
        let omni = catalog.iter().find(|m| m.id == OMNILINGUAL_ASR_MODEL_ID).expect("omnilingual in catalog");
        assert_eq!(omni.company.as_deref(), Some("Meta"));
        assert_eq!(omni.engine_key.as_deref(), Some("omnilingual"));

        let cohere = catalog.iter().find(|m| m.id == COHERE_TRANSCRIBE_MODEL_ID).expect("cohere in catalog");
        assert_eq!(cohere.company.as_deref(), Some("Cohere"));
        assert_eq!(cohere.engine_key.as_deref(), Some("cohere"));

        let parakeet = catalog.iter().find(|m| m.id == STT_MODEL_ID).expect("parakeet in catalog");
        assert_eq!(parakeet.company.as_deref(), Some("NVIDIA"));

        let whisper = catalog.iter().find(|m| m.id == WHISPER_TURBO_MODEL_ID).expect("whisper in catalog");
        assert_eq!(whisper.company.as_deref(), Some("OpenAI"));
    }

    #[test]
    fn installed_state_matches_disk() {
        let tmp = fixture("installed");
        let model_dir = tmp.join("opendictate").join("models").join(DEFAULT_MODEL);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("model.int8.onnx"), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(model_dir.join("tokens.txt"), vec![0u8; 200]).unwrap();
        let _root = TestModelRoot::new(&tmp);
        let initial_catalog = catalog();
        let parakeet = initial_catalog.iter().find(|m| m.id == STT_MODEL_ID).unwrap();
        assert!(parakeet.installed);
        assert_eq!(parakeet.disk_bytes, 2_000_200);
        let whisper = initial_catalog
            .iter()
            .find(|m| m.id == WHISPER_TINY_MODEL_ID)
            .unwrap();
        assert!(!whisper.installed);
        assert_eq!(whisper.disk_bytes, 0);

        // Omnilingual installed
        let omni_dir = tmp.join("opendictate").join("models").join(OMNILINGUAL_ASR_MODEL_ID);
        std::fs::create_dir_all(&omni_dir).unwrap();
        std::fs::write(omni_dir.join("model.int8.onnx"), vec![0u8; 1_500_000]).unwrap();
        std::fs::write(omni_dir.join("tokens.txt"), vec![0u8; 150]).unwrap();

        // Cohere installed
        let cohere_dir = tmp.join("opendictate").join("models").join(COHERE_TRANSCRIBE_MODEL_ID);
        std::fs::create_dir_all(&cohere_dir).unwrap();
        std::fs::write(cohere_dir.join("encoder.int8.onnx"), vec![0u8; 1_500_000]).unwrap();
        std::fs::write(cohere_dir.join("decoder.int8.onnx"), vec![0u8; 1_500_000]).unwrap();
        std::fs::write(cohere_dir.join("tokens.txt"), vec![0u8; 150]).unwrap();

        let updated_catalog = catalog();
        let omni = updated_catalog.iter().find(|m| m.id == OMNILINGUAL_ASR_MODEL_ID).unwrap();
        assert!(omni.installed);
        assert_eq!(omni.disk_bytes, 1_500_150);

        let cohere = updated_catalog.iter().find(|m| m.id == COHERE_TRANSCRIBE_MODEL_ID).unwrap();
        assert!(cohere.installed);
        assert_eq!(cohere.disk_bytes, 3_000_150);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cancellation_aborts_immediately_and_cleans_up() {
        let tmp = fixture("cancel");
        let _root = TestModelRoot::new(&tmp);
        let is_cancelled = || true;
        let mut progress_called = false;
        let result = ensure_model_with_cancel(
            WHISPER_TINY_MODEL_ID,
            &mut |_, _, _| progress_called = true,
            &is_cancelled,
        );
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(result.is_err());
        assert!(!progress_called);
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("cancelled by user"));
    }

    #[test]
    fn whisper_turbo_install_state_detects_canonical_and_int8_files() {
        let tmp = fixture("whisper-turbo");
        let model_dir = tmp
            .join("opendictate")
            .join("models")
            .join(WHISPER_TURBO_MODEL_ID);
        std::fs::create_dir_all(&model_dir).unwrap();

        // 1. With turbo-*.int8.onnx and turbo-tokens.txt names
        std::fs::write(
            model_dir.join("turbo-encoder.int8.onnx"),
            vec![0u8; 6_000_000],
        )
        .unwrap();
        std::fs::write(
            model_dir.join("turbo-decoder.int8.onnx"),
            vec![0u8; 6_000_000],
        )
        .unwrap();
        std::fs::write(model_dir.join("turbo-tokens.txt"), vec![0u8; 500]).unwrap();

        let _root = TestModelRoot::new(&tmp);
        assert!(is_model_installed(WHISPER_TURBO_MODEL_ID));

        // 2. Also with canonical names encoder.onnx / decoder.onnx / tokens.txt
        let _ = std::fs::remove_dir_all(&model_dir);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("encoder.onnx"), vec![0u8; 6_000_000]).unwrap();
        std::fs::write(model_dir.join("decoder.onnx"), vec![0u8; 6_000_000]).unwrap();
        std::fs::write(model_dir.join("tokens.txt"), vec![0u8; 500]).unwrap();

        assert!(is_model_installed(WHISPER_TURBO_MODEL_ID));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn whisper_turbo_archive_extraction_and_resolution() {
        use bzip2::write::BzEncoder;
        use bzip2::Compression;
        use tar::Builder;

        let tmp = fixture("whisper-turbo-archive");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let archive_path = tmp.join("whisper-turbo.tar.bz2");
        let extract_dir = tmp.join(".extract-whisper-turbo");

        // Build a mock tar.bz2 mimicking sherpa-onnx-whisper-turbo layout
        {
            let tar_bz2 = File::create(&archive_path).unwrap();
            let enc = BzEncoder::new(tar_bz2, Compression::fast());
            let mut builder = Builder::new(enc);

            // 1. Small stub unquantized encoder (< 5MB)
            let stub = vec![0u8; 700_000];
            let mut header = tar::Header::new_gnu();
            header.set_size(stub.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "sherpa-onnx-whisper-turbo/turbo-encoder.onnx",
                    &stub[..],
                )
                .unwrap();

            // 2. Full int8 encoder (6MB)
            let enc_int8 = vec![0u8; 6_000_000];
            let mut header = tar::Header::new_gnu();
            header.set_size(enc_int8.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "sherpa-onnx-whisper-turbo/turbo-encoder.int8.onnx",
                    &enc_int8[..],
                )
                .unwrap();

            // 3. Full int8 decoder (6MB)
            let dec_int8 = vec![0u8; 6_000_000];
            let mut header = tar::Header::new_gnu();
            header.set_size(dec_int8.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "sherpa-onnx-whisper-turbo/turbo-decoder.int8.onnx",
                    &dec_int8[..],
                )
                .unwrap();

            // 4. Tokens (>= 100 bytes)
            let tokens = [b'a'; 200];
            let mut header = tar::Header::new_gnu();
            header.set_size(tokens.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "sherpa-onnx-whisper-turbo/turbo-tokens.txt",
                    &tokens[..],
                )
                .unwrap();

            builder.finish().unwrap();
        }

        // Extract using our production extractor (which must create extract_dir if not present)
        extract_archive(&archive_path, &extract_dir, &|| false).unwrap();

        // Run file collection and resolution
        let mut files = Vec::new();
        collect_files(&extract_dir, &mut files);

        let find_part = |needle: &str| {
            let mut hits: Vec<_> = files
                .iter()
                .filter(|p| {
                    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                        let lower = n.to_lowercase();
                        lower.ends_with(".onnx")
                            && lower.contains(needle)
                            && valid_file_size(p, WHISPER_PART_MIN_BYTES).is_some()
                    })
                })
                .cloned()
                .collect();
            hits.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
            hits.first().cloned()
        };

        let encoder = find_part("encoder").expect("must find int8 encoder");
        assert!(
            encoder
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("turbo-encoder.int8.onnx"),
            "must pick int8 encoder, not small stub"
        );

        let decoder = find_part("decoder").expect("must find int8 decoder");
        assert!(decoder
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("turbo-decoder.int8.onnx"));

        let tokens = files
            .iter()
            .find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    let lower = n.to_lowercase();
                    lower.contains("tokens") && lower.ends_with(".txt")
                })
            })
            .cloned()
            .expect("must find turbo-tokens.txt");

        // Install to target model directory
        let target_dir = tmp
            .join("opendictate")
            .join("models")
            .join(WHISPER_TURBO_MODEL_ID);
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::copy(&encoder, target_dir.join("encoder.onnx")).unwrap();
        std::fs::copy(&decoder, target_dir.join("decoder.onnx")).unwrap();
        std::fs::copy(&tokens, target_dir.join("tokens.txt")).unwrap();

        let _root = TestModelRoot::new(&tmp);
        assert!(is_model_installed(WHISPER_TURBO_MODEL_ID));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn download_emits_initial_zero_progress_and_user_agent() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = std::thread::spawn(move || {
            let mut stream = accept_fixture(listener);
            let mut req_buf = [0u8; 2048];
            let n = std::io::Read::read(&mut stream, &mut req_buf).unwrap();
            let req_str = String::from_utf8_lossy(&req_buf[..n]);
            let expected_ua = format!("OpenDictate/{} (Desktop; Tauri)", env!("CARGO_PKG_VERSION"));
            assert!(
                req_str.contains(&format!("User-Agent: {expected_ua}"))
                    || req_str.contains(&format!("user-agent: {expected_ua}")),
                "Request missing User-Agent header: {req_str}"
            );

            let body = b"hello open dictate test payload";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                std::str::from_utf8(body).unwrap()
            );
            std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
            std::io::Write::flush(&mut stream).unwrap();
        });

        let tmp = fixture("dl-progress");
        let dest = tmp.join("model.bin");
        let _ = std::fs::remove_dir_all(&tmp);

        let mut progress_events = Vec::new();
        let url = format!("http://127.0.0.1:{port}/model.bin");
        let result = download_to_with_progress_cancel(
            &url,
            &dest,
            &mut |received, total| progress_events.push((received, total)),
            &|| false,
        );

        server_thread.join().unwrap();
        assert!(result.is_ok(), "download failed: {:?}", result.err());

        // Verify initial 0 progress was emitted right away
        assert!(!progress_events.is_empty());
        assert_eq!(progress_events[0], (0, 31));
        assert_eq!(*progress_events.last().unwrap(), (31, 31));

        // Verify destination file exists and temp part file is gone
        assert!(dest.exists());
        let part_file = tmp.join("model.bin.part");
        assert!(!part_file.exists());
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"hello open dictate test payload"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn download_cleans_up_part_file_on_cancel() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = std::thread::spawn(move || {
            let mut stream = accept_fixture(listener);
            let mut req_buf = [0u8; 2048];
            let _ = std::io::Read::read(&mut stream, &mut req_buf);

            let body = vec![b'x'; 256 * 1024]; // 256KB payload
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = std::io::Write::write_all(&mut stream, header.as_bytes());
            let _ = std::io::Write::write_all(&mut stream, &body);
        });

        let tmp = fixture("dl-cancel");
        let dest = tmp.join("model.bin");
        let _ = std::fs::remove_dir_all(&tmp);

        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let url = format!("http://127.0.0.1:{port}/model.bin");
        let result = download_to_with_progress_cancel(
            &url,
            &dest,
            &mut |received, _| {
                // Cancel once initial 0 progress is emitted
                if received == 0 {
                    cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            },
            &|| cancelled.load(std::sync::atomic::Ordering::SeqCst),
        );

        let _ = server_thread.join();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cancelled by user"));

        // Neither dest nor part file should exist
        assert!(!dest.exists(), "dest should not exist on cancel");
        let part_file = tmp.join("model.bin.part");
        assert!(
            !part_file.exists(),
            "part file should be cleaned up on cancel"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn download_cleans_up_part_file_on_io_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = std::thread::spawn(move || {
            let mut stream = accept_fixture(listener);
            let mut req_buf = [0u8; 2048];
            let _ = std::io::Read::read(&mut stream, &mut req_buf);

            let header = "HTTP/1.1 200 OK\r\nContent-Length: 50000\r\nConnection: close\r\n\r\n";
            let _ = std::io::Write::write_all(&mut stream, header.as_bytes());
            let _ = std::io::Write::write_all(&mut stream, b"incomplete");
            // Abruptly close socket by shutting down and dropping stream
            let _ = stream.shutdown(std::net::Shutdown::Both);
        });

        let tmp = fixture("dl-error");
        let dest = tmp.join("model.bin");
        let _ = std::fs::remove_dir_all(&tmp);

        let url = format!("http://127.0.0.1:{port}/model.bin");
        let result = download_to_with_progress_cancel(&url, &dest, &mut |_, _| {}, &|| false);

        let _ = server_thread.join();
        assert!(result.is_err());

        // Neither dest nor part file should exist
        assert!(!dest.exists(), "dest should not exist on error");
        let part_file = tmp.join("model.bin.part");
        assert!(
            !part_file.exists(),
            "part file should be cleaned up on error"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
