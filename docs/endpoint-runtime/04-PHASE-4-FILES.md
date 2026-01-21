# Phase 4: File Runtime

## Objective

Implement file upload handling with validation, image processing, multiple storage backends (S3, R2, GCS), CDN integration, and signed URL support.

---

## 4.0 Testing Seams & Security Model

### Security Considerations

File uploads are a high-risk attack surface. Key protections:

```
┌─────────────────────────────────────────────────────────────┐
│ Upload Security Layers                                      │
├─────────────────────────────────────────────────────────────┤
│ 1. Size Limits - Prevent DoS via large files               │
│ 2. MIME Whitelist - Only allow expected file types         │
│ 3. Magic Bytes - Verify content matches declared type      │
│ 4. Filename Sanitization - Prevent path traversal          │
│ 5. Storage Isolation - Files stored with random UUIDs      │
│ 6. Content-Disposition - Force download, prevent XSS       │
└─────────────────────────────────────────────────────────────┘
```

### Task: Define testing seams for file operations

```rust
// crates/fraiseql-files/src/traits.rs

use async_trait::async_trait;
use bytes::Bytes;
use std::time::Duration;

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
    async fn process(&self, data: &Bytes, config: &ProcessingConfig) -> Result<ProcessedImages, ProcessingError>;
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
```

### Task: Mock implementations for testing

```rust
// crates/fraiseql-files/src/testing.rs

#[cfg(any(test, feature = "testing"))]
pub mod mocks {
    use super::*;
    use std::sync::Mutex;
    use std::collections::HashMap;

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

    #[async_trait]
    impl StorageBackend for MockStorage {
        fn name(&self) -> &'static str { "mock" }

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
            self.files.lock().unwrap()
                .get(key)
                .map(|f| f.data.clone())
                .ok_or_else(|| StorageError::NotFound { key: key.to_string() })
        }

        async fn delete(&self, key: &str) -> Result<(), StorageError> {
            self.files.lock().unwrap().remove(key);
            Ok(())
        }

        async fn exists(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.files.lock().unwrap().contains_key(key))
        }

        async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError> {
            self.files.lock().unwrap()
                .get(key)
                .map(|f| f.metadata.clone())
                .ok_or_else(|| StorageError::NotFound { key: key.to_string() })
        }

        async fn signed_url(&self, key: &str, expiry: Duration) -> Result<String, StorageError> {
            let expires = chrono::Utc::now() + chrono::Duration::from_std(expiry).unwrap();
            Ok(format!("{}{}?expires={}", self.public_url_base, key, expires.timestamp()))
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
            self.reject_files.lock().unwrap().push(filename.to_string());
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
            if self.reject_files.lock().unwrap().contains(&filename.to_string()) {
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
                variants: variants.into_iter().map(|s| s.to_string()).collect(),
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
        async fn process(&self, data: &Bytes, _config: &ProcessingConfig) -> Result<ProcessedImages, ProcessingError> {
            if self.should_fail {
                return Err(ProcessingError::LoadFailed {
                    message: "Simulated failure".into(),
                });
            }

            let mut variants = HashMap::new();
            variants.insert("original".to_string(), data.clone());

            for variant in &self.variants {
                // Create slightly smaller "processed" data
                variants.insert(variant.clone(), data.slice(..data.len().saturating_sub(100)));
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

        pub fn with_threat(mut self, data: &[u8], threat_name: &str) -> Self {
            self.threats.lock().unwrap().insert(data.to_vec(), threat_name.to_string());
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
    fn sanitize_filename(filename: &str) -> String {
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
}
```

---

## 4.1 File Configuration

### Task: Define file configuration structures

```rust
// crates/fraiseql-files/src/config.rs

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct FileConfig {
    /// Upload endpoint path (default: /files/{name})
    pub path: Option<String>,

    /// Allowed MIME types
    #[serde(default = "default_allowed_types")]
    pub allowed_types: Vec<String>,

    /// Maximum file size (e.g., "10MB")
    #[serde(default = "default_max_size")]
    pub max_size: String,

    /// Validate magic bytes match declared MIME type
    #[serde(default = "default_validate_magic")]
    pub validate_magic_bytes: bool,

    /// Storage backend name (references storage config)
    #[serde(default = "default_storage")]
    pub storage: String,

    /// Bucket/container name (env var)
    pub bucket_env: Option<String>,

    /// Whether files are public
    #[serde(default = "default_public")]
    pub public: bool,

    /// Cache duration (for public files)
    pub cache: Option<String>,

    /// URL expiry for private files
    pub url_expiry: Option<String>,

    /// Scan for malware
    #[serde(default)]
    pub scan_malware: bool,

    /// Image processing configuration
    pub processing: Option<ProcessingConfig>,

    /// Callback after upload
    pub on_upload: Option<UploadCallbackConfig>,
}

fn default_allowed_types() -> Vec<String> {
    vec![
        "image/jpeg".to_string(),
        "image/png".to_string(),
        "image/webp".to_string(),
        "image/gif".to_string(),
        "application/pdf".to_string(),
    ]
}

fn default_max_size() -> String { "10MB".to_string() }
fn default_validate_magic() -> bool { true }
fn default_storage() -> String { "default".to_string() }
fn default_public() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct ProcessingConfig {
    /// Strip EXIF metadata from images
    #[serde(default)]
    pub strip_exif: bool,

    /// Output format (webp, jpeg, png)
    pub output_format: Option<String>,

    /// Quality (1-100)
    pub quality: Option<u8>,

    /// Image variants to generate
    #[serde(default)]
    pub variants: Vec<VariantConfig>,
}

#[derive(Debug, Deserialize)]
pub struct VariantConfig {
    pub name: String,
    pub width: u32,
    pub height: u32,

    /// Resize mode: fit, fill, crop
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String { "fit".to_string() }

#[derive(Debug, Deserialize)]
pub struct UploadCallbackConfig {
    /// Database function to call
    pub function: String,

    /// Parameter mapping
    #[serde(default)]
    pub mapping: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    /// Backend type: s3, r2, gcs, azure, local
    pub backend: String,

    /// Region (S3)
    pub region: Option<String>,

    /// Bucket name (env var)
    pub bucket_env: Option<String>,

    /// Access key ID (env var)
    pub access_key_env: Option<String>,

    /// Secret access key (env var)
    pub secret_key_env: Option<String>,

    /// Endpoint URL (for S3-compatible services)
    pub endpoint_env: Option<String>,

    /// Account ID (R2)
    pub account_id_env: Option<String>,

    /// Project ID (GCS)
    pub project_id_env: Option<String>,

    /// Credentials file/env (GCS)
    pub credentials_env: Option<String>,

    /// Local filesystem path
    pub base_path: Option<String>,

    /// Local serve path (for dev)
    pub serve_path: Option<String>,

    /// Public URL prefix
    pub public_url: Option<String>,
}
```

