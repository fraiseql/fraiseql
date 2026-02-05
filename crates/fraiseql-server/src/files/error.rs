//! Error types for file operations

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("File too large: {size} bytes (max: {max} bytes)")]
    TooLarge { size: usize, max: usize },

    #[error("File type not allowed: {got} (allowed: {allowed:?})")]
    InvalidType {
        got:     String,
        allowed: Vec<String>,
    },

    #[error("MIME type mismatch: declared {declared}, detected {detected}")]
    MimeMismatch { declared: String, detected: String },

    #[error("No file provided")]
    MissingFile,

    #[error("Invalid filename: {reason}")]
    InvalidFilename { reason: String },

    #[error("Invalid file content: {message}")]
    InvalidContent { message: String },

    #[error("Malware detected: {threat_name}")]
    MalwareDetected { threat_name: String },

    #[error("Storage backend not configured: {backend}")]
    StorageNotConfigured { backend: String },
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("File not found: {key}")]
    NotFound { key: String },

    #[error("Access denied: {key}")]
    AccessDenied { key: String },

    #[error("Upload failed: {message}")]
    UploadFailed { message: String },

    #[error("Download failed: {message}")]
    DownloadFailed { message: String },

    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("Provider error: {message}")]
    Provider { message: String },
}

#[derive(Debug, Error)]
pub enum ProcessingError {
    #[error("Failed to load image: {message}")]
    LoadFailed { message: String },

    #[error("Failed to resize image: {message}")]
    ResizeFailed { message: String },

    #[error("Failed to encode image: {message}")]
    EncodeFailed { message: String },

    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("Scan timeout")]
    Timeout,

    #[error("Scanner service unavailable")]
    ServiceUnavailable,

    #[error("Invalid scanner response: {message}")]
    InvalidResponse { message: String },
}
