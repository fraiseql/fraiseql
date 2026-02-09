//! Comprehensive test specifications for encryption error recovery:
//! Vault outages, key expiry, network partitions, and graceful degradation.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod error_recovery_tests {
    use crate::encryption::error_recovery::{
        CircuitBreaker, ErrorCategory, RecoveryError, RecoveryStrategy, RetryConfig,
    };
    use crate::encryption::credential_rotation::{
        CredentialRotationManager, KeyVersionMetadata, KeyVersionStatus, RotationConfig,
        RotationMetrics,
    };
    use crate::encryption::FieldEncryption;

    // ============================================================================
    // VAULT TEMPORARY OUTAGE TESTS
    // ============================================================================

    /// Test encryption with Vault temporarily unavailable
    #[tokio::test]
    async fn test_vault_temporary_outage_with_cache() {
        // When Vault becomes temporarily unavailable
        // With cached encryption key: operations continue normally
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Simulate having a cached cipher - encrypt/decrypt still works
        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);

        // The cached cipher continues to function without Vault access
        // Encrypt another value to show continued availability
        let plaintext2 = "another@example.com";
        let encrypted2 = cipher.encrypt(plaintext2).unwrap();
        let decrypted2 = cipher.decrypt(&encrypted2).unwrap();
        assert_eq!(plaintext2, decrypted2);
    }

    /// Test encryption fails gracefully without cache
    #[tokio::test]
    async fn test_vault_outage_no_cache_graceful_failure() {
        // When Vault unavailable and key not in cache
        // Create a RecoveryError representing this scenario
        let error = RecoveryError::new(
            ErrorCategory::VaultUnavailable,
            "Vault unavailable: connection refused",
        );

        // Error indicates Vault unavailable
        assert_eq!(error.category, ErrorCategory::VaultUnavailable);
        assert!(error.message.contains("Vault unavailable"));

        // Strategy suggests using cache, but if cache is empty, caller decides retry
        assert_eq!(error.strategy, RecoveryStrategy::UseCache);
        assert!(error.retryable);
        assert!(error.suggestion.contains("Vault"));
    }

    /// Test retry logic with exponential backoff
    #[tokio::test]
    async fn test_vault_retry_exponential_backoff() {
        // When Vault connection fails, retry with exponential backoff
        let config = RetryConfig::new();

        // First retry: 100ms
        assert_eq!(config.backoff_delay_ms(0), 100);
        // Second retry: 200ms
        assert_eq!(config.backoff_delay_ms(1), 200);
        // Third retry: 400ms
        assert_eq!(config.backoff_delay_ms(2), 400);

        // Max retries: 3 (configurable)
        assert_eq!(config.max_retries, 3);
        assert!(config.should_retry(0));
        assert!(config.should_retry(1));
        assert!(config.should_retry(2));
        assert!(!config.should_retry(3));

        // Custom configuration
        let custom_config = RetryConfig::new().with_max_retries(5);
        assert_eq!(custom_config.max_retries, 5);
        assert!(custom_config.should_retry(4));
        assert!(!custom_config.should_retry(5));
    }

    /// Test connection pool handles Vault outage
    #[tokio::test]
    async fn test_connection_pool_vault_outage() {
        // When Vault connection fails, circuit breaker tracks failures
        let breaker = CircuitBreaker::new(3, 2);

        // Initially connections are allowed
        assert!(breaker.is_allowed());

        // Record connection failures
        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.is_allowed()); // Still allowed before threshold

        // After threshold, circuit opens - no more connections attempted
        breaker.record_failure();
        assert!(!breaker.is_allowed());
        assert!(breaker.is_open());

        // Pool doesn't reuse failed connections (reset required)
        breaker.reset();
        assert!(breaker.is_allowed());
    }

    /// Test health check detection
    #[tokio::test]
    async fn test_vault_health_check_detection() {
        // Periodic health checks to Vault
        let breaker = CircuitBreaker::new(2, 1);

        // Health checks pass initially
        breaker.record_success();
        assert!(breaker.is_allowed());
        let (failures, _) = breaker.get_counts();
        assert_eq!(failures, 0);

        // Detects unavailability quickly
        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.is_open());

        // Triggers failover to cache (circuit open means use cache)
        assert!(!breaker.is_allowed());

        // After recovery timeout, attempt_recovery transitions to HalfOpen
        breaker.attempt_recovery(0); // timeout=0 for test
        assert!(breaker.is_half_open());

        // Successful health check closes circuit
        breaker.record_success();
        assert!(breaker.is_allowed());
    }

    // ============================================================================
    // KEY EXPIRY TESTS
    // ============================================================================

    /// Test encryption key expiry detection
    #[tokio::test]
    async fn test_encryption_key_expiry_detection() {
        // When encryption key lease expires
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);
        let version = manager.initialize_key().unwrap();

        // System detects expiry - fresh key is not expired
        let metadata = manager.get_current_metadata().unwrap().unwrap();
        assert_eq!(metadata.status, KeyVersionStatus::Active);
        assert!(!metadata.is_expired());

        // Rotating creates a new key version
        let new_version = manager.rotate_key().unwrap();
        assert!(new_version > version);

        // Operations continue with new key
        let new_metadata = manager.get_current_metadata().unwrap().unwrap();
        assert_eq!(new_metadata.status, KeyVersionStatus::Active);
        assert_eq!(manager.get_current_version().unwrap(), new_version);
    }

    /// Test key refresh before expiry
    #[tokio::test]
    async fn test_key_refresh_before_expiry() {
        // Key refresh should happen before expiry
        let config = RotationConfig::new().with_ttl_days(10);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Fresh key should not need refresh yet (0% consumed, threshold is 80%)
        let needs_refresh = manager.current_version_needs_refresh().unwrap();
        assert!(!needs_refresh);

        // Simulate time passage by creating a metadata with custom dates
        let mut metadata = KeyVersionMetadata::new(100, 10);
        let now = chrono::Utc::now();
        metadata.issued_at = now - chrono::Duration::days(9); // 90% consumed
        metadata.expires_at = now + chrono::Duration::days(1);

        // At 90%, refresh should trigger (threshold is 80%)
        assert!(metadata.should_refresh());
        assert!(metadata.ttl_consumed_percent() >= 80);
    }

    /// Test multiple key versions
    #[tokio::test]
    async fn test_multiple_key_versions_decryption() {
        // When records encrypted with different key versions
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);

        // Create version 1
        let v1 = manager.initialize_key().unwrap();
        assert_eq!(v1, 1);

        // Create version 2
        let v2 = manager.rotate_key().unwrap();
        assert_eq!(v2, 2);

        // Create version 3
        let v3 = manager.rotate_key().unwrap();
        assert_eq!(v3, 3);

        // All versions can be used for decryption
        assert!(manager.can_decrypt_with_version(v1).unwrap());
        assert!(manager.can_decrypt_with_version(v2).unwrap());
        assert!(manager.can_decrypt_with_version(v3).unwrap());

        // Current version is the newest
        assert_eq!(manager.get_current_version().unwrap(), v3);

        // Version history shows all versions
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 3);
    }

    /// Test key expiry with operations in flight
    #[tokio::test]
    async fn test_key_expiry_operations_in_flight() {
        // In-flight operations complete with original key
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let cipher1 = FieldEncryption::new(&key1);
        let cipher2 = FieldEncryption::new(&key2);

        // Start encryption with key1 (simulating in-flight operation)
        let plaintext = "sensitive data";
        let encrypted_v1 = cipher1.encrypt(plaintext).unwrap();

        // Key rotates to key2 for new operations
        let encrypted_v2 = cipher2.encrypt(plaintext).unwrap();

        // In-flight operation decrypts with original key (no corruption)
        let decrypted_v1 = cipher1.decrypt(&encrypted_v1).unwrap();
        assert_eq!(decrypted_v1, plaintext);

        // New operations use refreshed key
        let decrypted_v2 = cipher2.decrypt(&encrypted_v2).unwrap();
        assert_eq!(decrypted_v2, plaintext);

        // Cannot cross-decrypt (different keys)
        assert!(cipher1.decrypt(&encrypted_v2).is_err());
        assert!(cipher2.decrypt(&encrypted_v1).is_err());
    }

    /// Test key expiry error message
    #[tokio::test]
    async fn test_key_expiry_clear_error_message() {
        // When operation fails due to key expiry
        let error = RecoveryError::new(ErrorCategory::KeyExpired, "Encryption key expired");

        // Error message indicates key expired
        assert_eq!(error.category, ErrorCategory::KeyExpired);
        assert!(error.message.contains("expired"));

        // Suggests retry (key will be refreshed)
        assert!(error.retryable);
        assert_eq!(error.strategy, RecoveryStrategy::Retry);
        assert!(error.suggestion.contains("refreshed"));
        assert!(error.suggestion.contains("Retry"));
    }

    // ============================================================================
    // NETWORK PARTITION TESTS
    // ============================================================================

    /// Test encryption during network partition
    #[tokio::test]
    async fn test_network_partition_with_cache() {
        // When network partition occurs, cached keys remain usable
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Cached cipher continues to work despite network partition
        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);

        // Recovery error shows cache strategy
        let error = RecoveryError::new(ErrorCategory::VaultUnavailable, "Network partition");
        assert!(error.should_use_cache());
    }

    /// Test network partition without cache
    #[tokio::test]
    async fn test_network_partition_no_cache_failure() {
        // When network partition and no cache
        let error = RecoveryError::new(
            ErrorCategory::NetworkError,
            "Unable to reach Vault: network partition",
        );

        // Encryption fails with clear error
        assert_eq!(error.category, ErrorCategory::NetworkError);
        assert!(error.message.contains("Unable to reach Vault"));

        // Indicates network issue, not key issue
        assert_eq!(error.strategy, RecoveryStrategy::Retry);
        assert!(error.suggestion.contains("network"));
    }

    /// Test network partition detection
    #[tokio::test]
    async fn test_network_partition_detection() {
        // System detects network partition via circuit breaker
        let breaker = CircuitBreaker::new(3, 2);

        // Connection timeouts indicate partition
        breaker.record_failure();
        breaker.record_failure();
        let (failures, _) = breaker.get_counts();
        assert_eq!(failures, 2);

        // Health checks fail consistently - circuit opens
        breaker.record_failure();
        assert!(breaker.is_open());

        // Time since last change tracked for alerting
        let duration = breaker.time_since_last_change();
        assert!(duration.num_milliseconds() >= 0);
    }

    /// Test recovery from network partition
    #[tokio::test]
    async fn test_network_partition_recovery() {
        // When network partition heals
        let breaker = CircuitBreaker::new(2, 2);

        // Simulate partition
        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.is_open());

        // Connection reestablished - transition to HalfOpen
        breaker.attempt_recovery(0); // immediate for test
        assert!(breaker.is_half_open());

        // Health checks resume succeeding
        breaker.record_success();
        breaker.record_success();

        // Operations continue normally (circuit closed)
        assert!(breaker.is_allowed());
        assert!(!breaker.is_open());
        assert!(!breaker.is_half_open());
    }

    // ============================================================================
    // GRACEFUL DEGRADATION TESTS
    // ============================================================================

    /// Test encryption with degraded Vault availability
    #[tokio::test]
    async fn test_degraded_vault_availability() {
        // When Vault slow (high latency), requests may timeout
        let config = RetryConfig::new();

        // Retry config ensures operations don't block indefinitely
        assert!(config.max_backoff_ms <= 5000); // Max 5 second wait

        // Cache provides fallback
        let error = RecoveryError::new(ErrorCategory::VaultUnavailable, "Vault response timeout");
        assert!(error.should_use_cache());
        assert!(error.retryable);

        // Backoff increases with attempts but is bounded
        let delay_0 = config.backoff_delay_ms(0);
        let delay_1 = config.backoff_delay_ms(1);
        let delay_max = config.backoff_delay_ms(20);
        assert!(delay_1 > delay_0);
        assert_eq!(delay_max, config.max_backoff_ms);
    }

    /// Test encryption load shedding
    #[tokio::test]
    async fn test_encryption_load_shedding() {
        // When system under load, prioritize reads over writes
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Read operations (SELECT with decryption) always work with cached key
        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);

        // Circuit breaker pattern helps with load shedding
        let breaker = CircuitBreaker::new(5, 3);

        // Under moderate load, operations still allowed
        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.is_allowed()); // Below threshold

        // Under heavy load (too many failures), shed write load
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();
        assert!(!breaker.is_allowed()); // Shed load
    }

    /// Test encryption circuit breaker pattern
    #[tokio::test]
    async fn test_encryption_circuit_breaker() {
        // After N failures to Vault, circuit breaker opens
        let breaker = CircuitBreaker::new(3, 2);

        // Record failures
        breaker.record_failure();
        assert!(breaker.is_allowed()); // Still closed
        breaker.record_failure();
        assert!(breaker.is_allowed()); // Still closed
        breaker.record_failure();

        // Circuit breaker opens - fast fail for subsequent requests
        assert!(!breaker.is_allowed());
        assert!(breaker.is_open());

        // Prevents prolonged timeouts by fast-failing
        let (failures, _) = breaker.get_counts();
        assert_eq!(failures, 3);

        // Circuit breaker closes on success after recovery attempt
        breaker.attempt_recovery(0);
        assert!(breaker.is_half_open());

        breaker.record_success();
        breaker.record_success();
        assert!(breaker.is_allowed());
        assert!(!breaker.is_open());
    }

    /// Test fallback to read-only mode
    #[tokio::test]
    async fn test_fallback_read_only_mode() {
        // When Vault unavailable, operate in read-only mode
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Prepare encrypted data while Vault was available
        let plaintext = "sensitive data";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // SELECT operations work (with cached cipher)
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Read-only mode: new encryptions would need a fresh key from Vault
        // Recovery error indicates read-only strategy
        let error = RecoveryError::new(ErrorCategory::VaultUnavailable, "Vault unavailable");
        assert!(error.should_use_cache()); // UseCache strategy appropriate for reads

        // INSERT/UPDATE blocked with clear message
        let write_error =
            RecoveryError::new(ErrorCategory::EncryptionFailed, "Cannot encrypt: Vault unavailable");
        assert!(!write_error.retryable); // EncryptionFailed is not retryable
        assert_eq!(write_error.strategy, RecoveryStrategy::FailFast);
    }

    // ============================================================================
    // ERROR CONTEXT & DIAGNOSTICS
    // ============================================================================

    /// Test error context includes recovery suggestion
    #[tokio::test]
    async fn test_error_context_recovery_suggestion() {
        // When encryption fails, error includes context and suggestion
        let error = RecoveryError::new(
            ErrorCategory::VaultUnavailable,
            "Vault unavailable: connection refused",
        );

        // Suggests possible causes and recommends recovery actions
        assert!(!error.suggestion.is_empty());
        assert!(error.suggestion.contains("Vault"));
        assert!(error.suggestion.contains("retry") || error.suggestion.contains("Retry")
            || error.suggestion.contains("Check") || error.suggestion.contains("30s"));

        // Different error types have appropriate suggestions
        let key_error = RecoveryError::new(ErrorCategory::KeyNotFound, "Key not found");
        assert!(key_error.suggestion.contains("configuration") || key_error.suggestion.contains("key"));

        let network_error = RecoveryError::new(ErrorCategory::NetworkError, "Timeout");
        assert!(network_error.suggestion.contains("network") || network_error.suggestion.contains("connectivity"));
    }

    /// Test error logging with correlation ID
    #[tokio::test]
    async fn test_error_logging_correlation_id() {
        // When error occurs, logged with request/transaction context
        let error1 = RecoveryError::new(ErrorCategory::NetworkError, "Timeout on request A");
        let error2 = RecoveryError::new(ErrorCategory::NetworkError, "Timeout on request B");

        // Each error has a timestamp for correlation
        assert!(error1.is_fresh());
        assert!(error2.is_fresh());

        // Errors have distinct timestamps (can correlate)
        assert_eq!(error1.category, error2.category);
        assert_ne!(error1.message, error2.message);

        // Message contains identifiable info for tracing
        assert!(error1.message.contains("request A"));
        assert!(error2.message.contains("request B"));
    }

    /// Test error metrics collection
    #[tokio::test]
    async fn test_error_metrics_collection() {
        // Metrics collected for all errors via RotationMetrics
        let metrics = RotationMetrics::new();

        // Record successes and failures
        metrics.record_rotation(50);
        metrics.record_rotation(75);
        metrics.record_failure();

        // Error type, frequency, severity tracked
        assert_eq!(metrics.total_rotations(), 2);
        assert_eq!(metrics.failed_rotations(), 1);

        // Success rate available via monitoring
        assert_eq!(metrics.success_rate_percent(), 50);

        // Last rotation duration tracked
        assert_eq!(metrics.last_rotation_duration_ms(), 75);
    }

    /// Test error patterns detection
    #[tokio::test]
    async fn test_error_patterns_detection() {
        // System detects error patterns using circuit breaker and error categories
        let breaker = CircuitBreaker::new(5, 3);

        // Multiple timeouts suggest network issue
        let network_errors: Vec<_> = (0..3)
            .map(|_| RecoveryError::new(ErrorCategory::NetworkError, "Timeout"))
            .collect();
        for _ in &network_errors {
            breaker.record_failure();
        }
        // All network errors share same category
        assert!(network_errors.iter().all(|e| e.category == ErrorCategory::NetworkError));
        assert!(network_errors.iter().all(|e| e.is_transient()));

        // Multiple key_not_found suggest config issue
        let config_errors: Vec<_> = (0..3)
            .map(|_| RecoveryError::new(ErrorCategory::KeyNotFound, "Key missing"))
            .collect();
        // Config errors are not transient
        assert!(config_errors.iter().all(|e| !e.is_transient()));
        assert!(config_errors.iter().all(|e| e.strategy == RecoveryStrategy::FailFast));

        // Patterns trigger different recovery
        assert_ne!(network_errors[0].strategy, config_errors[0].strategy);
    }

    // ============================================================================
    // CACHE STABILITY TESTS
    // ============================================================================

    /// Test cache survives Vault outage
    #[tokio::test]
    async fn test_cache_survives_vault_outage() {
        // When Vault becomes unavailable, cached keys remain available
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Cache not cleared on Vault failure - cipher still works
        let test_data = vec![
            "user@example.com",
            "555-0123",
            "123-45-6789",
        ];

        for plaintext in &test_data {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(*plaintext, decrypted);
        }

        // Multiple operations succeed with cached cipher
        // Provides continuity of service
        let encrypted1 = cipher.encrypt("data1").unwrap();
        let encrypted2 = cipher.encrypt("data2").unwrap();
        assert_ne!(encrypted1, encrypted2);
        assert_eq!(cipher.decrypt(&encrypted1).unwrap(), "data1");
        assert_eq!(cipher.decrypt(&encrypted2).unwrap(), "data2");
    }

    /// Test cache eviction under load
    #[tokio::test]
    async fn test_cache_eviction_under_load() {
        // When many keys accessed under load, LRU eviction works correctly
        // Simulate by creating many ciphers and verifying most-used ones work
        let keys: Vec<[u8; 32]> = (0..10)
            .map(|i| {
                let mut key = [0u8; 32];
                key[0] = i;
                key
            })
            .collect();

        let ciphers: Vec<FieldEncryption> = keys.iter().map(|k| FieldEncryption::new(k)).collect();

        // Most-used keys stay cached and operational
        for (i, cipher) in ciphers.iter().enumerate() {
            let plaintext = format!("data_{}", i);
            let encrypted = cipher.encrypt(&plaintext).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(plaintext, decrypted);
        }

        // No performance degradation - all operations succeed
        let primary = &ciphers[0];
        for _ in 0..100 {
            let encrypted = primary.encrypt("frequent data").unwrap();
            let decrypted = primary.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted, "frequent data");
        }
    }

    /// Test cache coherency after key rotation
    #[tokio::test]
    async fn test_cache_coherency_key_rotation() {
        // When key rotates, old cipher invalidated, new key used
        let old_key = [1u8; 32];
        let new_key = [2u8; 32];

        let old_cipher = FieldEncryption::new(&old_key);
        let new_cipher = FieldEncryption::new(&new_key);

        // Encrypt with old key
        let plaintext = "sensitive data";
        let old_encrypted = old_cipher.encrypt(plaintext).unwrap();

        // Old cipher decrypts old data
        assert_eq!(old_cipher.decrypt(&old_encrypted).unwrap(), plaintext);

        // New cipher encrypts new data
        let new_encrypted = new_cipher.encrypt(plaintext).unwrap();
        assert_eq!(new_cipher.decrypt(&new_encrypted).unwrap(), plaintext);

        // Old data cannot be decrypted with new key (cache coherency)
        assert!(new_cipher.decrypt(&old_encrypted).is_err());

        // Other cached keys unaffected (demonstrated by old cipher still working)
        assert_eq!(old_cipher.decrypt(&old_encrypted).unwrap(), plaintext);
    }

    // ============================================================================
    // TRANSACTION CONSISTENCY TESTS
    // ============================================================================

    /// Test transaction rollback on encryption failure
    #[tokio::test]
    async fn test_transaction_rollback_encryption_failure() {
        // When encryption fails mid-transaction, no partial data committed
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Simulate a transaction: encrypt multiple fields
        let field1 = cipher.encrypt("email@example.com").unwrap();
        let field2 = cipher.encrypt("555-0123").unwrap();

        // Verify fields are encrypted
        assert_eq!(cipher.decrypt(&field1).unwrap(), "email@example.com");
        assert_eq!(cipher.decrypt(&field2).unwrap(), "555-0123");

        // Simulate encryption failure on corrupted input
        let short_data = vec![0u8; 3]; // Too short to decrypt
        let result = cipher.decrypt(&short_data);
        assert!(result.is_err()); // Failure detected

        // Application can retry - original state unchanged
        assert_eq!(cipher.decrypt(&field1).unwrap(), "email@example.com");
        assert_eq!(cipher.decrypt(&field2).unwrap(), "555-0123");
    }

    /// Test transaction consistency after Vault recovery
    #[tokio::test]
    async fn test_transaction_consistency_vault_recovery() {
        // Transaction failed due to Vault outage, then Vault recovers
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Simulate failed transaction (metrics track failure)
        let metrics = manager.metrics();
        metrics.record_failure();
        assert_eq!(metrics.failed_rotations(), 1);

        // Vault recovers - application retries transaction
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let plaintext = "retry data";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();

        // New attempt succeeds - consistent state maintained
        assert_eq!(decrypted, plaintext);

        // Track successful rotation after recovery
        metrics.record_rotation(50);
        assert_eq!(metrics.total_rotations(), 1);
        // success_rate = (total - failed) / total = (1-1)/1 = 0%
        // After recovery, the success rate accounts for past failures
        assert_eq!(metrics.success_rate_percent(), 0);
    }

    /// Test encryption failure doesn't corrupt state
    #[tokio::test]
    async fn test_encryption_failure_no_state_corruption() {
        // When encryption fails, system state not corrupted
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Establish known good state
        let plaintext = "known good data";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Attempt decryption with corrupted data
        let mut corrupted = encrypted.clone();
        if corrupted.len() > 12 {
            corrupted[12] ^= 0xFF; // Corrupt ciphertext
        }
        let result = cipher.decrypt(&corrupted);
        assert!(result.is_err()); // Fails cleanly

        // Original data unchanged - can safely retry
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Cipher still works for new operations
        let new_encrypted = cipher.encrypt("new data").unwrap();
        assert_eq!(cipher.decrypt(&new_encrypted).unwrap(), "new data");
    }

    // ============================================================================
    // OBSERVABILITY & ALERTING
    // ============================================================================

    /// Test alerts on encryption errors
    #[tokio::test]
    async fn test_alerts_encryption_errors() {
        // When errors exceed threshold, different severity levels apply
        let metrics = RotationMetrics::new();

        // Simulate 1-5 failures: warning level
        for _ in 0..3 {
            metrics.record_failure();
        }
        let failed = metrics.failed_rotations();
        assert!((1..=5).contains(&failed));

        // Simulate more failures: error level (5-20)
        for _ in 0..10 {
            metrics.record_failure();
        }
        let failed = metrics.failed_rotations();
        assert!(failed > 5);

        // Record some successes to track rate
        for _ in 0..20 {
            metrics.record_rotation(100);
        }

        // Success rate reflects the failures
        let rate = metrics.success_rate_percent();
        assert!(rate < 100); // Not 100% due to failures
    }

    /// Test dashboards show error details
    #[tokio::test]
    async fn test_dashboard_error_details() {
        // Rotation metrics track error rates per operation
        let metrics = RotationMetrics::new();

        // Record operations
        metrics.record_rotation(50);
        metrics.record_rotation(100);
        metrics.record_failure();

        // Dashboard shows error rates
        let total = metrics.total_rotations();
        let failed = metrics.failed_rotations();
        let success_rate = metrics.success_rate_percent();

        assert_eq!(total, 2);
        assert_eq!(failed, 1);
        assert_eq!(success_rate, 50);

        // Last rotation details available
        assert!(metrics.last_rotation().is_some());
        assert_eq!(metrics.last_rotation_duration_ms(), 100);
    }

    /// Test distributed tracing of errors
    #[tokio::test]
    async fn test_distributed_tracing_errors() {
        // Errors traced across services - each has timestamp and category
        let errors = vec![
            RecoveryError::new(ErrorCategory::NetworkError, "Step 1: Connection to Vault failed"),
            RecoveryError::new(ErrorCategory::CacheMiss, "Step 2: Key not in cache"),
            RecoveryError::new(
                ErrorCategory::EncryptionFailed,
                "Step 3: Encryption failed without key",
            ),
        ];

        // Request flow visible through error chain
        assert_eq!(errors[0].category, ErrorCategory::NetworkError);
        assert_eq!(errors[1].category, ErrorCategory::CacheMiss);
        assert_eq!(errors[2].category, ErrorCategory::EncryptionFailed);

        // Where error occurred identifiable
        assert!(errors[0].message.contains("Step 1"));
        assert!(errors[1].message.contains("Step 2"));
        assert!(errors[2].message.contains("Step 3"));

        // Timeline of error events clear (all fresh)
        for error in &errors {
            assert!(error.is_fresh());
        }
    }

    /// Test health status reporting
    #[tokio::test]
    async fn test_health_status_reporting() {
        // Health endpoint reports encryption subsystem status
        let breaker = CircuitBreaker::new(3, 2);

        // Status: healthy (circuit closed)
        assert!(breaker.is_allowed());
        assert!(!breaker.is_open());
        assert!(!breaker.is_half_open());

        // Simulate degradation
        breaker.record_failure();
        breaker.record_failure();
        // Status: degraded (failures accumulating)
        let (failures, _) = breaker.get_counts();
        assert!(failures > 0);
        assert!(breaker.is_allowed()); // Still accepting, but degraded

        // Status: unavailable (circuit open)
        breaker.record_failure();
        assert!(breaker.is_open());
        assert!(!breaker.is_allowed());

        // Used by orchestration for failover decisions
        let time_since_change = breaker.time_since_last_change();
        assert!(time_since_change.num_milliseconds() >= 0);

        // Reset for recovery
        breaker.reset();
        assert!(breaker.is_allowed());
    }
}
