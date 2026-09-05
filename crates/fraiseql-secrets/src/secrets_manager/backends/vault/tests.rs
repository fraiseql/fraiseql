#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::{
    backend::VaultBackend,
    cache::VaultResponse,
    validation::{MAX_VAULT_SECRET_NAME_BYTES, validate_vault_addr, validate_vault_secret_name},
};
use crate::secrets_manager::{SecretsBackend, SecretsError};

/// Test `VaultBackend` creation
#[test]
fn test_vault_backend_creation() {
    let vault = VaultBackend::new("https://vault.local:8200", "mytoken").unwrap();
    assert_eq!(vault.addr(), "https://vault.local:8200");
    assert_eq!(vault.token(), "mytoken");
}

/// Test `VaultBackend` placeholder returns error
#[tokio::test]
async fn test_vault_backend_placeholder() {
    let vault = VaultBackend::new("https://vault.local:8200", "token").unwrap();

    let result = vault.get_secret("any/path").await;
    assert!(result.is_err(), "placeholder vault should return an error: {result:?}");
}

/// Test multiple `VaultBackend` instances
#[test]
fn test_vault_backend_multiple() {
    let vault1 = VaultBackend::new("https://vault1.local:8200", "token1").unwrap();
    let vault2 = VaultBackend::new("https://vault2.local:8200", "token2").unwrap();

    assert_ne!(vault1.addr(), vault2.addr());
    assert_ne!(vault1.token(), vault2.token());
}

/// Test `VaultBackend` clone
#[test]
fn test_vault_backend_clone() {
    let vault1 = VaultBackend::new("https://vault.local:8200", "token").unwrap();
    let vault2 = vault1.clone();

    assert_eq!(vault1.addr(), vault2.addr());
    assert_eq!(vault1.token(), vault2.token());
}

// --- validate_vault_secret_name tests (S9-1) ---

#[test]
fn test_secret_name_empty_rejected() {
    let result = validate_vault_secret_name("");
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "empty name should be rejected: {result:?}"
    );
}

#[test]
fn test_secret_name_valid_paths() {
    validate_vault_secret_name("db/creds")
        .unwrap_or_else(|e| panic!("db/creds should be valid: {e}"));
    validate_vault_secret_name("secret/app_name/db-password")
        .unwrap_or_else(|e| panic!("nested path should be valid: {e}"));
    validate_vault_secret_name("kv/prod/postgres")
        .unwrap_or_else(|e| panic!("kv path should be valid: {e}"));
}

#[test]
fn test_secret_name_dot_rejected() {
    // `.` is not in the allowed character set — prevents `../` path traversal.
    let result = validate_vault_secret_name("../etc/passwd");
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "path traversal should be rejected: {result:?}"
    );
    let result = validate_vault_secret_name("secret/../../etc");
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "double traversal should be rejected: {result:?}"
    );
    let result = validate_vault_secret_name("secret/app.name");
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "dot in name should be rejected: {result:?}"
    );
}

#[test]
fn test_secret_name_special_chars_rejected() {
    let result = validate_vault_secret_name("secret/app name");
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "space should be rejected: {result:?}"
    );
    let result = validate_vault_secret_name("secret/app\0name");
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "null byte should be rejected: {result:?}"
    );
    let result = validate_vault_secret_name("secret/app;name");
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "semicolon should be rejected: {result:?}"
    );
}

// ── Length-guard tests ─────────────────────────────────────────────────────

#[test]
fn test_secret_name_exactly_max_length_accepted() {
    // MAX_VAULT_SECRET_NAME_BYTES exactly — must be accepted.
    let name = "a".repeat(MAX_VAULT_SECRET_NAME_BYTES);
    validate_vault_secret_name(&name)
        .unwrap_or_else(|e| panic!("name at max length must be accepted: {e}"));
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
    let result = validate_vault_secret_name(&name);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "1 MiB name must be rejected: {result:?}"
    );
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
        VaultBackend::extract_secret_from_response(&response, "database/creds/my-role").unwrap();
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
    assert!(
        result.expose().contains("supersecret"),
        "expected secret value in result (redacted in this message)"
    );
}

#[tokio::test]
async fn test_vault_fetch_secret_not_found_returns_not_found_error() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

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
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

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
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/secret/badjson"))
        .respond_with(ResponseTemplate::new(200).set_body_string("this is not valid json"))
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

    let vault = VaultBackend::new_for_test(mock.uri(), "old-token");
    vault.renew_token().await.expect("renewal should succeed");

    assert_eq!(vault.token(), "new-rotated-token", "token should be updated after renewal");
    assert_eq!(
        vault.token_ttl_secs(),
        Some(7200),
        "TTL should be updated from renewal response"
    );
}

