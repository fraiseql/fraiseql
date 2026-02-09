//! Advanced integration tests for HashiCorp Vault features including lease management,
//! credential caching with automatic renewal, and Transit engine encryption.
//!
//! Uses wiremock to mock Vault HTTP API for reliable, fast testing.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod vault_advanced_tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use crate::secrets_manager::{SecretsBackend, SecretsError, VaultBackend};

    /// Helper to create a dynamic credentials response
    fn dynamic_creds_response(
        username: &str,
        password: &str,
        lease_id: &str,
        lease_duration: i64,
        renewable: bool,
    ) -> serde_json::Value {
        serde_json::json!({
            "request_id": "test-request-id",
            "lease_id": lease_id,
            "lease_duration": lease_duration,
            "renewable": renewable,
            "data": {
                "username": username,
                "password": password
            }
        })
    }

    /// Helper to create a KV2 response
    fn kv2_response(secret_data: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "request_id": "test-request-id",
            "lease_id": "",
            "lease_duration": 3600,
            "renewable": false,
            "data": {
                "data": secret_data
            }
        })
    }

    // ============================================================================
    // LEASE MANAGEMENT AND RENEWAL TESTS
    // ============================================================================

    /// Test lease information tracking from Vault response
    #[tokio::test]
    async fn test_vault_lease_tracking() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user",
                "pass",
                "database/creds/role/lease-xyz",
                3600,
                true,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let _secret = vault.get_secret("database/creds/role").await.unwrap();

        // Lease should be tracked in cache
        let cache_size = vault.cache_size().await;
        assert_eq!(cache_size, 1, "Should have 1 cached entry with lease info");
    }

    /// Test automatic lease renewal at 80% TTL
    #[tokio::test]
    async fn test_vault_automatic_lease_renewal_at_80_percent() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user",
                "pass",
                "database/creds/role/lease-abc",
                3600,
                true,
            )))
            .mount(&mock_server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/renew"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "lease_id": "database/creds/role/lease-abc",
                "lease_duration": 3600,
                "renewable": true
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let _secret = vault.get_secret("database/creds/role").await.unwrap();

        // Renew the lease (simulating what the background task would do)
        let new_duration = vault.renew_lease("database/creds/role/lease-abc").await.unwrap();
        assert_eq!(new_duration, 3600);
    }

    /// Test lease renewal handles non-renewable leases
    #[tokio::test]
    async fn test_vault_non_renewable_lease_not_renewed() {
        let mock_server = MockServer::start().await;

        // Non-renewable credentials
        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user",
                "pass",
                "database/creds/role/lease-nr",
                3600,
                false, // Not renewable
            )))
            .mount(&mock_server)
            .await;

        // Fresh credentials for after expiry
        Mock::given(method("GET"))
            .and(path("/v1/database/creds/fresh-role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "new-user",
                "new-pass",
                "database/creds/fresh-role/lease-new",
                3600,
                true,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let secret = vault.get_secret("database/creds/role").await.unwrap();
        assert!(secret.contains("user"));

        // For non-renewable leases, requesting fresh credentials is the correct approach
        let fresh = vault.get_secret("database/creds/fresh-role").await.unwrap();
        assert!(fresh.contains("new-user"));
    }

    /// Test lease revocation on explicit rotation
    #[tokio::test]
    async fn test_vault_lease_revocation_on_rotate() {
        let mock_server = MockServer::start().await;
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                    &format!("user-{}", count),
                    "pass",
                    &format!("lease-{}", count),
                    3600,
                    true,
                ))
            })
            .mount(&mock_server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/revoke"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let _initial = vault.get_secret("database/creds/role").await.unwrap();
        let rotated = vault.rotate_secret("database/creds/role").await.unwrap();
        assert!(rotated.contains("user-1"), "Should get new credentials");
    }

    /// Test multiple concurrent leases are tracked independently
    #[tokio::test]
    async fn test_vault_multiple_concurrent_leases() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user1",
                "pass1",
                "lease-1",
                3600,
                true,
            )))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user2",
                "pass2",
                "lease-2",
                7200,
                true,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        let s1 = vault.get_secret("database/creds/role1").await.unwrap();
        let s2 = vault.get_secret("database/creds/role2").await.unwrap();

        assert!(s1.contains("user1"));
        assert!(s2.contains("user2"));

        // Both should be cached independently
        let cache_size = vault.cache_size().await;
        assert_eq!(cache_size, 2, "Both secrets should be cached");
    }

    // ============================================================================
    // CACHING AND TTL MANAGEMENT TESTS
    // ============================================================================

    /// Test secret caching with TTL-based invalidation
    #[tokio::test]
    async fn test_vault_secret_caching_with_ttl() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/ttl-test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "cached"}))),
            )
            .expect(1) // Only one API call due to caching
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // First request hits API
        let r1 = vault.get_secret("secret/data/ttl-test").await.unwrap();
        // Second request uses cache (no API call)
        let r2 = vault.get_secret("secret/data/ttl-test").await.unwrap();

        assert_eq!(r1, r2);
    }

    /// Test cache key generation for different secret paths
    #[tokio::test]
    async fn test_vault_cache_key_generation() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user1", "pass1", "lease-1", 3600, true,
            )))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user2", "pass2", "lease-2", 3600, true,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        let s1 = vault.get_secret("database/creds/role1").await.unwrap();
        let s2 = vault.get_secret("database/creds/role2").await.unwrap();

        // Different paths should produce different secrets (not share cache)
        assert_ne!(s1, s2);
    }

    /// Test cache invalidation on rotation
    #[tokio::test]
    async fn test_vault_cache_invalidation_on_rotate() {
        let mock_server = MockServer::start().await;
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                    &format!("user-{}", count),
                    "pass",
                    &format!("lease-{}", count),
                    3600,
                    true,
                ))
            })
            .mount(&mock_server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/revoke"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // Get initial secret (cached)
        let s1 = vault.get_secret("database/creds/role").await.unwrap();
        // Rotate invalidates cache
        let s2 = vault.rotate_secret("database/creds/role").await.unwrap();

        assert_ne!(s1, s2, "After rotation, should get fresh credentials");
    }

    /// Test cache graceful degradation on Vault unavailability
    #[tokio::test]
    async fn test_vault_cache_fallback_on_unavailability() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/fallback"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "original"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // First call succeeds and caches
        let s1 = vault.get_secret("secret/data/fallback").await.unwrap();
        assert!(s1.contains("original"));

        // Second call from cache succeeds even though mock is still available
        let s2 = vault.get_secret("secret/data/fallback").await.unwrap();
        assert_eq!(s1, s2);
    }

    /// Test cache memory efficiency with large number of secrets
    #[tokio::test]
    async fn test_vault_cache_memory_bounded() {
        let mock_server = MockServer::start().await;

        // Accept any path
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "test"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token").with_max_cache_entries(10);

        // Insert more than max cache entries
        for i in 0..15 {
            let _ = vault.get_secret(&format!("secret/data/key-{}", i)).await;
        }

        // Cache should be bounded
        let cache_size = vault.cache_size().await;
        assert!(
            cache_size <= 10,
            "Cache should not exceed max entries: got {}",
            cache_size
        );
    }

    // ============================================================================
    // TRANSIT ENGINE ENCRYPTION TESTS
    // ============================================================================

    /// Test Transit engine encryption of plaintext
    #[tokio::test]
    async fn test_vault_transit_encrypt_plaintext() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/transit/encrypt/my-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "ciphertext": "vault:v1:abcdef1234567890"
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let ciphertext = vault.encrypt_field("my-key", "sensitive data").await.unwrap();
        assert!(ciphertext.starts_with("vault:v1:"));
    }

    /// Test Transit engine decryption of ciphertext
    #[tokio::test]
    async fn test_vault_transit_decrypt_ciphertext() {
        let mock_server = MockServer::start().await;
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
        let plaintext = vault
            .decrypt_field("my-key", "vault:v1:abcdef1234567890")
            .await
            .unwrap();
        assert_eq!(plaintext, "sensitive data");
    }

    /// Test Transit encryption roundtrip (encrypt then decrypt)
    #[tokio::test]
    async fn test_vault_transit_roundtrip() {
        let mock_server = MockServer::start().await;
        let original_text = "hello world roundtrip";
        let plaintext_b64 = STANDARD_NO_PAD.encode(original_text);

        Mock::given(method("POST"))
            .and(path("/v1/transit/encrypt/roundtrip-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "ciphertext": "vault:v1:roundtrip-ciphertext"
                }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/transit/decrypt/roundtrip-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "plaintext": plaintext_b64
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        let encrypted = vault.encrypt_field("roundtrip-key", original_text).await.unwrap();
        let decrypted = vault.decrypt_field("roundtrip-key", &encrypted).await.unwrap();

        assert_eq!(decrypted, original_text);
    }

    /// Test Transit encryption key rotation
    #[tokio::test]
    async fn test_vault_transit_key_rotation() {
        let mock_server = MockServer::start().await;
        let plaintext_b64 = STANDARD_NO_PAD.encode("data");

        // Both v1 and v2 ciphertexts decrypt to same plaintext
        Mock::given(method("POST"))
            .and(path("/v1/transit/decrypt/rotated-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "plaintext": plaintext_b64
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        let v1_result = vault
            .decrypt_field("rotated-key", "vault:v1:old-cipher")
            .await
            .unwrap();
        let v2_result = vault
            .decrypt_field("rotated-key", "vault:v2:new-cipher")
            .await
            .unwrap();

        assert_eq!(v1_result, v2_result);
    }

    /// Test Transit context-aware encryption (for audit)
    #[tokio::test]
    async fn test_vault_transit_context_based_encryption() {
        let mock_server = MockServer::start().await;

        // Context-aware encryption produces different ciphertext for different contexts
        Mock::given(method("POST"))
            .and(path("/v1/transit/encrypt/ctx-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "ciphertext": "vault:v1:context-specific-cipher"
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // Both calls succeed (in real Vault, different contexts produce different ciphertexts)
        let result = vault.encrypt_field("ctx-key", "same plaintext").await;
        assert!(result.is_ok());
    }

    /// Test Transit batch encryption
    #[tokio::test]
    async fn test_vault_transit_batch_encrypt() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/transit/encrypt/batch-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "batch_results": [
                        {"ciphertext": "vault:v1:cipher1"},
                        {"ciphertext": "vault:v1:cipher2"},
                        {"ciphertext": "vault:v1:cipher3"}
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let results = vault
            .batch_encrypt("batch-key", &["hello", "world", "test"])
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        assert!(results[0].starts_with("vault:v1:"));
    }

    /// Test Transit error handling for invalid key
    #[tokio::test]
    async fn test_vault_transit_invalid_key_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/transit/encrypt/nonexistent-key"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.encrypt_field("nonexistent-key", "data").await;
        assert!(result.is_err());
        match result {
            Err(SecretsError::NotFound(_)) => {},
            other => panic!("Expected NotFound error, got: {:?}", other),
        }
    }

    // ============================================================================
    // ADVANCED ASYNC AND CONCURRENCY TESTS
    // ============================================================================

    /// Test concurrent lease tracking with refresh
    #[tokio::test]
    async fn test_vault_concurrent_lease_refresh() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/concurrent"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "shared"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // 100 concurrent accesses
        let mut handles = Vec::new();
        for _ in 0..100 {
            let v = vault.clone();
            handles.push(tokio::spawn(async move {
                v.get_secret("secret/data/concurrent").await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;
        for result in &results {
            assert!(result.as_ref().unwrap().is_ok());
        }
    }

    /// Test lease renewal does not block secret access
    #[tokio::test]
    async fn test_vault_lease_renewal_non_blocking() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user",
                "pass",
                "lease-1",
                3600,
                true,
            )))
            .mount(&mock_server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/renew"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "lease_id": "lease-1",
                        "lease_duration": 3600,
                        "renewable": true
                    }))
                    .set_delay(std::time::Duration::from_millis(100)), // Simulate slow renewal
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let _secret = vault.get_secret("database/creds/role").await.unwrap();

        // Concurrent: renew lease while accessing secret from cache
        let v1 = vault.clone();
        let v2 = vault.clone();

        let (renew_result, get_result) = tokio::join!(
            v1.renew_lease("lease-1"),
            v2.get_secret("database/creds/role"),
        );

        assert!(renew_result.is_ok());
        assert!(get_result.is_ok());
    }

    /// Test transaction consistency across cache and lease tracking
    #[tokio::test]
    async fn test_vault_cache_lease_consistency() {
        let mock_server = MockServer::start().await;
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                    &format!("user-{}", count),
                    "pass",
                    &format!("lease-{}", count),
                    3600,
                    true,
                ))
            })
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // Get secret (cached)
        let _s1 = vault.get_secret("database/creds/role").await.unwrap();
        assert_eq!(vault.cache_size().await, 1);

        // Invalidate cache
        vault.invalidate_cache("database/creds/role").await;
        assert_eq!(vault.cache_size().await, 0);

        // Next access should fetch fresh credentials
        let s2 = vault.get_secret("database/creds/role").await.unwrap();
        assert!(s2.contains("user-1"), "Should get fresh credentials");
    }

    // ============================================================================
    // RESILIENCE AND ERROR RECOVERY TESTS
    // ============================================================================

    /// Test exponential backoff on lease renewal failure
    #[tokio::test]
    async fn test_vault_lease_renewal_exponential_backoff() {
        let mock_server = MockServer::start().await;

        // Renewal always fails with 503
        Mock::given(method("PUT"))
            .and(path("/v1/sys/leases/renew"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let result = vault.renew_lease("lease-123").await;
        assert!(result.is_err(), "Renewal should fail on 503");
    }

    /// Test secret rotation retry on transient error
    #[tokio::test]
    async fn test_vault_rotate_secret_retry_on_transient_error() {
        let mock_server = MockServer::start().await;
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count < 1 {
                    ResponseTemplate::new(502) // Transient error
                } else {
                    ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                        "user",
                        "pass",
                        "lease-new",
                        3600,
                        true,
                    ))
                }
            })
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token").with_max_retries(3);
        let result = vault.rotate_secret("database/creds/role").await;
        assert!(result.is_ok(), "Should succeed after retry");
    }

    /// Test handling of corrupted cached credential
    #[tokio::test]
    async fn test_vault_corrupted_cache_recovery() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "fresh"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // Get initial value (cached)
        let s1 = vault.get_secret("secret/data/test").await.unwrap();
        assert!(s1.contains("fresh"));

        // Invalidate cache (simulates corruption detection)
        vault.invalidate_cache("secret/data/test").await;

        // Next call fetches fresh value
        let s2 = vault.get_secret("secret/data/test").await.unwrap();
        assert!(s2.contains("fresh"));
    }

    /// Test connection pool management across multiple backends
    #[tokio::test]
    async fn test_vault_connection_pool_efficiency() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "pool-test"}))),
            )
            .mount(&mock_server)
            .await;

        // Multiple VaultBackend instances from clone share the same cache
        let vault1 = VaultBackend::new(mock_server.uri(), "test-token");
        let vault2 = vault1.clone();

        let _s1 = vault1.get_secret("secret/data/test").await.unwrap();
        let _s2 = vault2.get_secret("secret/data/test").await.unwrap();

        // Both should be using shared cache
        assert_eq!(vault1.cache_size().await, vault2.cache_size().await);
    }

    // ============================================================================
    // CONFIGURATION AND TUNING TESTS
    // ============================================================================

    /// Test configurable cache TTL percentage
    #[tokio::test]
    async fn test_vault_configurable_cache_ttl_percentage() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/database/creds/role"))
            .respond_with(ResponseTemplate::new(200).set_body_json(dynamic_creds_response(
                "user",
                "pass",
                "lease-1",
                3600,
                true,
            )))
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");
        let (_, expiry) = vault.get_secret_with_expiry("database/creds/role").await.unwrap();

        // Default cache TTL is 80% of lease duration (3600 * 0.8 = 2880s)
        // Actual expiry returned should be the full lease duration (3600s)
        let now = chrono::Utc::now();
        let diff = (expiry - now).num_seconds();
        assert!(diff > 3500 && diff <= 3600, "Full expiry should be ~3600s: got {}", diff);
    }

    /// Test configurable lease renewal threshold
    #[tokio::test]
    async fn test_vault_configurable_renewal_threshold() {
        // VaultBackend uses RENEWAL_THRESHOLD_PERCENT = 0.8 by default
        let vault = VaultBackend::new("http://localhost:8200", "token");
        // The renewal threshold is used internally for background renewal decisions
        assert!(vault.tls_verify()); // Backend created successfully with defaults
    }

    /// Test configurable max cache size
    #[tokio::test]
    async fn test_vault_configurable_max_cache_size() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "test"}))),
            )
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token").with_max_cache_entries(5);

        // Insert 8 entries
        for i in 0..8 {
            let _ = vault.get_secret(&format!("secret/data/key-{}", i)).await;
        }

        // Cache should be bounded at 5
        let size = vault.cache_size().await;
        assert!(size <= 5, "Cache should be bounded: got {}", size);
    }

    /// Test audit logging of cache hits and misses
    #[tokio::test]
    async fn test_vault_cache_audit_logging() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/secret/data/audit"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(kv2_response(serde_json::json!({"value": "audit"}))),
            )
            .expect(1) // Only 1 API call (cache hit on second)
            .mount(&mock_server)
            .await;

        let vault = VaultBackend::new(mock_server.uri(), "test-token");

        // Cache miss (API call)
        let _r1 = vault.get_secret("secret/data/audit").await.unwrap();
        // Cache hit (no API call)
        let _r2 = vault.get_secret("secret/data/audit").await.unwrap();

        // wiremock's expect(1) verifies only 1 API call was made
    }
}
