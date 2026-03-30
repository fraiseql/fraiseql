#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::time::Duration;

use fraiseql_error::FileError;

use super::*;

// ── key validation ──────────────────────────────────────────────────

#[test]
fn validate_key_rejects_empty() {
    assert!(validate_key("").is_err());
}

#[test]
fn validate_key_rejects_path_traversal() {
    assert!(validate_key("../etc/passwd").is_err());
    assert!(validate_key("foo/../bar").is_err());
    assert!(validate_key("/absolute/path").is_err());
    assert!(validate_key("\\windows\\path").is_err());
}

#[test]
fn validate_key_accepts_valid() {
    assert!(validate_key("file.txt").is_ok());
    assert!(validate_key("path/to/file.txt").is_ok());
    assert!(validate_key("uploads/2024/01/image.png").is_ok());
}

// ── local backend ───────────────────────────────────────────────────

#[tokio::test]
async fn local_upload_download_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    let data = b"hello, world!";
    backend.upload("test/file.txt", data, "text/plain").await.unwrap();

    let downloaded = backend.download("test/file.txt").await.unwrap();
    assert_eq!(downloaded, data);
}

#[tokio::test]
async fn local_exists() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    assert!(!backend.exists("missing.txt").await.unwrap());

    backend.upload("exists.txt", b"data", "text/plain").await.unwrap();
    assert!(backend.exists("exists.txt").await.unwrap());
}

#[tokio::test]
async fn local_delete() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    backend.upload("to_delete.txt", b"data", "text/plain").await.unwrap();
    assert!(backend.exists("to_delete.txt").await.unwrap());

    backend.delete("to_delete.txt").await.unwrap();
    assert!(!backend.exists("to_delete.txt").await.unwrap());
}

#[tokio::test]
async fn local_download_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    let result = backend.download("nonexistent.txt").await;
    assert!(matches!(result, Err(FileError::NotFound { .. })));
}

#[tokio::test]
async fn local_delete_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    let result = backend.delete("nonexistent.txt").await;
    assert!(matches!(result, Err(FileError::NotFound { .. })));
}

#[tokio::test]
async fn local_presigned_url_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    let result = backend.presigned_url("file.txt", Duration::from_secs(3600)).await;
    assert!(matches!(result, Err(FileError::Storage { .. })));
}

#[tokio::test]
async fn local_path_traversal_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    let result = backend.upload("../escape.txt", b"data", "text/plain").await;
    assert!(matches!(result, Err(FileError::Storage { .. })));
}

#[tokio::test]
async fn local_nested_directory_creation() {
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalStorageBackend::new(dir.path().to_str().unwrap());

    backend.upload("a/b/c/deep.txt", b"deep data", "text/plain").await.unwrap();
    let data = backend.download("a/b/c/deep.txt").await.unwrap();
    assert_eq!(data, b"deep data");
}

// ── factory ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_backend_local() {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::StorageConfig {
        backend:      "local".to_string(),
        bucket:       None,
        path:         Some(dir.path().to_str().unwrap().to_string()),
        region:       None,
        endpoint:     None,
        project_id:   None,
        account_name: None,
    };

    let backend = create_backend(&config).await.unwrap();
    backend.upload("test.txt", b"data", "text/plain").await.unwrap();
    let data = backend.download("test.txt").await.unwrap();
    assert_eq!(data, b"data");
}

#[tokio::test]
async fn create_backend_local_missing_path() {
    let config = crate::config::StorageConfig {
        backend:      "local".to_string(),
        bucket:       None,
        path:         None,
        region:       None,
        endpoint:     None,
        project_id:   None,
        account_name: None,
    };

    let result = create_backend(&config).await;
    assert!(matches!(result, Err(FileError::Storage { .. })));
}

#[tokio::test]
async fn create_backend_unknown() {
    let config = crate::config::StorageConfig {
        backend:      "ftp".to_string(),
        bucket:       None,
        path:         None,
        region:       None,
        endpoint:     None,
        project_id:   None,
        account_name: None,
    };

    let result = create_backend(&config).await;
    assert!(matches!(result, Err(FileError::Storage { .. })));
}

#[cfg(not(feature = "aws-s3"))]
#[tokio::test]
async fn create_backend_s3_feature_not_enabled() {
    for name in &[
        "s3",
        "r2",
        "hetzner",
        "scaleway",
        "ovh",
        "exoscale",
        "backblaze",
    ] {
        let config = crate::config::StorageConfig {
            backend:      (*name).to_string(),
            bucket:       Some("bucket".to_string()),
            path:         None,
            region:       None,
            endpoint:     None,
            project_id:   None,
            account_name: None,
        };

        let result = create_backend(&config).await;
        assert!(
            matches!(result, Err(FileError::Storage { .. })),
            "expected Storage error for backend '{name}' without aws-s3 feature"
        );
    }
}

// ── S3-compatible provider defaults ──────────────────────────────────

#[test]
fn default_endpoint_hetzner() {
    let ep = default_s3_endpoint("hetzner", None).unwrap();
    assert!(ep.contains("fsn1"), "default region should be fsn1");
    assert!(ep.starts_with("https://"));
}

#[test]
fn default_endpoint_hetzner_custom_region() {
    let ep = default_s3_endpoint("hetzner", Some("nbg1")).unwrap();
    assert!(ep.contains("nbg1"));
}

#[test]
fn default_endpoint_scaleway() {
    let ep = default_s3_endpoint("scaleway", None).unwrap();
    assert!(ep.contains("fr-par"));
    assert!(ep.contains("scw.cloud"));
}

#[test]
fn default_endpoint_ovh() {
    let ep = default_s3_endpoint("ovh", None).unwrap();
    assert!(ep.contains("gra"));
    assert!(ep.contains("ovh.net"));
}

#[test]
fn default_endpoint_exoscale() {
    let ep = default_s3_endpoint("exoscale", None).unwrap();
    assert!(ep.contains("de-fra-1"));
    assert!(ep.contains("exo.io"));
}

#[test]
fn default_endpoint_backblaze() {
    let ep = default_s3_endpoint("backblaze", None).unwrap();
    assert!(ep.contains("backblazeb2.com"));
}

#[test]
fn default_endpoint_plain_s3_is_none() {
    assert!(default_s3_endpoint("s3", None).is_none());
}

#[test]
fn default_endpoint_r2_is_none() {
    assert!(default_s3_endpoint("r2", None).is_none());
}

#[cfg(not(feature = "gcs"))]
#[tokio::test]
async fn create_backend_gcs_feature_not_enabled() {
    let config = crate::config::StorageConfig {
        backend:      "gcs".to_string(),
        bucket:       Some("bucket".to_string()),
        path:         None,
        region:       None,
        endpoint:     None,
        project_id:   None,
        account_name: None,
    };

    let result = create_backend(&config).await;
    assert!(matches!(result, Err(FileError::Storage { .. })));
}

#[cfg(not(feature = "azure-blob"))]
#[tokio::test]
async fn create_backend_azure_feature_not_enabled() {
    let config = crate::config::StorageConfig {
        backend:      "azure".to_string(),
        bucket:       Some("container".to_string()),
        path:         None,
        region:       None,
        endpoint:     None,
        project_id:   None,
        account_name: Some("myaccount".to_string()),
    };

    let result = create_backend(&config).await;
    assert!(matches!(result, Err(FileError::Storage { .. })));
}
