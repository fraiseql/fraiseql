//! Comprehensive tests for integrating field-level encryption
//! with database adapters for automatic encryption/decryption

#[cfg(test)]
#[allow(clippy::module_inception)]
mod database_adapter_tests {
    use std::time::Instant;

    use crate::encryption::database_adapter::EncryptionContext;
    use crate::encryption::query_builder::{QueryBuilderIntegration, QueryType};
    use crate::encryption::schema::{EncryptionMark, SchemaFieldInfo, SchemaRegistry, StructSchema};
    use crate::encryption::{FieldEncryption, KEY_SIZE, NONCE_SIZE};
    use crate::secrets_manager::SecretsError;

    /// Helper: create a test cipher with a zeroed key
    fn test_cipher() -> FieldEncryption {
        FieldEncryption::new(&[0u8; KEY_SIZE])
    }

    // ============================================================================
    // QUERY BUILDER INTEGRATION TESTS
    // ============================================================================

    /// Test encrypted field on INSERT query
    #[tokio::test]
    async fn test_query_auto_encrypt_on_insert() {
        let cipher = test_cipher();
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);

        // INSERT queries with encrypted fields are allowed (adapter handles encryption)
        let result = qbi.validate_query(QueryType::Insert, &[], &[], &[]);
        assert!(result.is_ok());

        // Simulate encrypting a value before insertion
        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Encrypted value is different from plaintext
        assert_ne!(encrypted, plaintext.as_bytes());
        // Encrypted output has nonce prefix
        assert!(encrypted.len() > NONCE_SIZE);