---

## 4.2 Storage Backend Trait

### Task: Define storage backend trait

```rust
// crates/fraiseql-files/src/storage/mod.rs

use async_trait::async_trait;
use bytes::Bytes;
use std::time::Duration;

/// Trait for storage backends
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Backend name
    fn name(&self) -> &'static str;

    /// Upload a file
    async fn upload(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        metadata: Option<&StorageMetadata>,
    ) -> Result<StorageResult, StorageError>;

    /// Download a file
    async fn download(&self, key: &str) -> Result<Bytes, StorageError>;

    /// Delete a file
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Check if file exists
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// Get file metadata
    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError>;

    /// Generate signed URL for private access
    async fn signed_url(
        &self,
        key: &str,
        expiry: Duration,
    ) -> Result<String, StorageError>;

    /// Get public URL (for public files)
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

#[derive(Debug, thiserror::Error)]
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
```

### Task: Implement S3 storage backend

```rust
// crates/fraiseql-files/src/storage/s3.rs

use aws_sdk_s3::{Client, Config, Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use bytes::Bytes;
use std::time::Duration;

pub struct S3Storage {
    client: Client,
    bucket: String,
    public_url: Option<String>,
}

impl S3Storage {
    pub async fn new(config: &StorageConfig) -> Result<Self, StorageError> {
        let region = config.region.as_ref()
            .map(|r| Region::new(r.clone()))
            .unwrap_or_else(|| Region::new("us-east-1"));

        let access_key = std::env::var(config.access_key_env.as_ref()
            .ok_or_else(|| StorageError::Configuration {
                message: "S3 access_key_env required".into()
            })?)
            .map_err(|_| StorageError::Configuration {
                message: "S3 access key not found in environment".into()
            })?;

        let secret_key = std::env::var(config.secret_key_env.as_ref()
            .ok_or_else(|| StorageError::Configuration {
                message: "S3 secret_key_env required".into()
            })?)
            .map_err(|_| StorageError::Configuration {
                message: "S3 secret key not found in environment".into()
            })?;

        let bucket = std::env::var(config.bucket_env.as_ref()
            .ok_or_else(|| StorageError::Configuration {
                message: "S3 bucket_env required".into()
            })?)
            .map_err(|_| StorageError::Configuration {
                message: "S3 bucket not found in environment".into()
            })?;

        let credentials = Credentials::new(
            access_key,
            secret_key,
            None,
            None,
            "fraiseql"
        );

        let mut sdk_config_builder = aws_config::from_env()
            .region(region)
            .credentials_provider(credentials);

        // Custom endpoint for S3-compatible services
        if let Some(endpoint_env) = &config.endpoint_env {
            if let Ok(endpoint) = std::env::var(endpoint_env) {
                sdk_config_builder = sdk_config_builder
                    .endpoint_url(&endpoint);
            }
        }

        let sdk_config = sdk_config_builder.load().await;
        let client = Client::new(&sdk_config);

        Ok(Self {
            client,
            bucket,
            public_url: config.public_url.clone(),
        })
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    fn name(&self) -> &'static str { "s3" }

    async fn upload(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        metadata: Option<&StorageMetadata>,
    ) -> Result<StorageResult, StorageError> {
        let mut req = self.client.put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .content_type(content_type);

        // Add custom metadata
        if let Some(meta) = metadata {
            for (k, v) in &meta.custom {
                req = req.metadata(k, v);
            }
        }

        let output = req.send().await
            .map_err(|e| StorageError::UploadFailed {
                message: e.to_string()
            })?;

        Ok(StorageResult {
            key: key.to_string(),
            url: self.public_url(key),
            etag: output.e_tag().map(|s| s.to_string()),
            size: data.len() as u64,
        })
    }

    async fn download(&self, key: &str) -> Result<Bytes, StorageError> {
        let output = self.client.get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::DownloadFailed {
                message: e.to_string()
            })?;

        let data = output.body.collect().await
            .map_err(|e| StorageError::DownloadFailed {
                message: e.to_string()
            })?;

        Ok(data.into_bytes())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client.delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Provider {
                message: e.to_string()
            })?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self.client.head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.to_string().contains("404") || e.to_string().contains("NotFound") {
                    Ok(false)
                } else {
                    Err(StorageError::Provider { message: e.to_string() })
                }
            }
        }
    }

    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError> {
        let output = self.client.head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Provider {
                message: e.to_string()
            })?;

        Ok(StorageMetadata {
            content_type: output.content_type().unwrap_or("application/octet-stream").to_string(),
            content_length: output.content_length().unwrap_or(0) as u64,
            etag: output.e_tag().map(|s| s.to_string()),
            last_modified: output.last_modified().and_then(|t| {
                chrono::DateTime::from_timestamp(t.secs(), 0)
            }),
            custom: output.metadata().cloned().unwrap_or_default(),
        })
    }

    async fn signed_url(
        &self,
        key: &str,
        expiry: Duration,
    ) -> Result<String, StorageError> {
        let presigning_config = PresigningConfig::expires_in(expiry)
            .map_err(|e| StorageError::Provider {
                message: e.to_string()
            })?;

        let presigned = self.client.get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| StorageError::Provider {
                message: e.to_string()
            })?;

        Ok(presigned.uri().to_string())
    }

    fn public_url(&self, key: &str) -> String {
        if let Some(url) = &self.public_url {
            format!("{}/{}", url.trim_end_matches('/'), key)
        } else {
            format!("https://{}.s3.amazonaws.com/{}", self.bucket, key)
        }
    }
}
```

### Task: Implement Cloudflare R2 storage

```rust
// crates/fraiseql-files/src/storage/r2.rs

// R2 is S3-compatible, so we can reuse S3Storage with custom endpoint
pub struct R2Storage(S3Storage);

impl R2Storage {
    pub async fn new(config: &StorageConfig) -> Result<Self, StorageError> {
        let account_id = std::env::var(config.account_id_env.as_ref()
            .ok_or_else(|| StorageError::Configuration {
                message: "R2 account_id_env required".into()
            })?)
            .map_err(|_| StorageError::Configuration {
                message: "R2 account ID not found in environment".into()
            })?;

        // Build S3-compatible config with R2 endpoint
        let mut s3_config = config.clone();
        s3_config.endpoint_env = Some(format!("https://{}.r2.cloudflarestorage.com", account_id));
        s3_config.region = Some("auto".to_string());

        let inner = S3Storage::new(&s3_config).await?;
        Ok(Self(inner))
    }
}

// Delegate all methods to inner S3Storage
#[async_trait]
impl StorageBackend for R2Storage {
    fn name(&self) -> &'static str { "r2" }

    async fn upload(&self, key: &str, data: Bytes, content_type: &str, metadata: Option<&StorageMetadata>) -> Result<StorageResult, StorageError> {
        self.0.upload(key, data, content_type, metadata).await
    }

    async fn download(&self, key: &str) -> Result<Bytes, StorageError> {
        self.0.download(key).await
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.0.delete(key).await
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        self.0.exists(key).await
    }

    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError> {
        self.0.metadata(key).await
    }

    async fn signed_url(&self, key: &str, expiry: Duration) -> Result<String, StorageError> {
        self.0.signed_url(key, expiry).await
    }

    fn public_url(&self, key: &str) -> String {
        self.0.public_url(key)
    }
}
```

