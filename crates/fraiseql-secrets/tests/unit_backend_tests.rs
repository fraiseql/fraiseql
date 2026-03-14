//! Unit tests for `fraiseql-secrets` — backend types, error types, and `Secret` wrapper.
//!
//! These tests exercise the public API without requiring a real Vault server or
//! other external services. The `EnvBackend` and `FileBackend` are tested directly,
//! and `SecretsError` display/classification is verified.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use fraiseql_secrets::{
    EnvBackend, FileBackend, Secret, SecretsBackendConfig, SecretsError, create_secrets_manager,
    secrets_manager::SecretsBackend,
};

// ── SecretsError Display ──────────────────────────────────────────────────────

#[test]
fn secrets_error_not_found_displays_message() {
    let e = SecretsError::NotFound("MY_SECRET".into());
    let s = e.to_string();
    assert!(s.contains("MY_SECRET"), "got: {s}");
}

#[test]
fn secrets_error_backend_error_displays_message() {
    let e = SecretsError::BackendError("connection refused".into());
    let s = e.to_string();
    assert!(s.contains("connection refused"), "got: {s}");
}

#[test]
fn secrets_error_validation_error_displays_message() {
    let e = SecretsError::ValidationError("name is empty".into());
    let s = e.to_string();
    assert!(s.contains("name is empty"), "got: {s}");
}

#[test]
fn secrets_error_encryption_error_displays_message() {
    let e = SecretsError::EncryptionError("bad key".into());
    let s = e.to_string();
    assert!(s.contains("bad key"), "got: {s}");
}

#[test]
fn secrets_error_rotation_error_displays_message() {
    let e = SecretsError::RotationError("rotation not supported".into());
    let s = e.to_string();
    assert!(s.contains("rotation not supported"), "got: {s}");
}

#[test]
fn secrets_error_connection_error_displays_message() {
    let e = SecretsError::ConnectionError("vault unreachable".into());
    let s = e.to_string();
    assert!(s.contains("vault unreachable"), "got: {s}");
}

#[test]
fn secrets_error_expired_credential_displays() {
    let e = SecretsError::ExpiredCredential;
    assert!(!e.to_string().is_empty());
}

#[test]
fn secrets_error_implements_std_error() {
    let e = SecretsError::NotFound("x".into());
    let _: &dyn std::error::Error = &e;
}

// ── Secret wrapper ────────────────────────────────────────────────────────────

#[test]
fn secret_debug_redacts_value() {
    let s = Secret::new("hunter2".into());
    let d = format!("{s:?}");
    assert!(!d.contains("hunter2"), "debug should not reveal value: {d}");
    assert!(d.contains("***"), "debug should show ***: {d}");
}

#[test]
fn secret_display_redacts_value() {
    let s = Secret::new("hunter2".into());
    let d = s.to_string();
    assert!(!d.contains("hunter2"), "display should not reveal value: {d}");
    assert_eq!(d, "***");
}

#[test]
fn secret_expose_returns_actual_value() {
    let s = Secret::new("p@ssw0rd".into());
    assert_eq!(s.expose(), "p@ssw0rd");
}

#[test]
fn secret_into_exposed_consumes_and_returns() {
    let s = Secret::new("token-abc".into());
    let v = s.into_exposed();
    assert_eq!(v, "token-abc");
}

#[test]
fn secret_is_empty_for_empty_string() {
    let s = Secret::new(String::new());
    assert!(s.is_empty());
}

#[test]
fn secret_is_not_empty_for_nonempty() {
    let s = Secret::new("x".into());
    assert!(!s.is_empty());
}

#[test]
fn secret_len_matches_inner_string() {
    let s = Secret::new("hello".into());
    assert_eq!(s.len(), 5);
}

#[test]
fn secret_partial_eq_compares_values() {
    let a = Secret::new("abc".into());
    let b = Secret::new("abc".into());
    let c = Secret::new("xyz".into());
    assert_eq!(a, b);
    assert_ne!(a, c);
}

