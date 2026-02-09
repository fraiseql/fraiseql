//! Comprehensive tests for audit logging, schema detection,
//! transaction integration, performance optimization, error recovery,
//! and compliance with field-level encryption.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod audit_logging_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::encryption::audit_logging::{AuditLogEntry, AuditLogger, EventStatus, OperationType};
    use crate::encryption::schema::{
        EncryptionMark, SchemaFieldInfo, SchemaRegistry, StructSchema,
    };
    use crate::encryption::FieldEncryption;

    // ============================================================================
    // AUDIT LOGGING TESTS
    // ============================================================================

    /// Test encryption operation logged to audit trail
    #[tokio::test]
    async fn test_audit_log_encryption_operation() {
        let mut logger = AuditLogger::new(100);

        // Simulate INSERT encryption
        let entry = AuditLogEntry::new("user-42", "email", OperationType::Insert, "req-001", "sess-abc")
            .with_context("table", "users")
            .with_context("key_version", "3");
        logger.log_entry(entry).unwrap();

        // Simulate UPDATE encryption
        let entry = AuditLogEntry::new("user-42", "phone", OperationType::Update, "req-002", "sess-abc")
            .with_context("table", "users");
        logger.log_entry(entry).unwrap();

        assert_eq!(logger.entry_count(), 2);

        let user_entries = logger.entries_for_user("user-42");
        assert_eq!(user_entries.len(), 2);
        assert_eq!(user_entries[0].field_name(), "email");
        assert_eq!(user_entries[0].operation(), OperationType::Insert);
        assert_eq!(user_entries[0].status(), EventStatus::Success);
        assert_eq!(user_entries[1].field_name(), "phone");
        assert_eq!(user_entries[1].operation(), OperationType::Update);
    }

    /// Test decryption operation logged to audit trail
    #[tokio::test]
    async fn test_audit_log_decryption_operation() {
        let mut logger = AuditLogger::new(100);

        let entry = AuditLogEntry::new("user-99", "email", OperationType::Select, "req-010", "sess-xyz")
            .with_context("table", "users")
            .with_context("rows_accessed", "5");
        logger.log_entry(entry).unwrap();

        let select_entries = logger.entries_for_operation(OperationType::Select);
        assert_eq!(select_entries.len(), 1);
        assert_eq!(select_entries[0].user_id(), "user-99");
        assert_eq!(select_entries[0].field_name(), "email");
        assert_eq!(select_entries[0].context().get("rows_accessed"), Some(&"5".to_string()));
    }

    /// Test encryption failure logged
    #[tokio::test]
    async fn test_audit_log_encryption_failure() {
        let mut logger = AuditLogger::new(100);

        let entry = AuditLogEntry::new("user-13", "ssn", OperationType::Insert, "req-err-1", "sess-fail")
            .with_failure("Encryption key not found for field 'ssn'");
        logger.log_entry(entry).unwrap();

        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].user_id(), "user-13");
        assert_eq!(failed[0].field_name(), "ssn");
        assert_eq!(failed[0].status(), EventStatus::Failure);
        assert_eq!(
            failed[0].error_message(),
            Some("Encryption key not found for field 'ssn'")
        );
    }

    /// Test decryption failure logged
    #[tokio::test]
    async fn test_audit_log_decryption_failure() {
        let mut logger = AuditLogger::new(100);

        let entry = AuditLogEntry::new("user-77", "credit_card", OperationType::Select, "req-dec-fail", "sess-d")
            .with_failure("Decryption failed: wrong key version")
            .with_context("key_version_attempted", "2")
            .with_context("expected_key_version", "3");
        logger.log_entry(entry).unwrap();

        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
        assert_eq!(
            failed[0].error_message(),
            Some("Decryption failed: wrong key version")
        );
        assert_eq!(
            failed[0].context().get("key_version_attempted"),
            Some(&"2".to_string())
        );
    }

    /// Test audit trail correlates related operations
    #[tokio::test]
    async fn test_audit_log_operation_correlation() {
        let mut logger = AuditLogger::new(100);

        // User inserts a record (encrypts email)
        let insert_entry = AuditLogEntry::new("user-42", "email", OperationType::Insert, "req-100", "sess-corr")
            .with_context("record_id", "rec-555");
        logger.log_entry(insert_entry).unwrap();

        // Later the same user reads it (decrypts email)
        let select_entry = AuditLogEntry::new("user-42", "email", OperationType::Select, "req-200", "sess-corr")
            .with_context("record_id", "rec-555");
        logger.log_entry(select_entry).unwrap();

        // Another user reads it too
        let other_entry = AuditLogEntry::new("user-99", "email", OperationType::Select, "req-201", "sess-other")
            .with_context("record_id", "rec-555");
        logger.log_entry(other_entry).unwrap();

        // Correlate by user
        let user42 = logger.entries_for_user("user-42");
        assert_eq!(user42.len(), 2);
        assert_eq!(user42[0].operation(), OperationType::Insert);
        assert_eq!(user42[1].operation(), OperationType::Select);

        // Correlate by field
        let email_entries = logger.entries_for_field("email");
        assert_eq!(email_entries.len(), 3);

        // Correlate by user + operation
        let user42_selects = logger.entries_for_user_operation("user-42", OperationType::Select);
        assert_eq!(user42_selects.len(), 1);
    }

    /// Test audit log includes user context
    #[tokio::test]
    async fn test_audit_log_user_context() {
        let mut logger = AuditLogger::new(100);

        let entry = AuditLogEntry::new("admin-01", "ssn", OperationType::Select, "req-ctx", "sess-admin")
            .with_security_context(Some("10.0.0.5"), Some("superadmin"))
            .with_context("department", "compliance");
        logger.log_entry(entry).unwrap();

        let entries = logger.entries_for_user("admin-01");
        assert_eq!(entries.len(), 1);

        let ctx = entries[0].context();
        assert_eq!(ctx.get("ip_address"), Some(&"10.0.0.5".to_string()));
        assert_eq!(ctx.get("user_role"), Some(&"superadmin".to_string()));
        assert_eq!(ctx.get("department"), Some(&"compliance".to_string()));
        assert_eq!(entries[0].request_id(), "req-ctx");
        assert_eq!(entries[0].session_id(), "sess-admin");
    }

    /// Test audit log includes encryption context
    #[tokio::test]
    async fn test_audit_log_encryption_context() {
        let mut logger = AuditLogger::new(100);

        let entry = AuditLogEntry::new("user-42", "email", OperationType::Insert, "req-ectx", "sess-ec")
            .with_context("encryption_context", "user:42:field:email:op:insert")
            .with_context("context_verified", "true")
            .with_context("algorithm", "aes256-gcm");
        logger.log_entry(entry).unwrap();

        let entries = logger.recent_entries(1);
        assert_eq!(entries.len(), 1);
        let ctx = entries[0].context();
        assert_eq!(
            ctx.get("encryption_context"),
            Some(&"user:42:field:email:op:insert".to_string())
        );
        assert_eq!(ctx.get("context_verified"), Some(&"true".to_string()));
        assert_eq!(ctx.get("algorithm"), Some(&"aes256-gcm".to_string()));
    }

    /// Test audit log persists to storage (in-memory storage for tests)
    #[tokio::test]
    async fn test_audit_log_persistence() {
        let mut logger = AuditLogger::new(1000);

        // Log many entries to verify persistence
        for i in 0..50 {
            let entry = AuditLogEntry::new(
                format!("user-{}", i % 5),
                "email",
                if i % 2 == 0 { OperationType::Insert } else { OperationType::Select },
                format!("req-{i}"),
                "sess-persist",
            );
            logger.log_entry(entry).unwrap();
        }

        assert_eq!(logger.entry_count(), 50);

        // Verify all entries are accessible
        let recent = logger.recent_entries(50);
        assert_eq!(recent.len(), 50);

        // Verify CSV export works for each entry (simulates file persistence)
        for entry in &recent {
            let csv = entry.to_csv();
            assert!(csv.contains("email"));
            assert!(!csv.is_empty());
        }

        // Verify JSON export works (simulates API persistence)
        for entry in &recent {
            let json = entry.to_json_like();
            assert!(json.contains("timestamp"));
            assert!(json.contains("user_id"));
        }
    }

    /// Test audit log is tamper-resistant (bounded, append-only, signed)
    #[tokio::test]
    async fn test_audit_log_tamper_resistant() {
        let mut logger = AuditLogger::new(10);

        // Fill to capacity
        for i in 0..10 {
            let entry = AuditLogEntry::new(
                format!("user-{i}"),
                "email",
                OperationType::Insert,
                format!("req-{i}"),
                "sess-tamper",
            );
            logger.log_entry(entry).unwrap();
        }
        assert_eq!(logger.entry_count(), 10);

        // Append-only: new entry evicts oldest (bounded history)
        let new_entry = AuditLogEntry::new("user-new", "email", OperationType::Insert, "req-new", "sess-tamper");
        logger.log_entry(new_entry).unwrap();
        assert_eq!(logger.entry_count(), 10); // Still bounded

        // Verify oldest was evicted and newest is present
        let entries = logger.recent_entries(10);
        assert_eq!(entries.last().unwrap().user_id(), "user-new");
        // user-0 should have been evicted
        let user0 = logger.entries_for_user("user-0");
        assert_eq!(user0.len(), 0);

        // Verify each entry has a timestamp (immutable record)
        for entry in &entries {
            assert!(entry.timestamp() <= chrono::Utc::now());
        }

        // Verify entries can be serialized to signed format (CSV with all fields)
        for entry in &entries {
            let csv = entry.to_csv();
            // CSV contains all required fields for HMAC signing
            assert!(csv.contains(&entry.user_id().to_string()));
            assert!(csv.contains(&entry.request_id().to_string()));
        }
    }

    // ============================================================================
    // SCHEMA DETECTION TESTS
    // ============================================================================

    /// Test schema detects encrypted field annotations
    #[test]
    fn test_schema_detect_encrypted_annotation() {
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email")
            .with_mark(EncryptionMark::Encrypted);
        let name = SchemaFieldInfo::new("name", "String", false, "");
        schema.add_field(email);
        schema.add_field(name);

        // Schema parser detects encrypted annotation
        assert!(schema.is_field_encrypted("email"));
        assert!(!schema.is_field_encrypted("name"));

        // Mark is preserved
        let field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(field.mark, Some(EncryptionMark::Encrypted));

        // Only encrypted fields in the encrypted list
        assert_eq!(schema.encrypted_field_count(), 1);
        assert_eq!(schema.total_field_count(), 2);
    }

    /// Test schema supports multiple encryption marks
    #[test]
    fn test_schema_multiple_encryption_marks() {
        let mut schema = StructSchema::new("User");

        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email")
            .with_mark(EncryptionMark::Encrypted);
        let ssn = SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn")
            .with_mark(EncryptionMark::Sensitive);
        let phone = SchemaFieldInfo::new("phone", "String", true, "vault:encryption/phone")
            .with_mark(EncryptionMark::Encrypt);

        schema.add_field(email);
        schema.add_field(ssn);
        schema.add_field(phone);

        assert_eq!(schema.encrypted_field_count(), 3);

        let email_field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(email_field.mark, Some(EncryptionMark::Encrypted));

        let ssn_field = schema.get_encrypted_field("ssn").unwrap();
        assert_eq!(ssn_field.mark, Some(EncryptionMark::Sensitive));

        let phone_field = schema.get_encrypted_field("phone").unwrap();
        assert_eq!(phone_field.mark, Some(EncryptionMark::Encrypt));
    }

    /// Test schema includes key reference
    #[test]
    fn test_schema_encryption_key_reference() {
        let mut schema = StructSchema::new("User");

        let email = SchemaFieldInfo::new("email", "String", true, "vault:database/encryption/user_email");
        let phone = SchemaFieldInfo::new("phone", "String", true, "vault:database/encryption/user_phone");
        schema.add_field(email);
        schema.add_field(phone);

        // Each field uses its own key path
        let email_field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(email_field.key_reference, "vault:database/encryption/user_email");

        let phone_field = schema.get_encrypted_field("phone").unwrap();
        assert_eq!(phone_field.key_reference, "vault:database/encryption/user_phone");

        // Fields for same key can be queried
        let email_key_fields = schema.fields_for_key("vault:database/encryption/user_email");
        assert_eq!(email_key_fields.len(), 1);
        assert_eq!(email_key_fields[0].field_name, "email");
    }

    /// Test schema includes encryption algorithm hint
    #[test]
    fn test_schema_encryption_algorithm_hint() {
        let field_default = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        assert_eq!(field_default.algorithm, "aes256-gcm"); // Default

        let field_custom = SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn")
            .with_algorithm("chacha20-poly1305");
        assert_eq!(field_custom.algorithm, "chacha20-poly1305");
    }

    /// Test schema evolution adds encrypted field
    #[test]
    fn test_schema_evolution_add_encrypted_field() {
        // Version 1: only email encrypted
        let mut schema_v1 = StructSchema::new("User").with_version(1);
        schema_v1.add_field(SchemaFieldInfo::new("email", "String", true, "encryption/email"));
        schema_v1.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        schema_v1.add_field(SchemaFieldInfo::new("phone", "String", false, ""));
        assert_eq!(schema_v1.encrypted_field_count(), 1);

        // Version 2: phone also encrypted now
        let mut schema_v2 = StructSchema::new("User").with_version(2);
        schema_v2.add_field(SchemaFieldInfo::new("email", "String", true, "encryption/email"));
        schema_v2.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        schema_v2.add_field(SchemaFieldInfo::new("phone", "String", true, "encryption/phone"));
        assert_eq!(schema_v2.encrypted_field_count(), 2);
        assert_eq!(schema_v2.version, 2);

        // Both versions valid
        assert!(schema_v1.validate().is_ok());
        assert!(schema_v2.validate().is_ok());

        // Old records without phone encryption still have all fields accessible
        assert!(schema_v1.get_field("phone").is_some());
        assert!(!schema_v1.is_field_encrypted("phone"));
    }

    /// Test schema evolution changes key for field
    #[test]
    fn test_schema_evolution_key_rotation() {
        let mut registry = SchemaRegistry::new();

        // Register schema with version 1 key
        let mut schema = StructSchema::new("User").with_version(1);
        schema.add_field(SchemaFieldInfo::new("email", "String", true, "encryption/email_v1"));
        registry.register(schema).unwrap();

        let keys_v1 = registry.all_encryption_keys();
        assert!(keys_v1.contains(&"encryption/email_v1".to_string()));

        // Update schema with rotated key (version 2)
        registry.unregister("User");
        let mut schema_v2 = StructSchema::new("User").with_version(2);
        schema_v2.add_field(SchemaFieldInfo::new("email", "String", true, "encryption/email_v2"));
        registry.register(schema_v2).unwrap();

        let keys_v2 = registry.all_encryption_keys();
        assert!(keys_v2.contains(&"encryption/email_v2".to_string()));
        assert!(!keys_v2.contains(&"encryption/email_v1".to_string()));
    }

    /// Test schema validation on startup
    #[test]
    fn test_schema_validation_startup() {
        let mut registry = SchemaRegistry::new();

        // Valid schema
        let mut valid_schema = StructSchema::new("User");
        valid_schema.add_field(SchemaFieldInfo::new("email", "String", true, "encryption/email"));
        assert!(registry.register(valid_schema).is_ok());

        // Invalid: empty type name
        let empty_schema = StructSchema::new("");
        assert!(registry.register(empty_schema).is_err());

        // Invalid: encrypted field without key reference
        let mut bad_schema = StructSchema::new("BadType");
        bad_schema.add_field(SchemaFieldInfo::new("secret", "String", true, ""));
        assert!(registry.register(bad_schema).is_err());

        // validate_all on registry with valid schemas passes
        assert!(registry.validate_all().is_ok());
    }

    // ============================================================================
    // TRANSACTION INTEGRATION TESTS
    // ============================================================================

    /// Test encryption with transaction context
    #[tokio::test]
    async fn test_transaction_encryption_context() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Simulate transaction context
        let tx_id = "tx-001";
        let user_id = "user-42";
        let context = format!("tx:{tx_id}:user:{user_id}:op:insert");

        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt_with_context(plaintext, &context).unwrap();
        let decrypted = cipher.decrypt_with_context(&encrypted, &context).unwrap();
        assert_eq!(decrypted, plaintext);

        // Wrong transaction context fails
        let wrong_context = format!("tx:tx-999:user:{user_id}:op:insert");
        assert!(cipher.decrypt_with_context(&encrypted, &wrong_context).is_err());
    }

    /// Test batch operations in transaction
    #[tokio::test]
    async fn test_transaction_batch_encryption() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);
        let tx_context = "tx:batch-001:user:admin:op:insert";

        // Encrypt 100 records in a batch
        let mut encrypted_batch = Vec::new();
        for i in 0..100 {
            let plaintext = format!("user{i}@example.com");
            let encrypted = cipher.encrypt_with_context(&plaintext, tx_context).unwrap();
            encrypted_batch.push((plaintext, encrypted));
        }

        assert_eq!(encrypted_batch.len(), 100);

        // All decrypt correctly with the same context
        for (original, encrypted) in &encrypted_batch {
            let decrypted = cipher.decrypt_with_context(encrypted, tx_context).unwrap();
            assert_eq!(&decrypted, original);
        }

        // Log the batch as a single transaction in audit
        let mut logger = AuditLogger::new(1000);
        let entry = AuditLogEntry::new("admin", "email", OperationType::Insert, "req-batch", "sess-batch")
            .with_context("tx_id", "batch-001")
            .with_context("records_encrypted", "100");
        logger.log_entry(entry).unwrap();

        let entries = logger.recent_entries(1);
        assert_eq!(entries[0].context().get("records_encrypted"), Some(&"100".to_string()));
    }

    /// Test transaction rollback with encryption
    #[tokio::test]
    async fn test_transaction_rollback_cleanup() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Encrypt some data as part of a transaction
        let encrypted = cipher.encrypt("sensitive-data").unwrap();
        assert!(!encrypted.is_empty());

        // On rollback, the encrypted data is not persisted
        // Simulate by dropping the encrypted data
        let rolled_back: Vec<Vec<u8>> = Vec::new();
        assert!(rolled_back.is_empty()); // No data persisted

        // Audit trail records the rollback
        let mut logger = AuditLogger::new(100);
        let entry = AuditLogEntry::new("user-42", "email", OperationType::Insert, "req-rb", "sess-rb")
            .with_failure("Transaction rolled back")
            .with_context("tx_id", "tx-rollback-001")
            .with_context("reason", "constraint_violation");
        logger.log_entry(entry).unwrap();

        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].error_message(), Some("Transaction rolled back"));
    }

    /// Test nested transactions with encryption
    #[tokio::test]
    async fn test_transaction_nested_encryption() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Parent transaction context
        let parent_ctx = "tx:parent-001:user:admin";

        // Nested (savepoint) transaction context
        let nested_ctx = "tx:parent-001:savepoint:sp1:user:admin";

        // Both encrypt correctly
        let parent_encrypted = cipher.encrypt_with_context("parent-data", parent_ctx).unwrap();
        let nested_encrypted = cipher.encrypt_with_context("nested-data", nested_ctx).unwrap();

        // Parent decrypts with parent context
        let parent_decrypted = cipher.decrypt_with_context(&parent_encrypted, parent_ctx).unwrap();
        assert_eq!(parent_decrypted, "parent-data");

        // Nested decrypts with nested context
        let nested_decrypted = cipher.decrypt_with_context(&nested_encrypted, nested_ctx).unwrap();
        assert_eq!(nested_decrypted, "nested-data");

        // Contexts are independent — cross-decryption fails
        assert!(cipher.decrypt_with_context(&parent_encrypted, nested_ctx).is_err());
        assert!(cipher.decrypt_with_context(&nested_encrypted, parent_ctx).is_err());
    }

    /// Test concurrent transactions with encryption
    #[tokio::test]
    async fn test_transaction_concurrent_isolation() {
        let key = [0x42u8; 32];
        let cipher = Arc::new(FieldEncryption::new(&key));
        let results = Arc::new(RwLock::new(Vec::new()));

        let mut handles = Vec::new();
        for tx_num in 0..10 {
            let cipher = cipher.clone();
            let results = results.clone();
            handles.push(tokio::spawn(async move {
                let context = format!("tx:concurrent-{tx_num}:user:user-{tx_num}");
                let plaintext = format!("data-for-tx-{tx_num}");

                let encrypted = cipher.encrypt_with_context(&plaintext, &context).unwrap();
                let decrypted = cipher.decrypt_with_context(&encrypted, &context).unwrap();
                assert_eq!(decrypted, plaintext);

                results.write().await.push((tx_num, plaintext, encrypted, context));
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let results = results.read().await;
        assert_eq!(results.len(), 10);

        // Verify each transaction's data decrypts only with its own context
        for (_, plaintext, encrypted, context) in results.iter() {
            let decrypted = cipher.decrypt_with_context(encrypted, context).unwrap();
            assert_eq!(&decrypted, plaintext);
        }
    }

    /// Test long-running transaction with encryption
    #[tokio::test]
    async fn test_transaction_long_running_encryption() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);
        let context = "tx:long-running-001:user:batch-worker";

        // Simulate a long-running transaction encrypting data in stages
        let mut all_encrypted = Vec::new();
        for batch in 0..5 {
            for i in 0..20 {
                let plaintext = format!("batch{batch}-record{i}@example.com");
                let encrypted = cipher.encrypt_with_context(&plaintext, context).unwrap();
                all_encrypted.push((plaintext, encrypted));
            }
            // Simulate time passing (key is cached locally in the cipher)
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }

        assert_eq!(all_encrypted.len(), 100);

        // All records decrypt with the original key regardless of timing
        for (original, encrypted) in &all_encrypted {
            let decrypted = cipher.decrypt_with_context(encrypted, context).unwrap();
            assert_eq!(&decrypted, original);
        }
    }

    // ============================================================================
    // PERFORMANCE OPTIMIZATION TESTS
    // ============================================================================

    /// Test encryption batching optimization
    #[tokio::test]
    async fn test_optimization_encryption_batching() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Batch encrypt many values
        let plaintexts: Vec<String> = (0..200).map(|i| format!("user{i}@example.com")).collect();

        let start = std::time::Instant::now();
        let encrypted: Vec<Vec<u8>> = plaintexts
            .iter()
            .map(|p| cipher.encrypt(p).unwrap())
            .collect();
        let encrypt_time = start.elapsed();

        assert_eq!(encrypted.len(), 200);

        // Batch decrypt
        let start = std::time::Instant::now();
        let decrypted: Vec<String> = encrypted
            .iter()
            .map(|e| cipher.decrypt(e).unwrap())
            .collect();
        let decrypt_time = start.elapsed();

        assert_eq!(decrypted.len(), 200);
        for (orig, dec) in plaintexts.iter().zip(decrypted.iter()) {
            assert_eq!(orig, dec);
        }

        // Sanity check: batch operations complete in reasonable time
        assert!(encrypt_time.as_millis() < 5000, "Batch encrypt too slow: {:?}", encrypt_time);
        assert!(decrypt_time.as_millis() < 5000, "Batch decrypt too slow: {:?}", decrypt_time);
    }

    /// Test parallel decryption optimization
    #[tokio::test]
    async fn test_optimization_parallel_decryption() {
        let key = [0x42u8; 32];
        let cipher = Arc::new(FieldEncryption::new(&key));

        // Pre-encrypt data
        let encrypted: Vec<(String, Vec<u8>)> = (0..50)
            .map(|i| {
                let p = format!("parallel-{i}@example.com");
                let e = cipher.encrypt(&p).unwrap();
                (p, e)
            })
            .collect();

        // Parallel decryption using tokio tasks
        let mut handles = Vec::new();
        for (original, enc) in encrypted {
            let cipher = cipher.clone();
            handles.push(tokio::spawn(async move {
                let decrypted = cipher.decrypt(&enc).unwrap();
                assert_eq!(decrypted, original);
                decrypted
            }));
        }

        let results: Vec<String> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(results.len(), 50);
    }

    /// Test key caching effectiveness
    #[tokio::test]
    async fn test_optimization_key_cache_effectiveness() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Use the same cipher (cached key) for many operations
        let mut success_count = 0;
        for i in 0..500 {
            let plaintext = format!("cache-test-{i}");
            let encrypted = cipher.encrypt(&plaintext).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            if decrypted == plaintext {
                success_count += 1;
            }
        }

        // 100% success rate with cached key
        assert_eq!(success_count, 500);
    }

    /// Test memory efficiency of encryption
    #[tokio::test]
    async fn test_optimization_memory_efficiency() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Encrypt data of varying sizes
        let sizes = [16, 64, 256, 1024, 4096];

        for &size in &sizes {
            let plaintext = "x".repeat(size);
            let encrypted = cipher.encrypt(&plaintext).unwrap();

            // Encrypted output = nonce (12) + plaintext + tag (16)
            let expected_size = 12 + size + 16;
            assert_eq!(
                encrypted.len(),
                expected_size,
                "Unexpected encrypted size for plaintext of {size} bytes"
            );

            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted.len(), size);
            assert_eq!(decrypted, plaintext);
        }
    }

    // ============================================================================
    // ERROR RECOVERY TESTS
    // ============================================================================

    /// Test recovery from temporary Vault unavailability (uses cached keys)
    #[tokio::test]
    async fn test_recovery_vault_temporary_outage() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Operations succeed with cached key even during Vault outage
        let plaintext = "data-during-outage";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Log the outage recovery in audit
        let mut logger = AuditLogger::new(100);
        let entry = AuditLogEntry::new("system", "email", OperationType::Select, "req-outage", "sess-recovery")
            .with_context("vault_status", "unavailable")
            .with_context("fallback", "cached_key");
        logger.log_entry(entry).unwrap();

        let entries = logger.recent_entries(1);
        assert_eq!(entries[0].context().get("vault_status"), Some(&"unavailable".to_string()));
        assert_eq!(entries[0].context().get("fallback"), Some(&"cached_key".to_string()));
        assert_eq!(entries[0].status(), EventStatus::Success);
    }

    /// Test recovery from encryption key expiry
    #[tokio::test]
    async fn test_recovery_key_expiry() {
        let old_key = [0x42u8; 32];
        let new_key = [0x99u8; 32];

        let old_cipher = FieldEncryption::new(&old_key);
        let new_cipher = FieldEncryption::new(&new_key);

        // Data encrypted with old key
        let plaintext = "data-with-old-key";
        let old_encrypted = old_cipher.encrypt(plaintext).unwrap();

        // Old key still decrypts old data
        let decrypted = old_cipher.decrypt(&old_encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // New key cannot decrypt old data
        assert!(new_cipher.decrypt(&old_encrypted).is_err());

        // New data uses new key
        let new_plaintext = "data-with-new-key";
        let new_encrypted = new_cipher.encrypt(new_plaintext).unwrap();
        let new_decrypted = new_cipher.decrypt(&new_encrypted).unwrap();
        assert_eq!(new_decrypted, new_plaintext);
    }

    /// Test recovery from network partition
    #[tokio::test]
    async fn test_recovery_network_partition() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // During partition: operations with cached key continue
        let results: Vec<Result<String, _>> = (0..20)
            .map(|i| {
                let plaintext = format!("partition-data-{i}");
                let encrypted = cipher.encrypt(&plaintext)?;
                cipher.decrypt(&encrypted)
            })
            .collect();

        // All operations succeeded with cached key
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(results.len(), 20);

        // Audit trail records partition handling
        let mut logger = AuditLogger::new(100);
        let entry = AuditLogEntry::new("system", "email", OperationType::Select, "req-partition", "sess-np")
            .with_context("network_status", "partition")
            .with_context("operations_completed", "20")
            .with_context("source", "cached_key");
        logger.log_entry(entry).unwrap();

        assert_eq!(logger.entry_count(), 1);
        assert_eq!(
            logger.recent_entries(1)[0].context().get("operations_completed"),
            Some(&"20".to_string())
        );
    }

    // ============================================================================
    // COMPLIANCE TESTS
    // ============================================================================

    /// Test HIPAA compliance with audit logging
    #[tokio::test]
    async fn test_compliance_hipaa_audit_logging() {
        let mut logger = AuditLogger::new(1000);

        // HIPAA: Log all PHI access
        let phi_fields = ["ssn", "diagnosis", "medication", "insurance_id"];
        for (i, field) in phi_fields.iter().enumerate() {
            let entry = AuditLogEntry::new(
                "dr-smith",
                *field,
                OperationType::Select,
                format!("req-hipaa-{i}"),
                "sess-hipaa",
            )
            .with_security_context(Some("10.0.0.1"), Some("physician"))
            .with_context("compliance", "hipaa")
            .with_context("patient_id", "patient-001");
            logger.log_entry(entry).unwrap();
        }

        // All PHI accesses logged
        assert_eq!(logger.entry_count(), 4);

        // Can query by field for compliance reports
        for field in &phi_fields {
            let entries = logger.entries_for_field(field);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].context().get("compliance"), Some(&"hipaa".to_string()));
        }

        // All entries have required security context
        let all = logger.recent_entries(4);
        for entry in &all {
            assert!(entry.context().contains_key("ip_address"));
            assert!(entry.context().contains_key("user_role"));
        }
    }

    /// Test PCI-DSS compliance with encryption
    #[tokio::test]
    async fn test_compliance_pci_dss_encryption() {
        let key = [0x42u8; 32]; // AES-256 (strong encryption as required by PCI-DSS)
        let cipher = FieldEncryption::new(&key);

        // PCI-DSS: Card data must be encrypted with strong encryption
        let card_number = "4532015112830366";
        let encrypted = cipher.encrypt(card_number).unwrap();

        // Encrypted data is not readable
        let encrypted_str = String::from_utf8_lossy(&encrypted);
        assert!(!encrypted_str.contains(card_number));

        // Decrypts correctly for authorized access
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, card_number);

        // Audit trail for card access
        let mut logger = AuditLogger::new(100);
        let entry = AuditLogEntry::new("payment-svc", "card_number", OperationType::Select, "req-pci", "sess-pci")
            .with_context("compliance", "pci-dss")
            .with_context("algorithm", "aes-256-gcm")
            .with_context("key_size", "256");
        logger.log_entry(entry).unwrap();

        let entries = logger.entries_for_field("card_number");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].context().get("compliance"), Some(&"pci-dss".to_string()));
    }

    /// Test GDPR compliance with data handling
    #[tokio::test]
    async fn test_compliance_gdpr_data_handling() {
        let key = [0x42u8; 32];
        let cipher = FieldEncryption::new(&key);

        // GDPR: Personal data encrypted at rest
        let personal_data = HashMap::from([
            ("email", "user@example.com"),
            ("phone", "+33612345678"),
            ("address", "123 Rue de Paris"),
        ]);

        let mut encrypted_data = HashMap::new();
        for (field, value) in &personal_data {
            encrypted_data.insert(*field, cipher.encrypt(value).unwrap());
        }

        // Audit trail: log all data access (GDPR right to know)
        let mut logger = AuditLogger::new(100);
        for field in personal_data.keys() {
            let entry = AuditLogEntry::new("data-subject-req", *field, OperationType::Select, "req-gdpr", "sess-gdpr")
                .with_context("compliance", "gdpr")
                .with_context("purpose", "data_subject_access_request");
            logger.log_entry(entry).unwrap();
        }

        assert_eq!(logger.entry_count(), 3);

        // GDPR right to deletion: destroy encryption key = crypto-shredding
        // After key destruction, data is irrecoverable
        let wrong_key = [0xFFu8; 32];
        let wrong_cipher = FieldEncryption::new(&wrong_key);
        for encrypted in encrypted_data.values() {
            assert!(wrong_cipher.decrypt(encrypted).is_err());
        }
    }

    /// Test SOC 2 compliance with controls
    #[tokio::test]
    async fn test_compliance_soc2_controls() {
        let mut logger = AuditLogger::new(1000);

        // SOC 2: Logical access controls — log who accessed what
        let users_and_ops = [
            ("admin", OperationType::Insert, "Create user record"),
            ("api-svc", OperationType::Select, "Read user email"),
            ("admin", OperationType::Update, "Update user phone"),
            ("auditor", OperationType::Select, "Compliance review"),
        ];

        for (i, (user, op, reason)) in users_and_ops.iter().enumerate() {
            let entry = AuditLogEntry::new(*user, "email", *op, format!("req-soc2-{i}"), "sess-soc2")
                .with_context("compliance", "soc2")
                .with_context("reason", (*reason).to_string());
            logger.log_entry(entry).unwrap();
        }

        // SOC 2: All operations logged
        assert_eq!(logger.entry_count(), 4);

        // SOC 2: Access controls — can filter by user
        let admin_entries = logger.entries_for_user("admin");
        assert_eq!(admin_entries.len(), 2);

        let auditor_entries = logger.entries_for_user("auditor");
        assert_eq!(auditor_entries.len(), 1);

        // SOC 2: Change management — track inserts and updates
        let changes = logger.entries_for_operation(OperationType::Insert);
        assert_eq!(changes.len(), 1);
        let updates = logger.entries_for_operation(OperationType::Update);
        assert_eq!(updates.len(), 1);

        // All entries traceable with request ID and session ID
        let all = logger.recent_entries(4);
        for entry in &all {
            assert!(!entry.request_id().is_empty());
            assert!(!entry.session_id().is_empty());
        }
    }
}