#[tokio::test]
async fn test_renew_token_missing_client_token_returns_error() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth": {}  // client_token is absent
        })))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let result = vault.renew_token().await;
    assert!(
        matches!(result, Err(SecretsError::ConnectionError(_))),
        "missing client_token should return ConnectionError; got: {result:?}"
    );
}

#[tokio::test]
async fn test_renew_token_403_returns_connection_error() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "expired-token");
    // 403 response body is not valid JSON for the renewal struct → ConnectionError
    let result = vault.renew_token().await;
    assert!(
        matches!(result, Err(SecretsError::ConnectionError(_))),
        "403 renewal should return ConnectionError; got: {result:?}"
    );
}

// ── S30: Vault HTTP body-size guards ──────────────────────────────────────────

/// Vault secret fetch must reject responses larger than `MAX_VAULT_RESPONSE_BYTES`.
#[tokio::test]
async fn vault_fetch_secret_rejects_oversized_response() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    // Build a JSON-shaped body that exceeds 1 MiB
    let big_value = "x".repeat(1024 * 1024 + 1);
    let big_body = format!(
        r#"{{"request_id":"r","lease_id":"","lease_duration":3600,"renewable":false,"data":{{"value":"{big_value}"}}}}"#
    );
    Mock::given(method("GET"))
        .and(path("/v1/secret/db-password"))
        .respond_with(ResponseTemplate::new(200).set_body_string(big_body))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let result = vault.get_secret("secret/db-password").await;
    assert!(result.is_err(), "oversized response must be rejected; got: {result:?}");
}

/// Vault token renewal must reject responses larger than `MAX_VAULT_RESPONSE_BYTES`.
#[tokio::test]
async fn vault_token_renewal_rejects_oversized_response() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let big_value = "x".repeat(1024 * 1024 + 1);
    let big_body = format!(
        r#"{{"auth":{{"client_token":"{big_value}","lease_duration":3600,"renewable":true}}}}"#
    );
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .respond_with(ResponseTemplate::new(200).set_body_string(big_body))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "old-token");
    let result = vault.renew_token().await;
    assert!(result.is_err(), "oversized renewal response must be rejected; got: {result:?}");
}

/// Vault Transit operation must reject responses larger than `MAX_VAULT_RESPONSE_BYTES`.
#[tokio::test]
async fn vault_transit_rejects_oversized_response() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let big_value = "x".repeat(1024 * 1024 + 1);
    let big_body = format!(r#"{{"data":{{"ciphertext":"vault:v1:{big_value}","key_version":1}}}}"#);
    Mock::given(method("POST"))
        .and(path("/v1/transit/encrypt/my-key"))
        .respond_with(ResponseTemplate::new(200).set_body_string(big_body))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let result = vault.encrypt_field("transit/encrypt/my-key", "plaintext").await;
    assert!(result.is_err(), "oversized transit response must be rejected; got: {result:?}");
}

/// `AppRole` login must reject responses larger than `MAX_VAULT_RESPONSE_BYTES`.
///
/// `with_approle_for_test` bypasses SSRF validation so we can point at a loopback mock server.
#[tokio::test]
async fn vault_approle_rejects_oversized_response() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let big_value = "x".repeat(1024 * 1024 + 1);
    let big_body = format!(
        r#"{{"auth":{{"client_token":"{big_value}","lease_duration":3600,"renewable":true}}}}"#
    );
    Mock::given(method("POST"))
        .and(path("/v1/auth/approle/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(big_body))
        .mount(&mock)
        .await;

    // with_approle_for_test bypasses SSRF validation to allow pointing at loopback (test only).
    // This constructor is added in the GREEN phase alongside the body-size guard.
    let result =
        VaultBackend::with_approle_for_test(mock.uri().as_str(), "role-id", "secret-id").await;
    assert!(result.is_err(), "oversized approle response must be rejected; got: {result:?}");
}

// ── Phase 06 Cycle 2: Vault trio (H10 / H14 / H15) ────────────────────────────

/// H10: `rotate_secret` must not self-deadlock on its own per-secret rotation lock.
///
/// It previously acquired the lock, then called `get_secret_with_expiry`, which
/// re-acquired the same non-reentrant mutex → permanent hang. Wrapped in a timeout so
/// the deadlock surfaces as a test failure instead of hanging the suite.
#[tokio::test]
async fn vault_rotate_secret_does_not_self_deadlock() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/secret/db-password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "request_id": "abc-123",
            "lease_id": "",
            "lease_duration": 3600,
            "renewable": false,
            "data": {"value": "rotated-secret"}
        })))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let rotated = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        vault.rotate_secret("secret/db-password"),
    )
    .await
    .expect("rotate_secret self-deadlocked (H10): timed out");
    assert!(
        rotated.unwrap().expose().contains("rotated-secret"),
        "rotation should return the freshly fetched secret"
    );
}

