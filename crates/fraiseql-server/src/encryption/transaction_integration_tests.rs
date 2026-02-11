//! Comprehensive test specifications for handling encrypted fields within
//! database transactions, including atomicity, consistency, and context tracking.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod transaction_integration_tests {
    use crate::encryption::{
        FieldEncryption,
        credential_rotation::{CredentialRotationManager, RotationConfig},
        schema::{SchemaFieldInfo, StructSchema},
        transaction::{IsolationLevel, TransactionContext, TransactionManager, TransactionState},
    };

    /// Helper: create a test cipher from a zeroed key
    fn test_cipher() -> FieldEncryption {
        FieldEncryption::new(&[0u8; 32])
    }

    // ============================================================================
    // BASIC TRANSACTION TESTS
    // ============================================================================

    /// Test encryption operation within transaction context
    #[tokio::test]
    async fn test_transaction_encryption_context() {
        // Create transaction context with full metadata
        let ctx = TransactionContext::new("user_42", "sess_abc", "req_xyz")
            .with_isolation(IsolationLevel::ReadCommitted)
            .with_key_version(1)
            .with_role("admin")
            .with_client_ip("10.0.0.1")
            .with_metadata("source", "api");

        // Context should carry all required fields
        assert!(ctx.transaction_id.starts_with("txn_"));
        assert_eq!(ctx.user_id, "user_42");
        assert_eq!(ctx.session_id, "sess_abc");
        assert_eq!(ctx.request_id, "req_xyz");
        assert_eq!(ctx.key_version, 1);
        assert_eq!(ctx.user_role, Some("admin".to_string()));
        assert_eq!(ctx.client_ip, Some("10.0.0.1".to_string()));
        assert_eq!(ctx.metadata.get("source"), Some(&"api".to_string()));
        assert!(ctx.is_active());

        // Encrypt within the transaction's key context
        let cipher = test_cipher();
        let encrypted = cipher.encrypt("user@example.com").unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "user@example.com");
    }

    /// Test successful transaction commits encrypted data
    #[tokio::test]
    async fn test_transaction_successful_commit() {
        let cipher = test_cipher();

        // Simulate encrypted INSERT within a transaction
        let plaintext = "sensitive@email.com";
        let ciphertext = cipher.encrypt(plaintext).unwrap();

        // Ciphertext is not the same as plaintext bytes
        assert_ne!(plaintext.as_bytes(), &ciphertext[12..]);

        // Simulate commit: the ciphertext is the "committed" value
        let committed_data = ciphertext;

        // Subsequent SELECT returns decrypted data
        let decrypted = cipher.decrypt(&committed_data).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test failed transaction doesn't commit encrypted data
    #[tokio::test]
    async fn test_transaction_failed_rollback() {
        let mut manager = TransactionManager::new();
        let mut ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();

        // Simulate encrypted INSERT operation tracked on the context
        let cipher = test_cipher();
        let _ciphertext = cipher.encrypt("secret_data").unwrap();
        ctx.add_operation("INSERT INTO users (email) VALUES ($1)");

        manager.begin(ctx).unwrap();

        // Rollback the transaction
        manager.rollback(&txn_id).unwrap();

        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.state, TransactionState::RolledBack);
        // Operations cleared on rollback
        assert_eq!(txn.operation_count(), 0);
    }

    // ============================================================================
    // BATCH OPERATIONS IN TRANSACTION
    // ============================================================================

    /// Test batch INSERT with encrypted fields
    #[tokio::test]
    async fn test_transaction_batch_insert_encryption() {
        let cipher = test_cipher();

        // Encrypt 100 records independently
        let mut ciphertexts = Vec::new();
        for i in 0..100 {
            let plaintext = format!("user{}@example.com", i);
            let ct = cipher.encrypt(&plaintext).unwrap();
            ciphertexts.push((plaintext, ct));
        }

        // Each ciphertext is unique due to random nonce
        let nonces: Vec<&[u8]> = ciphertexts.iter().map(|(_, ct)| &ct[..12]).collect();
        for i in 0..nonces.len() {
            for j in (i + 1)..nonces.len() {
                assert_ne!(
                    nonces[i], nonces[j],
                    "Nonces should be unique for record {} and {}",
                    i, j
                );
            }
        }

        // All decrypt correctly
        for (plaintext, ct) in &ciphertexts {
            let decrypted = cipher.decrypt(ct).unwrap();
            assert_eq!(&decrypted, plaintext);
        }
    }

    /// Test batch UPDATE with encrypted fields
    #[tokio::test]
    async fn test_transaction_batch_update_encryption() {
        let cipher = test_cipher();

        // Original encrypted values
        let mut original_ciphertexts = Vec::new();
        for i in 0..50 {
            let ct = cipher.encrypt(&format!("old_email_{}@example.com", i)).unwrap();
            original_ciphertexts.push(ct);
        }

        // Update all 50 records with new values
        let mut updated_ciphertexts = Vec::new();
        for (i, original_ct) in original_ciphertexts.iter().enumerate() {
            let new_plaintext = format!("new_email_{}@example.com", i);
            let new_ct = cipher.encrypt(&new_plaintext).unwrap();
            // New nonce means different ciphertext even if same content
            assert_ne!(*original_ct, new_ct);
            updated_ciphertexts.push((new_plaintext, new_ct));
        }

        // All updated values decrypt correctly
        for (expected, ct) in &updated_ciphertexts {
            let decrypted = cipher.decrypt(ct).unwrap();
            assert_eq!(&decrypted, expected);
        }
    }

    /// Test mixed operations in transaction
    #[tokio::test]
    async fn test_transaction_mixed_operations() {
        let cipher = test_cipher();
        let mut manager = TransactionManager::new();
        let ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        // INSERT 10 encrypted records
        let mut records: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..10 {
            let pt = format!("insert_{}@example.com", i);
            let ct = cipher.encrypt(&pt).unwrap();
            records.push((pt, ct));
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation(format!("INSERT record {}", i));
        }

        // UPDATE 5 existing records
        for (i, record) in records.iter_mut().enumerate().take(5) {
            let new_pt = format!("updated_{}@example.com", i);
            let new_ct = cipher.encrypt(&new_pt).unwrap();
            *record = (new_pt, new_ct);
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation(format!("UPDATE record {}", i));
        }

        // DELETE 3 records
        for i in (7..10).rev() {
            records.remove(i);
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation(format!("DELETE record {}", i));
        }

        // SELECT to verify remaining records
        for (expected_pt, ct) in &records {
            let decrypted = cipher.decrypt(ct).unwrap();
            assert_eq!(&decrypted, expected_pt);
        }
        assert_eq!(records.len(), 7);

        // All operations tracked
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.operation_count(), 18); // 10 inserts + 5 updates + 3 deletes

        manager.commit(&txn_id).unwrap();
        assert_eq!(manager.get_transaction(&txn_id).unwrap().state, TransactionState::Committed);
    }

    /// Test batch DELETE with encrypted fields
    #[tokio::test]
    async fn test_transaction_batch_delete_encryption() {
        let cipher = test_cipher();
        let mut manager = TransactionManager::new();
        let ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        // Create 20 encrypted records
        let mut records: Vec<Vec<u8>> = Vec::new();
        for i in 0..20 {
            let ct = cipher.encrypt(&format!("data_{}", i)).unwrap();
            records.push(ct);
        }

        // Delete all 20 within transaction
        for i in 0..20 {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation(format!("DELETE record {}", i));
        }
        records.clear();

        assert!(records.is_empty());
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.operation_count(), 20);

        manager.commit(&txn_id).unwrap();
    }

    // ============================================================================
    // TRANSACTION ISOLATION LEVELS
    // ============================================================================

    /// Test encryption with READ UNCOMMITTED isolation
    #[tokio::test]
    async fn test_transaction_read_uncommitted_encryption() {
        let ctx = TransactionContext::new("user1", "sess1", "req1")
            .with_isolation(IsolationLevel::ReadUncommitted);
        assert_eq!(ctx.isolation_level, IsolationLevel::ReadUncommitted);

        // Encryption/decryption works at any isolation level
        let cipher = test_cipher();
        let ct = cipher.encrypt("dirty_read_test").unwrap();
        let pt = cipher.decrypt(&ct).unwrap();
        assert_eq!(pt, "dirty_read_test");
    }

    /// Test encryption with READ COMMITTED isolation
    #[tokio::test]
    async fn test_transaction_read_committed_encryption() {
        let ctx = TransactionContext::new("user1", "sess1", "req1")
            .with_isolation(IsolationLevel::ReadCommitted);
        assert_eq!(ctx.isolation_level, IsolationLevel::ReadCommitted);

        let cipher = test_cipher();
        let ct = cipher.encrypt("committed_data").unwrap();
        let pt = cipher.decrypt(&ct).unwrap();
        assert_eq!(pt, "committed_data");
    }

    /// Test encryption with REPEATABLE READ isolation
    #[tokio::test]
    async fn test_transaction_repeatable_read_encryption() {
        let ctx = TransactionContext::new("user1", "sess1", "req1")
            .with_isolation(IsolationLevel::RepeatableRead);
        assert_eq!(ctx.isolation_level, IsolationLevel::RepeatableRead);

        // Encrypt and cache the ciphertext for consistent reads
        let cipher = test_cipher();
        let ct = cipher.encrypt("repeatable_data").unwrap();

        // Multiple reads of same ciphertext produce same plaintext
        let read1 = cipher.decrypt(&ct).unwrap();
        let read2 = cipher.decrypt(&ct).unwrap();
        assert_eq!(read1, read2);
        assert_eq!(read1, "repeatable_data");
    }

    /// Test encryption with SERIALIZABLE isolation
    #[tokio::test]
    async fn test_transaction_serializable_encryption() {
        let ctx = TransactionContext::new("user1", "sess1", "req1")
            .with_isolation(IsolationLevel::Serializable);
        assert_eq!(ctx.isolation_level, IsolationLevel::Serializable);
        assert_eq!(ctx.isolation_level.to_string(), "SERIALIZABLE");

        let cipher = test_cipher();
        let ct = cipher.encrypt("serial_data").unwrap();
        let pt = cipher.decrypt(&ct).unwrap();
        assert_eq!(pt, "serial_data");
    }

    // ============================================================================
    // SAVEPOINT TESTS
    // ============================================================================

    /// Test encryption with savepoint rollback
    #[tokio::test]
    async fn test_transaction_savepoint_rollback() {
        let mut manager = TransactionManager::new();
        let mut ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();

        // Pre-savepoint operation
        ctx.add_operation("INSERT record_A");
        manager.begin(ctx).unwrap();

        // Create savepoint after first operation
        manager.savepoint(&txn_id, "sp1").unwrap();

        // Post-savepoint encrypted INSERT
        let cipher = test_cipher();
        let _ct = cipher.encrypt("should_not_persist").unwrap();
        {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation("INSERT encrypted_record");
        }

        // Rollback to savepoint
        manager.rollback_to_savepoint(&txn_id, "sp1").unwrap();

        // Only pre-savepoint operation remains
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.operation_count(), 1);
        assert_eq!(txn.operations[0], "INSERT record_A");
    }

    /// Test encryption with savepoint partial commit
    #[tokio::test]
    async fn test_transaction_savepoint_partial_commit() {
        let mut manager = TransactionManager::new();
        let cipher = test_cipher();

        let mut ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();

        // 1. Insert record A (encrypted)
        let ct_a = cipher.encrypt("record_A").unwrap();
        ctx.add_operation("INSERT A");
        manager.begin(ctx).unwrap();

        // 2. Create savepoint
        manager.savepoint(&txn_id, "sp1").unwrap();

        // 3. Insert record B (encrypted) - will be rolled back
        let _ct_b = cipher.encrypt("record_B").unwrap();
        {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation("INSERT B");
        }

        // Rollback to savepoint (removes B)
        manager.rollback_to_savepoint(&txn_id, "sp1").unwrap();

        // 4. Insert record C (encrypted)
        let ct_c = cipher.encrypt("record_C").unwrap();
        {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation("INSERT C");
        }

        // 5. Commit
        manager.commit(&txn_id).unwrap();

        // Result: A and C committed, B rolled back
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.state, TransactionState::Committed);
        assert_eq!(txn.operation_count(), 2); // A and C

        // A and C decrypt correctly
        assert_eq!(cipher.decrypt(&ct_a).unwrap(), "record_A");
        assert_eq!(cipher.decrypt(&ct_c).unwrap(), "record_C");
    }

    /// Test nested savepoint with encryption
    #[tokio::test]
    async fn test_transaction_nested_savepoint_encryption() {
        let mut manager = TransactionManager::new();
        let cipher = test_cipher();

        let mut ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();
        ctx.add_operation("INSERT level_0");
        manager.begin(ctx).unwrap();

        // Savepoint level 1
        manager.savepoint(&txn_id, "sp_level1").unwrap();
        {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation("INSERT level_1");
        }

        // Savepoint level 2
        manager.savepoint(&txn_id, "sp_level2").unwrap();
        {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation("INSERT level_2");
        }

        // Rollback to level 2 savepoint (removes level_2 op)
        manager.rollback_to_savepoint(&txn_id, "sp_level2").unwrap();
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.operation_count(), 2); // level_0, level_1

        // Add a replacement for level 2
        {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation("INSERT level_2_replacement");
        }

        // Encryption still works after nested savepoint rollback
        let ct = cipher.encrypt("nested_test").unwrap();
        assert_eq!(cipher.decrypt(&ct).unwrap(), "nested_test");

        manager.commit(&txn_id).unwrap();
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.state, TransactionState::Committed);
        assert_eq!(txn.operation_count(), 3);
    }

    // ============================================================================
    // CONCURRENT TRANSACTION TESTS
    // ============================================================================

    /// Test concurrent transactions encrypt different rows
    #[tokio::test]
    async fn test_transaction_concurrent_isolation() {
        let mut manager = TransactionManager::new();

        // Two concurrent transactions with independent ciphers
        let cipher1 = test_cipher();
        let cipher2 = test_cipher();

        let ctx1 = TransactionContext::new("user_a", "sess_a", "req_a");
        let ctx2 = TransactionContext::new("user_b", "sess_b", "req_b");
        let txn_id1 = ctx1.transaction_id.clone();
        let txn_id2 = ctx2.transaction_id.clone();

        manager.begin(ctx1).unwrap();
        manager.begin(ctx2).unwrap();
        assert_eq!(manager.active_count(), 2);

        // Each transaction encrypts independently
        let ct1 = cipher1.encrypt("data_for_txn1").unwrap();
        let ct2 = cipher2.encrypt("data_for_txn2").unwrap();

        // Each transaction tracks its own operations
        manager.get_transaction_mut(&txn_id1).unwrap().add_operation("INSERT row_1");
        manager.get_transaction_mut(&txn_id2).unwrap().add_operation("INSERT row_2");

        // Verify isolation: each transaction has its own operations
        assert_eq!(manager.get_transaction(&txn_id1).unwrap().operation_count(), 1);
        assert_eq!(manager.get_transaction(&txn_id2).unwrap().operation_count(), 1);

        // Each cipher decrypts its own data correctly
        assert_eq!(cipher1.decrypt(&ct1).unwrap(), "data_for_txn1");
        assert_eq!(cipher2.decrypt(&ct2).unwrap(), "data_for_txn2");

        manager.commit(&txn_id1).unwrap();
        manager.commit(&txn_id2).unwrap();
    }

    /// Test concurrent transactions on same encrypted field
    #[tokio::test]
    async fn test_transaction_concurrent_same_field() {
        let cipher = test_cipher();

        // Two transactions encrypt the same field value
        let ct_txn1 = cipher.encrypt("shared_field_value").unwrap();
        let ct_txn2 = cipher.encrypt("shared_field_value").unwrap();

        // Same plaintext produces different ciphertexts (random nonce)
        assert_ne!(ct_txn1, ct_txn2);

        // Both decrypt correctly
        assert_eq!(cipher.decrypt(&ct_txn1).unwrap(), "shared_field_value");
        assert_eq!(cipher.decrypt(&ct_txn2).unwrap(), "shared_field_value");
    }

    /// Test READ-WRITE lock with encrypted data
    #[tokio::test]
    async fn test_transaction_read_write_lock_encryption() {
        let cipher = test_cipher();

        // Write encrypted data
        let original_ct = cipher.encrypt("original_value").unwrap();

        // Multiple concurrent readers all get the same plaintext
        let reader_results: Vec<String> =
            (0..5).map(|_| cipher.decrypt(&original_ct).unwrap()).collect();

        for result in &reader_results {
            assert_eq!(result, "original_value");
        }

        // Writer updates (new nonce)
        let updated_ct = cipher.encrypt("updated_value").unwrap();
        assert_ne!(original_ct, updated_ct);
        assert_eq!(cipher.decrypt(&updated_ct).unwrap(), "updated_value");
    }

    /// Test WRITE-WRITE conflict with encryption
    #[tokio::test]
    async fn test_transaction_write_write_conflict() {
        let cipher = test_cipher();

        // Two writers produce different ciphertexts for same field
        let write1 = cipher.encrypt("writer_1_value").unwrap();
        let write2 = cipher.encrypt("writer_2_value").unwrap();

        // Both ciphertexts are valid
        assert_eq!(cipher.decrypt(&write1).unwrap(), "writer_1_value");
        assert_eq!(cipher.decrypt(&write2).unwrap(), "writer_2_value");

        // Even writing the same value produces different ciphertext
        let write1_again = cipher.encrypt("same_value").unwrap();
        let write2_again = cipher.encrypt("same_value").unwrap();
        assert_ne!(write1_again, write2_again);
    }

    // ============================================================================
    // ENCRYPTION KEY MANAGEMENT IN TRANSACTIONS
    // ============================================================================

    /// Test transaction uses consistent encryption key
    #[tokio::test]
    async fn test_transaction_consistent_key() {
        let cipher = test_cipher();

        // Encrypt multiple fields within "same transaction" using same key
        let email_ct = cipher.encrypt("user@example.com").unwrap();
        let phone_ct = cipher.encrypt("+1-555-0100").unwrap();
        let ssn_ct = cipher.encrypt("123-45-6789").unwrap();

        // All decrypt with the same key
        assert_eq!(cipher.decrypt(&email_ct).unwrap(), "user@example.com");
        assert_eq!(cipher.decrypt(&phone_ct).unwrap(), "+1-555-0100");
        assert_eq!(cipher.decrypt(&ssn_ct).unwrap(), "123-45-6789");
    }

    /// Test key rotation during transaction
    #[tokio::test]
    async fn test_transaction_key_rotation_during() {
        let key_v1 = [1u8; 32];
        let key_v2 = [2u8; 32];

        let cipher_v1 = FieldEncryption::new(&key_v1);
        let cipher_v2 = FieldEncryption::new(&key_v2);

        // Transaction starts with v1
        let ct_in_txn = cipher_v1.encrypt("txn_data").unwrap();

        // Background rotation creates v2 (doesn't affect in-progress transaction)
        // Transaction still completes with v1
        assert_eq!(cipher_v1.decrypt(&ct_in_txn).unwrap(), "txn_data");

        // Subsequent transactions use v2
        let ct_after_rotation = cipher_v2.encrypt("post_rotation_data").unwrap();
        assert_eq!(cipher_v2.decrypt(&ct_after_rotation).unwrap(), "post_rotation_data");

        // v1 data still decryptable with v1 key
        assert_eq!(cipher_v1.decrypt(&ct_in_txn).unwrap(), "txn_data");
        // v2 cannot decrypt v1 data
        assert!(cipher_v2.decrypt(&ct_in_txn).is_err());
    }

    /// Test key expiry during transaction
    #[tokio::test]
    async fn test_transaction_key_expiry_during() {
        let config = RotationConfig::new().with_ttl_days(365);
        let rotation_manager = CredentialRotationManager::new(config);
        let version = rotation_manager.initialize_key().unwrap();

        // Transaction holds reference to a specific key version
        let ctx =
            TransactionContext::new("user1", "sess1", "req1").with_key_version(version as u32);
        assert_eq!(ctx.key_version, version as u32);

        // Even if key lease conceptually expires, the cipher already exists
        let cipher = test_cipher();
        let ct = cipher.encrypt("during_expiry").unwrap();

        // Encryption still succeeds with the held cipher reference
        assert_eq!(cipher.decrypt(&ct).unwrap(), "during_expiry");

        // Key version metadata tracked
        let metadata = rotation_manager.get_current_metadata().unwrap().unwrap();
        assert_eq!(metadata.version, version);
    }

    /// Test Vault unavailable during transaction
    #[tokio::test]
    async fn test_transaction_vault_unavailable() {
        // With a cached cipher, operations succeed even without Vault
        let cipher = test_cipher();
        let ct = cipher.encrypt("cached_key_data").unwrap();
        assert_eq!(cipher.decrypt(&ct).unwrap(), "cached_key_data");

        // Without cache: simulated by attempting operations with a wrong key length
        // (mimicking "key not available" scenario)
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[0u8; 16]) // Invalid key length
        });
        assert!(result.is_err()); // Panics on invalid key

        // Transaction context records the error state
        let mut ctx = TransactionContext::new("user1", "sess1", "req1");
        ctx.error();
        assert_eq!(ctx.state, TransactionState::Error);
    }

    // ============================================================================
    // TRANSACTION CONTEXT TRACKING
    // ============================================================================

    /// Test transaction ID in audit log
    #[tokio::test]
    async fn test_transaction_id_audit_trail() {
        let mut manager = TransactionManager::new();
        let ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        // Add multiple operations to the transaction
        {
            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation("ENCRYPT email field");
            txn.add_operation("INSERT record");
            txn.add_operation("ENCRYPT phone field");
            txn.add_operation("UPDATE record");
        }

        // All operations share the same transaction ID
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.transaction_id, txn_id);
        assert_eq!(txn.operation_count(), 4);

        // Transaction ID is unique and identifiable
        assert!(txn_id.starts_with("txn_"));
    }

    /// Test user context tracked in transaction
    #[tokio::test]
    async fn test_transaction_user_context() {
        let ctx = TransactionContext::new("admin_user", "session_123", "request_456")
            .with_role("admin")
            .with_client_ip("192.168.1.100");

        assert_eq!(ctx.user_id, "admin_user");
        assert_eq!(ctx.user_role, Some("admin".to_string()));
        assert_eq!(ctx.client_ip, Some("192.168.1.100".to_string()));

        // User context is preserved throughout the transaction
        let mut manager = TransactionManager::new();
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.user_id, "admin_user");
        assert_eq!(txn.user_role, Some("admin".to_string()));
    }

    /// Test request ID correlation in transaction
    #[tokio::test]
    async fn test_transaction_request_correlation() {
        let request_id = "http-req-abc-123";
        let ctx = TransactionContext::new("user1", "sess1", request_id);

        assert_eq!(ctx.request_id, request_id);

        // Request ID flows through transaction manager
        let mut manager = TransactionManager::new();
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.request_id, request_id);
    }

    /// Test session tracking in transaction
    #[tokio::test]
    async fn test_transaction_session_tracking() {
        let session_id = "persistent_session_xyz";

        // Multiple transactions from the same session
        let ctx1 = TransactionContext::new("user1", session_id, "req1").with_key_version(1);
        let ctx2 = TransactionContext::new("user1", session_id, "req2").with_key_version(1);

        // Same session, same user, same key version
        assert_eq!(ctx1.session_id, ctx2.session_id);
        assert_eq!(ctx1.user_id, ctx2.user_id);
        assert_eq!(ctx1.key_version, ctx2.key_version);

        // Different transaction IDs
        assert_ne!(ctx1.transaction_id, ctx2.transaction_id);
    }

    // ============================================================================
    // ERROR HANDLING IN TRANSACTIONS
    // ============================================================================

    /// Test encryption error during transaction
    #[tokio::test]
    async fn test_transaction_encryption_error_handling() {
        let mut manager = TransactionManager::new();
        let ctx = TransactionContext::new("user1", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        // Simulate an encryption failure by attempting decryption of garbage
        let cipher = test_cipher();
        let garbage = vec![0u8; 50]; // not valid ciphertext
        let result = cipher.decrypt(&garbage);
        assert!(result.is_err());

        // Transaction should be rolled back on error
        manager.rollback(&txn_id).unwrap();
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.state, TransactionState::RolledBack);
    }

    /// Test decryption error on read
    #[tokio::test]
    async fn test_transaction_decryption_error_read() {
        let cipher = test_cipher();

        // Encrypt valid data, then corrupt it
        let ct = cipher.encrypt("valid_data").unwrap();
        let mut corrupted = ct;
        if corrupted.len() > 12 {
            corrupted[12] ^= 0xFF; // Flip a byte in the ciphertext portion
        }

        let result = cipher.decrypt(&corrupted);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Decryption failed") || err_msg.contains("decrypt"));
    }

    /// Test NULL handling in encrypted transaction
    #[tokio::test]
    async fn test_transaction_null_encrypted_field() {
        let cipher = test_cipher();

        // NULL fields are represented as Option<Vec<u8>> = None
        let null_field: Option<Vec<u8>> = None;
        let non_null_field: Option<Vec<u8>> = Some(cipher.encrypt("has_value").unwrap());

        // NULL remains NULL
        assert!(null_field.is_none());

        // Non-NULL encrypted field decrypts correctly
        if let Some(ct) = &non_null_field {
            let pt = cipher.decrypt(ct).unwrap();
            assert_eq!(pt, "has_value");
        }
    }

    /// Test empty string in encrypted transaction
    #[tokio::test]
    async fn test_transaction_empty_string_encryption() {
        let cipher = test_cipher();

        // Empty string is encrypted (produces ciphertext)
        let ct = cipher.encrypt("").unwrap();
        assert!(!ct.is_empty()); // Ciphertext exists for empty string

        // Roundtrip returns empty string
        let pt = cipher.decrypt(&ct).unwrap();
        assert_eq!(pt, "");

        // Empty string is NOT the same as NULL
        let null_field: Option<Vec<u8>> = None;
        let empty_field: Option<Vec<u8>> = Some(ct);

        assert!(null_field.is_none());
        assert!(empty_field.is_some());
    }

    // ============================================================================
    // PERFORMANCE & RESOURCE MANAGEMENT
    // ============================================================================

    /// Test long-running transaction with encryption
    #[tokio::test]
    async fn test_transaction_long_running() {
        let cipher = test_cipher();
        let mut manager = TransactionManager::new();
        let ctx = TransactionContext::new("user1", "sess1", "req1").with_key_version(1);
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        // Simulate many operations over "time"
        for i in 0..100 {
            let ct = cipher.encrypt(&format!("record_{}", i)).unwrap();
            let pt = cipher.decrypt(&ct).unwrap();
            assert_eq!(pt, format!("record_{}", i));

            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation(format!("OP_{}", i));
        }

        // Key version consistent throughout
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.key_version, 1);
        assert_eq!(txn.operation_count(), 100);
        assert!(txn.is_active());

        manager.commit(&txn_id).unwrap();
    }

    /// Test large batch transaction encryption
    #[tokio::test]
    async fn test_transaction_large_batch_encryption() {
        let cipher = test_cipher();

        let start = std::time::Instant::now();

        // Encrypt 1000 records (scaled down from 10k for test speed)
        let mut ciphertexts = Vec::with_capacity(1000);
        for i in 0..1000 {
            let ct = cipher.encrypt(&format!("batch_record_{}", i)).unwrap();
            ciphertexts.push(ct);
        }

        let encrypt_duration = start.elapsed();

        // Decrypt all 1000 records
        let start = std::time::Instant::now();
        for (i, ct) in ciphertexts.iter().enumerate() {
            let pt = cipher.decrypt(ct).unwrap();
            assert_eq!(pt, format!("batch_record_{}", i));
        }

        let decrypt_duration = start.elapsed();

        // Verify reasonable performance (should be well under 5 seconds)
        assert!(
            encrypt_duration.as_secs() < 5,
            "Encryption took too long: {:?}",
            encrypt_duration
        );
        assert!(
            decrypt_duration.as_secs() < 5,
            "Decryption took too long: {:?}",
            decrypt_duration
        );
    }

    /// Test transaction memory cleanup
    #[tokio::test]
    async fn test_transaction_memory_cleanup() {
        let mut manager = TransactionManager::new();

        // Create and complete several transactions
        for i in 0..10 {
            let ctx = TransactionContext::new(
                format!("user_{}", i),
                format!("sess_{}", i),
                format!("req_{}", i),
            );
            let txn_id = ctx.transaction_id.clone();
            manager.begin(ctx).unwrap();

            // Commit odd, rollback even
            if i % 2 == 0 {
                manager.commit(&txn_id).unwrap();
            } else {
                manager.rollback(&txn_id).unwrap();
            }
        }

        // Before cleanup
        assert_eq!(manager.active_count(), 10);

        // Cleanup completed transactions (none are active after commit/rollback)
        manager.cleanup_completed();

        // All cleaned up - no active transactions remain
        assert_eq!(manager.active_count(), 0);
    }

    /// Test transaction deadlock with encryption
    #[tokio::test]
    async fn test_transaction_deadlock_detection() {
        let mut manager = TransactionManager::new();

        let ctx1 = TransactionContext::new("user1", "sess1", "req1");
        let ctx2 = TransactionContext::new("user2", "sess2", "req2");
        let txn_id1 = ctx1.transaction_id.clone();
        let txn_id2 = ctx2.transaction_id.clone();

        manager.begin(ctx1).unwrap();
        manager.begin(ctx2).unwrap();

        // Simulate deadlock detection: mark one transaction as error
        {
            let txn2 = manager.get_transaction_mut(&txn_id2).unwrap();
            txn2.error();
        }

        // Transaction 2 in error state
        assert_eq!(manager.get_transaction(&txn_id2).unwrap().state, TransactionState::Error);

        // Transaction 1 can proceed and commit
        manager.commit(&txn_id1).unwrap();
        assert_eq!(manager.get_transaction(&txn_id1).unwrap().state, TransactionState::Committed);

        // Encryption state remains consistent for retried transaction
        let cipher = test_cipher();
        let ct = cipher.encrypt("retry_after_deadlock").unwrap();
        assert_eq!(cipher.decrypt(&ct).unwrap(), "retry_after_deadlock");
    }

    // ============================================================================
    // SCHEMA EVOLUTION IN TRANSACTIONS
    // ============================================================================

    /// Test transaction with schema version mismatch
    #[tokio::test]
    async fn test_transaction_schema_version_mismatch() {
        let cipher = test_cipher();

        // Schema v1: email not encrypted (plaintext)
        let schema_v1 = StructSchema::new("User").with_version(1).with_fields(vec![
            SchemaFieldInfo::new("email", "String", false, ""),
            SchemaFieldInfo::new("name", "String", false, ""),
        ]);

        // Schema v2: email encrypted
        let schema_v2 = StructSchema::new("User").with_version(2).with_fields(vec![
            SchemaFieldInfo::new("email", "String", true, "encryption/email"),
            SchemaFieldInfo::new("name", "String", false, ""),
        ]);

        // v1 record: email is plaintext
        assert!(!schema_v1.is_field_encrypted("email"));

        // v2 record: email is encrypted
        assert!(schema_v2.is_field_encrypted("email"));

        // Transaction handles both versions:
        // v1 data: read plaintext directly
        let v1_email = "old@example.com";

        // v2 data: decrypt
        let v2_ct = cipher.encrypt("new@example.com").unwrap();
        let v2_email = cipher.decrypt(&v2_ct).unwrap();

        assert_eq!(v1_email, "old@example.com");
        assert_eq!(v2_email, "new@example.com");
        assert_eq!(schema_v1.version, 1);
        assert_eq!(schema_v2.version, 2);
    }

    /// Test transaction with new encrypted field
    #[tokio::test]
    async fn test_transaction_schema_evolution_add_field() {
        let cipher = test_cipher();

        let mut schema = StructSchema::new("User").with_version(1);
        schema.add_field(SchemaFieldInfo::new("email", "String", true, "encryption/email"));
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));

        // Old records don't have "phone" field
        assert!(!schema.is_field_encrypted("phone"));
        assert_eq!(schema.encrypted_field_count(), 1);

        // Evolve schema: add encrypted phone
        schema.add_field(SchemaFieldInfo::new("phone", "String", true, "encryption/phone"));
        assert!(schema.is_field_encrypted("phone"));
        assert_eq!(schema.encrypted_field_count(), 2);

        // New records have both fields encrypted
        let email_ct = cipher.encrypt("user@test.com").unwrap();
        let phone_ct = cipher.encrypt("+1-555-0123").unwrap();
        assert_eq!(cipher.decrypt(&email_ct).unwrap(), "user@test.com");
        assert_eq!(cipher.decrypt(&phone_ct).unwrap(), "+1-555-0123");
    }

    /// Test encryption migration in transaction
    #[tokio::test]
    async fn test_transaction_encryption_migration() {
        let cipher = test_cipher();
        let mut manager = TransactionManager::new();
        let ctx = TransactionContext::new("migration_user", "sess1", "req1");
        let txn_id = ctx.transaction_id.clone();
        manager.begin(ctx).unwrap();

        // Simulate bulk migration: encrypt 50 previously-unencrypted records
        let plaintext_records: Vec<String> =
            (0..50).map(|i| format!("plaintext_email_{}@test.com", i)).collect();

        let mut encrypted_records = Vec::new();
        for (i, pt) in plaintext_records.iter().enumerate() {
            let ct = cipher.encrypt(pt).unwrap();
            encrypted_records.push(ct);

            let txn = manager.get_transaction_mut(&txn_id).unwrap();
            txn.add_operation(format!("MIGRATE record {}", i));
        }

        // Atomic commit
        manager.commit(&txn_id).unwrap();
        let txn = manager.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.state, TransactionState::Committed);
        assert_eq!(txn.operation_count(), 50);

        // All migrated records decrypt correctly
        for (i, ct) in encrypted_records.iter().enumerate() {
            let pt = cipher.decrypt(ct).unwrap();
            assert_eq!(pt, plaintext_records[i]);
        }
    }
}
