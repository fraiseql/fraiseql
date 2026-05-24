#![allow(missing_docs)]

use fraiseql_error::FileError;

#[test]
fn too_large_error_code_and_display() {
    let err = FileError::TooLarge {
        size: 2_000_000,
        max:  1_000_000,
    };
    assert_eq!(err.error_code(), "file_too_large");
    assert_eq!(err.to_string(), "File too large: 2000000 bytes (max: 1000000 bytes)");
}

#[test]
fn invalid_type_error_code_and_display() {
    let err = FileError::InvalidType {
        got:     "application/exe".into(),
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
        source:  None,
    };
    assert_eq!(err.error_code(), "file_storage_error");
    assert_eq!(err.to_string(), "Storage error: disk full");
}

#[test]
fn storage_error_with_source() {
    let io_err = std::io::Error::other("underlying failure");
    let err = FileError::Storage {
        message: "write failed".into(),
        source:  Some(Box::new(io_err)),
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

// ---------------------------------------------------------------------------
// F050 — backend-classification variants
// ---------------------------------------------------------------------------

#[test]
fn permission_denied_status_and_code() {
    let err = FileError::PermissionDenied {
        message: "bucket policy denied".into(),
        source:  None,
    };
    assert_eq!(err.error_code(), "file_permission_denied");
    assert_eq!(err.status_code(), 403);
    assert_eq!(err.to_string(), "Permission denied: bucket policy denied");
}

#[test]
fn permission_denied_preserves_source_chain() {
    let inner = std::io::Error::other("S3 returned 403 AccessDenied");
    let err = FileError::PermissionDenied {
        message: "denied".into(),
        source:  Some(Box::new(inner)),
    };
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn io_error_status_and_code() {
    let err = FileError::IoError {
        message: "disk read failed".into(),
        source:  None,
    };
    assert_eq!(err.error_code(), "file_io_error");
    assert_eq!(err.status_code(), 500);
    assert_eq!(err.to_string(), "I/O error: disk read failed");
}

#[test]
fn io_error_preserves_source_chain() {
    let inner = std::io::Error::other("ENOSPC");
    let err = FileError::IoError {
        message: "write failed".into(),
        source:  Some(Box::new(inner)),
    };
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn invalid_key_status_and_code() {
    let err = FileError::InvalidKey {
        message: "path-traversal segment".into(),
    };
    assert_eq!(err.error_code(), "file_invalid_key");
    assert_eq!(err.status_code(), 400);
    assert_eq!(err.to_string(), "Invalid storage key: path-traversal segment");
}

#[test]
fn not_implemented_status_and_code() {
    let err = FileError::NotImplemented {
        message: "list is unimplemented for GCS".into(),
    };
    assert_eq!(err.error_code(), "file_not_implemented");
    // Preserves legacy `FraiseQLError::Storage { code: Some("not_implemented") }`
    // behavior of routing to 500 (storage_error_response only special-cased
    // `not_found` and `permission_denied`).
    assert_eq!(err.status_code(), 500);
}

#[test]
fn unsupported_status_and_code() {
    let err = FileError::Unsupported {
        message: "presigned URLs unavailable for local backend".into(),
    };
    assert_eq!(err.error_code(), "file_unsupported");
    assert_eq!(err.status_code(), 500);
}

#[test]
fn size_limit_exceeded_status_and_code() {
    let err = FileError::SizeLimitExceeded {
        message: "upload exceeds 5 MiB cap".into(),
        limit:   Some(5 * 1024 * 1024),
        actual:  Some(6 * 1024 * 1024),
    };
    assert_eq!(err.error_code(), "file_size_limit_exceeded");
    assert_eq!(err.status_code(), 500);
}

#[test]
fn mime_type_not_allowed_status_and_code() {
    let err = FileError::MimeTypeNotAllowed {
        message: "application/octet-stream not on allow-list".into(),
        mime:    Some("application/octet-stream".into()),
    };
    assert_eq!(err.error_code(), "file_mime_type_not_allowed");
    assert_eq!(err.status_code(), 500);
}

#[test]
fn backend_status_and_code() {
    let err = FileError::Backend {
        message: "S3 PUT request failed".into(),
        source:  None,
    };
    assert_eq!(err.error_code(), "file_backend_error");
    assert_eq!(err.status_code(), 500);
}

#[test]
fn backend_preserves_source_chain() {
    let inner = std::io::Error::other("connection reset");
    let err = FileError::Backend {
        message: "S3 PUT failed".into(),
        source:  Some(Box::new(inner)),
    };
    assert!(std::error::Error::source(&err).is_some());
}

// ---------------------------------------------------------------------------
// Pre-F050 status codes (preserve existing behavior)
// ---------------------------------------------------------------------------

#[test]
fn legacy_not_found_status_is_404() {
    let err = FileError::NotFound {
        id: "missing".into(),
    };
    // Refinement from pre-F050: `FraiseQLError::File(FileError::NotFound)`
    // now returns 404 globally (was 400). This matches what
    // `storage_error_response` and `fraiseql-server`'s `file_error_response`
    // already returned for backend-not-found cases.
    assert_eq!(err.status_code(), 404);
}

#[test]
fn legacy_storage_status_unchanged_400() {
    let err = FileError::Storage {
        message: "legacy storage".into(),
        source:  None,
    };
    assert_eq!(err.status_code(), 400);
}

#[test]
fn legacy_too_large_status_unchanged_400() {
    let err = FileError::TooLarge {
        size: 100,
        max:  50,
    };
    assert_eq!(err.status_code(), 400);
}
