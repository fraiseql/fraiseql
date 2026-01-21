//! Mock implementations for testing

use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::files::config::{FileConfig, ProcessingConfig};
use crate::files::error::{FileError, ProcessingError, ScanError, StorageError};
use crate::files::processing::ProcessedImages;
use crate::files::traits::{
    FileValidator, ImageProcessor, MalwareScanner, ScanResult, StorageBackend, StorageMetadata,
    StorageResult, ValidatedFile,
};

/// In-memory storage for testing
pub struct MockStorage {
    files: Mutex<HashMap<String, MockFile>>,
    public_url_base: String,
    /// Simulate failures for specific keys
    pub fail_keys: Mutex<Vec<String>>,
}

#[derive(Clone)]
struct MockFile {
    data: Bytes,
    content_type: String,
    metadata: StorageMetadata,
}

impl MockStorage {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
            public_url_base: "https://mock-storage.test".to_string(),
            fail_keys: Mutex::new(Vec::new()),
        }
    }

    pub fn with_public_url(mut self, url: &str) -> Self {
        self.public_url_base = url.to_string();
        self
    }

    /// Configure a key to fail on next operation
    pub fn fail_on(&self, key: &str) {
        self.fail_keys.lock().unwrap().push(key.to_string());
    }

    pub fn get_files(&self) -> Vec<String> {
        self.files.lock().unwrap().keys().cloned().collect()
    }

    pub fn clear(&self) {
        self.files.lock().unwrap().clear();
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StorageBackend for MockStorage {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn upload(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        metadata: Option<&StorageMetadata>,
    ) -> Result<StorageResult, StorageError> {
        // Check for simulated failure
        if self.fail_keys.lock().unwrap().contains(&key.to_string()) {
            return Err(StorageError::UploadFailed {
                message: "Simulated failure".into(),
            });
        }

        let size = data.len() as u64;
        self.files.lock().unwrap().insert(
            key.to_string(),
            MockFile {
                data,
                content_type: content_type.to_string(),
                metadata: metadata.cloned().unwrap_or_else(|| StorageMetadata {
                    content_type: content_type.to_string(),
                    content_length: size,
                    etag: Some(format!("\"{}\"", uuid::Uuid::new_v4())),
                    last_modified: Some(chrono::Utc::now()),
                    custom: HashMap::new(),
                }),
            },
        );

        Ok(StorageResult {
            key: key.to_string(),
            url: self.public_url(key),
            etag: Some(format!("\"{}\"", uuid::Uuid::new_v4())),
            size,
        })
    }

    async fn download(&self, key: &str) -> Result<Bytes, StorageError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|f| f.data.clone())
            .ok_or_else(|| StorageError::NotFound {
                key: key.to_string(),
            })
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.files.lock().unwrap().remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.files.lock().unwrap().contains_key(key))
    }

    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|f| f.metadata.clone())
            .ok_or_else(|| StorageError::NotFound {
                key: key.to_string(),
            })
    }

    async fn signed_url(&self, key: &str, expiry: Duration) -> Result<String, StorageError> {
        let expires = chrono::Utc::now()
            + chrono::Duration::from_std(expiry).map_err(|e| StorageError::Provider {
                message: e.to_string(),
            })?;
        Ok(format!(
            "{}{}?expires={}",
            self.public_url_base,
            key,
            expires.timestamp()
        ))
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_url_base, key)
    }
}

/// Mock validator that accepts/rejects based on configuration
pub struct MockValidator {
    pub allowed_types: Vec<String>,
    pub max_size: usize,
    pub reject_files: Mutex<Vec<String>>,
}

impl MockValidator {
    pub fn permissive() -> Self {
        Self {
            allowed_types: vec!["*/*".to_string()],
            max_size: usize::MAX,
            reject_files: Mutex::new(Vec::new()),
        }
    }

    pub fn strict(allowed_types: Vec<String>, max_size: usize) -> Self {
        Self {
            allowed_types,
            max_size,
            reject_files: Mutex::new(Vec::new()),
        }
    }

    pub fn reject(&self, filename: &str) {
        self.reject_files
            .lock()
            .unwrap()
            .push(filename.to_string());
    }
}

impl FileValidator for MockValidator {
    fn validate(
        &self,
        data: &Bytes,
        declared_type: &str,
        filename: &str,
        _config: &FileConfig,
    ) -> Result<ValidatedFile, FileError> {
        if self
            .reject_files
            .lock()
            .unwrap()
            .contains(&filename.to_string())
        {
            return Err(FileError::InvalidType {
                got: declared_type.to_string(),
                allowed: self.allowed_types.clone(),
            });
        }

        if data.len() > self.max_size {
            return Err(FileError::TooLarge {
                size: data.len(),
                max: self.max_size,
            });
        }

        if !self.allowed_types.contains(&"*/*".to_string())
            && !self.allowed_types.contains(&declared_type.to_string())
        {
            return Err(FileError::InvalidType {
                got: declared_type.to_string(),
                allowed: self.allowed_types.clone(),
            });
        }

        Ok(ValidatedFile {
            content_type: declared_type.to_string(),
            sanitized_filename: sanitize_filename(filename),
            size: data.len(),
            detected_type: None,
        })
    }
}

/// Mock image processor that returns predefined variants
pub struct MockImageProcessor {
    pub should_fail: bool,
    pub variants: Vec<String>,
}

impl MockImageProcessor {
    pub fn new(variants: Vec<&str>) -> Self {
        Self {
            should_fail: false,
            variants: variants.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    pub fn failing() -> Self {
        Self {
            should_fail: true,
            variants: Vec::new(),
        }
    }
}

#[async_trait]
impl ImageProcessor for MockImageProcessor {
    async fn process(
        &self,
        data: &Bytes,
        _config: &ProcessingConfig,
    ) -> Result<ProcessedImages, ProcessingError> {
        if self.should_fail {
            return Err(ProcessingError::LoadFailed {
                message: "Simulated failure".into(),
            });
        }

        let mut variants = HashMap::new();
        variants.insert("original".to_string(), data.clone());

        for variant in &self.variants {
            // Create slightly smaller "processed" data
            variants.insert(
                variant.clone(),
                data.slice(..data.len().saturating_sub(100)),
            );
        }

        Ok(ProcessedImages { variants })
    }
}

/// Mock malware scanner
pub struct MockMalwareScanner {
    pub threats: Mutex<HashMap<Vec<u8>, String>>,
}

impl MockMalwareScanner {
    pub fn clean() -> Self {
        Self {
            threats: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_threat(self, data: &[u8], threat_name: &str) -> Self {
        self.threats
            .lock()
            .unwrap()
            .insert(data.to_vec(), threat_name.to_string());
        self
    }
}

#[async_trait]
impl MalwareScanner for MockMalwareScanner {
    async fn scan(&self, data: &Bytes) -> Result<ScanResult, ScanError> {
        let threats = self.threats.lock().unwrap();
        if let Some(threat_name) = threats.get(data.as_ref()) {
            Ok(ScanResult {
                clean: false,
                threat_name: Some(threat_name.clone()),
                scanner_version: "mock-1.0".to_string(),
            })
        } else {
            Ok(ScanResult {
                clean: true,
                threat_name: None,
                scanner_version: "mock-1.0".to_string(),
            })
        }
    }
}

/// Sanitize filename for testing (same as production)
pub fn sanitize_filename(filename: &str) -> String {
    // Remove path components
    let filename = filename.rsplit(['/', '\\']).next().unwrap_or(filename);

    // Replace dangerous characters
    filename
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}