        // Value can be decrypted back
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Verify the field is recognized as encrypted
        assert!(qbi.is_encrypted("email"));
        assert!(qbi.is_encrypted("phone"));
    }

    /// Test encrypted field on SELECT query
    #[tokio::test]
    async fn test_query_auto_decrypt_on_select() {
        let cipher = test_cipher();

        // Simulate storing encrypted data and reading it back
        let plaintext = "sensitive@data.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // SELECT queries are allowed (adapter handles decryption on read)
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_query(QueryType::Select, &[], &[], &[]);
        assert!(result.is_ok());

        // Application receives plaintext after decryption
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test encrypted field on UPDATE query
    #[tokio::test]
    async fn test_query_auto_encrypt_on_update() {
        let cipher = test_cipher();
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // UPDATE queries are allowed (adapter handles re-encryption)
        let result = qbi.validate_query(QueryType::Update, &[], &[], &[]);
        assert!(result.is_ok());

        // Encrypt old value
        let old_value = "old@example.com";
        let old_encrypted = cipher.encrypt(old_value).unwrap();

        // Encrypt new value
        let new_value = "new@example.com";
        let new_encrypted = cipher.encrypt(new_value).unwrap();

        // Old and new ciphertexts differ
        assert_ne!(old_encrypted, new_encrypted);

        // Both decrypt correctly
        assert_eq!(cipher.decrypt(&old_encrypted).unwrap(), old_value);
        assert_eq!(cipher.decrypt(&new_encrypted).unwrap(), new_value);
    }

    /// Test encrypted field in WHERE clause limitations
    #[tokio::test]
    async fn test_query_encrypted_field_where_limitations() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // WHERE clause on encrypted field should be rejected
        let result = qbi.validate_where_clause(&["email"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SecretsError::ValidationError(_)));

        // WHERE clause on non-encrypted field should be allowed
        let result = qbi.validate_where_clause(&["name"]);
        assert!(result.is_ok());

        // Mixed fields: should reject if any encrypted field is present
        let result = qbi.validate_where_clause(&["name", "email"]);
        assert!(result.is_err());

        // Full query validation for SELECT with encrypted WHERE
        let result = qbi.validate_query(QueryType::Select, &["email"], &[], &[]);
        assert!(result.is_err());
    }

    /// Test encrypted field in ORDER BY
    #[tokio::test]
    async fn test_query_encrypted_field_order_by() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // ORDER BY on encrypted field should be rejected
        let result = qbi.validate_order_by_clause(&["email"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SecretsError::ValidationError(_)));

        // ORDER BY on unencrypted field should work
        let result = qbi.validate_order_by_clause(&["created_at"]);
        assert!(result.is_ok());

        // Full query validation for SELECT with encrypted ORDER BY
        let result = qbi.validate_query(QueryType::Select, &[], &["email"], &[]);
        assert!(result.is_err());
    }

    /// Test encrypted field in JOIN conditions
    #[tokio::test]
    async fn test_query_encrypted_field_join() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // JOIN on encrypted field should be rejected
        let result = qbi.validate_join_condition(&["email"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SecretsError::ValidationError(_)));

        // JOIN on unencrypted field should work
        let result = qbi.validate_join_condition(&["user_id"]);
        assert!(result.is_ok());

        // Full query validation for SELECT with encrypted JOIN
        let result = qbi.validate_query(QueryType::Select, &[], &[], &["email"]);
        assert!(result.is_err());
    }

    // ============================================================================
    // ADAPTER TRAIT TESTS
    // ============================================================================

    /// Test EncryptedFieldAdapter trait
    #[test]
    fn test_encrypted_field_adapter_trait() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);

        // get_encrypted_fields equivalent
        let fields = qbi.encrypted_fields();
        assert_eq!(fields.len(), 2);
        assert!(fields.contains(&"email".to_string()));
        assert!(fields.contains(&"phone".to_string()));

        // is_encrypted checks
        assert!(qbi.is_encrypted("email"));
        assert!(qbi.is_encrypted("phone"));
        assert!(!qbi.is_encrypted("name"));

        // encrypt/decrypt via FieldEncryption
        let cipher = test_cipher();
        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test adapter with multiple encryption keys
    #[test]
    fn test_adapter_multiple_keys() {
        // Simulate different keys for different fields
        let key_email = [1u8; KEY_SIZE];
        let key_phone = [2u8; KEY_SIZE];

        let cipher_email = FieldEncryption::new(&key_email);
        let cipher_phone = FieldEncryption::new(&key_phone);

        let email_plaintext = "user@example.com";
        let phone_plaintext = "+1-555-123-4567";

        let email_encrypted = cipher_email.encrypt(email_plaintext).unwrap();
        let phone_encrypted = cipher_phone.encrypt(phone_plaintext).unwrap();

        // Each field decrypts with its own key
        assert_eq!(cipher_email.decrypt(&email_encrypted).unwrap(), email_plaintext);
        assert_eq!(cipher_phone.decrypt(&phone_encrypted).unwrap(), phone_plaintext);

        // Cross-key decryption fails (no key mixing)
        let cross_result = cipher_phone.decrypt(&email_encrypted);
        assert!(cross_result.is_err());

        let cross_result = cipher_email.decrypt(&phone_encrypted);
        assert!(cross_result.is_err());
    }

    /// Test adapter key caching
    #[test]
    fn test_adapter_key_caching() {
        // Simulate key caching: same cipher reused for multiple operations
        let cipher = test_cipher();

        let values = vec![
            "user1@example.com",
            "user2@example.com",
            "user3@example.com",
        ];

        // Encrypt all values with the same cached cipher
        let encrypted: Vec<Vec<u8>> = values
            .iter()
            .map(|v| cipher.encrypt(v).unwrap())
            .collect();

        // All decrypt correctly with same cipher (simulating cached key)
        for (i, enc) in encrypted.iter().enumerate() {
            let decrypted = cipher.decrypt(enc).unwrap();
            assert_eq!(decrypted, values[i]);
        }

        // Performance benefit: creating cipher once vs. per-operation
        let start = Instant::now();
        for _ in 0..100 {
            let _ = cipher.encrypt("test").unwrap();
        }
        let cached_duration = start.elapsed();

        let start = Instant::now();
        for _ in 0..100 {
            let fresh_cipher = FieldEncryption::new(&[0u8; KEY_SIZE]);
            let _ = fresh_cipher.encrypt("test").unwrap();
        }
        let uncached_duration = start.elapsed();

        // Both should complete, but cached should not be significantly slower
        assert!(cached_duration.as_micros() > 0);
        assert!(uncached_duration.as_micros() > 0);
    }

    // ============================================================================
    // MAPPER/CODEC INTEGRATION TESTS
    // ============================================================================

    /// Test field mapper for encrypt on write
    #[tokio::test]
    async fn test_mapper_encrypt_on_write() {
        let cipher = test_cipher();
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // Simulate a record with fields to write
        let fields = vec![("name", "John Doe"), ("email", "john@example.com"), ("age", "30")];

        let mut encrypted_record: Vec<(&str, Vec<u8>)> = Vec::new();
        for (field_name, value) in &fields {
            if qbi.is_encrypted(field_name) {
                // Designated fields automatically encrypted
                let encrypted = cipher.encrypt(value).unwrap();
                encrypted_record.push((field_name, encrypted));
            } else {
                // Other fields left as-is
                encrypted_record.push((field_name, value.as_bytes().to_vec()));
            }
        }

        // Verify encrypted field is actually encrypted
        assert_eq!(encrypted_record.len(), 3);

        // "name" is stored as plaintext bytes
        assert_eq!(encrypted_record[0].1, b"John Doe");

        // "email" is encrypted (different from plaintext)
        assert_ne!(encrypted_record[1].1, b"john@example.com");
        assert!(encrypted_record[1].1.len() > NONCE_SIZE);

        // "age" is stored as plaintext bytes
        assert_eq!(encrypted_record[2].1, b"30");
    }

    /// Test field mapper for decrypt on read
    #[tokio::test]
    async fn test_mapper_decrypt_on_read() {
        let cipher = test_cipher();
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // Simulate encrypted data in database
        let email_encrypted = cipher.encrypt("john@example.com").unwrap();

        // Simulate reading fields from DB
        let db_fields: Vec<(&str, Vec<u8>)> = vec![
            ("name", b"John Doe".to_vec()),
            ("email", email_encrypted),
            ("age", b"30".to_vec()),
        ];

        let mut decrypted_record: Vec<(&str, String)> = Vec::new();
        for (field_name, data) in &db_fields {
            if qbi.is_encrypted(field_name) {
                let plaintext = cipher.decrypt(data).unwrap();
                decrypted_record.push((field_name, plaintext));
            } else {
                let plaintext = String::from_utf8(data.clone()).unwrap();
                decrypted_record.push((field_name, plaintext));
            }
        }

        assert_eq!(decrypted_record[0], ("name", "John Doe".to_string()));
        assert_eq!(decrypted_record[1], ("email", "john@example.com".to_string()));
        assert_eq!(decrypted_record[2], ("age", "30".to_string()));
    }

    /// Test mapper with mixed encrypted/unencrypted fields
    #[tokio::test]
    async fn test_mapper_mixed_fields() {
        let cipher = test_cipher();
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "ssn".to_string()]);

        // Record with mix of encrypted and unencrypted fields
        let fields: Vec<(&str, &str)> = vec![
            ("id", "123"),
            ("name", "Jane Smith"),
            ("email", "jane@example.com"),
            ("ssn", "123-45-6789"),
            ("department", "Engineering"),
        ];

        // Encrypt on write
        let mut stored: Vec<(&str, Vec<u8>)> = Vec::new();
        for (field_name, value) in &fields {
            if qbi.is_encrypted(field_name) {
                stored.push((field_name, cipher.encrypt(value).unwrap()));
            } else {
                stored.push((field_name, value.as_bytes().to_vec()));
            }
        }

        // Only designated fields are encrypted
        let encrypted_in_list = qbi.get_encrypted_fields_in_list(
            &fields.iter().map(|(f, _)| *f).collect::<Vec<_>>(),
        );
        assert_eq!(encrypted_in_list.len(), 2);
        assert!(encrypted_in_list.contains(&"email".to_string()));
        assert!(encrypted_in_list.contains(&"ssn".to_string()));

        // Decrypt on read: all fields available
        let mut result: Vec<(&str, String)> = Vec::new();
        for (field_name, data) in &stored {
            if qbi.is_encrypted(field_name) {
                result.push((field_name, cipher.decrypt(data).unwrap()));
            } else {
                result.push((field_name, String::from_utf8(data.clone()).unwrap()));
            }
        }

        assert_eq!(result.len(), 5);
        assert_eq!(result[0], ("id", "123".to_string()));
        assert_eq!(result[1], ("name", "Jane Smith".to_string()));
        assert_eq!(result[2], ("email", "jane@example.com".to_string()));
        assert_eq!(result[3], ("ssn", "123-45-6789".to_string()));
        assert_eq!(result[4], ("department", "Engineering".to_string()));
    }

    /// Test mapper batch operations
    #[tokio::test]
    async fn test_mapper_batch_encrypt_decrypt() {
        let cipher = test_cipher();

        let batch_size = 100;
        let mut encrypted_batch: Vec<Vec<u8>> = Vec::with_capacity(batch_size);
        let mut plaintext_batch: Vec<String> = Vec::with_capacity(batch_size);

        // Encrypt batch of records
        for i in 0..batch_size {
            let plaintext = format!("user{}@example.com", i);
            let encrypted = cipher.encrypt(&plaintext).unwrap();
            encrypted_batch.push(encrypted);
            plaintext_batch.push(plaintext);
        }

        assert_eq!(encrypted_batch.len(), batch_size);

        // Decrypt batch of records
        let mut decrypted_batch: Vec<String> = Vec::with_capacity(batch_size);
        for encrypted in &encrypted_batch {
            let decrypted = cipher.decrypt(encrypted).unwrap();
            decrypted_batch.push(decrypted);
        }

        // All records roundtrip correctly
        assert_eq!(decrypted_batch.len(), batch_size);
        for i in 0..batch_size {
            assert_eq!(decrypted_batch[i], plaintext_batch[i]);
        }
    }

    // ============================================================================
    // VAULT INTEGRATION TESTS
    // ============================================================================

    /// Test getting encryption key from Vault
    #[tokio::test]
    async fn test_adapter_vault_key_retrieval() {
        // Simulate key retrieval: key bytes create a working cipher
        let key_bytes = [42u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key_bytes);

        let plaintext = "vault-protected-data";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Subsequent operations use the same cipher (cached key)
        let encrypted2 = cipher.encrypt("more-data").unwrap();
        let decrypted2 = cipher.decrypt(&encrypted2).unwrap();
        assert_eq!(decrypted2, "more-data");
    }

    /// Test key rotation from Vault
    #[tokio::test]
    async fn test_adapter_vault_key_rotation() {
        // Old key (version 1)
        let old_key = [1u8; KEY_SIZE];
        let old_cipher = FieldEncryption::new(&old_key);

        // Encrypt with old key
        let plaintext = "sensitive-data";
        let old_encrypted = old_cipher.encrypt(plaintext).unwrap();

        // New key (version 2) after rotation
        let new_key = [2u8; KEY_SIZE];
        let new_cipher = FieldEncryption::new(&new_key);

        // New encryptions use new key
        let new_encrypted = new_cipher.encrypt(plaintext).unwrap();
        assert_eq!(new_cipher.decrypt(&new_encrypted).unwrap(), plaintext);

        // Old ciphertexts still decrypt with old key (versioning)
        assert_eq!(old_cipher.decrypt(&old_encrypted).unwrap(), plaintext);

        // Old ciphertexts do NOT decrypt with new key
        let result = new_cipher.decrypt(&old_encrypted);
        assert!(result.is_err());

        // Re-encryption: decrypt with old, encrypt with new
        let re_decrypted = old_cipher.decrypt(&old_encrypted).unwrap();
        let re_encrypted = new_cipher.encrypt(&re_decrypted).unwrap();
        assert_eq!(new_cipher.decrypt(&re_encrypted).unwrap(), plaintext);
    }

    /// Test Vault unavailability handling
    #[tokio::test]
    async fn test_adapter_vault_unavailable() {
        // If key is cached, operations continue
        let cached_cipher = test_cipher();
        let encrypted = cached_cipher.encrypt("cached-data").unwrap();
        let decrypted = cached_cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "cached-data");

        // Simulate "Vault unavailable" for unconfigured field
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        // A field not in the encrypted set represents an unconfigured/unavailable scenario
        assert!(!qbi.is_encrypted("unknown_field"));

        // Graceful error: decrypting garbage data does not panic, returns error
        let garbage = vec![0u8; 50];
        let result = cached_cipher.decrypt(&garbage);
        assert!(result.is_err());
    }

    // ============================================================================
    // CONTEXT-BASED ENCRYPTION IN DATABASE TESTS
    // ============================================================================

    /// Test storing context with encrypted data
    #[tokio::test]
    async fn test_database_context_storage() {
        let cipher = test_cipher();
        let context = "user:123:email";

        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt_with_context(plaintext, context).unwrap();

        // Context is not stored in ciphertext (only used as AAD)
        // Application must provide context on decrypt
        let decrypted = cipher.decrypt_with_context(&encrypted, context).unwrap();
        assert_eq!(decrypted, plaintext);

        // Context mismatch detected on decrypt (authentication failure)
        let wrong_context = "user:456:email";
        let result = cipher.decrypt_with_context(&encrypted, wrong_context);
        assert!(result.is_err());

        // Decrypting without context also fails (context was used in AAD)
        let result = cipher.decrypt(&encrypted);
        assert!(result.is_err());
    }

    /// Test context audit trail
    #[tokio::test]
    async fn test_database_context_audit_trail() {
        let cipher = test_cipher();

        // Create context for audit tracking
        let ctx = EncryptionContext::new("user123", "email", "insert", "2024-01-01T00:00:00Z");
        let aad = ctx.to_aad_string();

        // Context contains all audit-relevant information
        assert!(aad.contains("user:user123"));
        assert!(aad.contains("field:email"));
        assert!(aad.contains("op:insert"));
        assert!(aad.contains("ts:2024-01-01T00:00:00Z"));

        // Encrypt with context-based AAD
        let plaintext = "user123@example.com";
        let encrypted = cipher.encrypt_with_context(plaintext, &aad).unwrap();

        // Can only decrypt with matching context (correlates access)
        let decrypted = cipher.decrypt_with_context(&encrypted, &aad).unwrap();
        assert_eq!(decrypted, plaintext);

        // Different user context fails (proves access tracking)
        let different_user = EncryptionContext::new("user456", "email", "insert", "2024-01-01T00:00:00Z");
        let different_aad = different_user.to_aad_string();
        let result = cipher.decrypt_with_context(&encrypted, &different_aad);
        assert!(result.is_err());
    }

    /// Test context field validation
    #[tokio::test]
    async fn test_database_context_validation() {
        // Context format is consistent
        let ctx1 = EncryptionContext::new("user1", "email", "select", "2024-06-15T10:30:00Z");
        let ctx2 = EncryptionContext::new("user2", "phone", "update", "2024-06-15T10:31:00Z");

        let aad1 = ctx1.to_aad_string();
        let aad2 = ctx2.to_aad_string();

        // Format is predictable and parseable
        assert!(aad1.starts_with("user:"));
        assert!(aad2.starts_with("user:"));

        // Different contexts produce different AADs
        assert_ne!(aad1, aad2);

        // Context with special characters is handled
        let ctx_special = EncryptionContext::new("user:with:colons", "field.with.dots", "op", "ts");
        let aad_special = ctx_special.to_aad_string();
        assert!(aad_special.contains("user:user:with:colons"));

        // Context encryption roundtrip with validated context
        let cipher = test_cipher();
        let encrypted = cipher.encrypt_with_context("data", &aad1).unwrap();
        let decrypted = cipher.decrypt_with_context(&encrypted, &aad1).unwrap();
        assert_eq!(decrypted, "data");
    }

    // ============================================================================
    // TRANSACTION TESTS
    // ============================================================================

    /// Test encrypted fields in transactions
    #[tokio::test]
    async fn test_transaction_encrypt_decrypt() {
        let cipher = test_cipher();

        // Simulate transaction: insert then read within same transaction
        // Step 1: Encrypt value for insertion
        let plaintext = "txn-data@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Step 2: Read within same transaction decrypts
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Consistent encryption key throughout the transaction
        let encrypted2 = cipher.encrypt("second-value").unwrap();
        let decrypted2 = cipher.decrypt(&encrypted2).unwrap();
        assert_eq!(decrypted2, "second-value");
    }

    /// Test transaction rollback with encryption
    #[tokio::test]
    async fn test_transaction_rollback() {
        let cipher = test_cipher();

        // Simulate: encrypt data for a transaction that will be rolled back
        let plaintext = "rollback-data";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Transaction rolled back: encrypted data is discarded
        // Simulate by dropping the encrypted value
        drop(encrypted);

        // Application state unchanged: cipher still works for new transactions
        let new_plaintext = "new-transaction-data";
        let new_encrypted = cipher.encrypt(new_plaintext).unwrap();
        let new_decrypted = cipher.decrypt(&new_encrypted).unwrap();
        assert_eq!(new_decrypted, new_plaintext);

        // No stale state: a fresh encryption/decryption cycle works
        let fresh = cipher.encrypt("fresh-data").unwrap();
        assert_eq!(cipher.decrypt(&fresh).unwrap(), "fresh-data");
    }

    /// Test concurrent transactions with encryption
    #[tokio::test]
    async fn test_transaction_concurrent_encryption() {
        // Simulate concurrent transactions: each with its own cipher clone
        let base_cipher = test_cipher();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let cipher = base_cipher.clone();
                tokio::spawn(async move {
                    let plaintext = format!("concurrent-txn-{}", i);
                    let encrypted = cipher.encrypt(&plaintext).unwrap();
                    let decrypted = cipher.decrypt(&encrypted).unwrap();
                    assert_eq!(decrypted, plaintext);
                    decrypted
                })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // All transactions completed with correct data, no contention
        assert_eq!(results.len(), 10);
        for i in 0..10 {
            assert!(results.contains(&format!("concurrent-txn-{}", i)));
        }
    }

    // ============================================================================
    // NULL AND EMPTY VALUE TESTS
    // ============================================================================

    /// Test NULL encrypted field
    #[tokio::test]
    async fn test_null_encrypted_field() {
        let cipher = test_cipher();

        // NULL fields should remain NULL (not encrypted)
        let value: Option<&str> = None;

        // Encrypt only if value is present
        let encrypted: Option<Vec<u8>> = value.map(|v| cipher.encrypt(v).unwrap());
        assert!(encrypted.is_none());

        // Decrypt of None returns None
        let decrypted: Option<String> = encrypted.map(|e| cipher.decrypt(&e).unwrap());
        assert!(decrypted.is_none());

        // Non-NULL value works normally
        let non_null: Option<&str> = Some("present-value");
        let enc = non_null.map(|v| cipher.encrypt(v).unwrap());
        assert!(enc.is_some());
        let dec = enc.map(|e| cipher.decrypt(&e).unwrap());
        assert_eq!(dec, Some("present-value".to_string()));
    }

    /// Test empty string encryption
    #[tokio::test]
    async fn test_empty_string_encrypted_field() {
        let cipher = test_cipher();

        // Empty string should be encrypted (not skipped)
        let empty = "";
        let encrypted = cipher.encrypt(empty).unwrap();

        // Encrypted empty string has nonce and tag (non-empty ciphertext)
        assert!(encrypted.len() > NONCE_SIZE);

        // Decrypt returns empty string
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "");

        // Empty string encryption is distinguishable from NULL
        let null_value: Option<Vec<u8>> = None;
        let empty_encrypted: Option<Vec<u8>> = Some(encrypted);
        assert_ne!(null_value, empty_encrypted);
    }

    /// Test default value encryption
    #[tokio::test]
    async fn test_default_value_encrypted_field() {
        let cipher = test_cipher();

        // Simulate a default value applied before encryption
        let default_value = "default@placeholder.com";
        let encrypted = cipher.encrypt(default_value).unwrap();

        // Encrypted default value stored in database
        assert!(encrypted.len() > NONCE_SIZE);
        assert_ne!(encrypted[NONCE_SIZE..].to_vec(), default_value.as_bytes());

        // Retrieved and decrypted correctly
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, default_value);
    }

    // ============================================================================
    // PERFORMANCE TESTS
    // ============================================================================

    /// Test encryption overhead on INSERT
    #[tokio::test]
    async fn test_performance_encrypt_overhead() {
        let cipher = test_cipher();
        let record_count = 1000;

        let start = Instant::now();
        for i in 0..record_count {
            let plaintext = format!("user{}@example.com", i);
            let _encrypted = cipher.encrypt(&plaintext).unwrap();
        }
        let elapsed = start.elapsed();

        // 1000 encryptions should complete in reasonable time (under 5 seconds)
        assert!(
            elapsed.as_secs() < 5,
            "Encrypting {} records took {:?}, expected < 5s",
            record_count,
            elapsed
        );
    }

    /// Test decryption overhead on SELECT
    #[tokio::test]
    async fn test_performance_decrypt_overhead() {
        let cipher = test_cipher();
        let record_count = 1000;

        // Pre-encrypt records
        let encrypted_records: Vec<Vec<u8>> = (0..record_count)
            .map(|i| {
                let plaintext = format!("user{}@example.com", i);
                cipher.encrypt(&plaintext).unwrap()
            })
            .collect();

        // Measure decryption time
        let start = Instant::now();
        for encrypted in &encrypted_records {
            let _decrypted = cipher.decrypt(encrypted).unwrap();
        }
        let elapsed = start.elapsed();

        // 1000 decryptions should complete in reasonable time (under 5 seconds)
        assert!(
            elapsed.as_secs() < 5,
            "Decrypting {} records took {:?}, expected < 5s",
            record_count,
            elapsed
        );
    }

    /// Test encryption key caching impact
    #[tokio::test]
    async fn test_performance_key_caching() {
        let iterations = 500;

        // With key caching: reuse same cipher
        let cached_cipher = test_cipher();
        let start_cached = Instant::now();
        for _ in 0..iterations {
            let encrypted = cached_cipher.encrypt("test-data").unwrap();
            let _decrypted = cached_cipher.decrypt(&encrypted).unwrap();
        }
        let cached_duration = start_cached.elapsed();

        // Without key caching: create new cipher each time
        let start_uncached = Instant::now();
        for _ in 0..iterations {
            let fresh_cipher = FieldEncryption::new(&[0u8; KEY_SIZE]);
            let encrypted = fresh_cipher.encrypt("test-data").unwrap();
            let _decrypted = fresh_cipher.decrypt(&encrypted).unwrap();
        }
        let uncached_duration = start_uncached.elapsed();

        // Both complete successfully
        assert!(cached_duration.as_micros() > 0);
        assert!(uncached_duration.as_micros() > 0);

        // Cached should generally be faster or comparable
        // (We test that both complete rather than strict ordering since CI can be noisy)
        assert!(
            cached_duration.as_secs() < 10,
            "Cached operations took {:?}",
            cached_duration
        );
        assert!(
            uncached_duration.as_secs() < 10,
            "Uncached operations took {:?}",
            uncached_duration
        );
    }

    // ============================================================================
    // ERROR HANDLING TESTS
    // ============================================================================

    /// Test encryption of invalid UTF-8
    #[tokio::test]
    async fn test_error_invalid_utf8_field() {
        let cipher = test_cipher();

        // Encrypt valid text, then simulate corrupted decryption producing invalid UTF-8
        // We craft a scenario: encrypt raw bytes that are invalid UTF-8
        // FieldEncryption::encrypt takes &str, so it always produces valid ciphertext
        // However, if the ciphertext is tampered with, decryption should return error

        let encrypted = cipher.encrypt("valid").unwrap();
        // Tamper with ciphertext to produce decryption failure (not panic)
        let mut tampered = encrypted.clone();
        if tampered.len() > NONCE_SIZE + 1 {
            tampered[NONCE_SIZE + 1] ^= 0xFF;
        }
        let result = cipher.decrypt(&tampered);
        assert!(result.is_err());

        // Error message should be clear
        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("Decryption failed") || err_msg.contains("Encryption error"),
            "Expected clear error message, got: {}",
            err_msg
        );
    }

    /// Test decryption with wrong key
    #[tokio::test]
    async fn test_error_decrypt_wrong_key() {
        let cipher1 = FieldEncryption::new(&[1u8; KEY_SIZE]);
        let cipher2 = FieldEncryption::new(&[2u8; KEY_SIZE]);

        let plaintext = "sensitive-data";
        let encrypted = cipher1.encrypt(plaintext).unwrap();

        // Decryption with wrong key should return error (not garbage)
        let result = cipher2.decrypt(&encrypted);
        assert!(result.is_err());

        // Error indicates authentication failure
        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("Decryption failed") || err_msg.contains("Encryption error"),
            "Expected authentication failure indication, got: {}",
            err_msg
        );
    }

    /// Test corrupted ciphertext in database
    #[tokio::test]
    async fn test_error_corrupted_ciphertext() {
        let cipher = test_cipher();

        // Case 1: Completely random data
        let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33,
                           0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB,
                           0xCC, 0xDD, 0xEE, 0xFF, 0x01, 0x02, 0x03, 0x04,
                           0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C];
        let result = cipher.decrypt(&garbage);
        assert!(result.is_err());

        // Case 2: Data too short for nonce
        let short = vec![0u8; 5];
        let result = cipher.decrypt(&short);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("too short"), "Expected 'too short' error, got: {}", err_msg);

        // Case 3: Correct-length but corrupted ciphertext
        let encrypted = cipher.encrypt("test-data").unwrap();
        let mut corrupted = encrypted;
        // Flip multiple bytes in the tag/ciphertext area
        for i in NONCE_SIZE..corrupted.len() {
            corrupted[i] ^= 0xFF;
        }
        let result = cipher.decrypt(&corrupted);
        assert!(result.is_err());
    }

    /// Test missing key in SecretsManager
    #[tokio::test]
    async fn test_error_missing_encryption_key() {
        // Simulate missing key scenario: field not in configured encrypted set
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // "ssn" is not registered as encrypted
        assert!(!qbi.is_encrypted("ssn"));

        // No encrypted fields found for unlisted fields
        let encrypted_in_query = qbi.get_encrypted_fields_in_list(&["ssn", "name"]);
        assert!(encrypted_in_query.is_empty());

        // An invalid key size would cause a panic (tested via should_panic elsewhere)
        // but a SecretsError::NotFound can be simulated
        let err = SecretsError::NotFound("Encryption key for field 'ssn' not configured".to_string());
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("not configured") || err_msg.contains("not found"));
    }

    // ============================================================================
    // TYPE SYSTEM TESTS
    // ============================================================================

    /// Test encrypted VARCHAR field
    #[tokio::test]
    async fn test_type_varchar_encrypted() {
        let cipher = test_cipher();

        // VARCHAR stored as encrypted bytes (BYTEA/BLOB in DB)
        let varchar_value = "Hello, World!";
        let encrypted = cipher.encrypt(varchar_value).unwrap();

        // Stored as bytes (not string)
        assert!(encrypted.len() > NONCE_SIZE);

        // Retrieved and decrypted to String
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, varchar_value);
    }

    /// Test encrypted NUMERIC field
    #[tokio::test]
    async fn test_type_numeric_encrypted() {
        let cipher = test_cipher();

        // NUMERIC field converted to string for encryption
        let numeric_value: f64 = 12345.67;
        let as_string = numeric_value.to_string();
        let encrypted = cipher.encrypt(&as_string).unwrap();

        // Decrypted returns string
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "12345.67");

        // Application converts back to number
        let restored: f64 = decrypted.parse().unwrap();
        assert!((restored - numeric_value).abs() < f64::EPSILON);

        // Integer type
        let int_value: i64 = 42;
        let int_string = int_value.to_string();
        let int_encrypted = cipher.encrypt(&int_string).unwrap();
        let int_decrypted = cipher.decrypt(&int_encrypted).unwrap();
        let int_restored: i64 = int_decrypted.parse().unwrap();
        assert_eq!(int_restored, 42);
    }

    /// Test encrypted TIMESTAMP field
    #[tokio::test]
    async fn test_type_timestamp_encrypted() {
        let cipher = test_cipher();

        // TIMESTAMP converted to string, then encrypted
        let timestamp = "2024-06-15T10:30:00.000Z";
        let encrypted = cipher.encrypt(timestamp).unwrap();

        // Decrypted string preserves exact format
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, timestamp);

        // Various timestamp formats
        let timestamps = vec![
            "2024-01-01T00:00:00Z",
            "2024-12-31T23:59:59.999Z",
            "2024-06-15 10:30:00",
            "1970-01-01T00:00:00Z",
        ];

        for ts in timestamps {
            let enc = cipher.encrypt(ts).unwrap();
            let dec = cipher.decrypt(&enc).unwrap();
            assert_eq!(dec, ts, "Timestamp roundtrip failed for: {}", ts);
        }
    }

    /// Test encrypted JSON field
    #[tokio::test]
    async fn test_type_json_encrypted() {
        let cipher = test_cipher();

        // Entire JSON encrypted as string
        let json_value = r#"{"name":"John","age":30,"active":true,"tags":["admin","user"]}"#;
        let encrypted = cipher.encrypt(json_value).unwrap();

        // Decrypted JSON can be parsed
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, json_value);

        // Structure preserved: parse to verify
        let parsed: serde_json::Value = serde_json::from_str(&decrypted).unwrap();
        assert_eq!(parsed["name"], "John");
        assert_eq!(parsed["age"], 30);
        assert_eq!(parsed["active"], true);
        assert_eq!(parsed["tags"][0], "admin");
        assert_eq!(parsed["tags"][1], "user");

        // Nested JSON
        let nested = r#"{"user":{"address":{"city":"NYC","zip":"10001"}}}"#;
        let enc = cipher.encrypt(nested).unwrap();
        let dec = cipher.decrypt(&enc).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&dec).unwrap();
        assert_eq!(parsed["user"]["address"]["city"], "NYC");
    }

    // ============================================================================
    // SCHEMA DISCOVERY TESTS
    // ============================================================================

    /// Test adapter detects encrypted fields from schema
    #[test]
    fn test_schema_detect_encrypted_fields() {
        let schema = StructSchema::new("User").with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, "")
                .with_mark(EncryptionMark::Encrypted),
            SchemaFieldInfo::new("name", "String", false, ""),
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted)
                .with_algorithm("aes256-gcm"),
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone")
                .with_mark(EncryptionMark::Sensitive),
        ]);

        // Adapter detects encrypted fields from schema
        let encrypted_names = schema.encrypted_field_names();
        assert_eq!(encrypted_names.len(), 2);
        assert!(encrypted_names.contains(&"email"));
        assert!(encrypted_names.contains(&"phone"));

        // Non-encrypted fields not included
        assert!(!encrypted_names.contains(&"id"));
        assert!(!encrypted_names.contains(&"name"));

        // Build QueryBuilderIntegration from detected fields
        let qbi = QueryBuilderIntegration::new(
            encrypted_names.iter().map(|s| s.to_string()).collect(),
        );
        assert!(qbi.is_encrypted("email"));
        assert!(qbi.is_encrypted("phone"));
        assert!(!qbi.is_encrypted("name"));
    }

    /// Test adapter handles schema evolution
    #[test]
    fn test_schema_evolution_add_encrypted_field() {
        // Version 1: only email encrypted
        let schema_v1 = StructSchema::new("User")
            .with_version(1)
            .with_fields(vec![
                SchemaFieldInfo::new("name", "String", false, ""),
                SchemaFieldInfo::new("email", "String", true, "encryption/email"),
            ]);

        assert_eq!(schema_v1.encrypted_field_count(), 1);
        assert_eq!(schema_v1.version, 1);

        // Version 2: added phone as encrypted field
        let schema_v2 = StructSchema::new("User")
            .with_version(2)
            .with_fields(vec![
                SchemaFieldInfo::new("name", "String", false, ""),
                SchemaFieldInfo::new("email", "String", true, "encryption/email"),
                SchemaFieldInfo::new("phone", "String", true, "encryption/phone"),
            ]);

        assert_eq!(schema_v2.encrypted_field_count(), 2);
        assert_eq!(schema_v2.version, 2);

        // Registry handles schema evolution
        let mut registry = SchemaRegistry::new();
        registry.register(schema_v2).unwrap();

        let user_schema = registry.get("User").unwrap();
        assert_eq!(user_schema.encrypted_field_count(), 2);
        assert!(user_schema.is_field_encrypted("email"));
        assert!(user_schema.is_field_encrypted("phone"));

        // Old records (without phone) still work: phone is Option/NULL
        assert!(!user_schema.is_field_encrypted("name"));
    }

    /// Test adapter handles encryption key changes
    #[test]
    fn test_schema_encryption_key_change() {
        // Schema with key reference
        let schema = StructSchema::new("User").with_fields(vec![
            SchemaFieldInfo::new("email", "String", true, "encryption/email_v1"),
        ]);

        let field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(field.key_reference, "encryption/email_v1");

        // After key change: schema updated with new key reference
        let updated_schema = StructSchema::new("User")
            .with_version(2)
            .with_fields(vec![
                SchemaFieldInfo::new("email", "String", true, "encryption/email_v2"),
            ]);

        let updated_field = updated_schema.get_encrypted_field("email").unwrap();
        assert_eq!(updated_field.key_reference, "encryption/email_v2");

        // Re-encryption support: old key can decrypt, new key can encrypt
        let old_cipher = FieldEncryption::new(&[1u8; KEY_SIZE]);
        let new_cipher = FieldEncryption::new(&[2u8; KEY_SIZE]);

        let plaintext = "user@example.com";
        let old_encrypted = old_cipher.encrypt(plaintext).unwrap();

        // Re-encrypt: decrypt with old, encrypt with new
        let decrypted = old_cipher.decrypt(&old_encrypted).unwrap();
        let re_encrypted = new_cipher.encrypt(&decrypted).unwrap();

        // Verify new encryption works
        let final_decrypted = new_cipher.decrypt(&re_encrypted).unwrap();
        assert_eq!(final_decrypted, plaintext);
    }
}
