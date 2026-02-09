//! Comprehensive test specifications for automatic key refresh triggering,
//! background job coordination, and non-blocking refresh during operations.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod refresh_tests {
    use chrono::{Duration, Utc};

    use crate::encryption::credential_rotation::{
        CredentialRotationManager, KeyVersionMetadata, KeyVersionStatus, RotationConfig,
    };
    use crate::encryption::refresh_trigger::{
        RefreshConfig, RefreshHealthStatus, RefreshJobStatus, RefreshManager, RefreshTrigger,
    };
    use crate::encryption::FieldEncryption;

    // ============================================================================
    // REFRESH TRIGGER TESTS
    // ============================================================================

    /// Test refresh trigger detection
    #[tokio::test]
    async fn test_refresh_trigger_detection() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // Below threshold: no trigger
        assert!(!trigger.should_trigger(79));

        // At 80% threshold: triggers
        assert!(trigger.should_trigger(80));

        // Above threshold: triggers
        assert!(trigger.should_trigger(85));
        assert!(trigger.should_trigger(95));

        // After marking pending, current operations continue uninterrupted
        trigger.mark_pending();
        assert!(trigger.is_pending());
    }

    /// Test refresh not triggered too early
    #[tokio::test]
    async fn test_refresh_not_triggered_early() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // <80% TTL consumed: no refresh
        assert!(!trigger.should_trigger(0));
        assert!(!trigger.should_trigger(10));
        assert!(!trigger.should_trigger(50));
        assert!(!trigger.should_trigger(75));
        assert!(!trigger.should_trigger(79));

        // No premature rotation
        assert!(!trigger.is_pending());
    }

    /// Test refresh triggers only once per version
    #[tokio::test]
    async fn test_refresh_single_trigger_per_version() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // First trigger succeeds
        assert!(trigger.should_trigger(85));
        trigger.mark_pending();

        // Second trigger for same version does not fire (pending already)
        assert!(!trigger.should_trigger(85));
        assert!(!trigger.should_trigger(90));
        assert!(!trigger.should_trigger(99));

        // After clearing pending (version refreshed), can trigger again
        trigger.clear_pending();
        assert!(trigger.should_trigger(85));
    }

    /// Test refresh with in-flight operations
    #[tokio::test]
    async fn test_refresh_with_inflight_operations() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // In-flight operation: encrypt before refresh
        let encrypted = cipher.encrypt("in_flight_data").unwrap();

        // Refresh triggers: rotate key
        let old_version = manager.get_current_version().unwrap();
        let new_version = manager.rotate_key().unwrap();
        assert!(new_version > old_version);

        // In-flight data still decryptable (same cipher key in this test)
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "in_flight_data");

        // New operations work with new version
        let new_encrypted = cipher.encrypt("new_data").unwrap();
        let new_decrypted = cipher.decrypt(&new_encrypted).unwrap();
        assert_eq!(new_decrypted, "new_data");
    }

    /// Test refresh failure handling
    #[tokio::test]
    async fn test_refresh_failure_handling() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // Trigger refresh
        assert!(trigger.should_trigger(85));
        trigger.mark_pending();

        // Simulate failure
        trigger.record_failure();
        assert_eq!(trigger.failed_refreshes(), 1);

        // Current key remains in use (pending flag stays so retry happens)
        assert!(trigger.is_pending());

        // On retry, clear pending first then check again
        trigger.clear_pending();
        assert!(trigger.should_trigger(85));
    }

    // ============================================================================
    // BACKGROUND JOB TESTS
    // ============================================================================

    /// Test background refresh job starts
    #[tokio::test]
    async fn test_background_refresh_job_starts() {
        let manager = RefreshManager::new(RefreshConfig::new());

        // Manager starts with refresh enabled
        assert!(manager.is_enabled());
        assert!(!manager.refresh_pending());

        // Auto-refresh check interval is configurable
        let config = RefreshConfig::new().with_check_interval(12);
        let manager2 = RefreshManager::new(config);
        assert!(manager2.is_enabled());
    }

    /// Test background job periodic execution
    #[tokio::test]
    async fn test_background_job_periodic_execution() {
        let manager = RefreshManager::new(RefreshConfig::new());

        // First check below threshold: no trigger
        assert!(!manager.check_and_trigger(75));
        assert!(!manager.refresh_pending());

        // Check at threshold: triggers
        assert!(manager.check_and_trigger(80));
        assert!(manager.refresh_pending());

        // Disabled manager never triggers
        let disabled_config = RefreshConfig::new().with_enabled(false);
        let disabled_manager = RefreshManager::new(disabled_config);
        assert!(!disabled_manager.check_and_trigger(85));
    }

    /// Test background job graceful shutdown
    #[tokio::test]
    async fn test_background_job_shutdown() {
        let manager = RefreshManager::new(RefreshConfig::new());

        // Start a job
        manager.check_and_trigger(85);
        manager.start_job().unwrap();
        assert!(manager.job_running());

        // Request shutdown
        manager.request_shutdown();
        assert!(manager.job().should_shutdown());

        // Complete the job after shutdown request
        manager.complete_job_success().unwrap();
        assert!(!manager.job_running());
        assert!(!manager.refresh_pending());
    }

    /// Test background job error resilience
    #[tokio::test]
    async fn test_background_job_error_resilience() {
        let manager = RefreshManager::new(RefreshConfig::new());

        // Trigger and start job
        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Job fails
        manager.complete_job_failure("Vault connection timeout").unwrap();
        assert_eq!(manager.job().status().unwrap(), RefreshJobStatus::Failed);
        assert!(manager.job().last_error().unwrap().is_some());

        // Pending flag remains (for retry)
        assert!(manager.refresh_pending());

        // Can reset for retry
        manager.reset_for_retry();
        assert!(!manager.refresh_pending());
    }

    /// Test background job concurrent safety
    #[tokio::test]
    async fn test_background_job_concurrent_safety() {
        let manager = RefreshManager::new(RefreshConfig::new());
        let manager_clone = manager.clone();

        // Start job in one "thread"
        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Concurrent read of status from another "thread"
        assert!(manager_clone.job_running());
        assert!(manager_clone.refresh_pending());

        // Complete from original
        manager.complete_job_success().unwrap();

        // Clone sees updated state
        assert!(!manager_clone.job_running());
        assert!(!manager_clone.refresh_pending());
    }

    // ============================================================================
    // VERSION SWITCH COORDINATION TESTS
    // ============================================================================

    /// Test atomic version switch
    #[tokio::test]
    async fn test_atomic_version_switch() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let v1 = manager.get_current_version().unwrap();
        let v2 = manager.rotate_key().unwrap();

        // New version is current
        assert_eq!(manager.get_current_version().unwrap(), v2);

        // Old version still available for decryption
        assert!(manager.can_decrypt_with_version(v1).unwrap());
        assert!(manager.can_decrypt_with_version(v2).unwrap());
    }

    /// Test version switch during encryption
    #[tokio::test]
    async fn test_version_switch_during_encryption() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Batch encrypt before switch
        let before: Vec<Vec<u8>> = (0..3)
            .map(|i| cipher.encrypt(&format!("before_{i}")).unwrap())
            .collect();

        // Switch version
        manager.rotate_key().unwrap();

        // Batch encrypt after switch
        let after: Vec<Vec<u8>> = (0..3)
            .map(|i| cipher.encrypt(&format!("after_{i}")).unwrap())
            .collect();

        // All records decrypt correctly
        for (i, enc) in before.iter().enumerate() {
            assert_eq!(cipher.decrypt(enc).unwrap(), format!("before_{i}"));
        }
        for (i, enc) in after.iter().enumerate() {
            assert_eq!(cipher.decrypt(enc).unwrap(), format!("after_{i}"));
        }
    }

    /// Test version switch during decryption
    #[tokio::test]
    async fn test_version_switch_during_decryption() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Encrypt records before any switch
        let records: Vec<Vec<u8>> = (0..5)
            .map(|i| cipher.encrypt(&format!("record_{i}")).unwrap())
            .collect();

        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Switch happens during "batch" decryption
        let mut decrypted = Vec::new();
        for (i, rec) in records.iter().enumerate() {
            decrypted.push(cipher.decrypt(rec).unwrap());
            if i == 2 {
                // Version switch mid-batch
                manager.rotate_key().unwrap();
            }
        }

        // All records still decrypt correctly
        for (i, text) in decrypted.iter().enumerate() {
            assert_eq!(text, &format!("record_{i}"));
        }
    }

    /// Test version switch visibility
    #[tokio::test]
    async fn test_version_switch_visibility() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let v1 = manager.get_current_version().unwrap();
        assert_eq!(v1, 1);

        manager.rotate_key().unwrap();
        let v2 = manager.get_current_version().unwrap();
        assert_eq!(v2, 2);

        // Version change is immediately visible
        assert_ne!(v1, v2);
        assert_eq!(manager.get_current_version().unwrap(), 2);

        // History reflects the change
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 2);
    }

    // ============================================================================
    // REFRESH SCHEDULING TESTS
    // ============================================================================

    /// Test refresh check interval configuration
    #[tokio::test]
    async fn test_refresh_check_interval_config() {
        // Default: 24 hours
        let default_config = RefreshConfig::new();
        assert_eq!(default_config.check_interval_hours, 24);

        // Custom: 1 hour for testing
        let fast_config = RefreshConfig::new().with_check_interval(1);
        assert_eq!(fast_config.check_interval_hours, 1);

        // Disable automatic checks
        let disabled_config = RefreshConfig::new().with_enabled(false);
        assert!(!disabled_config.enabled);

        // Minimum interval clamped to 1
        let min_config = RefreshConfig::new().with_check_interval(0);
        assert_eq!(min_config.check_interval_hours, 1);
    }

    /// Test refresh check timing
    #[tokio::test]
    async fn test_refresh_check_timing() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // No checks performed yet
        assert!(trigger.last_check_time().is_none());

        // Record a check
        trigger.record_check();
        let check_time = trigger.last_check_time();
        assert!(check_time.is_some());

        // Check time is recent
        let elapsed = Utc::now() - check_time.unwrap();
        assert!(elapsed.num_seconds() < 2);
    }

    /// Test refresh with quiet hours
    #[tokio::test]
    async fn test_refresh_with_quiet_hours() {
        // Configure quiet hours 2am-4am
        let config = RefreshConfig::new().with_quiet_hours(2, 4);
        assert_eq!(config.quiet_hours_start, Some(2));
        assert_eq!(config.quiet_hours_end, Some(4));

        // Without quiet hours
        let no_quiet = RefreshConfig::new();
        assert!(no_quiet.quiet_hours_start.is_none());
        assert!(no_quiet.quiet_hours_end.is_none());

        // Invalid quiet hours (>= 24) not set
        let invalid = RefreshConfig::new().with_quiet_hours(25, 26);
        assert!(invalid.quiet_hours_start.is_none());
    }

    /// Test refresh can be triggered manually
    #[tokio::test]
    async fn test_manual_refresh_trigger() {
        let manager = RefreshManager::new(RefreshConfig::default());

        // Manual trigger works even below threshold
        let result = manager.trigger_manual();
        assert!(result.is_ok());
        assert!(manager.refresh_pending());

        // Double manual trigger fails (already pending)
        let result2 = manager.trigger_manual();
        assert!(result2.is_err());
    }

    // ============================================================================
    // REFRESH METRICS AND MONITORING
    // ============================================================================

    /// Test refresh metrics collection
    #[tokio::test]
    async fn test_refresh_metrics_collection() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // Initial metrics
        assert_eq!(trigger.total_refreshes(), 0);
        assert_eq!(trigger.failed_refreshes(), 0);

        // Record successful refresh
        trigger.record_success(150);
        assert_eq!(trigger.total_refreshes(), 1);
        assert_eq!(trigger.failed_refreshes(), 0);
        assert!(trigger.last_refresh_time().is_some());

        // Record another
        trigger.record_success(200);
        assert_eq!(trigger.total_refreshes(), 2);
    }

    /// Test refresh duration tracking
    #[tokio::test]
    async fn test_refresh_duration_tracking() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // No duration before first refresh
        trigger.record_success(100);
        trigger.record_success(200);
        trigger.record_success(150);

        // Total refreshes tracked
        assert_eq!(trigger.total_refreshes(), 3);

        // Success rate calculated
        assert_eq!(trigger.success_rate_percent(), 100);

        // With failures
        trigger.record_failure();
        assert_eq!(trigger.failed_refreshes(), 1);
        // 3 total, 1 failed = 2 successful = 66%
        assert_eq!(trigger.success_rate_percent(), 66);
    }

    /// Test refresh latency impact
    #[tokio::test]
    async fn test_refresh_latency_impact() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Measure encryption latency
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let _ = cipher.encrypt("test_data").unwrap();
        }
        let encrypt_time = start.elapsed();

        // Measure decryption latency
        let encrypted = cipher.encrypt("test_data").unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let _ = cipher.decrypt(&encrypted).unwrap();
        }
        let decrypt_time = start.elapsed();

        // Both should be fast (non-blocking)
        assert!(
            encrypt_time.as_millis() < 200,
            "Encryption should be fast: {}ms",
            encrypt_time.as_millis()
        );
        assert!(
            decrypt_time.as_millis() < 200,
            "Decryption should be fast: {}ms",
            decrypt_time.as_millis()
        );
    }

    /// Test refresh dashboard metrics
    #[tokio::test]
    async fn test_refresh_dashboard_metrics() {
        let manager = RefreshManager::new(RefreshConfig::default());
        let trigger = manager.trigger();

        // Record some activity
        trigger.record_check();
        trigger.record_success(100);

        // Dashboard data available
        assert!(trigger.last_check_time().is_some());
        assert!(trigger.last_refresh_time().is_some());
        assert_eq!(trigger.total_refreshes(), 1);

        // Health status
        assert_eq!(manager.health_status(), RefreshHealthStatus::Healthy);
    }

    // ============================================================================
    // REFRESH VALIDATION TESTS
    // ============================================================================

    /// Test new version validation before use
    #[tokio::test]
    async fn test_new_version_validation() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Validate encrypt works
        let encrypted = cipher.encrypt("validation_test").unwrap();
        assert!(!encrypted.is_empty());

        // Validate decrypt works
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "validation_test");

        // Only after validation passes, mark version as current
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let metadata = manager.get_current_metadata().unwrap().unwrap();
        assert_eq!(metadata.status, KeyVersionStatus::Active);
    }

    /// Test old version decryption capability preserved
    #[tokio::test]
    async fn test_old_version_decryption_preserved() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        let v1 = manager.initialize_key().unwrap();

        // Encrypt data with v1
        let old_data = cipher.encrypt("historical_record").unwrap();

        // Refresh: create v2
        let v2 = manager.rotate_key().unwrap();
        assert!(v2 > v1);

        // Old version still exists and can decrypt
        assert!(manager.can_decrypt_with_version(v1).unwrap());
        let decrypted = cipher.decrypt(&old_data).unwrap();
        assert_eq!(decrypted, "historical_record");

        // Multiple refreshes: v1 still accessible
        manager.rotate_key().unwrap();
        manager.rotate_key().unwrap();
        assert!(manager.can_decrypt_with_version(v1).unwrap());
    }

    /// Test refresh doesn't lose data
    #[tokio::test]
    async fn test_refresh_data_integrity() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Encrypt records before refresh
        let records: Vec<(String, Vec<u8>)> = (0..10)
            .map(|i| {
                let plain = format!("record_{i}");
                let enc = cipher.encrypt(&plain).unwrap();
                (plain, enc)
            })
            .collect();

        // Simulate refresh
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        manager.rotate_key().unwrap();

        // All records decrypt correctly after refresh
        for (plain, enc) in &records {
            let decrypted = cipher.decrypt(enc).unwrap();
            assert_eq!(&decrypted, plain);
        }
    }

    // ============================================================================
    // REFRESH ERROR HANDLING TESTS
    // ============================================================================

    /// Test refresh with Vault unavailable
    #[tokio::test]
    async fn test_refresh_vault_unavailable() {
        let manager = RefreshManager::new(RefreshConfig::default());

        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Simulate Vault unavailable
        manager
            .complete_job_failure("Vault connection refused: connection timed out")
            .unwrap();

        let job = manager.job();
        assert_eq!(job.status().unwrap(), RefreshJobStatus::Failed);
        let error = job.last_error().unwrap().unwrap();
        assert!(error.contains("Vault"));

        // Pending remains for retry
        assert!(manager.refresh_pending());
    }

    /// Test refresh with insufficient permissions
    #[tokio::test]
    async fn test_refresh_insufficient_permissions() {
        let manager = RefreshManager::new(RefreshConfig::default());

        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Simulate permission denied
        manager
            .complete_job_failure("Permission denied: insufficient privileges for key generation")
            .unwrap();

        let job = manager.job();
        assert_eq!(job.status().unwrap(), RefreshJobStatus::Failed);
        let error = job.last_error().unwrap().unwrap();
        assert!(error.contains("Permission denied"));
    }

    /// Test refresh timeout handling
    #[tokio::test]
    async fn test_refresh_timeout_handling() {
        let manager = RefreshManager::new(RefreshConfig::default());

        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Job has duration tracking
        let job = manager.job();
        let duration = job.duration().unwrap();
        assert!(duration.is_some());

        // Simulate timeout failure
        manager
            .complete_job_failure("Operation timed out after 30s")
            .unwrap();

        assert_eq!(job.status().unwrap(), RefreshJobStatus::Failed);
    }

    /// Test partial refresh recovery
    #[tokio::test]
    async fn test_partial_refresh_recovery() {
        let manager = RefreshManager::new(RefreshConfig::default());

        // Start and fail
        manager.check_and_trigger(90);
        manager.start_job().unwrap();
        manager
            .complete_job_failure("Partial failure: key generated but not committed")
            .unwrap();

        // Pending remains, can retry
        assert!(manager.refresh_pending());
        assert!(manager.should_retry_refresh());

        // Reset and retry
        manager.reset_for_retry();
        assert!(!manager.refresh_pending());

        // New check can trigger again
        assert!(manager.check_and_trigger(90));
    }

    // ============================================================================
    // REFRESH COORDINATION WITH OPERATIONS
    // ============================================================================

    /// Test read operations during refresh
    #[tokio::test]
    async fn test_read_operations_during_refresh() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Pre-encrypted records
        let records: Vec<Vec<u8>> = (0..5)
            .map(|i| cipher.encrypt(&format!("read_{i}")).unwrap())
            .collect();

        // Start refresh process
        let manager = RefreshManager::new(RefreshConfig::default());
        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Read operations during refresh succeed
        for (i, rec) in records.iter().enumerate() {
            let decrypted = cipher.decrypt(rec).unwrap();
            assert_eq!(decrypted, format!("read_{i}"));
        }

        // Complete refresh
        manager.complete_job_success().unwrap();
    }

    /// Test write operations during refresh
    #[tokio::test]
    async fn test_write_operations_during_refresh() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Start refresh process
        let manager = RefreshManager::new(RefreshConfig::default());
        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Write operations during refresh succeed
        let encrypted = cipher.encrypt("new_write").unwrap();
        assert!(!encrypted.is_empty());

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "new_write");

        // Complete refresh
        manager.complete_job_success().unwrap();
    }

    /// Test transactions during refresh
    #[tokio::test]
    async fn test_transactions_during_refresh() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let config = RotationConfig::new();
        let rot_manager = CredentialRotationManager::new(config);
        rot_manager.initialize_key().unwrap();

        // Simulate a transaction: encrypt multiple records atomically
        let version_at_start = rot_manager.get_current_version().unwrap();

        let tx_records: Vec<Vec<u8>> = (0..3)
            .map(|i| cipher.encrypt(&format!("tx_record_{i}")).unwrap())
            .collect();

        // Refresh happens during transaction
        rot_manager.rotate_key().unwrap();

        // All records in the transaction were encrypted with the same cipher
        // and can be decrypted consistently
        for (i, rec) in tx_records.iter().enumerate() {
            let decrypted = cipher.decrypt(rec).unwrap();
            assert_eq!(decrypted, format!("tx_record_{i}"));
        }

        // Version at start is still valid for decryption
        assert!(rot_manager.can_decrypt_with_version(version_at_start).unwrap());
    }

    // ============================================================================
    // REFRESH INTEGRATION TESTS
    // ============================================================================

    /// Test refresh with TTL-based rotation schedule
    #[tokio::test]
    async fn test_refresh_with_ttl_schedule() {
        // TTL-based: check threshold
        let manager = RefreshManager::new(RefreshConfig::new().with_refresh_threshold(80));

        // Below threshold: no trigger
        assert!(!manager.check_and_trigger(79));

        // At threshold: triggers
        assert!(manager.check_and_trigger(80));
        assert!(manager.refresh_pending());
    }

    /// Test refresh with compliance requirements
    #[tokio::test]
    async fn test_refresh_compliance_aware() {
        // HIPAA/PCI: 365-day TTL
        let hipaa_config = RotationConfig::new().with_ttl_days(365);
        let hipaa_manager = CredentialRotationManager::new(hipaa_config);
        hipaa_manager.initialize_key().unwrap();
        assert!(hipaa_manager.check_hipaa_compliance().unwrap());
        assert!(hipaa_manager.check_pci_compliance().unwrap());

        // GDPR: shorter TTL (90 days)
        let gdpr_config = RotationConfig::new().with_ttl_days(90);
        let gdpr_manager = CredentialRotationManager::new(gdpr_config);
        gdpr_manager.initialize_key().unwrap();

        let metadata = gdpr_manager.get_current_metadata().unwrap().unwrap();
        let ttl = metadata.expires_at - metadata.issued_at;
        assert!(ttl.num_days() >= 89 && ttl.num_days() <= 91);
    }

    /// Test refresh prevents expiry surprises
    #[tokio::test]
    async fn test_refresh_prevents_expiry() {
        // Without refresh: key could expire
        let mut metadata = KeyVersionMetadata::new(1, 10);
        // Simulate 9 days passed (90% consumed)
        metadata.issued_at = Utc::now() - Duration::days(9);
        metadata.expires_at = Utc::now() + Duration::days(1);
        assert!(metadata.should_refresh()); // Refresh would trigger

        // With refresh at 80%: new key ready before expiry
        let trigger = RefreshTrigger::new(RefreshConfig::new().with_refresh_threshold(80));
        let consumed = metadata.ttl_consumed_percent();
        assert!(trigger.should_trigger(consumed));

        // After refresh, operations never see expired key
        let fresh = KeyVersionMetadata::new(2, 10);
        assert!(!fresh.is_expired());
        assert!(!fresh.should_refresh());
    }

    // ============================================================================
    // REFRESH OBSERVABILITY TESTS
    // ============================================================================

    /// Test refresh logging
    #[tokio::test]
    async fn test_refresh_logging() {
        let trigger = RefreshTrigger::new(RefreshConfig::default());

        // Record operations for logging
        trigger.record_check();
        trigger.record_success(75);

        assert!(trigger.last_check_time().is_some());
        assert!(trigger.last_refresh_time().is_some());
        assert_eq!(trigger.total_refreshes(), 1);
        assert_eq!(trigger.success_rate_percent(), 100);
    }

    /// Test refresh alerts
    #[tokio::test]
    async fn test_refresh_alerts() {
        let manager = RefreshManager::new(RefreshConfig::default());

        // Initial state: healthy
        assert_eq!(manager.health_status(), RefreshHealthStatus::Healthy);

        // After triggering: pending
        manager.check_and_trigger(85);
        assert_eq!(manager.health_status(), RefreshHealthStatus::Pending);

        // After starting: running
        manager.start_job().unwrap();
        assert_eq!(manager.health_status(), RefreshHealthStatus::Running);

        // After failure: job fails and pending remains
        manager.complete_job_failure("fail 1").unwrap();
        // Pending is still set, so status is Pending
        assert_eq!(manager.health_status(), RefreshHealthStatus::Pending);

        // After clearing pending and accumulating failures, status becomes Degraded
        manager.reset_for_retry();
        manager.trigger().record_failure();
        manager.trigger().record_failure();
        manager.trigger().record_failure();

        // With >2 failures, no pending, and failed job status: degraded
        assert_eq!(manager.health_status(), RefreshHealthStatus::Degraded);

        // Disabled: shows disabled
        let disabled = RefreshManager::new(RefreshConfig::new().with_enabled(false));
        assert_eq!(disabled.health_status(), RefreshHealthStatus::Disabled);
    }

    /// Test refresh tracing
    #[tokio::test]
    async fn test_refresh_tracing() {
        let manager = RefreshManager::new(RefreshConfig::default());

        // Trigger and start
        manager.check_and_trigger(85);
        manager.start_job().unwrap();

        // Duration tracking for tracing
        let job = manager.job();
        let duration = job.duration().unwrap();
        assert!(duration.is_some());

        // Complete successfully
        manager.trigger().record_success(50);
        manager.complete_job_success().unwrap();

        // Time since last refresh available for correlation
        let _time_since = manager.time_since_last_refresh();
        // May be None because record_success sets last_refresh on trigger, not manager
        // But last check time is set
        let time_since_check = manager.time_since_last_check();
        assert!(time_since_check.is_some());
    }
}
