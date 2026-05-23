#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test functions are self-describing

use tempfile::TempDir;

use super::*;
use crate::{backend::LocalBackend, config::BucketAccess};

fn temp_service(
    max_size: Option<u64>,
    allowed_types: Option<Vec<String>>,
) -> (BucketService, TempDir) {
    let tmpdir = TempDir::new().expect("create tempdir");
    let backend = StorageBackend::Local(LocalBackend::new(tmpdir.path().to_str().unwrap()));
    let config = BucketConfig {
        name:               "test".to_string(),
        max_object_bytes:   max_size,
        allowed_mime_types: allowed_types,
        access:             BucketAccess::Private,
        transform_presets:  None,
    };
    (BucketService::new(backend, config), tmpdir)
}

#[tokio::test]
async fn test_size_limit_rejected() {
    let (service, _tmpdir) = temp_service(Some(100), None);

    let result = service.upload("test.bin", &[0u8; 150], "application/octet-stream").await;

    let err = result.expect_err("should reject oversized upload");
    assert!(matches!(err, FraiseQLError::Storage { .. }), "should be a Storage error");
    if let FraiseQLError::Storage { code, .. } = err {
        assert_eq!(code, Some("size_limit_exceeded".to_string()));
    }
}

#[tokio::test]
async fn test_size_limit_accepted() {
    let (service, _tmpdir) = temp_service(Some(100), None);

    let result = service.upload("test.bin", &[0u8; 50], "application/octet-stream").await;

    result.expect("should accept upload within limit");
}

#[tokio::test]
async fn test_mime_type_rejected() {
    let (service, _tmpdir) = temp_service(None, Some(vec!["image/jpeg".to_string()]));

    let result = service.upload("test.txt", b"text", "text/plain").await;

    let err = result.expect_err("should reject disallowed MIME type");
    if let FraiseQLError::Storage { code, .. } = err {
        assert_eq!(code, Some("mime_type_not_allowed".to_string()));
    }
}

#[tokio::test]
async fn test_mime_type_wildcard() {
    let (service, _tmpdir) = temp_service(None, Some(vec!["*/*".to_string()]));

    let result = service.upload("test.anything", b"data", "application/anything").await;

    result.expect("wildcard should accept any MIME type");
}

#[tokio::test]
async fn test_no_policy_passes_through() {
    let (service, _tmpdir) = temp_service(None, None);

    let result = service
        .upload("test.bin", &vec![0u8; 1_000_000], "application/octet-stream")
        .await;

    result.expect("no limits should allow any upload");
}

#[tokio::test]
async fn test_list_delegates_to_backend() {
    let (service, _tmpdir) = temp_service(None, None);

    service.upload("file1.txt", b"data", "text/plain").await.expect("upload");
    service.upload("file2.txt", b"data", "text/plain").await.expect("upload");

    let result = service.list("", None, 100).await.expect("list");

    assert_eq!(result.objects.len(), 2, "should list both files");
}