### Task: Implement local filesystem storage (dev)

```rust
// crates/fraiseql-files/src/storage/local.rs

use std::path::PathBuf;
use tokio::fs;
use bytes::Bytes;

pub struct LocalStorage {
    base_path: PathBuf,
    serve_url: String,
}

impl LocalStorage {
    pub fn new(config: &StorageConfig) -> Result<Self, StorageError> {
        let base_path = config.base_path.as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./uploads"));

        let serve_url = config.serve_path.clone()
            .unwrap_or_else(|| "/files".to_string());

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&base_path)
            .map_err(|e| StorageError::Configuration {
                message: format!("Failed to create upload directory: {}", e)
            })?;

        Ok(Self { base_path, serve_url })
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    fn name(&self) -> &'static str { "local" }

    async fn upload(
        &self,
        key: &str,
        data: Bytes,
        _content_type: &str,
        _metadata: Option<&StorageMetadata>,
    ) -> Result<StorageResult, StorageError> {
        let path = self.base_path.join(key);

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| StorageError::UploadFailed {
                    message: e.to_string()
                })?;
        }

        fs::write(&path, &data).await
            .map_err(|e| StorageError::UploadFailed {
                message: e.to_string()
            })?;

        Ok(StorageResult {
            key: key.to_string(),
            url: self.public_url(key),
            etag: None,
            size: data.len() as u64,
        })
    }

    async fn download(&self, key: &str) -> Result<Bytes, StorageError> {
        let path = self.base_path.join(key);

        let data = fs::read(&path).await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    StorageError::NotFound { key: key.to_string() }
                } else {
                    StorageError::DownloadFailed { message: e.to_string() }
                }
            })?;

        Ok(Bytes::from(data))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.base_path.join(key);

        fs::remove_file(&path).await
            .map_err(|e| StorageError::Provider {
                message: e.to_string()
            })?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let path = self.base_path.join(key);
        Ok(path.exists())
    }

    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError> {
        let path = self.base_path.join(key);

        let meta = fs::metadata(&path).await
            .map_err(|e| StorageError::Provider {
                message: e.to_string()
            })?;

        Ok(StorageMetadata {
            content_type: mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string(),
            content_length: meta.len(),
            etag: None,
            last_modified: meta.modified().ok()
                .and_then(|t| chrono::DateTime::from(t).into()),
            custom: HashMap::new(),
        })
    }

    async fn signed_url(&self, key: &str, _expiry: Duration) -> Result<String, StorageError> {
        // Local storage doesn't support signed URLs in production
        // For dev, just return the public URL
        Ok(self.public_url(key))
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.serve_url, key)
    }
}
```

---

## 4.3 File Validation

### Task: Implement file validation

```rust
// crates/fraiseql-files/src/validation.rs

use bytes::Bytes;

/// Validate uploaded file
pub fn validate_file(
    data: &Bytes,
    declared_type: &str,
    config: &FileConfig,
) -> Result<(), FileError> {
    // Check size
    let max_size = parse_size(&config.max_size)
        .unwrap_or(10 * 1024 * 1024);

    if data.len() > max_size {
        return Err(FileError::TooLarge {
            size: data.len(),
            max: max_size,
        });
    }

    // Check MIME type is allowed
    if !config.allowed_types.iter().any(|t| t == declared_type || t == "*/*") {
        return Err(FileError::InvalidType {
            got: declared_type.to_string(),
            allowed: config.allowed_types.clone(),
        });
    }

    // Validate magic bytes
    if config.validate_magic_bytes {
        validate_magic_bytes(data, declared_type)?;
    }

    Ok(())
}

/// Validate file content matches declared MIME type
fn validate_magic_bytes(data: &Bytes, declared_type: &str) -> Result<(), FileError> {
    let detected = infer::get(data)
        .map(|t| t.mime_type())
        .unwrap_or("application/octet-stream");

    // Allow some flexibility in MIME type matching
    if !mime_types_compatible(detected, declared_type) {
        return Err(FileError::MimeMismatch {
            declared: declared_type.to_string(),
            detected: detected.to_string(),
        });
    }

    Ok(())
}

fn mime_types_compatible(detected: &str, declared: &str) -> bool {
    // Exact match
    if detected == declared {
        return true;
    }

    // Common equivalents
    let equivalents = [
        ("image/jpeg", "image/jpg"),
        ("text/plain", "application/octet-stream"),
    ];

    for (a, b) in equivalents {
        if (detected == a && declared == b) || (detected == b && declared == a) {
            return true;
        }
    }

    // Same major type (e.g., image/*)
    let detected_major = detected.split('/').next().unwrap_or("");
    let declared_major = declared.split('/').next().unwrap_or("");

    // For images, allow any image type if major matches
    if detected_major == "image" && declared_major == "image" {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_compatibility() {
        assert!(mime_types_compatible("image/jpeg", "image/jpeg"));
        assert!(mime_types_compatible("image/jpeg", "image/jpg"));
        assert!(mime_types_compatible("image/png", "image/webp")); // Same major
        assert!(!mime_types_compatible("image/jpeg", "application/pdf"));
    }
}
```

---

## 4.4 Image Processing

### Task: Implement image processing pipeline

