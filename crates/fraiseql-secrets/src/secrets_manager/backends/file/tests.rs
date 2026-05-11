#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

/// Test `FileBackend` reads from file
#[tokio::test]
async fn test_file_backend_read_secret() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let secret_file = dir.path().join("test_secret");
    tokio::fs::write(&secret_file, "secret_content_123").await.unwrap();

    let backend = FileBackend::new(dir.path());
    let secret = backend.get_secret("test_secret").await.unwrap();

    assert_eq!(secret, "secret_content_123");
}

/// Test `FileBackend` returns error for missing file
#[tokio::test]
async fn test_file_backend_not_found() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let backend = FileBackend::new(dir.path());
    let result = backend.get_secret("nonexistent.txt").await;

    assert!(
        matches!(result, Err(SecretsError::BackendError(_))),
        "expected BackendError for missing file, got: {result:?}"
    );
}

/// Test `FileBackend` trims whitespace
#[tokio::test]
async fn test_file_backend_trims_whitespace() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let secret_file = dir.path().join("whitespace_secret");
    tokio::fs::write(&secret_file, "  secret_value  \n").await.unwrap();

    let backend = FileBackend::new(dir.path());
    let secret = backend.get_secret("whitespace_secret").await.unwrap();

    assert_eq!(secret, "secret_value");
}

/// Test `FileBackend` `with_expiry` returns future date
#[tokio::test]
async fn test_file_backend_with_expiry() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let secret_file = dir.path().join("expiry_test");
    tokio::fs::write(&secret_file, "value").await.unwrap();

    let backend = FileBackend::new(dir.path());
    let (secret, expiry) = backend.get_secret_with_expiry("expiry_test").await.unwrap();

    assert_eq!(secret, "value");
    assert!(expiry > Utc::now());
}

/// Test `FileBackend` rotate returns error
#[tokio::test]
async fn test_file_backend_rotate_not_supported() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let backend = FileBackend::new(dir.path());
    let result = backend.rotate_secret("any_file").await;

    assert!(
        matches!(result, Err(SecretsError::RotationError(_))),
        "expected RotationError, got: {result:?}"
    );
}

/// Test `FileBackend` with multiple files
#[tokio::test]
async fn test_file_backend_multiple_secrets() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("secret1"), "value1").await.unwrap();
    tokio::fs::write(dir.path().join("secret2"), "value2").await.unwrap();

    let backend = FileBackend::new(dir.path());

    let s1 = backend.get_secret("secret1").await.unwrap();
    let s2 = backend.get_secret("secret2").await.unwrap();

    assert_eq!(s1, "value1");
    assert_eq!(s2, "value2");
}

/// Test `FileBackend` handles empty files
#[tokio::test]
async fn test_file_backend_empty_file() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("empty"), "").await.unwrap();

    let backend = FileBackend::new(dir.path());
    let secret = backend.get_secret("empty").await.unwrap();

    assert_eq!(secret, "");
}

/// Test `FileBackend` rejects path traversal in secret name
#[tokio::test]
async fn test_file_backend_rejects_path_traversal() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let backend = FileBackend::new(dir.path());

    let result = backend.get_secret("../../etc/passwd").await;
    assert!(result.is_err(), "path traversal must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("path traversal"),
        "error should mention path traversal; got: {msg}"
    );
}

/// Test `FileBackend` rejects relative parent in secret name
#[tokio::test]
async fn test_file_backend_rejects_dotdot_in_name() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let backend = FileBackend::new(dir.path());

    let result = backend.get_secret("subdir/../../../etc/shadow").await;
    assert!(result.is_err());
}
