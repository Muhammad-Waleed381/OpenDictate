pub mod capture;
pub mod vad;

#[cfg(target_os = "linux")]
pub mod pulse;

use serde::Serialize;

/// A selectable microphone as presented to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct MicDevice {
    pub id: String,
    pub label: String,
}
