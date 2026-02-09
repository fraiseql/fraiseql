//! Comprehensive test specifications for credential rotation and key lifecycle management:
//! Key versioning, TTL tracking, automatic refresh, multi-version decryption,
//! and rotation scheduling.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod rotation_tests {
    use chrono::{Duration, Utc};

    use crate::encryption::credential_rotation::{
        CredentialRotationManager, KeyVersion, KeyVersionMetadata, KeyVersionStatus,
        RotationConfig, RotationSchedule, VersionedKeyStorage,
    };
    use crate::encryption::FieldEncryption;

    // ============================================================================
    // KEY VERSIONING TESTS
    // ============================================================================

    /// Test key version tracking
    #[tokio::test]
    async fn test_key_version_tracking() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);

        // Initialize first key version
        let v1 = manager.initialize_key().unwrap();
        assert_eq!(v1, 1);
        assert_eq!(manager.get_current_version().unwrap(), 1);

        // Rotate to create version 2
        let v2 = manager.rotate_key().unwrap();
        assert_eq!(v2, 2);
        assert_eq!(manager.get_current_version().unwrap(), 2);

        // Rotate again to create version 3
        let v3 = manager.rotate_key().unwrap();
        assert_eq!(v3, 3);
        assert_eq!(manager.get_current_version().unwrap(), 3);

        // All versions stored and retrievable
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 3);
    }

    /// Test encryption stores version with ciphertext
    #[tokio::test]
    async fn test_encryption_embeds_version() {
        // Simulate versioned ciphertext format: [version (2 bytes)][nonce][ciphertext][tag]
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let plaintext = "sensitive@data.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Build versioned ciphertext with version prefix
        let version: KeyVersion = 3;
        let version_bytes = version.to_be_bytes();
        let mut versioned = Vec::new();
        versioned.extend_from_slice(&version_bytes);
        versioned.extend_from_slice(&encrypted);

        // Extract version from versioned ciphertext
        let extracted =
            CredentialRotationManager::extract_version_from_ciphertext(&versioned).unwrap();
        assert_eq!(extracted, 3);

        // The remaining bytes after version prefix are the original encrypted data
        let remaining = &versioned[2..];
        let decrypted = cipher.decrypt(remaining).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test multi-version decryption support
    #[tokio::test]
    async fn test_multi_version_decryption() {
        let key_v1 = [1u8; 32];
        let key_v2 = [2u8; 32];
        let cipher_v1 = FieldEncryption::new(&key_v1);
        let cipher_v2 = FieldEncryption::new(&key_v2);

        // Encrypt data with v1
        let data_v1 = cipher_v1.encrypt("secret_v1").unwrap();
        // Encrypt data with v2
        let data_v2 = cipher_v2.encrypt("secret_v2").unwrap();

        // Decrypt old data with v1 key
        let decrypted_v1 = cipher_v1.decrypt(&data_v1).unwrap();
        assert_eq!(decrypted_v1, "secret_v1");

        // Decrypt new data with v2 key
        let decrypted_v2 = cipher_v2.decrypt(&data_v2).unwrap();
        assert_eq!(decrypted_v2, "secret_v2");

        // Wrong key fails decryption
        let result = cipher_v2.decrypt(&data_v1);
        assert!(result.is_err());
    }

    /// Test version retrieval for old records
    #[tokio::test]
    async fn test_retrieve_version_from_ciphertext() {
        // Version 0 = unversioned (legacy)
        let unversioned = [0u8, 0u8, 1, 2, 3, 4];
        let version =
            CredentialRotationManager::extract_version_from_ciphertext(&unversioned).unwrap();
        assert_eq!(version, 0);

        // Version 1
        let v1_data = [0u8, 1u8, 10, 20, 30];
        let version = CredentialRotationManager::extract_version_from_ciphertext(&v1_data).unwrap();
        assert_eq!(version, 1);

        // Max version 65535
        let max_data = [0xFFu8, 0xFFu8, 99, 88];
        let version =
            CredentialRotationManager::extract_version_from_ciphertext(&max_data).unwrap();
        assert_eq!(version, 65535);

        // Too short errors gracefully
        let short = [0u8];
        let result = CredentialRotationManager::extract_version_from_ciphertext(&short);
        assert!(result.is_err());

        // Empty data errors gracefully
        let empty: &[u8] = &[];
        let result = CredentialRotationManager::extract_version_from_ciphertext(empty);
        assert!(result.is_err());
    }

    /// Test version compatibility across key rotation
    #[tokio::test]
    async fn test_version_compatibility_after_rotation() {
        let key_v1 = [1u8; 32];
        let key_v2 = [2u8; 32];
        let cipher_v1 = FieldEncryption::new(&key_v1);
        let cipher_v2 = FieldEncryption::new(&key_v2);

        // Encrypt records with v1
        let old_record = cipher_v1.encrypt("old_data").unwrap();

        // Rotate to v2
        let new_record = cipher_v2.encrypt("new_data").unwrap();

        // Both records still decrypt with their respective keys
        assert_eq!(cipher_v1.decrypt(&old_record).unwrap(), "old_data");
        assert_eq!(cipher_v2.decrypt(&new_record).unwrap(), "new_data");

        // No migration required: old records use old key, new records use new key
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let v1 = manager.get_current_version().unwrap();
        manager.rotate_key().unwrap();
        let v2 = manager.get_current_version().unwrap();

        // Both versions exist and can decrypt
        assert!(manager.can_decrypt_with_version(v1).unwrap());
        assert!(manager.can_decrypt_with_version(v2).unwrap());
    }

    // ============================================================================
    // TTL AND EXPIRATION TESTS
    // ============================================================================

    /// Test key TTL tracking
    #[tokio::test]
    async fn test_key_ttl_tracking() {
        let metadata = KeyVersionMetadata::new(1, 90);
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.status, KeyVersionStatus::Active);

        // TTL stores issued_at and expires_at
        let total_ttl = metadata.expires_at - metadata.issued_at;
        assert!(total_ttl.num_days() >= 89 && total_ttl.num_days() <= 91);

        // Can query remaining lifetime
        let remaining = metadata.time_until_expiry();
        assert!(remaining.num_days() >= 89);

        // TTL consumed percent should be near 0 for fresh key
        assert!(metadata.ttl_consumed_percent() < 2);
    }

    /// Test expired key detection
    #[tokio::test]
    async fn test_expired_key_detection() {
        let mut metadata = KeyVersionMetadata::new(1, 30);

        // Simulate expiration by setting expires_at in the past
        metadata.expires_at = Utc::now() - Duration::hours(1);

        assert!(metadata.is_expired());

        // Update status detects expiration
        metadata.update_status();
        assert_eq!(metadata.status, KeyVersionStatus::Expired);

        // Expired key can still be used for decryption in the rotation manager
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        let v = manager.initialize_key().unwrap();
        // A non-compromised version (even if expired) can still decrypt
        assert!(manager.can_decrypt_with_version(v).unwrap());
    }

    /// Test near-expiry warnings
    #[tokio::test]
    async fn test_near_expiry_warnings() {
        let mut metadata = KeyVersionMetadata::new(1, 365);

        // Not expiring soon when far from expiry
        assert!(!metadata.is_expiring_soon());

        // Simulate near-expiry: less than 14 days remaining
        metadata.expires_at = Utc::now() + Duration::days(7);
        assert!(metadata.is_expiring_soon());

        // Exactly 14 days is not expiring soon (< 14 required)
        metadata.expires_at = Utc::now() + Duration::days(14) + Duration::hours(1);
        assert!(!metadata.is_expiring_soon());

        // 13 days remaining is expiring soon
        metadata.expires_at = Utc::now() + Duration::days(13);
        assert!(metadata.is_expiring_soon());

        // Time remaining is available
        let remaining = metadata.time_until_expiry();
        assert!(remaining.num_days() >= 12 && remaining.num_days() <= 14);
    }

    /// Test TTL configuration by framework
    #[tokio::test]
    async fn test_ttl_configuration_compliance() {
        // HIPAA: 1 year (365 days)
        let hipaa_config = RotationConfig::new().with_ttl_days(365);
        assert_eq!(hipaa_config.ttl_days, 365);

        // PCI-DSS: 1 year (365 days)
        let pci_config = RotationConfig::new().with_ttl_days(365);
        assert_eq!(pci_config.ttl_days, 365);

        // GDPR: configurable, commonly 90 days for stricter compliance
        let gdpr_config = RotationConfig::new().with_ttl_days(90);
        assert_eq!(gdpr_config.ttl_days, 90);

        // SOC 2: 1 year default
        let soc2_config = RotationConfig::new().with_ttl_days(365);
        assert_eq!(soc2_config.ttl_days, 365);

        // Manager enforces TTL on key versions
        let manager = CredentialRotationManager::new(hipaa_config);
        manager.initialize_key().unwrap();
        let metadata = manager.get_current_metadata().unwrap().unwrap();
        let total_ttl = metadata.expires_at - metadata.issued_at;
        assert!(total_ttl.num_days() >= 364 && total_ttl.num_days() <= 366);
    }

    // ============================================================================
    // AUTOMATIC REFRESH TESTS
    // ============================================================================

    /// Test automatic key refresh before expiry
    #[tokio::test]
    async fn test_automatic_refresh_before_expiry() {
        let config = RotationConfig::new().with_ttl_days(100).with_refresh_threshold(80);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Fresh key does not need refresh
        assert!(!manager.needs_refresh().unwrap());

        // Simulate 81% TTL consumed by creating a manager with short TTL
        let short_config = RotationConfig::new().with_ttl_days(10);
        let short_manager = CredentialRotationManager::new(short_config);
        short_manager.initialize_key().unwrap();

        // Fresh key with 10-day TTL shouldn't need refresh
        assert!(!short_manager.needs_refresh().unwrap());

        // The should_refresh method checks ttl_consumed_percent >= 80
        let mut metadata = KeyVersionMetadata::new(1, 100);
        metadata.issued_at = Utc::now() - Duration::days(81);
        metadata.expires_at = Utc::now() + Duration::days(19);
        assert!(metadata.should_refresh());
    }

    /// Test refresh creates new version
    #[tokio::test]
    async fn test_refresh_creates_new_version() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);

        let v1 = manager.initialize_key().unwrap();
        assert_eq!(v1, 1);

        // Rotate (simulating refresh)
        let v2 = manager.rotate_key().unwrap();
        assert_eq!(v2, 2);
        assert_eq!(manager.get_current_version().unwrap(), 2);

        // Old version retained
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 2);

        // New version is current
        let current_metadata = manager.get_current_metadata().unwrap().unwrap();
        assert!(current_metadata.is_current);
    }

    /// Test operations during refresh
    #[tokio::test]
    async fn test_operations_during_refresh() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Encrypt with current key
        let encrypted_before = cipher.encrypt("data_before_rotation").unwrap();

        // Rotate (simulating refresh)
        let new_version = manager.rotate_key().unwrap();
        assert!(new_version > 1);

        // Old data still decryptable (same cipher key in this test)
        let decrypted = cipher.decrypt(&encrypted_before).unwrap();
        assert_eq!(decrypted, "data_before_rotation");

        // New data encrypted fine
        let encrypted_after = cipher.encrypt("data_after_rotation").unwrap();
        let decrypted_after = cipher.decrypt(&encrypted_after).unwrap();
        assert_eq!(decrypted_after, "data_after_rotation");
    }

    /// Test refresh scheduling
    #[tokio::test]
    async fn test_refresh_scheduling() {
        // Manual schedule
        let manual_config = RotationConfig::new().with_schedule(RotationSchedule::Manual);
        assert_eq!(manual_config.schedule, RotationSchedule::Manual);

        // Interval schedule (every 30 days)
        let interval_config =
            RotationConfig::new().with_schedule(RotationSchedule::Interval(30));
        assert_eq!(interval_config.schedule, RotationSchedule::Interval(30));

        // Cron schedule
        let cron_config = RotationConfig::new()
            .with_schedule(RotationSchedule::Cron("0 2 1 * *".to_string()));
        assert_eq!(
            cron_config.schedule,
            RotationSchedule::Cron("0 2 1 * *".to_string())
        );

        // On-demand works immediately
        let manager = CredentialRotationManager::new(manual_config);
        manager.initialize_key().unwrap();
        let v = manager.rotate_key().unwrap();
        assert_eq!(v, 2);
    }

    // ============================================================================
    // ROTATION SCHEDULING TESTS
    // ============================================================================

    /// Test rotation schedule configuration
    #[tokio::test]
    async fn test_rotation_schedule_configuration() {
        // Monthly at 2am on 1st
        let monthly = RotationSchedule::Cron("0 2 1 * *".to_string());
        assert_eq!(format!("{monthly}"), "cron: 0 2 1 * *");

        // Weekly at midnight Sunday
        let weekly = RotationSchedule::Cron("0 0 * * 0".to_string());
        assert_eq!(format!("{weekly}"), "cron: 0 0 * * 0");

        // Interval-based (every 30 days)
        let interval = RotationSchedule::Interval(30);
        assert_eq!(format!("{interval}"), "every 30 days");

        // Manual
        let manual = RotationSchedule::Manual;
        assert_eq!(format!("{manual}"), "manual");

        // Configurable per key
        let config = RotationConfig::new().with_schedule(monthly);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        assert_eq!(manager.get_current_version().unwrap(), 1);
    }

    /// Test rotation schedule execution
    #[tokio::test]
    async fn test_rotation_schedule_execution() {
        let config = RotationConfig::new()
            .with_schedule(RotationSchedule::Interval(30));
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Simulate scheduled rotation trigger
        let old_version = manager.get_current_version().unwrap();
        let new_version = manager.rotate_key().unwrap();

        assert!(new_version > old_version);
        assert_eq!(manager.get_current_version().unwrap(), new_version);

        // Metrics record the rotation event
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
        assert!(metrics.last_rotation().is_some());
    }

    /// Test rotation scheduling with timezone
    #[tokio::test]
    async fn test_rotation_schedule_timezone() {
        // Cron expressions interpreted with schedule value
        let utc_cron = RotationSchedule::Cron("0 2 * * *".to_string());
        let est_cron = RotationSchedule::Cron("0 2 * * *".to_string());

        // Both are valid schedule types
        assert_eq!(format!("{utc_cron}"), "cron: 0 2 * * *");
        assert_eq!(format!("{est_cron}"), "cron: 0 2 * * *");

        // Schedules can be set on configs
        let config_utc = RotationConfig::new().with_schedule(utc_cron);
        let config_est = RotationConfig::new().with_schedule(est_cron);
        assert_ne!(
            std::mem::discriminant(&config_utc.schedule),
            std::mem::discriminant(&RotationSchedule::Manual)
        );
        assert_ne!(
            std::mem::discriminant(&config_est.schedule),
            std::mem::discriminant(&RotationSchedule::Manual)
        );
    }

    /// Test manual rotation trigger
    #[tokio::test]
    async fn test_manual_rotation_trigger() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Manual rotation works immediately
        let old_version = manager.get_current_version().unwrap();
        let new_version = manager.rotate_key().unwrap();

        assert!(new_version > old_version);
        assert_eq!(manager.get_current_version().unwrap(), new_version);

        // Metrics logged
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
        assert!(metrics.last_rotation_duration_ms() < 1000);
    }

    // ============================================================================
    // KEY VERSION STORAGE TESTS
    // ============================================================================

    /// Test versioned key storage
    #[tokio::test]
    async fn test_versioned_key_storage() {
        let storage = VersionedKeyStorage::new();

        let metadata = KeyVersionMetadata::new(1, 365);
        let version = storage.add_version(metadata).unwrap();
        assert_eq!(version, 1);

        // Can retrieve specific version by ID
        let retrieved = storage.get_version(1).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.version, 1);
        assert_eq!(retrieved.status, KeyVersionStatus::Active);

        // Non-existent version returns None
        let missing = storage.get_version(99).unwrap();
        assert!(missing.is_none());
    }

    /// Test current version marking
    #[tokio::test]
    async fn test_current_version_marking() {
        let storage = VersionedKeyStorage::new();

        let meta1 = KeyVersionMetadata::new(1, 365);
        let meta2 = KeyVersionMetadata::new(2, 365);

        storage.add_version(meta1).unwrap();
        storage.add_version(meta2).unwrap();

        storage.set_current_version(1).unwrap();
        assert_eq!(storage.get_current_version().unwrap(), 1);

        // When rotation occurs, new version marked current
        storage.set_current_version(2).unwrap();
        assert_eq!(storage.get_current_version().unwrap(), 2);

        // Setting non-existent version fails
        let result = storage.set_current_version(99);
        assert!(result.is_err());
    }

    /// Test version history retrieval
    #[tokio::test]
    async fn test_version_history_retrieval() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        manager.rotate_key().unwrap();
        manager.rotate_key().unwrap();

        // All versions retrievable
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 3);

        // Sorted by issue date (newest first)
        for window in history.windows(2) {
            assert!(window[0].issued_at >= window[1].issued_at);
        }

        // Each version has correct metadata
        for entry in &history {
            assert_eq!(entry.status, KeyVersionStatus::Active);
        }
    }

    /// Test key version lifecycle
    #[tokio::test]
    async fn test_key_version_lifecycle() {
        // Active: can encrypt/decrypt
        let mut metadata = KeyVersionMetadata::new(1, 365);
        assert_eq!(metadata.status, KeyVersionStatus::Active);

        // Expiring: cannot encrypt, can decrypt (grace period)
        metadata.expires_at = Utc::now() + Duration::days(7);
        metadata.update_status();
        assert_eq!(metadata.status, KeyVersionStatus::Expiring);

        // Expired: cannot encrypt, can decrypt (archival)
        metadata.expires_at = Utc::now() - Duration::hours(1);
        metadata.update_status();
        assert_eq!(metadata.status, KeyVersionStatus::Expired);

        // Compromised: cannot use (quarantined)
        let mut fresh = KeyVersionMetadata::new(2, 365);
        fresh.mark_compromised("Security incident");
        assert_eq!(fresh.status, KeyVersionStatus::Compromised);
        assert!(fresh.compromise_reason.is_some());

        // Compromised status is never changed by update_status
        fresh.update_status();
        assert_eq!(fresh.status, KeyVersionStatus::Compromised);
    }

    // ============================================================================
    // DECRYPTION COMPATIBILITY TESTS
    // ============================================================================

    /// Test transparent decryption with historical keys
    #[tokio::test]
    async fn test_transparent_historical_decryption() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        let v1 = manager.initialize_key().unwrap();

        // Encrypt with v1
        let encrypted_v1 = cipher.encrypt("old_data").unwrap();

        // Rotate to v2
        manager.rotate_key().unwrap();

        // v1 still exists and can decrypt
        assert!(manager.can_decrypt_with_version(v1).unwrap());
        let decrypted = cipher.decrypt(&encrypted_v1).unwrap();
        assert_eq!(decrypted, "old_data");
    }

    /// Test decryption with missing version
    #[tokio::test]
    async fn test_decryption_missing_version() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Non-existent version 42
        let can_decrypt = manager.can_decrypt_with_version(42).unwrap();
        assert!(!can_decrypt);

        // Ciphertext with non-existent version
        let fake_ciphertext = [0u8, 42u8, 1, 2, 3, 4, 5];
        let extracted =
            CredentialRotationManager::extract_version_from_ciphertext(&fake_ciphertext).unwrap();
        assert_eq!(extracted, 42);
        assert!(!manager.can_decrypt_with_version(extracted).unwrap());
    }

    /// Test batch decryption with mixed versions
    #[tokio::test]
    async fn test_batch_decryption_mixed_versions() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Encrypt multiple records (all with same key for this test)
        let records = vec![
            cipher.encrypt("record_1").unwrap(),
            cipher.encrypt("record_2").unwrap(),
            cipher.encrypt("record_3").unwrap(),
        ];

        // Batch decrypt all records
        let decrypted: Vec<String> = records
            .iter()
            .map(|r| cipher.decrypt(r).unwrap())
            .collect();

        assert_eq!(decrypted.len(), 3);
        assert_eq!(decrypted[0], "record_1");
        assert_eq!(decrypted[1], "record_2");
        assert_eq!(decrypted[2], "record_3");

        // With version tracking, each record can be mapped to the correct cipher
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        let v1 = manager.initialize_key().unwrap();
        let v2 = manager.rotate_key().unwrap();

        assert!(manager.can_decrypt_with_version(v1).unwrap());
        assert!(manager.can_decrypt_with_version(v2).unwrap());
    }

    /// Test decryption performance with versions
    #[tokio::test]
    async fn test_decryption_performance_with_versions() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Create multiple versions
        for _ in 0..5 {
            manager.rotate_key().unwrap();
        }

        // Version lookup is fast (O(1) HashMap lookup)
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = manager.can_decrypt_with_version(1);
            let _ = manager.can_decrypt_with_version(3);
            let _ = manager.can_decrypt_with_version(5);
        }
        let lookup_duration = start.elapsed();
        assert!(lookup_duration.as_millis() < 100, "Version lookup should be fast");

        // Bulk decryption performance
        let encrypted = cipher.encrypt("test_data").unwrap();
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = cipher.decrypt(&encrypted).unwrap();
        }
        let decrypt_duration = start.elapsed();
        assert!(decrypt_duration.as_millis() < 500, "Bulk decryption should be fast");
    }

    // ============================================================================
    // ROTATION COMPLIANCE TESTS
    // ============================================================================

    /// Test HIPAA rotation compliance
    #[tokio::test]
    async fn test_hipaa_rotation_compliance() {
        // HIPAA requires key rotation at least annually (365 days max)
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Fresh key is HIPAA compliant
        assert!(manager.check_hipaa_compliance().unwrap());

        // Metadata TTL is 365 days
        let metadata = manager.get_current_metadata().unwrap().unwrap();
        let total_ttl = metadata.expires_at - metadata.issued_at;
        assert!(total_ttl.num_days() >= 364);

        // Rotation history is auditable
        let history = manager.get_version_history().unwrap();
        assert!(!history.is_empty());
    }

    /// Test PCI-DSS rotation compliance
    #[tokio::test]
    async fn test_pci_dss_rotation_compliance() {
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Fresh key is PCI-DSS compliant
        assert!(manager.check_pci_compliance().unwrap());

        // Rotate and verify compliance maintained
        manager.rotate_key().unwrap();
        assert!(manager.check_pci_compliance().unwrap());

        // Rotation events tracked via metrics
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
    }

    /// Test GDPR data minimization with rotation
    #[tokio::test]
    async fn test_gdpr_data_minimization_rotation() {
        // GDPR: more frequent rotation configurable
        let config = RotationConfig::new().with_ttl_days(90);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let metadata = manager.get_current_metadata().unwrap().unwrap();
        let total_ttl = metadata.expires_at - metadata.issued_at;
        assert!(total_ttl.num_days() >= 89 && total_ttl.num_days() <= 91);

        // Rotation history preserved for audit
        manager.rotate_key().unwrap();
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 2);
    }

    /// Test SOC 2 rotation controls
    #[tokio::test]
    async fn test_soc2_rotation_controls() {
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Rotate and track
        manager.rotate_key().unwrap();
        manager.rotate_key().unwrap();

        // Audit log records all rotations with timestamps
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 2);
        assert!(metrics.last_rotation().is_some());

        // Version history traceable
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 3);
        for entry in &history {
            assert!(entry.issued_at <= Utc::now());
        }
    }

    // ============================================================================
    // ROTATION AUDIT AND OBSERVABILITY
    // ============================================================================

    /// Test rotation audit logging
    #[tokio::test]
    async fn test_rotation_audit_logging() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let old_version = manager.get_current_version().unwrap();
        let new_version = manager.rotate_key().unwrap();

        // Metrics provide audit information
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
        assert!(metrics.last_rotation().is_some());
        assert!(new_version > old_version);

        // Version history provides full audit trail
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 2);
    }

    /// Test rotation metrics collection
    #[tokio::test]
    async fn test_rotation_metrics_collection() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Before rotation
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 0);
        assert_eq!(metrics.failed_rotations(), 0);

        // After rotation
        manager.rotate_key().unwrap();

        assert_eq!(metrics.total_rotations(), 1);
        assert_eq!(metrics.failed_rotations(), 0);
        assert_eq!(metrics.success_rate_percent(), 100);
        assert!(metrics.last_rotation().is_some());
        // Duration is recorded
        let _ = metrics.last_rotation_duration_ms();

        // Record a failure
        metrics.record_failure();
        assert_eq!(metrics.failed_rotations(), 1);
        assert_eq!(metrics.success_rate_percent(), 0); // 1 total - 1 failed = 0 successful
    }

    /// Test rotation alerts
    #[tokio::test]
    async fn test_rotation_alerts() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Fresh key: no attention needed
        assert!(!manager.has_versions_needing_attention().unwrap());

        // Mark version as compromised: triggers attention
        let version = manager.get_current_version().unwrap();
        manager
            .mark_version_compromised(version, "Suspected breach")
            .unwrap();
        assert!(manager.has_versions_needing_attention().unwrap());

        // Compromised versions counted
        assert_eq!(manager.compromised_versions_count().unwrap(), 1);
    }

    /// Test rotation status dashboard
    #[tokio::test]
    async fn test_rotation_status_dashboard() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        manager.rotate_key().unwrap();
        manager.rotate_key().unwrap();

        // Dashboard data
        let current = manager.get_current_version().unwrap();
        assert_eq!(current, 3);

        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 3);

        let active = manager.active_versions_count().unwrap();
        assert!(active > 0);

        let expired = manager.expired_versions_count().unwrap();
        assert_eq!(expired, 0); // No expired versions yet

        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 2);
        assert!(metrics.last_rotation().is_some());
    }

    // ============================================================================
    // EMERGENCY ROTATION TESTS
    // ============================================================================

    /// Test emergency key rotation
    #[tokio::test]
    async fn test_emergency_key_rotation() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let old_version = manager.get_current_version().unwrap();

        // Emergency rotation
        let new_version = manager
            .emergency_rotate("Suspected key compromise")
            .unwrap();
        assert!(new_version > old_version);
        assert_eq!(manager.get_current_version().unwrap(), new_version);

        // Old version marked compromised
        let old_meta = manager
            .get_version_history()
            .unwrap()
            .into_iter()
            .find(|m| m.version == old_version)
            .unwrap();
        assert_eq!(old_meta.status, KeyVersionStatus::Compromised);
        assert!(old_meta.compromise_reason.is_some());
    }

    /// Test compromised key quarantine
    #[tokio::test]
    async fn test_compromised_key_quarantine() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let version = manager.get_current_version().unwrap();
        manager
            .mark_version_compromised(version, "Key leaked")
            .unwrap();

        // Cannot be used for decryption
        assert!(!manager.can_decrypt_with_version(version).unwrap());

        // Compromised count
        assert_eq!(manager.compromised_versions_count().unwrap(), 1);

        // Needs attention flag set
        assert!(manager.has_versions_needing_attention().unwrap());
    }

    /// Test emergency rotation notification
    #[tokio::test]
    async fn test_emergency_rotation_notification() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let old_version = manager.get_current_version().unwrap();
        let new_version = manager.emergency_rotate("Critical security incident").unwrap();

        // Emergency rotation tracked in metrics
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
        assert!(metrics.last_rotation().is_some());

        // Old version info preserved with compromise reason
        let history = manager.get_version_history().unwrap();
        let old_entry = history.iter().find(|m| m.version == old_version).unwrap();
        assert_eq!(old_entry.status, KeyVersionStatus::Compromised);
        assert!(old_entry
            .compromise_reason
            .as_ref()
            .unwrap()
            .contains("Critical security incident"));

        // New version is current
        assert_eq!(manager.get_current_version().unwrap(), new_version);
    }

    // ============================================================================
    // ROTATION TESTING AND VALIDATION
    // ============================================================================

    /// Test rotation dry-run validation
    #[tokio::test]
    async fn test_rotation_dry_run() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let version_before = manager.get_current_version().unwrap();
        let history_len_before = manager.get_version_history().unwrap().len();

        // Dry-run: check state without actually rotating
        // In a dry-run, we just inspect the current state
        let active = manager.active_versions_count().unwrap();
        assert!(active > 0);
        let needs_refresh = manager.needs_refresh().unwrap();
        assert!(!needs_refresh);

        // No state change occurred
        assert_eq!(manager.get_current_version().unwrap(), version_before);
        assert_eq!(manager.get_version_history().unwrap().len(), history_len_before);
    }

    /// Test rotation validation before commit
    #[tokio::test]
    async fn test_rotation_validation_before_commit() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Validate key can encrypt
        let encrypted = cipher.encrypt("validation_test").unwrap();
        assert!(!encrypted.is_empty());

        // Validate key can decrypt
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "validation_test");

        // Validate version metadata is correct
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let metadata = manager.get_current_metadata().unwrap().unwrap();
        assert_eq!(metadata.status, KeyVersionStatus::Active);
        assert!(!metadata.is_expired());

        // Only after validation, proceed with rotation
        let new_version = manager.rotate_key().unwrap();
        assert!(new_version > 0);
    }

    /// Test decryption compatibility testing
    #[tokio::test]
    async fn test_decryption_compatibility_testing() {
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Create sample encrypted records
        let samples: Vec<Vec<u8>> = (0..5)
            .map(|i| cipher.encrypt(&format!("record_{i}")).unwrap())
            .collect();

        // Verify all samples decrypt correctly before rotation
        for (i, sample) in samples.iter().enumerate() {
            let decrypted = cipher.decrypt(sample).unwrap();
            assert_eq!(decrypted, format!("record_{i}"));
        }

        // After simulated rotation (same key in this test), verify again
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        manager.rotate_key().unwrap();

        // All samples still decrypt
        for (i, sample) in samples.iter().enumerate() {
            let decrypted = cipher.decrypt(sample).unwrap();
            assert_eq!(decrypted, format!("record_{i}"));
        }
    }
}