/// H14: Transit `encrypt_field` must send PADDED standard base64 (real Vault rejects
/// unpadded for lengths not divisible by 3). "hello" is 5 bytes → padded `aGVsbG8=`.
#[tokio::test]
async fn vault_transit_encrypt_sends_padded_base64() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/transit/encrypt/my-key"))
        .and(body_json(serde_json::json!({"plaintext": "aGVsbG8="})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"ciphertext": "vault:v1:xyz"}
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let ciphertext = vault.encrypt_field("my-key", "hello").await.unwrap();
    assert_eq!(ciphertext, "vault:v1:xyz", "encrypt must send padded base64 Vault accepts");
}

/// H14: Transit `decrypt_field` must accept Vault's always-PADDED standard base64
/// plaintext. The old `STANDARD_NO_PAD` decoder errors on the trailing `=`.
#[tokio::test]
async fn vault_transit_decrypt_accepts_padded_base64() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/transit/decrypt/my-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"plaintext": "aGVsbG8="} // base64("hello"), padded, as real Vault returns
        })))
        .mount(&mock)
        .await;

    let vault = VaultBackend::new_for_test(mock.uri(), "test-token");
    let plaintext = vault.decrypt_field("my-key", "vault:v1:xyz").await.unwrap();
    assert_eq!(plaintext, "hello", "decrypt must accept Vault's padded base64 plaintext");
}

/// H15: `with_approle` must validate the address (SSRF guard) BEFORE posting the
/// `role_id`/`secret_id`. A loopback address is blocked, so zero requests must reach
/// the server — `expect(0)` fails if any credential POST was sent.
#[tokio::test]
async fn vault_approle_validates_address_before_sending_credentials() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/approle/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth": {"client_token": "should-never-be-requested", "lease_duration": 3600}
        })))
        .expect(0)
        .mount(&mock)
        .await;

    // mock.uri() is loopback (127.0.0.1) → blocked by validate_vault_addr.
    let result = VaultBackend::with_approle(mock.uri().as_str(), "role-id", "secret-id").await;
    assert!(result.is_err(), "with_approle must reject a loopback address (H15): {result:?}");
    // Mock drop verifies expect(0): no credential POST was sent.
}

// ── S32: Debug redaction ──────────────────────────────────────────────────────

/// `format!("{:?}")` on `VaultBackend` must not expose the auth token.
#[test]
fn vault_debug_does_not_expose_token() {
    let vault =
        VaultBackend::new("https://vault.example.com:8200", "super-secret-token-12345").unwrap();
    let debug_output = format!("{vault:?}");
    assert!(
        !debug_output.contains("super-secret-token-12345"),
        "token must not appear in Debug output: {debug_output}"
    );
    assert!(
        debug_output.contains("[REDACTED]"),
        "Debug output must show [REDACTED] for token: {debug_output}"
    );
}

// ── S47: Rotation-lock field exists ──────────────────────────────────────────

/// S47: `VaultBackend` must have a `rotation_locks` field (`DashMap`).
/// This test confirms the struct field exists and can be queried.
#[test]
fn vault_backend_has_rotation_locks_field() {
    let vault = VaultBackend::new("https://vault.example.com:8200", "test-token").unwrap();
    assert_eq!(vault.rotation_locks_len(), 0, "fresh backend should have no locks");
}

// ── The shared outbound corpus, at this crate's entry point ───────────────────