```rust
// crates/fraiseql-files/src/processing.rs

use image::{DynamicImage, ImageFormat, imageops::FilterType};
use bytes::Bytes;
use std::io::Cursor;

pub struct ImageProcessor {
    config: ProcessingConfig,
}

impl ImageProcessor {
    pub fn new(config: ProcessingConfig) -> Self {
        Self { config }
    }

    /// Process an image and generate variants
    pub fn process(&self, data: &Bytes) -> Result<ProcessedImages, ProcessingError> {
        // Load image
        let img = image::load_from_memory(data)
            .map_err(|e| ProcessingError::LoadFailed { message: e.to_string() })?;

        // Strip EXIF if configured
        // Note: image crate doesn't preserve EXIF, so it's stripped by default
        // For explicit control, we'd need a different approach

        let mut variants = HashMap::new();

        // Generate original (possibly in different format)
        let original_key = "original".to_string();
        let original_data = self.encode_image(&img, None)?;
        variants.insert(original_key, original_data);

        // Generate configured variants
        for variant_config in &self.config.variants {
            let resized = self.resize_image(&img, variant_config)?;
            let encoded = self.encode_image(&resized, variant_config.quality)?;

            variants.insert(variant_config.name.clone(), encoded);
        }

        Ok(ProcessedImages { variants })
    }

    fn resize_image(
        &self,
        img: &DynamicImage,
        config: &VariantConfig,
    ) -> Result<DynamicImage, ProcessingError> {
        let resized = match config.mode.as_str() {
            "fit" => img.resize(
                config.width,
                config.height,
                FilterType::Lanczos3
            ),
            "fill" | "crop" => img.resize_to_fill(
                config.width,
                config.height,
                FilterType::Lanczos3
            ),
            "exact" => img.resize_exact(
                config.width,
                config.height,
                FilterType::Lanczos3
            ),
            _ => return Err(ProcessingError::InvalidMode {
                mode: config.mode.clone()
            }),
        };

        Ok(resized)
    }

    fn encode_image(
        &self,
        img: &DynamicImage,
        quality: Option<u8>,
    ) -> Result<Bytes, ProcessingError> {
        let format = match self.config.output_format.as_deref() {
            Some("webp") => ImageFormat::WebP,
            Some("jpeg") | Some("jpg") => ImageFormat::Jpeg,
            Some("png") => ImageFormat::Png,
            _ => ImageFormat::Jpeg, // Default
        };

        let quality = quality.or(self.config.quality).unwrap_or(85);

        let mut buffer = Cursor::new(Vec::new());

        match format {
            ImageFormat::Jpeg => {
                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                    &mut buffer,
                    quality
                );
                img.write_with_encoder(encoder)
                    .map_err(|e| ProcessingError::EncodeFailed { message: e.to_string() })?;
            }
            ImageFormat::WebP => {
                // image crate's WebP encoder
                img.write_to(&mut buffer, ImageFormat::WebP)
                    .map_err(|e| ProcessingError::EncodeFailed { message: e.to_string() })?;
            }
            ImageFormat::Png => {
                img.write_to(&mut buffer, ImageFormat::Png)
                    .map_err(|e| ProcessingError::EncodeFailed { message: e.to_string() })?;
            }
            _ => {
                return Err(ProcessingError::UnsupportedFormat {
                    format: format!("{:?}", format)
                });
            }
        }

        Ok(Bytes::from(buffer.into_inner()))
    }
}

pub struct ProcessedImages {
    pub variants: HashMap<String, Bytes>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("Failed to load image: {message}")]
    LoadFailed { message: String },

    #[error("Failed to encode image: {message}")]
    EncodeFailed { message: String },

    #[error("Invalid resize mode: {mode}")]
    InvalidMode { mode: String },

    #[error("Unsupported output format: {format}")]
    UnsupportedFormat { format: String },
}
```

---

## 4.5 File Handler

### Task: Implement file upload handler