// ── EnvBackend ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn env_backend_reads_existing_variable() {
    temp_env::async_with_vars([("FRAISEQL_TEST_ENV_UNIT", Some("secret_value"))], async {
        let b = EnvBackend::new();
        let v = b.get_secret("FRAISEQL_TEST_ENV_UNIT").await.unwrap();
        assert_eq!(v, "secret_value");
    })
    .await;
}

#[tokio::test]
async fn env_backend_returns_not_found_for_missing() {
    let b = EnvBackend::new();
    let r = b.get_secret("__FRAISEQL_DEFINITELY_NOT_SET_UNIT_TEST_123").await;
    assert!(matches!(r, Err(SecretsError::NotFound(_))));
}

#[tokio::test]
async fn env_backend_empty_name_returns_validation_error() {
    let b = EnvBackend::new();
    let r = b.get_secret("").await;
    assert!(matches!(r, Err(SecretsError::ValidationError(_))));
}

#[tokio::test]
async fn env_backend_rotate_returns_rotation_error() {
    let b = EnvBackend::new();
    let r = b.rotate_secret("ANY_KEY").await;
    assert!(matches!(r, Err(SecretsError::RotationError(_))));
}

#[tokio::test]
async fn env_backend_get_with_expiry_returns_future_date() {
    temp_env::async_with_vars([("FRAISEQL_EXPIRY_UNIT", Some("v"))], async {
        let b = EnvBackend::new();
        let (val, expiry) = b.get_secret_with_expiry("FRAISEQL_EXPIRY_UNIT").await.unwrap();
        assert_eq!(val, "v");
        assert!(expiry > chrono::Utc::now(), "expiry should be in the future");
    })
    .await;
}

// ── FileBackend ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn file_backend_reads_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("db_pass"), "s3cret").await.unwrap();

    let b = FileBackend::new(dir.path().to_str().unwrap());
    let v = b.get_secret("db_pass").await.unwrap();
    assert_eq!(v, "s3cret");
}

#[tokio::test]
async fn file_backend_trims_trailing_whitespace() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("padded"), "value\n").await.unwrap();

    let b = FileBackend::new(dir.path().to_str().unwrap());
    let v = b.get_secret("padded").await.unwrap();
    assert_eq!(v, "value", "trailing newline should be trimmed");
}

#[tokio::test]
async fn file_backend_returns_error_for_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let b = FileBackend::new(dir.path().to_str().unwrap());
    let r = b.get_secret("does_not_exist").await;
    assert!(r.is_err());
}

#[tokio::test]
async fn file_backend_get_with_expiry_returns_future_date() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("key"), "val").await.unwrap();

    let b = FileBackend::new(dir.path().to_str().unwrap());
    let (val, expiry) = b.get_secret_with_expiry("key").await.unwrap();
    assert_eq!(val, "val");
    assert!(expiry > chrono::Utc::now());
}

// ── create_secrets_manager (integration point) ────────────────────────────────

#[tokio::test]
async fn secrets_manager_file_backend_works() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("api_key"), "my-api-key").await.unwrap();

    let manager = create_secrets_manager(SecretsBackendConfig::File {
        path: dir.path().to_path_buf(),
    })
    .await
    .unwrap();

    let v = manager.get_secret("api_key").await.unwrap();
    assert_eq!(v, "my-api-key");
}

#[tokio::test]
async fn secrets_manager_env_backend_works() {
    let key = "FRAISEQL_UNIT_BACKEND_SM_TEST";
    temp_env::async_with_vars([(key, Some("sm_value"))], async {
        let manager = create_secrets_manager(SecretsBackendConfig::Env).await.unwrap();
        let v = manager.get_secret(key).await.unwrap();
        assert_eq!(v, "sm_value");
    })
    .await;
}

#[tokio::test]
async fn secrets_manager_missing_secret_returns_not_found() {
    let manager = create_secrets_manager(SecretsBackendConfig::Env).await.unwrap();
    let r = manager.get_secret("__FRAISEQL_DEFINITELY_ABSENT_XYZ").await;
    assert!(r.is_err());
}