/// Clear the bypass and the posture markers so the guard is actually exercised.
///
/// Every test in this section goes through this helper or through
/// [`with_bypass_requested`], and that is not a style preference. `temp_env`
/// serialises through a global mutex, so a lock-free reader races whichever sibling
/// is inside `with_bypass_requested`, and in that window `validate_vault_addr`
/// returns `Ok(())` from the bypass before the guard runs at all. Four tests here
/// read lock-free and reddened `Dagger — test` at random (#1272) — and the one of
/// them that asserted `Ok` reddened nothing: it passed with the SSRF guard refusing
/// every address, which is the direction that stays silent forever.
/// `tools/check-guard-test-lock.py` is the gate.
fn with_guard_engaged<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> T {
    let mut out = None;
    temp_env::with_vars(
        [
            ("FRAISEQL_VAULT_ALLOW_INSECURE", None::<&str>),
            ("FRAISEQL_ENV", None),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || out = Some(f()),
    );
    out.expect("temp_env ran the closure")
}

#[test]
fn vault_addr_refuses_every_blocked_corpus_entry() {
    use fraiseql_guard::net::vectors::{MUST_BLOCK, MUST_BLOCK_HOSTS, url_host};
    with_guard_engaged(|| {
        for (addr, why) in MUST_BLOCK {
            let url = format!("https://{}:8200", url_host(addr));
            assert!(validate_vault_addr(&url).is_err(), "must refuse {addr} ({why})");
        }
        for (host, why) in MUST_BLOCK_HOSTS {
            let url = format!("https://{host}");
            assert!(validate_vault_addr(&url).is_err(), "must refuse {host} ({why})");
        }
    });
}

#[test]
fn vault_addr_permits_every_allowed_corpus_entry() {
    use fraiseql_guard::net::vectors::{MUST_ALLOW, url_host};
    with_guard_engaged(|| {
        for addr in MUST_ALLOW {
            let url = format!("https://{}:8200", url_host(addr));
            assert!(validate_vault_addr(&url).is_ok(), "must permit {addr}");
        }
    });
}

/// The scheme rule is this crate's own: the shared corpus classifies *hosts*, so no
/// entry in it reaches `file://`, `ftp://`, or a bare `host:port`. Measured — with the
/// scheme branch deleted both corpus tests above stay green and only this one reddens.
#[test]
fn test_vault_addr_scheme_must_be_http() {
    with_guard_engaged(|| {
        for addr in [
            "file:///etc/passwd",
            "ftp://vault.example.com:8200",
            "vault.example.com:8200",
        ] {
            assert!(
                matches!(validate_vault_addr(addr), Err(SecretsError::ValidationError(_))),
                "scheme must be http(s): {addr}"
            );
        }
    });
}

/// The counterweight for *hostnames*, which nothing else reaches from this entry point:
/// `vault_addr_permits_every_allowed_corpus_entry` iterates `MUST_ALLOW`, every row of
/// which is an IP literal, and `MUST_ALLOW_HOSTS` is consumed by no dependent crate in
/// the workspace (#1280). Measured — block every non-literal host and both corpus
/// tests stay green while this one reddens. `http://vault.local:8200` also keeps the
/// `http` + explicit-port shape covered; `url_host` builds corpus URLs as `https`
/// without a port.
#[test]
fn test_vault_addr_allows_public_addresses() {
    with_guard_engaged(|| {
        validate_vault_addr("https://vault.example.com:8200")
            .unwrap_or_else(|e| panic!("public vault addr should pass: {e}"));
        // Not 203.0.113.x: that is TEST-NET-3, an RFC 5737 documentation range the shared
        // guard refuses because it is not globally routable.
        validate_vault_addr("https://93.184.216.34:8200")
            .unwrap_or_else(|e| panic!("public IP vault addr should pass: {e}"));
        validate_vault_addr("http://vault.local:8200")
            .unwrap_or_else(|e| panic!("vault.local should pass: {e}"));
    });
}

// --- #726: renewal is reachable through Arc and actually renews on a loop ---

/// The defect in #726 was structural: `renew_token(&mut self)` could not be
/// called through the `Arc` the factory returns, so no shipped deployment could
/// renew. This pin is primarily a compile-level guarantee — it drives renewal
/// through `Arc<VaultBackend>` with no `&mut` anywhere.
#[tokio::test]
async fn renew_token_is_reachable_through_an_arc() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth": { "client_token": "renewed-through-arc", "lease_duration": 3600 }
        })))
        .mount(&server)
        .await;

    let vault = std::sync::Arc::new(VaultBackend::new_for_test(server.uri(), "initial-token"));
    vault.renew_token().await.expect("renewal must succeed through the Arc");
    assert_eq!(vault.token(), "renewed-through-arc");
}