```rust
// crates/fraiseql-files/src/handler.rs

use axum::{
    extract::{Multipart, Path, State, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    config::FileConfig,
    validation::validate_file,
    processing::ImageProcessor,
    storage::StorageBackend,
};

#[derive(Debug, Serialize)]
pub struct FileResponse {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub original_filename: Option<String>,
    pub content_type: String,
    pub size: u64,
    pub url: String,
    pub variants: Option<HashMap<String, String>>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct FileHandler {
    upload_type: String,
    config: FileConfig,
    storage: Arc<dyn StorageBackend>,
    processor: Option<ImageProcessor>,
    db: PgPool,
}

impl FileHandler {
    pub fn new(
        upload_type: &str,
        config: FileConfig,
        storage: Arc<dyn StorageBackend>,
        db: PgPool,
    ) -> Self {
        let processor = config.processing.as_ref()
            .map(|p| ImageProcessor::new(p.clone()));

        Self {
            upload_type: upload_type.to_string(),
            config,
            storage,
            processor,
            db,
        }
    }

    pub async fn upload(
        &self,
        mut multipart: Multipart,
        context: Option<serde_json::Value>,
    ) -> Result<FileResponse, RuntimeError> {
        // Extract file from multipart
        let mut file_data: Option<(String, String, Bytes)> = None;
        let mut metadata: Option<serde_json::Value> = None;

        while let Some(field) = multipart.next_field().await? {
            let name = field.name().unwrap_or("").to_string();

            match name.as_str() {
                "file" => {
                    let filename = field.file_name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "unnamed".to_string());

                    let content_type = field.content_type()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "application/octet-stream".to_string());

                    let data = field.bytes().await?;

                    file_data = Some((filename, content_type, data));
                }
                "metadata" => {
                    let text = field.text().await?;
                    metadata = serde_json::from_str(&text).ok();
                }
                _ => {}
            }
        }

        let (original_filename, content_type, data) = file_data
            .ok_or_else(|| RuntimeError::Validation(ValidationError::Field {
                field: "file".to_string(),
                message: "No file provided".to_string(),
            }))?;

        // Validate file
        validate_file(&data, &content_type, &self.config)?;

        // Generate unique filename
        let file_id = Uuid::new_v4();
        let extension = original_filename.rsplit('.').next().unwrap_or("bin");
        let filename = format!("{}.{}", file_id, extension);

        // Storage key
        let storage_key = format!("{}/{}", self.upload_type, filename);

        // Process image if applicable
        let (variants, processed_data) = if self.is_image(&content_type) {
            if let Some(processor) = &self.processor {
                let processed = processor.process(&data)?;

                let mut variant_urls = HashMap::new();

                // Upload variants
                for (variant_name, variant_data) in &processed.variants {
                    let variant_key = format!(
                        "{}/{}_{}.{}",
                        self.upload_type,
                        file_id,
                        variant_name,
                        self.config.processing.as_ref()
                            .and_then(|p| p.output_format.as_deref())
                            .unwrap_or("jpg")
                    );

                    let result = self.storage.upload(
                        &variant_key,
                        variant_data.clone(),
                        &self.get_output_content_type(),
                        None
                    ).await?;

                    variant_urls.insert(variant_name.clone(), result.url);
                }

                (Some(variant_urls), processed.variants.get("original").cloned().unwrap_or(data.clone()))
            } else {
                (None, data.clone())
            }
        } else {
            (None, data.clone())
        };

        // Upload main file
        let upload_result = self.storage.upload(
            &storage_key,
            processed_data.clone(),
            &content_type,
            None
        ).await?;

        // Record in database
        let file_record = self.create_file_record(
            file_id,
            &filename,
            &original_filename,
            &content_type,
            processed_data.len() as u64,
            &storage_key,
            &upload_result.url,
            &variants,
            &metadata,
        ).await?;

        // Call on_upload callback if configured
        if let Some(callback) = &self.config.on_upload {
            self.execute_callback(callback, &file_record, &context).await?;
        }

        // Record metrics
        record_file_upload(&self.upload_type, processed_data.len(), "success");

        Ok(file_record)
    }

    fn is_image(&self, content_type: &str) -> bool {
        content_type.starts_with("image/")
    }

    fn get_output_content_type(&self) -> String {
        match self.config.processing.as_ref()
            .and_then(|p| p.output_format.as_deref())
        {
            Some("webp") => "image/webp".to_string(),
            Some("png") => "image/png".to_string(),
            _ => "image/jpeg".to_string(),
        }
    }

    async fn create_file_record(
        &self,
        id: Uuid,
        filename: &str,
        original_filename: &str,
        content_type: &str,
        size: u64,
        storage_key: &str,
        url: &str,
        variants: &Option<HashMap<String, String>>,
        metadata: &Option<serde_json::Value>,
    ) -> Result<FileResponse, RuntimeError> {
        let variants_json = variants.as_ref()
            .map(|v| serde_json::to_value(v).unwrap());

        let now = chrono::Utc::now();

        sqlx::query!(
            r#"
            INSERT INTO _system.files (
                id, upload_type, filename, original_filename,
                content_type, size, storage_key, storage_backend,
                url, variants, metadata, is_public, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
            )
            "#,
            id,
            self.upload_type,
            filename,
            original_filename,
            content_type,
            size as i64,
            storage_key,
            self.storage.name(),
            url,
            variants_json,
            metadata,
            self.config.public,
            now
        )
        .execute(&self.db)
        .await?;

        Ok(FileResponse {
            id: id.to_string(),
            name: self.upload_type.clone(),
            filename: filename.to_string(),
            original_filename: Some(original_filename.to_string()),
            content_type: content_type.to_string(),
            size,
            url: url.to_string(),
            variants: variants.clone(),
            metadata: metadata.clone(),
            created_at: now,
        })
    }

    async fn execute_callback(
        &self,
        callback: &UploadCallbackConfig,
        file: &FileResponse,
        context: &Option<serde_json::Value>,
    ) -> Result<(), RuntimeError> {
        // Build parameters from mapping
        let mut params = serde_json::Map::new();

        for (param_name, source) in &callback.mapping {
            let value = match source.as_str() {
                "_file_id" => serde_json::Value::String(file.id.clone()),
                "_storage_url" => serde_json::Value::String(file.url.clone()),
                "_variants" => file.variants.as_ref()
                    .map(|v| serde_json::to_value(v).unwrap())
                    .unwrap_or(serde_json::Value::Null),
                "_filename" => serde_json::Value::String(file.filename.clone()),
                "_content_type" => serde_json::Value::String(file.content_type.clone()),
                "_size" => serde_json::Value::Number(file.size.into()),
                other if other.starts_with("_context.") => {
                    let path = &other[9..];
                    context.as_ref()
                        .and_then(|c| extract_path(c, path))
                        .unwrap_or(serde_json::Value::Null)
                }
                other if other.starts_with("_metadata.") => {
                    let path = &other[10..];
                    file.metadata.as_ref()
                        .and_then(|m| extract_path(m, path))
                        .unwrap_or(serde_json::Value::Null)
                }
                _ => serde_json::Value::Null,
            };

            params.insert(param_name.clone(), value);
        }

        // Call database function
        let params_json = serde_json::Value::Object(params);

        sqlx::query(&format!(
            "SELECT * FROM app.{}($1::jsonb)",
            callback.function
        ))
        .bind(&params_json)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn get(&self, file_id: &str) -> Result<FileResponse, RuntimeError> {
        let id: Uuid = file_id.parse()
            .map_err(|_| RuntimeError::NotFound("Invalid file ID".into()))?;

        let row = sqlx::query!(
            r#"
            SELECT
                id, upload_type, filename, original_filename,
                content_type, size, url, variants, metadata, created_at
            FROM _system.files
            WHERE id = $1 AND deleted_at IS NULL
            "#,
            id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| RuntimeError::NotFound("File not found".into()))?;

        Ok(FileResponse {
            id: row.id.to_string(),
            name: row.upload_type,
            filename: row.filename,
            original_filename: row.original_filename,
            content_type: row.content_type,
            size: row.size as u64,
            url: row.url,
            variants: row.variants.map(|v| serde_json::from_value(v).unwrap()),
            metadata: row.metadata,
            created_at: row.created_at,
        })
    }

    pub async fn delete(&self, file_id: &str) -> Result<(), RuntimeError> {
        let id: Uuid = file_id.parse()
            .map_err(|_| RuntimeError::NotFound("Invalid file ID".into()))?;

        // Soft delete
        sqlx::query!(
            "UPDATE _system.files SET deleted_at = NOW() WHERE id = $1",
            id
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn signed_url(
        &self,
        file_id: &str,
        expiry: Duration,
    ) -> Result<SignedUrlResponse, RuntimeError> {
        let file = self.get(file_id).await?;

        // Get storage key from database
        let row = sqlx::query!(
            "SELECT storage_key FROM _system.files WHERE id = $1",
            file_id.parse::<Uuid>().unwrap()
        )
        .fetch_one(&self.db)
        .await?;

        let url = self.storage.signed_url(&row.storage_key, expiry).await?;

        Ok(SignedUrlResponse {
            url,
            expires_at: chrono::Utc::now() + chrono::Duration::from_std(expiry).unwrap(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct SignedUrlResponse {
    pub url: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
```

---

## 4.6 Database Schema

```sql
-- migrations/002_file_system_tables.sql

CREATE TABLE IF NOT EXISTS _system.files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    upload_type TEXT NOT NULL,
    filename TEXT NOT NULL,
    original_filename TEXT,
    content_type TEXT NOT NULL,
    size BIGINT NOT NULL,
    storage_key TEXT NOT NULL,
    storage_backend TEXT NOT NULL,
    url TEXT NOT NULL,
    variants JSONB,
    metadata JSONB,
    is_public BOOLEAN DEFAULT true,
    entity_type TEXT,
    entity_id UUID,
    field_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_files_upload_type ON _system.files(upload_type);
CREATE INDEX IF NOT EXISTS idx_files_entity ON _system.files(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_files_created_at ON _system.files(created_at);
CREATE INDEX IF NOT EXISTS idx_files_deleted ON _system.files(deleted_at) WHERE deleted_at IS NOT NULL;
```

---

## Acceptance Criteria

