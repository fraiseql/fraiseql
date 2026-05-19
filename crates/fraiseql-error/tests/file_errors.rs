#![allow(missing_docs)]

use fraiseql_error::FileError;

#[test]
fn too_large_error_code_and_display() {
    let err = FileError::TooLarge {
        size: 2_000_000,
        max: 1_000_000,
    };
    assert_eq!(err.error_code(), "file_too_large");
    assert_eq!(err.to_string(), "File too large: 2000000 bytes (max: 1000000 bytes)");
}

#[test]
fn invalid_type_error_code_and_display() {
    let err = FileError::InvalidType {
        got: "application/exe".into(),
        allowed: vec!["image/png".into(), "image/jpeg".into()],
    };
    assert_eq!(err.error_code(), "file_invalid_type");
    assert_eq!(
        err.to_string(),
        r#"Invalid file type: application/exe (allowed: ["image/png", "image/jpeg"])"#
    );
}

#[test]
fn mime_mismatch_error_code_and_display() {
    let err = FileError::MimeMismatch {
        declared: "image/png".into(),
        detected: "image/jpeg".into(),
    };
    assert_eq!(err.error_code(), "file_mime_mismatch");
    assert_eq!(err.to_string(), "MIME type mismatch: declared image/png, detected image/jpeg");
}

#[test]
fn storage_error_without_source() {
    let err = FileError::Storage {
        message: "disk full".into(),
        source: None,
    };
    assert_eq!(err.error_code(), "file_storage_error");
    assert_eq!(err.to_string(), "Storage error: disk full");
}

#[test]
fn storage_error_with_source() {
    let io_err = std::io::Error::other("underlying failure");
    let err = FileError::Storage {
        message: "write failed".into(),
        source: Some(Box::new(io_err)),
    };
    assert_eq!(err.error_code(), "file_storage_error");
    assert_eq!(err.to_string(), "Storage error: write failed");
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn processing_error_code_and_display() {
    let err = FileError::Processing {
        message: "corrupt image".into(),
    };
    assert_eq!(err.error_code(), "file_processing_error");
    assert_eq!(err.to_string(), "Processing error: corrupt image");
}

#[test]
fn not_found_error_code_and_display() {
    let err = FileError::NotFound {
        id: "abc-123".into(),
    };
    assert_eq!(err.error_code(), "file_not_found");
    assert_eq!(err.to_string(), "File not found: abc-123");
}

#[test]
fn virus_detected_error_code_and_display() {
    let err = FileError::VirusDetected {
        details: "EICAR test".into(),
    };
    assert_eq!(err.error_code(), "file_virus_detected");
    assert_eq!(err.to_string(), "Virus detected: EICAR test");
}

#[test]
fn quota_exceeded_error_code_and_display() {
    assert_eq!(FileError::QuotaExceeded.error_code(), "file_quota_exceeded");
    assert_eq!(FileError::QuotaExceeded.to_string(), "Upload quota exceeded");
}
