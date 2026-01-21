//! File configuration structures

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
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

fn default_max_size() -> String {
    "10MB".to_string()
}
fn default_validate_magic() -> bool {
    true
}
fn default_storage() -> String {
    "default".to_string()
}
fn default_public() -> bool {
    true
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            path: None,
            allowed_types: default_allowed_types(),
            max_size: default_max_size(),
            validate_magic_bytes: default_validate_magic(),
            storage: default_storage(),
            bucket_env: None,
            public: default_public(),
            cache: None,
            url_expiry: None,
            scan_malware: false,
            processing: None,
            on_upload: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct VariantConfig {
    pub name: String,
    pub width: u32,
    pub height: u32,

    /// Resize mode: fit, fill, crop
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "fit".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadCallbackConfig {
    /// Database function to call
    pub function: String,

    /// Parameter mapping
    #[serde(default)]
    pub mapping: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
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

/// Parse size string like "10MB" to bytes
pub fn parse_size(size_str: &str) -> Result<usize, String> {
    let size_str = size_str.trim().to_uppercase();

    let (num_part, unit) = if let Some(pos) = size_str.find(|c: char| !c.is_ascii_digit() && c != '.') {
        (&size_str[..pos], &size_str[pos..])
    } else {
        (size_str.as_str(), "")
    };

    let num: f64 = num_part.parse().map_err(|_| format!("Invalid number: {}", num_part))?;

    let multiplier = match unit.trim() {
        "" | "B" => 1,
        "KB" => 1024,
        "MB" => 1024 * 1024,
        "GB" => 1024 * 1024 * 1024,
        _ => return Err(format!("Unknown unit: {}", unit)),
    };

    Ok((num * multiplier as f64) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100").unwrap(), 100);
        assert_eq!(parse_size("100B").unwrap(), 100);
        assert_eq!(parse_size("10KB").unwrap(), 10 * 1024);
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
    }
}