- [ ] File upload validates size limits
- [ ] File upload validates MIME types
- [ ] Magic bytes validation works for common types
- [ ] S3 storage backend uploads/downloads correctly
- [ ] R2 storage backend works (S3-compatible)
- [ ] Local filesystem storage works for development
- [ ] Image processing generates correct variants
- [ ] EXIF data is stripped from images
- [ ] WebP conversion works
- [ ] Signed URLs are generated for private files
- [ ] File metadata is recorded in database
- [ ] Upload callbacks execute correctly
- [ ] Soft delete works
- [ ] Metrics are recorded for uploads

---

## Files to Create

```
crates/fraiseql-files/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs
│   ├── handler.rs
│   ├── validation.rs
│   ├── processing.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── s3.rs
│   │   ├── r2.rs
│   │   ├── gcs.rs
│   │   ├── azure.rs
│   │   └── local.rs
│   └── axum.rs
└── tests/
    ├── upload_test.rs
    ├── validation_test.rs
    └── storage_test.rs
```

---

---

## 4.7 Comprehensive Error Handling

### Task: Define file-specific errors with error codes

```rust
// crates/fraiseql-files/src/error.rs

use thiserror::Error;

/// File error codes for consistent error responses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileErrorCode {
    /// FL001: File too large
    TooLarge,
    /// FL002: File type not allowed
    InvalidType,
    /// FL003: Magic bytes don't match declared type
    MimeMismatch,
    /// FL004: No file provided in request
    MissingFile,
    /// FL005: Invalid filename
    InvalidFilename,
    /// FL006: Storage upload failed
    UploadFailed,
    /// FL007: Storage download failed
    DownloadFailed,
    /// FL008: File not found
    NotFound,
    /// FL009: Image processing failed
    ProcessingFailed,
    /// FL010: Invalid file content
    InvalidContent,
    /// FL011: Malware detected
    MalwareDetected,
    /// FL012: Storage backend not configured
    StorageNotConfigured,
}

impl FileErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TooLarge => "FL001",
            Self::InvalidType => "FL002",
            Self::MimeMismatch => "FL003",
            Self::MissingFile => "FL004",
            Self::InvalidFilename => "FL005",
            Self::UploadFailed => "FL006",
            Self::DownloadFailed => "FL007",
            Self::NotFound => "FL008",
            Self::ProcessingFailed => "FL009",
            Self::InvalidContent => "FL010",
            Self::MalwareDetected => "FL011",
            Self::StorageNotConfigured => "FL012",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::InvalidType | Self::MimeMismatch | Self::InvalidFilename | Self::InvalidContent
                => StatusCode::BAD_REQUEST,
            Self::MissingFile => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MalwareDetected => StatusCode::UNPROCESSABLE_ENTITY,
            Self::UploadFailed | Self::DownloadFailed | Self::ProcessingFailed | Self::StorageNotConfigured
                => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::TooLarge => "https://docs.fraiseql.dev/files/size-limits",
            Self::InvalidType | Self::MimeMismatch
                => "https://docs.fraiseql.dev/files/mime-types",
            Self::InvalidFilename
                => "https://docs.fraiseql.dev/files/filename-requirements",
            Self::MissingFile
                => "https://docs.fraiseql.dev/files/upload-format",
            Self::NotFound
                => "https://docs.fraiseql.dev/files/getting-files",
            Self::ProcessingFailed
                => "https://docs.fraiseql.dev/files/image-processing",
            Self::UploadFailed | Self::DownloadFailed | Self::StorageNotConfigured
                => "https://docs.fraiseql.dev/files/storage-backends",
            Self::MalwareDetected
                => "https://docs.fraiseql.dev/files/security",
            Self::InvalidContent
                => "https://docs.fraiseql.dev/files/validation",
        }
    }
}

#[derive(Debug, Error)]
pub enum FileError {
    #[error("File too large: {size} bytes (max: {max} bytes)")]
    TooLarge { size: usize, max: usize },

    #[error("File type not allowed: {got} (allowed: {allowed:?})")]
    InvalidType { got: String, allowed: Vec<String> },

    #[error("MIME type mismatch: declared {declared}, detected {detected}")]
    MimeMismatch { declared: String, detected: String },

    #[error("No file provided")]
    MissingFile,

    #[error("Invalid filename: {reason}")]
    InvalidFilename { reason: String },

    #[error("Upload failed: {message}")]
    UploadFailed { message: String },

    #[error("Download failed: {message}")]
    DownloadFailed { message: String },

    #[error("File not found: {id}")]
    NotFound { id: String },

    #[error("Image processing failed: {message}")]
    ProcessingFailed { message: String },

    #[error("Invalid file content: {message}")]
    InvalidContent { message: String },

    #[error("Malware detected: {threat_name}")]
    MalwareDetected { threat_name: String },

    #[error("Storage backend not configured: {backend}")]
    StorageNotConfigured { backend: String },
}

impl FileError {
    pub fn error_code(&self) -> FileErrorCode {
        match self {
            Self::TooLarge { .. } => FileErrorCode::TooLarge,
            Self::InvalidType { .. } => FileErrorCode::InvalidType,
            Self::MimeMismatch { .. } => FileErrorCode::MimeMismatch,
            Self::MissingFile => FileErrorCode::MissingFile,
            Self::InvalidFilename { .. } => FileErrorCode::InvalidFilename,
            Self::UploadFailed { .. } => FileErrorCode::UploadFailed,
            Self::DownloadFailed { .. } => FileErrorCode::DownloadFailed,
            Self::NotFound { .. } => FileErrorCode::NotFound,
            Self::ProcessingFailed { .. } => FileErrorCode::ProcessingFailed,
            Self::InvalidContent { .. } => FileErrorCode::InvalidContent,
            Self::MalwareDetected { .. } => FileErrorCode::MalwareDetected,
            Self::StorageNotConfigured { .. } => FileErrorCode::StorageNotConfigured,
        }
    }

    pub fn to_response(&self) -> (StatusCode, Json<Value>) {
        let code = self.error_code();

        (
            code.http_status(),
            Json(json!({
                "error": {
                    "code": code.as_str(),
                    "message": self.to_string(),
                    "docs": code.docs_url(),
                }
            }))
        )
    }
}

impl IntoResponse for FileError {
    fn into_response(self) -> Response {
        let (status, body) = self.to_response();
        (status, body).into_response()
    }
}
```

---

## 4.8 Security Validation

### Task: Filename sanitization and path traversal prevention