/// The renewal loop drives `renew_token` once 80% of the TTL has elapsed and
/// keeps the backend usable past the original TTL — the availability property
/// `AppRole` deployments lost to #726.
#[tokio::test]
async fn the_renewal_loop_renews_before_the_token_ttl_expires() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };
    let server = MockServer::start().await;
    // AppRole login issues a 2-second token.
    Mock::given(method("POST"))
        .and(path("/v1/auth/approle/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth": { "client_token": "short-lived-token", "lease_duration": 2 }
        })))
        .mount(&server)
        .await;
    // renew-self only answers for the token the login issued, proving the loop
    // sent the current token, and hands back a fresh one.
    Mock::given(method("POST"))
        .and(path("/v1/auth/token/renew-self"))
        .and(header("X-Vault-Token", "short-lived-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth": { "client_token": "renewed-token", "lease_duration": 3600 }
        })))
        .mount(&server)
        .await;

    let vault = std::sync::Arc::new(
        VaultBackend::with_approle_for_test(&server.uri(), "role", "secret")
            .await
            .expect("mock AppRole login succeeds"),
    );
    assert_eq!(vault.token_ttl_secs(), Some(2));

    let (_handle, cancel) =
        VaultBackend::spawn_token_renewal(&vault, std::time::Duration::from_millis(200));

    // 80% of 2s = 1.6s; by 2.5s the loop must have renewed (several 200ms ticks
    // land inside the [1.6s, 2s) window).
    tokio::time::sleep(std::time::Duration::from_millis(2_500)).await;
    assert_eq!(
        vault.token(),
        "renewed-token",
        "the renewal loop must have replaced the token before its 2s TTL expired"
    );
    assert!(
        !vault.token_needs_renewal(),
        "a fresh 3600s lease is nowhere near its renewal threshold"
    );
    drop(cancel);
}

// --- #727: the SSRF allowlist admits exact hosts only ---

#[test]
fn allowlist_admits_exact_hosts_case_insensitively() {
    use super::validation::host_is_allowlisted;
    assert!(host_is_allowlisted("10.2.3.4", Some("vault.internal, 10.2.3.4")));
    assert!(host_is_allowlisted("Vault.Internal", Some("vault.internal")));
    assert!(!host_is_allowlisted("10.2.3.5", Some("10.2.3.4")));
    assert!(!host_is_allowlisted("10.2.3.4", None));
    assert!(!host_is_allowlisted("10.2.3.4", Some("")));
    assert!(!host_is_allowlisted("", Some(",,")), "empty host never matches stray commas");
    assert!(
        !host_is_allowlisted("sub.vault.internal", Some("vault.internal")),
        "no suffix/wildcard matching — exact hosts only"
    );
}

// ── #882: the escape hatch is refused in production, at this call site ────────

/// Request the bypass, then set the posture. The corpus tests above clear the
/// bypass so the guard is exercised; these set it, so the *hatch* is exercised.
fn with_bypass_requested<T>(
    env: Option<&str>,
    f: impl FnOnce() -> T + std::panic::UnwindSafe,
) -> T {
    let mut out = None;
    temp_env::with_vars(
        [
            ("FRAISEQL_VAULT_ALLOW_INSECURE", Some("1")),
            ("FRAISEQL_ENV", env),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
            ("FRAISEQL_VAULT_ALLOWED_HOSTS", None),
        ],
        || out = Some(f()),
    );
    out.expect("temp_env ran the closure")
}

/// The address the bypass used to expose: `validate_vault_addr` returned `Ok`
/// on the env var alone, under every environment including an explicit
/// `FRAISEQL_ENV=production`.
const METADATA_SERVICE: &str = "http://169.254.169.254:8200";

#[test]
fn vault_allow_insecure_is_refused_under_production_posture() {
    assert!(
        with_bypass_requested(Some("production"), || validate_vault_addr(METADATA_SERVICE))
            .is_err(),
        "#882: FRAISEQL_VAULT_ALLOW_INSECURE must not disable the SSRF guard in \
         production — a stray .env line or Dockerfile ENV must not open the \
         instance-metadata service"
    );
    assert!(
        with_bypass_requested(None, || validate_vault_addr(METADATA_SERVICE)).is_err(),
        "unset FRAISEQL_ENV is production: the hatch must not be honoured by default"
    );
}

#[test]
fn vault_allow_insecure_is_still_honoured_in_a_declared_development_environment() {
    assert!(
        with_bypass_requested(Some("development"), || validate_vault_addr(METADATA_SERVICE))
            .is_ok(),
        "the hatch must keep working where it is meant to — otherwise the test \
         above would pass with the hatch simply deleted"
    );
}
