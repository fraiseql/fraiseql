#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use crate::secrets_manager::SecretsError;
use super::backend::VaultBackend;
use super::cache::VaultResponse;
use super::validation::{MAX_VAULT_SECRET_NAME_BYTES, validate_vault_addr, validate_vault_secret_name};
use crate::secrets_manager::SecretsBackend;

/// Test VaultBackend creation
#[test]
fn test_vault_backend_creation() {
    let vault = VaultBackend::new("https://vault.local:8200", "mytoken");
    assert_eq!(vault.addr(), "https://vault.local:8200");
    assert_eq!(vault.token(), "mytoken");
}

/// Test VaultBackend placeholder returns error
#[tokio::test]
async fn test_vault_backend_placeholder() {
    let vault = VaultBackend::new("https://vault.local:8200", "token");

    let result = vault.get_secret("any/path").await;
    assert!(result.is_err());
}

/// Test multiple VaultBackend instances
#[test]
fn test_vault_backend_multiple() {
    let vault1 = VaultBackend::new("https://vault1.local:8200", "token1");
    let vault2 = VaultBackend::new("https://vault2.local:8200", "token2");

    assert_ne!(vault1.addr(), vault2.addr());
    assert_ne!(vault1.token(), vault2.token());
}

/// Test VaultBackend clone
#[test]
fn test_vault_backend_clone() {
    let vault1 = VaultBackend::new("https://vault.local:8200", "token");
    let vault2 = vault1.clone();

    assert_eq!(vault1.addr(), vault2.addr());
    assert_eq!(vault1.token(), vault2.token());
}

// --- validate_vault_secret_name tests (S9-1) ---

#[test]
fn test_secret_name_empty_rejected() {
    assert!(validate_vault_secret_name("").is_err());
}

#[test]
fn test_secret_name_valid_paths() {
    assert!(validate_vault_secret_name("db/creds").is_ok());
    assert!(validate_vault_secret_name("secret/app_name/db-password").is_ok());
    assert!(validate_vault_secret_name("kv/prod/postgres").is_ok());
}

#[test]
fn test_secret_name_dot_rejected() {
    // `.` is not in the allowed character set — prevents `../` path traversal.
    assert!(validate_vault_secret_name("../etc/passwd").is_err());
    assert!(validate_vault_secret_name("secret/../../etc").is_err());
    assert!(validate_vault_secret_name("secret/app.name").is_err());
}

#[test]
fn test_secret_name_special_chars_rejected() {
    assert!(validate_vault_secret_name("secret/app name").is_err()); // space
    assert!(validate_vault_secret_name("secret/app\0name").is_err()); // null byte
    assert!(validate_vault_secret_name("secret/app;name").is_err()); // semicolon
}

// ── Length-guard tests ─────────────────────────────────────────────────────

#[test]
fn test_secret_name_exactly_max_length_accepted() {
    // MAX_VAULT_SECRET_NAME_BYTES exactly — must be accepted.
    let name = "a".repeat(MAX_VAULT_SECRET_NAME_BYTES);
    assert!(
        validate_vault_secret_name(&name).is_ok(),
        "name at max length must be accepted"
    );
}

#[test]
fn test_secret_name_exceeds_max_length_rejected() {
    // MAX + 1 bytes — must be rejected by the length guard.
    let name = "a".repeat(MAX_VAULT_SECRET_NAME_BYTES + 1);
    let err = validate_vault_secret_name(&name).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("too long") || msg.contains("1024"),
        "error must mention length limit: {msg}"
    );
}

#[test]
fn test_secret_name_very_long_rejected_before_char_scan() {
    // A 1 MiB string — length guard must fire without scanning every character.
    let name = "a/".repeat(512 * 1024); // 1 MiB of valid-char data
    assert!(validate_vault_secret_name(&name).is_err(), "1 MiB name must be rejected");
}

// --- extract_secret_from_response unit tests (S10-3) ---

fn make_vault_response(data: serde_json::Value) -> VaultResponse {
    VaultResponse {
        request_id:     "req-1234".to_string(),
        lease_id:       "lease-5678".to_string(),
        lease_duration: 3600,
        renewable:      true,
        data:           serde_json::from_value(data).unwrap(),
    }
}

#[test]
fn test_extract_secret_kv2_nested_data() {
    // KV v2: response.data.data contains the actual secret map.
    let response = make_vault_response(serde_json::json!({
        "data": {"username": "admin", "password": "s3cr3t"}
    }));
    let result = VaultBackend::extract_secret_from_response(&response, "kv/myapp").unwrap();
    // Should serialize the inner data object.
    assert!(result.contains("admin") && result.contains("s3cr3t"), "got: {result}");
}