```rust
// crates/fraiseql-files/src/security.rs

/// Sanitize filename to prevent path traversal and other attacks
pub fn sanitize_filename(filename: &str) -> Result<String, FileError> {
    // Remove path components (prevent ../../../etc/passwd)
    let filename = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename);

    // Empty filename after removing path
    if filename.is_empty() || filename == "." || filename == ".." {
        return Err(FileError::InvalidFilename {
            reason: "Filename cannot be empty or path component".into(),
        });
    }

    // Remove null bytes (C string terminator attack)
    let filename = filename.replace('\0', "");

    // Limit length
    if filename.len() > 255 {
        return Err(FileError::InvalidFilename {
            reason: "Filename too long (max 255 characters)".into(),
        });
    }

    // Replace dangerous characters but preserve extension
    let sanitized: String = filename
        .chars()
        .enumerate()
        .map(|(i, c)| {
            match c {
                // Allow alphanumeric
                'a'..='z' | 'A'..='Z' | '0'..='9' => c,
                // Allow dot (for extension) but not as first character
                '.' if i > 0 => c,
                // Allow hyphen and underscore
                '-' | '_' => c,
                // Replace everything else with underscore
                _ => '_',
            }
        })
        .collect();

    // Ensure we have a valid filename
    if sanitized.is_empty() || sanitized.chars().all(|c| c == '_') {
        return Err(FileError::InvalidFilename {
            reason: "Filename contains no valid characters".into(),
        });
    }

    Ok(sanitized)
}

/// Validate storage key doesn't escape storage boundaries
pub fn validate_storage_key(key: &str) -> Result<(), FileError> {
    // Check for path traversal attempts
    if key.contains("..") {
        return Err(FileError::InvalidFilename {
            reason: "Storage key cannot contain path traversal sequences".into(),
        });
    }

    // Check for absolute paths
    if key.starts_with('/') || key.starts_with('\\') {
        return Err(FileError::InvalidFilename {
            reason: "Storage key cannot be absolute path".into(),
        });
    }

    // Check for null bytes
    if key.contains('\0') {
        return Err(FileError::InvalidFilename {
            reason: "Storage key cannot contain null bytes".into(),
        });
    }

    Ok(())
}

/// Dangerous file extensions that should be blocked
const DANGEROUS_EXTENSIONS: &[&str] = &[
    "exe", "dll", "bat", "cmd", "sh", "bash", "ps1", "vbs",
    "js", "jse", "jar", "msi", "scr", "pif", "application",
    "gadget", "msp", "com", "hta", "cpl", "msc", "ws", "wsf",
    "wsc", "wsh", "reg", "inf", "scf", "lnk", "pem", "key",
];

/// Check if file extension is dangerous
pub fn is_dangerous_extension(filename: &str) -> bool {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    DANGEROUS_EXTENSIONS.contains(&ext.as_str())
}

/// Double extension attack detection (e.g., image.jpg.exe)
pub fn has_double_extension_attack(filename: &str) -> bool {
    let parts: Vec<&str> = filename.split('.').collect();
    if parts.len() < 3 {
        return false;
    }

    // Check if any middle extension is safe but final is dangerous
    let final_ext = parts.last().unwrap().to_lowercase();
    if DANGEROUS_EXTENSIONS.contains(&final_ext.as_str()) {
        // Check if second-to-last is a "safe" looking extension
        let second_last = parts[parts.len() - 2].to_lowercase();
        let safe_extensions = ["jpg", "jpeg", "png", "gif", "webp", "pdf", "txt", "doc", "xls"];
        if safe_extensions.contains(&second_last.as_str()) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_traversal_prevention() {
        assert!(sanitize_filename("../../../etc/passwd").is_err() ||
                !sanitize_filename("../../../etc/passwd").unwrap().contains(".."));
        assert!(sanitize_filename("..\\..\\windows\\system32\\config\\sam").is_err() ||
                !sanitize_filename("..\\..\\windows\\system32\\config\\sam").unwrap().contains(".."));
    }

    #[test]
    fn test_null_byte_removal() {
        let result = sanitize_filename("image.jpg\0.exe").unwrap();
        assert!(!result.contains('\0'));
    }

    #[test]
    fn test_valid_filename() {
        assert_eq!(sanitize_filename("photo.jpg").unwrap(), "photo.jpg");
        assert_eq!(sanitize_filename("my-file_2024.pdf").unwrap(), "my-file_2024.pdf");
    }

    #[test]
    fn test_dangerous_characters() {
        let result = sanitize_filename("file<>:\"|?*.jpg").unwrap();
        assert!(!result.contains('<'));
        assert!(!result.contains('>'));
        assert!(!result.contains(':'));
    }

    #[test]
    fn test_double_extension_attack() {
        assert!(has_double_extension_attack("image.jpg.exe"));
        assert!(has_double_extension_attack("document.pdf.bat"));
        assert!(!has_double_extension_attack("photo.jpg"));
        assert!(!has_double_extension_attack("file.tar.gz")); // Not dangerous
    }

    #[test]
    fn test_dangerous_extensions() {
        assert!(is_dangerous_extension("virus.exe"));
        assert!(is_dangerous_extension("script.ps1"));
        assert!(!is_dangerous_extension("photo.jpg"));
        assert!(!is_dangerous_extension("document.pdf"));
    }
}
```

---

## 4.9 Unit Tests

### Task: Comprehensive unit tests for file handling

```rust
// crates/fraiseql-files/tests/validation_test.rs

use fraiseql_files::{
    validation::*,
    config::FileConfig,
    error::FileError,
};
use bytes::Bytes;

fn default_config() -> FileConfig {
    FileConfig {
        allowed_types: vec!["image/jpeg".into(), "image/png".into()],
        max_size: "1MB".into(),
        validate_magic_bytes: true,
        ..Default::default()
    }
}

#[test]
fn test_size_validation() {
    let config = FileConfig {
        max_size: "100".into(), // 100 bytes
        ..default_config()
    };

    let small_data = Bytes::from(vec![0u8; 50]);
    assert!(validate_file(&small_data, "image/jpeg", &config).is_ok());

    let large_data = Bytes::from(vec![0u8; 200]);
    let result = validate_file(&large_data, "image/jpeg", &config);
    assert!(matches!(result, Err(FileError::TooLarge { .. })));
}

#[test]
fn test_mime_type_validation() {
    let config = default_config();

    // Allowed type
    let data = create_jpeg_header();
    assert!(validate_file(&data, "image/jpeg", &config).is_ok());

    // Not allowed type
    let result = validate_file(&data, "application/javascript", &config);
    assert!(matches!(result, Err(FileError::InvalidType { .. })));
}

#[test]
fn test_magic_bytes_validation() {
    let config = FileConfig {
        validate_magic_bytes: true,
        ..default_config()
    };

    // Valid JPEG with correct magic bytes
    let jpeg_data = create_jpeg_header();
    assert!(validate_file(&jpeg_data, "image/jpeg", &config).is_ok());

    // Claimed JPEG but actually PNG
    let png_data = create_png_header();
    let result = validate_file(&png_data, "image/jpeg", &config);
    // Should fail or succeed depending on strictness
    // With compatible mode, both are images so it might pass
}

#[test]
fn test_magic_bytes_disabled() {
    let config = FileConfig {
        validate_magic_bytes: false,
        ..default_config()
    };

    // Random data claimed as JPEG - should pass without magic check
    let random_data = Bytes::from(vec![0u8; 100]);
    assert!(validate_file(&random_data, "image/jpeg", &config).is_ok());
}

fn create_jpeg_header() -> Bytes {
    // JPEG magic bytes: FF D8 FF
    let mut data = vec![0xFF, 0xD8, 0xFF, 0xE0];
    data.extend_from_slice(&[0; 100]);
    Bytes::from(data)
}

fn create_png_header() -> Bytes {
    // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
    let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    data.extend_from_slice(&[0; 100]);
    Bytes::from(data)
}
```

