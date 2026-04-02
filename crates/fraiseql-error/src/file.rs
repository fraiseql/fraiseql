/// Errors that occur during file upload, validation, storage, or retrieval.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FileError {
    /// The uploaded file exceeds the configured maximum size.
    #[error("File too large: {size} bytes (max: {max} bytes)")]
    TooLarge {
        /// Actual size of the uploaded file in bytes.
        size: usize,
        /// Maximum allowed file size in bytes.
        max:  usize,
    },

    /// The file's extension or declared MIME type is not on the allow-list.
    #[error("Invalid file type: {got} (allowed: {allowed:?})")]
    InvalidType {
        /// The MIME type or extension that was supplied.
        got:     String,
        /// The set of allowed MIME types or extensions.
        allowed: Vec<String>,
    },

    /// The file's declared MIME type does not match its detected MIME type.
    ///
    /// This can indicate a spoofed `Content-Type` header.
    #[error("MIME type mismatch: declared {declared}, detected {detected}")]
    MimeMismatch {
        /// The MIME type stated by the client.
        declared: String,
        /// The MIME type detected by content inspection.
        detected: String,
    },

    /// An error occurred while writing to or reading from the backing storage
    /// system (e.g. local disk, object store).
    #[error("Storage error: {message}")]
    Storage {
        /// Description of the storage failure.
        message: String,
        /// Optional chained error from the storage backend.
        #[source]
        source:  Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// An error occurred while processing the file contents (e.g. image
    /// resizing, format conversion).
    #[error("Processing error: {message}")]
    Processing {
        /// Description of the processing failure.
        message: String,
    },

    /// The requested file does not exist in the storage backend.
    #[error("File not found: {id}")]
    NotFound {
        /// Identifier of the file that was not found.
        id: String,
    },

    /// A virus or malware scanner flagged the uploaded file.
    #[error("Virus detected: {details}")]
    VirusDetected {
        /// Scanner-provided details about the detected threat (server-side only).
        details: String,
    },

    /// The user or tenant has exhausted their file storage quota.
    #[error("Upload quota exceeded")]
    QuotaExceeded,
}

impl FileError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
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
