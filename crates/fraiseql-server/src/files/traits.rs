//! Testing seams for file operations

use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use std::time::Duration;

use crate::files::config::{FileConfig, ProcessingConfig};
use crate::files::error::{FileError, ProcessingError, ScanError, StorageError};
use crate::files::processing::ProcessedImages;

/// Storage backend abstraction for testing
#[async_trait]
pub trait StorageBackend: Send + Sync {
    fn name(&self) -> &'static str;

    async fn upload(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        metadata: Option<&StorageMetadata>,
    ) -> Result<StorageResult, StorageError>;

    async fn download(&self, key: &str) -> Result<Bytes, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError>;
    async fn signed_url(&self, key: &str, expiry: Duration) -> Result<String, StorageError>;
    fn public_url(&self, key: &str) -> String;
}

#[derive(Debug, Clone)]
pub struct StorageMetadata {
    pub content_type: String,
    pub content_length: u64,
    pub etag: Option<String>,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub custom: HashMap<String, String>,
}

#[derive(Debug)]
pub struct StorageResult {
    pub key: String,
    pub url: String,
    pub etag: Option<String>,
    pub size: u64,
}

/// File validator abstraction for testing
pub trait FileValidator: Send + Sync {
    fn validate(
        &self,
        data: &Bytes,
        declared_type: &str,
        filename: &str,
        config: &FileConfig,
    ) -> Result<ValidatedFile, FileError>;
}

/// Image processor abstraction for testing
#[async_trait]
pub trait ImageProcessor: Send + Sync {
    async fn process(
        &self,
        data: &Bytes,
        config: &ProcessingConfig,
    ) -> Result<ProcessedImages, ProcessingError>;
}

/// Malware scanner abstraction (optional external service)
#[async_trait]
pub trait MalwareScanner: Send + Sync {
    async fn scan(&self, data: &Bytes) -> Result<ScanResult, ScanError>;
}

#[derive(Debug)]
pub struct ScanResult {
    pub clean: bool,
    pub threat_name: Option<String>,
    pub scanner_version: String,
}

#[derive(Debug)]
pub struct ValidatedFile {
    pub content_type: String,
    pub sanitized_filename: String,
    pub size: usize,
    pub detected_type: Option<String>,
}
