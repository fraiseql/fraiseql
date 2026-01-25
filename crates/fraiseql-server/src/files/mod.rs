//! FraiseQL File Runtime
//!
//! Provides file upload handling with validation, image processing, multiple storage
//! backends (S3, R2, local), and security features.

pub mod config;
pub mod error;
pub mod handler;
pub mod processing;
pub mod storage;
pub mod traits;
pub mod validation;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

// Re-exports
pub use config::{
    FileConfig, ProcessingConfig, StorageConfig, UploadCallbackConfig, VariantConfig,
};
pub use error::{FileError, ProcessingError, ScanError, StorageError};
pub use handler::{FileHandler, FileResponse, SignedUrlResponse};
pub use processing::{ImageProcessorImpl, ProcessedImages};
pub use storage::{StorageBackend, StorageMetadata, StorageResult};
pub use traits::{FileValidator, ImageProcessor, MalwareScanner, ScanResult, ValidatedFile};
pub use validation::{detect_content_type, sanitize_filename, validate_file};
