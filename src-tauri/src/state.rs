use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use opendictate_core::audio::capture::AudioRecorder;
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "ctrl+alt+space".to_string(),
            mic: None,
            engine: "parakeet".to_string(),
            language: "auto".to_string(),
            onboarded: false,
            stt_model: default_stt_model(),
            insert_mode: default_insert_mode(),
            heatmap_color: default_heatmap_color(),
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
pub struct ModelsStatus {
    pub stt_ready: bool,
    pub vad_ready: bool,
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
}

impl AppState {
    pub fn is_test_mode(&self) -> bool {
        self.test_mode.load(Ordering::SeqCst)
    }

    pub fn set_test_mode(&self, enabled: bool) {
        self.test_mode.store(enabled, Ordering::SeqCst);
    }
}