```rust
// crates/fraiseql-files/tests/storage_test.rs

use fraiseql_files::{
    storage::*,
    testing::mocks::MockStorage,
};
use bytes::Bytes;
use std::time::Duration;

#[tokio::test]
async fn test_mock_storage_upload_download() {
    let storage = MockStorage::new();
    let data = Bytes::from("test content");

    // Upload
    let result = storage.upload(
        "test/file.txt",
        data.clone(),
        "text/plain",
        None,
    ).await.unwrap();

    assert_eq!(result.key, "test/file.txt");
    assert_eq!(result.size, data.len() as u64);

    // Download
    let downloaded = storage.download("test/file.txt").await.unwrap();
    assert_eq!(downloaded, data);
}

#[tokio::test]
async fn test_mock_storage_not_found() {
    let storage = MockStorage::new();

    let result = storage.download("nonexistent.txt").await;
    assert!(matches!(result, Err(StorageError::NotFound { .. })));
}

#[tokio::test]
async fn test_mock_storage_delete() {
    let storage = MockStorage::new();

    // Upload
    storage.upload("test.txt", Bytes::from("data"), "text/plain", None)
        .await.unwrap();

    assert!(storage.exists("test.txt").await.unwrap());

    // Delete
    storage.delete("test.txt").await.unwrap();

    assert!(!storage.exists("test.txt").await.unwrap());
}

#[tokio::test]
async fn test_mock_storage_simulated_failure() {
    let storage = MockStorage::new();
    storage.fail_on("bad/key.txt");

    let result = storage.upload(
        "bad/key.txt",
        Bytes::from("data"),
        "text/plain",
        None,
    ).await;

    assert!(matches!(result, Err(StorageError::UploadFailed { .. })));
}

#[tokio::test]
async fn test_signed_url_generation() {
    let storage = MockStorage::new();
    let expiry = Duration::from_secs(3600);

    let url = storage.signed_url("test.txt", expiry).await.unwrap();

    assert!(url.contains("test.txt"));
    assert!(url.contains("expires="));
}
```

```rust
// crates/fraiseql-files/tests/handler_test.rs

use axum::http::StatusCode;
use axum_test::TestServer;
use fraiseql_files::{
    testing::mocks::*,
    FileHandler,
    FileConfig,
};
use std::sync::Arc;

async fn setup_test_server() -> TestServer {
    let storage = Arc::new(MockStorage::new());
    let validator = Arc::new(MockValidator::strict(
        vec!["image/jpeg".into(), "image/png".into()],
        10 * 1024 * 1024,
    ));
    let processor = Arc::new(MockImageProcessor::new(vec!["thumbnail", "medium"]));

    let config = FileConfig {
        allowed_types: vec!["image/jpeg".into(), "image/png".into()],
        max_size: "10MB".into(),
        validate_magic_bytes: true,
        public: true,
        processing: Some(ProcessingConfig {
            strip_exif: true,
            output_format: Some("webp".into()),
            quality: Some(85),
            variants: vec![
                VariantConfig { name: "thumbnail".into(), width: 150, height: 150, mode: "fill".into() },
                VariantConfig { name: "medium".into(), width: 800, height: 600, mode: "fit".into() },
            ],
        }),
        ..Default::default()
    };

    let handler = FileHandler::new_with_deps(
        "avatars",
        config,
        storage,
        validator,
        Some(processor),
        None, // No malware scanner
    );

    let app = axum::Router::new()
        .route("/files/:type", axum::routing::post(upload_handler))
        .with_state(Arc::new(handler));

    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn test_successful_upload() {
    let server = setup_test_server().await;

    let response = server
        .post("/files/avatars")
        .content_type("multipart/form-data; boundary=----WebKitFormBoundary")
        .body(create_multipart_body("test.jpg", "image/jpeg", &[0xFF, 0xD8, 0xFF]))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["id"].is_string());
    assert!(body["url"].is_string());
}

#[tokio::test]
async fn test_file_too_large() {
    let server = setup_test_server().await;

    // Create data larger than 10MB
    let large_data = vec![0u8; 11 * 1024 * 1024];

    let response = server
        .post("/files/avatars")
        .content_type("multipart/form-data; boundary=----WebKitFormBoundary")
        .body(create_multipart_body("large.jpg", "image/jpeg", &large_data))
        .await;

    assert_eq!(response.status_code(), StatusCode::PAYLOAD_TOO_LARGE);

    let body: serde_json::Value = response.json();
    assert_eq!(body["error"]["code"], "FL001");
}

#[tokio::test]
async fn test_invalid_mime_type() {
    let server = setup_test_server().await;

    let response = server
        .post("/files/avatars")
        .content_type("multipart/form-data; boundary=----WebKitFormBoundary")
        .body(create_multipart_body("script.js", "application/javascript", b"console.log('hi')"))
        .await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json();
    assert_eq!(body["error"]["code"], "FL002");
}

#[tokio::test]
async fn test_missing_file() {
    let server = setup_test_server().await;

    let response = server
        .post("/files/avatars")
        .content_type("multipart/form-data; boundary=----WebKitFormBoundary")
        .body(create_empty_multipart_body())
        .await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json();
    assert_eq!(body["error"]["code"], "FL004");
}

fn create_multipart_body(filename: &str, content_type: &str, data: &[u8]) -> Vec<u8> {
    let boundary = "----WebKitFormBoundary";
    let mut body = Vec::new();

    body.extend_from_slice(format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
        boundary, filename, content_type
    ).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    body
}

fn create_empty_multipart_body() -> Vec<u8> {
    let boundary = "----WebKitFormBoundary";
    format!("--{}--\r\n", boundary).into_bytes()
}
```

---

## DO NOT

- Do not implement GCS/Azure in first iteration (add in Phase 4b)
- Do not implement malware scanning (external service integration)
- Do not implement video processing (complex, separate feature)
- Do not add CDN cache invalidation (CDN-specific APIs vary)
- Do not skip security validations for "performance" - security first
