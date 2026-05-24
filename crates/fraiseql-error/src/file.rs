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

    // ------------------------------------------------------------------------
    // Backend / infrastructure variants (F050 — absorb `FraiseQLError::Storage`)
    // ------------------------------------------------------------------------
    /// The backend refused the request because the caller lacks permission
    /// (e.g. an object-store bucket policy denied the operation, IAM credentials
    /// are missing the required action, the SAS token is expired).
    ///
    /// Distinct from a missing-credentials authentication failure — those
    /// surface as [`crate::FraiseQLError::Authentication`].
    #[error("Permission denied: {message}")]
    PermissionDenied {
        /// Description of the permission failure (server-side only; the
        /// HTTP body is generic).
        message: String,
        /// Optional chained error from the storage backend.
        #[source]
        source:  Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// A low-level I/O failure occurred while talking to the storage backend
    /// (filesystem read/write error, socket failure, JSON-parse failure on a
    /// backend response).
    #[error("I/O error: {message}")]
    IoError {
        /// Description of the I/O failure.
        message: String,
        /// Optional chained error from the storage backend.
        #[source]
        source:  Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// The caller supplied a key that fails validation (empty, path-traversal
    /// segment, leading `/` or `\`). User-fixable; HTTP 400.
    #[error("Invalid storage key: {message}")]
    InvalidKey {
        /// Description of why the key was rejected.
        message: String,
    },

    /// The requested operation is not implemented for this backend
    /// (e.g. `list` on GCS, presigned URLs without V4 signing on Azure).
    /// The capability exists in the API surface but is unimplemented.
    #[error("Not implemented: {message}")]
    NotImplemented {
        /// Description of the unimplemented operation.
        message: String,
    },

    /// The requested operation is not supported by this backend at all
    /// (e.g. presigned URLs for the local filesystem, presign-PUT for the
    /// non-S3 enum variants).
    #[error("Unsupported: {message}")]
    Unsupported {
        /// Description of the unsupported operation.
        message: String,
    },

    /// The upload exceeds the configured per-bucket size limit.
    ///
    /// Distinct from [`Self::TooLarge`], which is set by client-side
    /// validation against `max_object_bytes`. This variant is raised by the
    /// service layer when the limit is enforced post-upload (e.g. for streaming
    /// uploads where the total size is only known once the body has been
    /// read).
    #[error("Size limit exceeded: {message}")]
    SizeLimitExceeded {
        /// Description of the size-limit violation.
        message: String,
        /// Configured maximum size in bytes, if known.
        limit:   Option<u64>,
        /// Actual size of the payload in bytes, if known.
        actual:  Option<u64>,
    },

    /// The uploaded content type is not on the per-bucket allow-list.
    ///
    /// Distinct from [`Self::InvalidType`], which is raised at extension /
    /// MIME-sniffing time. This variant is raised by the service layer
    /// against the configured `allowed_mime_types` list.
    #[error("MIME type not allowed: {message}")]
    MimeTypeNotAllowed {
        /// Description of the rejection.
        message: String,
        /// The rejected MIME type, if known.
        mime:    Option<String>,
    },

    /// A generic backend / infrastructure failure with no more-specific
    /// classification.
    ///
    /// Used for object-store authentication setup failures (missing env vars,
    /// invalid credentials JSON), backend HTTP-request failures, configuration
    /// errors (`s3` backend without `bucket` config), and database failures in
    /// the storage metadata layer. Returns HTTP 500.
    #[error("Backend error: {message}")]
    Backend {
        /// Description of the backend failure.
        message: String,
        /// Optional chained error from the underlying SDK / IO call.
        #[source]
        source:  Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl FileError {
    /// Returns a short, stable error code string suitable for API responses and
    /// structured logging.
    #[must_use]
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
            Self::PermissionDenied { .. } => "file_permission_denied",
            Self::IoError { .. } => "file_io_error",
            Self::InvalidKey { .. } => "file_invalid_key",
            Self::NotImplemented { .. } => "file_not_implemented",
            Self::Unsupported { .. } => "file_unsupported",
            Self::SizeLimitExceeded { .. } => "file_size_limit_exceeded",
            Self::MimeTypeNotAllowed { .. } => "file_mime_type_not_allowed",
            Self::Backend { .. } => "file_backend_error",
        }
    }

    /// Returns the HTTP status code this variant maps to when surfaced
    /// through [`crate::FraiseQLError::File`].
    ///
    /// User-fixable validation failures map to 4xx; backend / infrastructure
    /// failures map to 5xx. The split here preserves what
    /// `fraiseql-storage/src/routes/mod.rs::storage_error_response` previously
    /// hard-coded via the `code: Option<String>` discriminator on the
    /// (now-deprecated) `FraiseQLError::Storage` variant:
    ///
    /// - `NotFound` → 404 — backend reports object missing
    /// - `PermissionDenied` → 403 — backend refuses the operation
    /// - `InvalidKey` → 400 — caller supplied a malformed key
    /// - `IoError`, `Backend`, `NotImplemented`, `Unsupported`,
    ///   `SizeLimitExceeded`, `MimeTypeNotAllowed` → 500 — preserves the
    ///   legacy behavior of `FraiseQLError::Storage` (which routed every
    ///   `code` *except* `not_found`/`permission_denied` to 500 via
    ///   `storage_error_response`)
    /// - All other (pre-existing) variants — `TooLarge`, `InvalidType`,
    ///   `MimeMismatch`, `VirusDetected`, `QuotaExceeded`, `Storage`,
    ///   `Processing` — fall back to the `FraiseQLError::File`-level 400
    ///   via the wildcard arm so that pre-F050 callers see unchanged HTTP
    ///   responses.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::NotFound { .. } => 404,
            Self::PermissionDenied { .. } => 403,
            Self::InvalidKey { .. } => 400,
            Self::IoError { .. }
            | Self::Backend { .. }
            | Self::NotImplemented { .. }
            | Self::Unsupported { .. }
            | Self::SizeLimitExceeded { .. }
            | Self::MimeTypeNotAllowed { .. } => 500,
            // Pre-F050 variants — preserve the legacy `FraiseQLError::File`
            // → 400 mapping for backwards-compatibility.
            Self::TooLarge { .. }
            | Self::InvalidType { .. }
            | Self::MimeMismatch { .. }
            | Self::VirusDetected { .. }
            | Self::QuotaExceeded
            | Self::Storage { .. }
            | Self::Processing { .. } => 400,
        }
    }
}
