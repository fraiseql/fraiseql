#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("File too large: {size} bytes (max: {max} bytes)")]
    TooLarge { size: usize, max: usize },

    #[error("Invalid file type: {got} (allowed: {allowed:?})")]
    InvalidType { got: String, allowed: Vec<String> },

    #[error("MIME type mismatch: declared {declared}, detected {detected}")]
    MimeMismatch { declared: String, detected: String },

    #[error("Storage error: {message}")]
    Storage { message: String, #[source] source: Option<Box<dyn std::error::Error + Send + Sync>> },

    #[error("Processing error: {message}")]
    Processing { message: String },

    #[error("File not found: {id}")]
    NotFound { id: String },

    #[error("Virus detected: {details}")]
    VirusDetected { details: String },

    #[error("Upload quota exceeded")]
    QuotaExceeded,
}

impl FileError {
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::TooLarge { .. } => "file_too_large",
            Self::InvalidType { .. } => "file_invalid_type",
            Self::MimeMismatch { .. } => "file_mime_mismatch",
            Self::Storage { .. } => "file_storage_error",
            Self::Processing { .. } => "file_processing_error",
            Self::NotFound { .. } => "file_not_found",
            Self::VirusDetected { .. } => "file_virus_detected",
            Self::QuotaExceeded => "file_quota_exceeded",
        }
    }
}
