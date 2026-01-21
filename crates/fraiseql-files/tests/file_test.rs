//! Simplified file upload tests

use bytes::Bytes;
use fraiseql_files::config::{FileConfig, ProcessingConfig, VariantConfig};
use fraiseql_files::testing::{MockImageProcessor, MockMalwareScanner, MockStorage, MockValidator};
use fraiseql_files::traits::{FileValidator, ImageProcessor, MalwareScanner, StorageBackend};
use fraiseql_files::validation::{sanitize_filename, validate_file};
use std::time::Duration;

// ============================================================================
// Mock Storage Tests
// ============================================================================

#[tokio::test]
async fn test_mock_storage_basic() {
    let storage = MockStorage::new();
    let data = Bytes::from("test content");

    // Upload using trait
    let result = <MockStorage as StorageBackend>::upload(&storage, "test.txt", data.clone(), "text/plain", None)
        .await
        .unwrap();

    assert_eq!(result.key, "test.txt");
    assert_eq!(result.size, 12);

    // Download
    let downloaded: Bytes = <MockStorage as StorageBackend>::download(&storage, "test.txt")
        .await
        .unwrap();
    assert_eq!(downloaded, data);

    // Exists
    assert!(<MockStorage as StorageBackend>::exists(&storage, "test.txt").await.unwrap());

    // Delete
    <MockStorage as StorageBackend>::delete(&storage, "test.txt").await.unwrap();
    assert!(!<MockStorage as StorageBackend>::exists(&storage, "test.txt").await.unwrap());
}

#[tokio::test]
async fn test_mock_storage_signed_url() {
    let storage = MockStorage::new();
    let expiry = Duration::from_secs(3600);

    let url = <MockStorage as StorageBackend>::signed_url(&storage, "test.txt", expiry)
        .await
        .unwrap();

    assert!(url.contains("test.txt"));
    assert!(url.contains("expires="));
}

// ============================================================================
// Mock Validator Tests
// ============================================================================

#[test]
fn test_mock_validator_basic() {
    let validator = MockValidator::permissive();
    let data = Bytes::from(vec![0u8; 100]);
    let config = FileConfig::default();

    let result = <MockValidator as FileValidator>::validate(&validator, &data, "image/jpeg", "test.jpg", &config)
        .unwrap();

    assert_eq!(result.content_type, "image/jpeg");
    assert_eq!(result.size, 100);
}

#[test]
fn test_mock_validator_size_limit() {
    let validator = MockValidator::strict(vec!["image/jpeg".to_string()], 50);
    let config = FileConfig::default();

    // Small file - should pass
    let small_data = Bytes::from(vec![0u8; 30]);
    assert!(<MockValidator as FileValidator>::validate(&validator, &small_data, "image/jpeg", "test.jpg", &config).is_ok());

    // Large file - should fail
    let large_data = Bytes::from(vec![0u8; 100]);
    let result = <MockValidator as FileValidator>::validate(&validator, &large_data, "image/jpeg", "test.jpg", &config);
    assert!(result.is_err());
}

// ============================================================================
// Mock Image Processor Tests
// ============================================================================

#[tokio::test]
async fn test_mock_image_processor_basic() {
    let processor = MockImageProcessor::new(vec!["thumbnail"]);
    let data = Bytes::from(vec![0u8; 200]);
    let config = ProcessingConfig {
        strip_exif: false,
        output_format: None,
        quality: None,
        variants: vec![
            VariantConfig {
                name: "thumbnail".to_string(),
                width: 150,
                height: 150,
                mode: "fill".to_string(),
            },
        ],
    };

    let result = <MockImageProcessor as ImageProcessor>::process(&processor, &data, &config)
        .await
        .unwrap();

    assert!(result.variants.contains_key("original"));
    assert!(result.variants.contains_key("thumbnail"));
}

// ============================================================================
// Mock Malware Scanner Tests
// ============================================================================

#[tokio::test]
async fn test_mock_malware_scanner_clean() {
    let scanner = MockMalwareScanner::clean();
    let data = Bytes::from("safe content");

    let result = <MockMalwareScanner as MalwareScanner>::scan(&scanner, &data)
        .await
        .unwrap();

    assert!(result.clean);
    assert!(result.threat_name.is_none());
}

#[tokio::test]
async fn test_mock_malware_scanner_threat() {
    let malicious_data = b"EICAR";
    let scanner = MockMalwareScanner::clean().with_threat(malicious_data, "EICAR-Test");

    let data = Bytes::from(&malicious_data[..]);
    let result = <MockMalwareScanner as MalwareScanner>::scan(&scanner, &data)
        .await
        .unwrap();

    assert!(!result.clean);
    assert_eq!(result.threat_name, Some("EICAR-Test".to_string()));
}

// ============================================================================
// Validation Tests
// ============================================================================

#[test]
fn test_filename_sanitization() {
    // Normal filenames
    assert_eq!(sanitize_filename("photo.jpg").unwrap(), "photo.jpg");
    assert_eq!(sanitize_filename("my-file_2024.pdf").unwrap(), "my-file_2024.pdf");

    // Path traversal
    let result = sanitize_filename("../../../etc/passwd").unwrap();
    assert!(!result.contains(".."));
    assert_eq!(result, "passwd");

    // Dangerous characters
    let result = sanitize_filename("file<>:\"|?*.jpg").unwrap();
    assert!(!result.contains('<'));
    assert!(!result.contains('>'));
    assert!(!result.contains(':'));
}

#[test]
fn test_file_size_validation() {
    let config = FileConfig {
        max_size: "100".to_string(),
        allowed_types: vec!["image/jpeg".to_string()],
        validate_magic_bytes: false,
        ..Default::default()
    };

    // Small file - should pass
    let small_data = Bytes::from(vec![0u8; 50]);
    assert!(validate_file(&small_data, "image/jpeg", "test.jpg", &config).is_ok());

    // Large file - should fail
    let large_data = Bytes::from(vec![0u8; 200]);
    let result = validate_file(&large_data, "image/jpeg", "test.jpg", &config);
    assert!(result.is_err());
}

#[test]
fn test_mime_type_validation() {
    let config = FileConfig {
        max_size: "1MB".to_string(),
        allowed_types: vec!["image/jpeg".to_string(), "image/png".to_string()],
        validate_magic_bytes: false,
        ..Default::default()
    };

    let data = Bytes::from(vec![0u8; 100]);

    // Allowed type
    assert!(validate_file(&data, "image/jpeg", "test.jpg", &config).is_ok());

    // Disallowed type
    let result = validate_file(&data, "application/pdf", "test.pdf", &config);
    assert!(result.is_err());
}