#[test]
fn test_extract_secret_dynamic_credentials() {
    // Dynamic creds (database engine, etc.): no nested "data" key.
    let response = make_vault_response(serde_json::json!({
        "username": "v-root-abc123",
        "password": "A1B2C3"
    }));
    let result =
        VaultBackend::extract_secret_from_response(&response, "database/creds/my-role")
            .unwrap();
    assert!(result.contains("v-root-abc123") && result.contains("A1B2C3"), "got: {result}");
}

// --- Vault HTTP mock integration tests (S10-2) ---

#[tokio::test]
async fn test_vault_fetch_secret_success() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/secret/db-password"))
        .and(header("X-Vault-Token", "test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "request_id": "abc-123",
            "lease_id": "",
            "lease_duration": 3600,
            "renewable": false,
            "data": {"value": "supersecret"}
        })))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let result = vault.get_secret("secret/db-password").await.unwrap();
    assert!(result.contains("supersecret"), "expected secret value in result; got: {result}");
}

#[tokio::test]
async fn test_vault_fetch_secret_not_found_returns_not_found_error() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/secret/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let result = vault.get_secret("secret/missing").await;
    assert!(
        matches!(result, Err(SecretsError::NotFound(_))),
        "expected NotFound error; got: {result:?}"
    );
}

#[tokio::test]
async fn test_vault_fetch_secret_403_returns_backend_error() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/secret/restricted"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "bad-token");
    let result = vault.get_secret("secret/restricted").await;
    assert!(
        matches!(result, Err(SecretsError::BackendError(_))),
        "expected BackendError for 403; got: {result:?}"
    );
}

#[tokio::test]
async fn test_vault_fetch_secret_invalid_json_returns_backend_error() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/secret/badjson"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("this is not valid json"),
        )
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let result = vault.get_secret("secret/badjson").await;
    assert!(
        matches!(result, Err(SecretsError::BackendError(_))),
        "expected BackendError for invalid JSON; got: {result:?}"
    );
}

// --- renew_token mock tests (S11-1 / H7) ---

#[tokio::test]
async fn test_renew_token_success_updates_token_and_ttl() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .and(header("X-Vault-Token", "old-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth": {
                "client_token": "new-rotated-token",
                "lease_duration": 7200,
                "renewable": true
            }
        })))
        .mount(&mock)
        .await;

    let mut vault = VaultBackend::new_for_test(mock.uri(), "old-token");
    vault.renew_token().await.expect("renewal should succeed");

    assert_eq!(
        vault.token(),
        "new-rotated-token",
        "token should be updated after renewal"
    );
    assert_eq!(
        vault.token_ttl_secs_for_test(),
        Some(7200),
        "TTL should be updated from renewal response"
    );
}

#[tokio::test]
async fn test_renew_token_missing_client_token_returns_error() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth": {}  // client_token is absent
        })))
        .mount(&mock)
        .await;

    let mut vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let result = vault.renew_token().await;
    assert!(
        matches!(result, Err(SecretsError::ConnectionError(_))),
        "missing client_token should return ConnectionError; got: {result:?}"
    );
}

#[tokio::test]
async fn test_renew_token_403_returns_connection_error() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock)
        .await;

    let mut vault = VaultBackend::new_for_test(mock.uri(), "expired-token");
    // 403 response body is not valid JSON for the renewal struct → ConnectionError
    let result = vault.renew_token().await;
    assert!(
        result.is_err(),
        "403 renewal response should return an error; got: {result:?}"
    );
}

// --- validate_vault_addr SSRF tests (S9-2) ---

#[test]
fn test_vault_addr_scheme_must_be_http() {
    assert!(validate_vault_addr("file:///etc/passwd").is_err());
    assert!(validate_vault_addr("ftp://vault.example.com:8200").is_err());
    assert!(validate_vault_addr("vault.example.com:8200").is_err());
}

#[test]
fn test_vault_addr_blocks_loopback() {
    assert!(validate_vault_addr("http://localhost:8200").is_err());
    assert!(validate_vault_addr("http://127.0.0.1:8200").is_err());
    assert!(validate_vault_addr("http://[::1]:8200").is_err());
}

#[test]
fn test_vault_addr_blocks_private_ranges() {
    assert!(validate_vault_addr("http://10.0.0.1:8200").is_err());
    assert!(validate_vault_addr("http://172.16.0.1:8200").is_err());
    assert!(validate_vault_addr("http://192.168.1.1:8200").is_err());
    // AWS metadata service
    assert!(validate_vault_addr("http://169.254.169.254:8200").is_err());
    // CGNAT range
    assert!(validate_vault_addr("http://100.64.0.1:8200").is_err());
}

#[test]
fn test_vault_addr_allows_public_addresses() {
    assert!(validate_vault_addr("https://vault.example.com:8200").is_ok());
    assert!(validate_vault_addr("https://203.0.113.10:8200").is_ok());
    assert!(validate_vault_addr("http://vault.local:8200").is_ok());
}
