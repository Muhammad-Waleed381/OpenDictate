use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use opendictate_core::audio::capture::AudioRecorder;
use opendictate_core::stt::engine::SttEngine;
use opendictate_core::stt::streaming::{StreamingRecognizer, StreamingSession};
use opendictate_core::audio::vad::SileroVad;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub hotkey: Option<String>,
    pub engine: Option<String>,
    pub language: Option<String>,
    pub stt_model: Option<String>,
    pub insert_mode: Option<String>,
    pub heatmap_color: Option<String>,
    pub vad_sensitivity: Option<f32>,
    pub continuous: Option<bool>,
    pub autostart: Option<bool>,
    pub spoken_punctuation: Option<bool>,
    pub audio_feedback: Option<bool>,
    pub audio_feedback_volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub hotkey: String,
    pub mic: Option<String>,
    pub engine: String,
    pub language: String,
    pub onboarded: bool,
    #[serde(default = "default_stt_model", alias = "sttModel")]
    pub stt_model: String,
    #[serde(default = "default_insert_mode")]
    pub insert_mode: String,
    #[serde(default = "default_heatmap_color")]
    pub heatmap_color: String,
    #[serde(default = "default_vad_sensitivity")]
    pub vad_sensitivity: f32,
    #[serde(default)]
    pub continuous: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub spoken_punctuation: bool,
    #[serde(default)]
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
}

fn default_stt_model() -> String {
    opendictate_core::stt::models::STT_MODEL_ID.to_string()
}

fn default_insert_mode() -> String {
    "auto".to_string()
}

fn default_heatmap_color() -> String {
    "#16a34a".to_string()
}

fn default_vad_sensitivity() -> f32 {
    0.5
}

fn default_audio_feedback_volume() -> f32 {
    0.5
}

/// macOS reserves the Ctrl+Alt/Option row for input-source switching and treats
/// Cmd as the primary modifier, so `ctrl+alt+space` is both unidiomatic and
/// liable to collide there. Cmd+Space (Spotlight) and Cmd+Option+Space (Finder
/// search) are taken by the system; Cmd+Shift+Space is free.
#[cfg(target_os = "macos")]
pub fn default_hotkey() -> String {
    "cmd+shift+space".to_string()
}

#[cfg(not(target_os = "macos"))]
pub fn default_hotkey() -> String {
    "ctrl+alt+space".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            mic: None,
            engine: "parakeet".to_string(),
            language: "auto".to_string(),
            onboarded: false,
            stt_model: default_stt_model(),
            insert_mode: default_insert_mode(),
            heatmap_color: default_heatmap_color(),
            vad_sensitivity: default_vad_sensitivity(),
            continuous: false,
            autostart: false,
            spoken_punctuation: false,
            audio_feedback: false,
            audio_feedback_volume: default_audio_feedback_volume(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub created_at: String,
    pub duration_ms: u64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictEntry {
    pub id: i64,
    pub word: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetEntry {
    pub id: i64,
    pub trigger: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsStatus {
    pub stt_ready: bool,
    pub vad_ready: bool,
    pub caption_ready: bool,
    pub streaming_rtf_x100: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptResult {
    pub text: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayWords {
    pub day: String,
    pub words: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordStats {
    pub daily: Vec<DayWords>,
    pub total_words: u64,
    pub total_sessions: u64,
    pub streak_days: u64,
    pub best_day: Option<String>,
    pub best_words: u64,
}

pub struct AppState {
    pub recorder: Arc<AudioRecorder>,
    pub test_mode: Arc<AtomicBool>,
    pub db: Arc<Mutex<Connection>>,
    pub settings: Arc<Mutex<Settings>>,
    pub hotkey: Arc<Mutex<Option<String>>>,
    pub continuous: Arc<AtomicBool>,
    pub stream: Arc<Mutex<Option<StreamingPipe>>>,
    pub stream_active: Arc<AtomicBool>,
    /// Live-caption engine (small zipformer): runs during any recording and
    /// owns `partial` emission; the selected accuracy model still produces
    /// the final transcript.
    pub caption_engine: Arc<Mutex<Option<CachedStreamingEngine>>>,
    pub caption_stream: Arc<Mutex<Option<StreamingPipe>>>,
    pub caption_active: Arc<AtomicBool>,
    /// Measured decode speed of the selectable streaming STT model, x100
    /// (e.g. 1500 = RTF 15.0). 0 = not benchmarked yet.
    pub streaming_rtf_x100: Arc<AtomicU32>,
    pub last_inserted: Arc<Mutex<Option<String>>>,
    pub stt_engine: Arc<Mutex<Option<CachedSttEngine>>>,
    pub streaming_engine: Arc<Mutex<Option<CachedStreamingEngine>>>,
    pub vad: Arc<Mutex<Option<CachedVad>>>,
}

pub struct CachedSttEngine {
    pub model_id: String,
    pub language: String,
    pub engine: Arc<SttEngine>,
}

pub struct CachedStreamingEngine {
    pub model_id: String,
    pub recognizer: Arc<StreamingRecognizer>,
}

pub struct CachedVad {
    pub sensitivity: f32,
    pub detector: Arc<SileroVad>,
}

impl AppState {
    pub fn is_test_mode(&self) -> bool {
        self.test_mode.load(Ordering::SeqCst)
    }

    pub fn set_test_mode(&self, enabled: bool) {
        self.test_mode.store(enabled, Ordering::SeqCst);
    }

    pub fn set_continuous(&self, enabled: bool) {
        self.continuous.store(enabled, Ordering::SeqCst);
    }

    pub fn is_streaming_active(&self) -> bool {
        self.stream_active.load(Ordering::SeqCst)
    }

    pub fn set_streaming(&self, enabled: bool) {
        self.stream_active.store(enabled, Ordering::SeqCst);
    }
}

/// Live state for streaming ASR: the recognizer, its active session and the
/// capture-buffer watermark. All access is serialized through the mutex.
pub struct StreamingPipe {
    pub recognizer: Arc<StreamingRecognizer>,
    pub session: StreamingSession,
    /// Offset into the capture buffer of the first sample not yet fed to the
    /// recognizer. Monotonic across utterances within one streaming session.
    pub watermark: usize,
    pub total_fed: usize,
}
