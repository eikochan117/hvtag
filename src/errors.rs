use crate::folders::types::RJCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HvtError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Work {0} removed from DLSite")]
    RemovedWork(RJCode),

    #[error("Folder reading error: {0}")]
    FolderReading(String),

    #[error("Operating System not supported: {0}")]
    UnsupportedOS(String),

    #[error("Path creation failed: {0}")]
    PathCreationFailed(String),

    #[error("Environment variable unavailable: {0}")]
    UnavailableEnvVariable(String),

    #[error("Audio tagging error: {0}")]
    AudioTag(String),

    #[error("Audio conversion error: {0}")]
    AudioConversion(String),

    #[error("Image processing error: {0}")]
    Image(String),

    #[error("Generic error: {0}")]
    Generic(String),
}

// Legacy type aliases for backwards compatibility during migration
pub type DbLoaderError = HvtError;
pub type DatabaseError = HvtError;
