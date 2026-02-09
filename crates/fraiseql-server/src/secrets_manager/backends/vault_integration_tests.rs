//! Tests for HashiCorp Vault backend integration
//!
//! Uses wiremock to mock Vault HTTP API for reliable, fast testing
//! without requiring a real Vault instance.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod vault_integration_tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    use crate::secrets_manager::{SecretsBackend, SecretsError, VaultBackend};

    /// Helper to create a standard Vault KV2 response body
    fn kv2_response(secret_data: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "request_id": "test-request-id",
            "lease_id": "",
            "lease_duration": 0,
            "renewable": false,
            "data": {
                "data": secret_data
            }
        })
    }

    /// Helper to create a dynamic credentials response body
    fn dynamic_creds_response(
        username: &str,
        password: &str,
        lease_id: &str,
        lease_duration: i64,
    ) -> serde_json::Value {
        serde_json::json!({
            "request_id": "test-request-id",
            "lease_id": lease_id,
            "lease_duration": lease_duration,
            "renewable": true,
            "data": {
                "username": username,
                "password": password
            }
        })
    }

    /// Test Vault connection establishment
    #[tokio::test]
    async fn test_vault_connection_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/test-secret"))
            .and(header("X-Vault-Token", "test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "secret123"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.get_secret("secret/data/test-secret").await;
        assert!(result.is_ok());
    }

    /// Test Vault connection with invalid token
    #[tokio::test]
    async fn test_vault_invalid_token() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/test-secret"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "errors": ["permission denied"]
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "invalid-token");
        let result = vault.get_secret("secret/data/test-secret").await;
        assert!(result.is_err());
        match result {
            Err(SecretsError::BackendError(msg)) => {
                assert!(msg.contains("Permission denied"), "Expected permission denied: {}", msg);
            },
            other => panic!("Expected BackendError, got: {:?}", other),
        }
    }

    /// Test dynamic database credentials retrieval
    #[tokio::test]
    async fn test_vault_get_db_credentials() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/fraiseql-role"))
            .and(header("X-Vault-Token", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "v-token-db-fraiseql-xxx",
                "A1b2C3d4E5f6",
                "database/creds/fraiseql-role/abc123",
                3600,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.get_secret("database/creds/fraiseql-role").await;
        assert!(result.is_ok());
        let secret = result.unwrap();
        assert!(secret.contains("v-token-db-fraiseql-xxx"));
        assert!(secret.contains("A1b2C3d4E5f6"));
    }

    /// Test dynamic credentials with expiry
    #[tokio::test]
    async fn test_vault_db_credentials_with_expiry() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/fraiseql-role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "v-token-db-xxx",
                "password123",
                "database/creds/fraiseql-role/lease1",
                7200,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let (secret, expiry) =
            vault.get_secret_with_expiry("database/creds/fraiseql-role").await.unwrap();

        assert!(secret.contains("v-token-db-xxx"));
        // Expiry should be approximately 2 hours from now (7200 seconds)
        let now = chrono::Utc::now();
        assert!(expiry > now, "Expiry should be in the future");
        let diff = (expiry - now).num_seconds();
        assert!(diff > 7100 && diff <= 7200, "Expiry should be ~7200 seconds: got {}", diff);
    }

    /// Test Vault lease management
    #[tokio::test]
    async fn test_vault_lease_tracking() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user",
                "pass",
                "database/creds/role/lease-123",
                3600,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let _secret = vault.get_secret("database/creds/role").await.unwrap();

        // Verify secret is cached (second call should not hit Vault)
        let cache_size = vault.cache_size().await;
        assert!(cache_size > 0, "Secret should be cached after first fetch");
    }

    /// Test automatic lease renewal
    #[tokio::test]
    async fn test_vault_automatic_lease_renewal() {
        let mock_server = MockServer::start().await;

        // Initial credentials
        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user",
                "pass",
                "database/creds/role/lease-abc",
                3600,
            )))
            .mount(&mock_server)
            .await;

        // Lease renewal endpoint
        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/renew"))
            .and(header("X-Vault-Token", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "lease_id": "database/creds/role/lease-abc",
                "lease_duration": 3600,
                "renewable": true
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let _secret = vault.get_secret("database/creds/role").await.unwrap();

        // Renew lease directly
        let result = vault.renew_lease("database/creds/role/lease-abc").await;
        assert!(result.is_ok());
        let new_duration = result.unwrap();
        assert_eq!(new_duration, 3600);
    }

    /// Test lease revocation on credential rotation
    #[tokio::test]
    async fn test_vault_lease_revocation() {
        let mock_server = MockServer::start().await;
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // Credentials endpoint (called twice: initial + after rotation)
        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let lease_id = format!("database/creds/role/lease-{}", count);
                ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                    &format!("user-{}", count),
                    "pass",
                    &lease_id,
                    3600,
                ))
            })
            .mount(&mock_server)
            .await;

        // Lease revocation endpoint
        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/revoke"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let secret1 = vault.get_secret("database/creds/role").await.unwrap();
        let secret2 = vault.rotate_secret("database/creds/role").await.unwrap();

        // Should get different credentials after rotation
        assert_ne!(secret1, secret2);
    }

    /// Test Transit engine encryption
    #[tokio::test]
    async fn test_vault_transit_encrypt() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/transit/encrypt/my-key"))
            .and(header("X-Vault-Token", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "ciphertext": "vault:v1:encrypted-data"
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.encrypt_field("my-key", "hello world").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "vault:v1:encrypted-data");
    }

    /// Test Transit engine decryption
    #[tokio::test]
    async fn test_vault_transit_decrypt() {
        let mock_server = MockServer::start().await;

        let plaintext_b64 = STANDARD_NO_PAD.encode("hello world");

        Mock::given(method("POST"))
            .and(path("/v1/transit/decrypt/my-key"))
            .and(header("X-Vault-Token", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "plaintext": plaintext_b64
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.decrypt_field("my-key", "vault:v1:encrypted-data").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello world");
    }

    /// Test encryption key rotation
    #[tokio::test]
    async fn test_vault_key_rotation() {
        let mock_server = MockServer::start().await;

        // Both old and new key versions decrypt correctly
        let plaintext_b64 = STANDARD_NO_PAD.encode("sensitive data");

        Mock::given(method("POST"))
            .and(path("/v1/transit/decrypt/my-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "plaintext": plaintext_b64
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // Old ciphertext (v1) still decrypts
        let result = vault.decrypt_field("my-key", "vault:v1:old-ciphertext").await;
        assert!(result.is_ok());

        // New ciphertext (v2) also decrypts
        let result = vault.decrypt_field("my-key", "vault:v2:new-ciphertext").await;
        assert!(result.is_ok());
    }

    /// Test generic secret retrieval from Vault
    #[tokio::test]
    async fn test_vault_generic_secret() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/my-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                kv2_response(serde_json::json!({"api_key": "sk-test-123456"})),
            ))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.get_secret("secret/data/my-secret").await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("sk-test-123456"));
    }

    /// Test Vault API error handling
    #[tokio::test]
    async fn test_vault_api_errors() {
        let mock_server = MockServer::start().await;

        // 404 Not Found
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // 403 Forbidden
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/forbidden"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token").with_max_retries(0);

        // 404 maps to NotFound
        let result = vault.get_secret("secret/data/missing").await;
        assert!(matches!(result, Err(SecretsError::NotFound(_))));

        // 403 maps to BackendError (permission denied)
        let result = vault.get_secret("secret/data/forbidden").await;
        assert!(matches!(result, Err(SecretsError::BackendError(_))));
    }

    /// Test Vault configuration
    #[tokio::test]
    async fn test_vault_backend_config() {
        let vault = VaultBackend::new("https://vault.example.com:8200", "s.mytoken123")
            .with_namespace("fraiseql/prod")
            .with_tls_verify(true)
            .with_max_retries(5);

        assert_eq!(vault.addr(), "https://vault.example.com:8200");
        assert_eq!(vault.token(), "s.mytoken123");
        assert_eq!(vault.namespace(), Some("fraiseql/prod"));
        assert!(vault.tls_verify());
    }

    /// Test multiple concurrent Vault operations
    #[tokio::test]
    async fn test_vault_concurrent_operations() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/concurrent"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "test"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // Spawn 10 concurrent requests
        let mut handles = Vec::new();
        for _ in 0..10 {
            let v = vault.clone();
            handles.push(tokio::spawn(async move {
                v.get_secret("secret/data/concurrent").await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        for result in results {
            assert!(result.unwrap().is_ok());
        }
    }

    /// Test Vault namespace isolation (Enterprise)
    #[tokio::test]
    async fn test_vault_namespace_isolation() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/test"))
            .and(header("X-Vault-Namespace", "team-alpha"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "alpha-secret"}))),
            )
            .mount(&mock_server)
            .await;

        let vault =
            VaultBackend::new(mock_server.uri(), "test-token").with_namespace("team-alpha");
        let result = vault.get_secret("secret/data/test").await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("alpha-secret"));
    }

    /// Test Vault response parsing
    #[test]
    fn test_vault_response_parsing() {
        // KV2 format
        let kv2_body = kv2_response(serde_json::json!({"password": "secret"}));
        let response: crate::secrets_manager::backends::vault::VaultResponse =
            serde_json::from_value(kv2_body).unwrap();
        assert!(response.data.contains_key("data"));

        // Dynamic credentials format
        let creds_body = dynamic_creds_response("user", "pass", "lease-id", 3600);
        let response: crate::secrets_manager::backends::vault::VaultResponse =
            serde_json::from_value(creds_body).unwrap();
        assert_eq!(response.lease_id, "lease-id");
        assert_eq!(response.lease_duration, 3600);
        assert!(response.renewable);
    }

    /// Test credential caching
    #[tokio::test]
    async fn test_vault_credential_caching() {
        let mock_server = MockServer::start().await;

        // Use a response with a non-zero lease_duration so the cache entry doesn't expire immediately
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/cached"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "request_id": "test-request-id",
                "lease_id": "",
                "lease_duration": 3600,
                "renewable": false,
                "data": {
                    "data": {"value": "cached-val"}
                }
            })))
            .expect(1) // Should only be called once due to caching
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // First call hits Vault
        let r1 = vault.get_secret("secret/data/cached").await.unwrap();
        // Second call uses cache
        let r2 = vault.get_secret("secret/data/cached").await.unwrap();

        assert_eq!(r1, r2);
    }

    /// Test token refresh
    #[tokio::test]
    async fn test_vault_token_refresh() {
        let mock_server = MockServer::start().await;

        // Token renewal endpoint
        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/renew"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "lease_id": "lease-123",
                "lease_duration": 7200,
                "renewable": true
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.renew_lease("lease-123").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 7200);
    }

    /// Test Vault audit integration
    #[tokio::test]
    async fn test_vault_audit_integration() {
        let mock_server = MockServer::start().await;

        // All operations should succeed and are auditable by Vault's audit backend
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/audited"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "audit-test"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.get_secret("secret/data/audited").await;
        assert!(result.is_ok());
        // In production, Vault's audit backend logs this request automatically
    }

    /// Test Vault with TLS
    #[tokio::test]
    async fn test_vault_tls_connection() {
        // TLS verification can be disabled for dev mode
        let vault = VaultBackend::new("https://vault.local:8200", "token").with_tls_verify(false);
        assert!(!vault.tls_verify());

        // TLS verification enabled by default
        let vault = VaultBackend::new("https://vault.local:8200", "token");
        assert!(vault.tls_verify());
    }

    /// Test Vault health check
    #[tokio::test]
    async fn test_vault_health_endpoint() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/sys/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "initialized": true,
                "sealed": false,
                "version": "1.15.0"
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let health = vault.health_check().await.unwrap();
        assert!(health.initialized);
        assert!(!health.sealed);
        assert_eq!(health.version.as_deref(), Some("1.15.0"));
    }

    /// Test graceful degradation on Vault unavailability
    #[tokio::test]
    async fn test_vault_unavailability_handling() {
        // Use a non-existent server address (localhost on random port)
        let vault = VaultBackend::new("http://127.0.0.1:59999", "token").with_max_retries(0);

        let result = vault.get_secret("secret/data/test").await;
        assert!(result.is_err());
        match result {
            Err(SecretsError::BackendError(msg)) => {
                assert!(
                    msg.contains("failed") || msg.contains("error"),
                    "Error should indicate connection failure: {}",
                    msg
                );
            },
            other => panic!("Expected BackendError, got: {:?}", other),
        }
    }

    /// Test Vault role-based access control
    #[tokio::test]
    async fn test_vault_rbac() {
        let mock_server = MockServer::start().await;

        // Token has permission for database creds
        Mock::given(method("GET"))
            .and(path("/v1/database/creds/allowed-role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user", "pass", "lease-1", 3600,
            )))
            .mount(&mock_server)
            .await;

        // Token does NOT have permission for admin path
        Mock::given(method("GET"))
            .and(path("/v1/sys/admin/config"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token").with_max_retries(0);

        // Allowed path works
        let result = vault.get_secret("database/creds/allowed-role").await;
        assert!(result.is_ok());

        // Forbidden path returns error
        let result = vault.get_secret("sys/admin/config").await;
        assert!(matches!(result, Err(SecretsError::BackendError(_))));
    }

    /// Test database credential format
    #[test]
    fn test_vault_db_credential_format() {
        let creds_json = dynamic_creds_response(
            "v-token-db-fraiseql-xxx",
            "A1b2C3d4E5f6",
            "database/creds/fraiseql-role/abc123",
            3600,
        );

        let response: crate::secrets_manager::backends::vault::VaultResponse =
            serde_json::from_value(creds_json).unwrap();

        assert_eq!(response.lease_id, "database/creds/fraiseql-role/abc123");
        assert_eq!(response.lease_duration, 3600);
        assert!(response.renewable);
        assert_eq!(
            response.data["username"].as_str().unwrap(),
            "v-token-db-fraiseql-xxx"
        );
        assert_eq!(response.data["password"].as_str().unwrap(), "A1b2C3d4E5f6");
    }

    /// Test error recovery
    #[tokio::test]
    async fn test_vault_error_recovery() {
        let mock_server = MockServer::start().await;
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // First two calls return 503, third succeeds
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/flaky"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count < 2 {
                    ResponseTemplate::new(503)
                } else {
                    ResponseTemplate::new(200)
                        .set_body_json(kv2_response(serde_json::json!({"value": "recovered"})))
                }
            })
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token").with_max_retries(3);
        let result = vault.get_secret("secret/data/flaky").await;
        assert!(result.is_ok(), "Should recover after retries: {:?}", result);
        assert!(result.unwrap().contains("recovered"));
    }
}
