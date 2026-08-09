#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("audio error: {0}")]
    Audio(String),
    #[error("transcription error: {0}")]
    Transcription(String),
    #[error("model error: {0}")]
    Model(String),
    #[error("download error: {0}")]
    Download(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
