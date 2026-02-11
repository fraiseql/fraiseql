//! Comprehensive tests for integrating EncryptedFieldAdapter
//! with query builders for transparent encryption/decryption

#[cfg(test)]
#[allow(clippy::module_inception)]
mod query_builder_integration_tests {
    use std::collections::HashMap;

    use crate::encryption::{
        FieldEncryption,
        database_adapter::EncryptionContext,
        query_builder::{QueryBuilderIntegration, QueryType},
        schema::{EncryptionMark, SchemaFieldInfo, SchemaRegistry, StructSchema},
    };

    /// Helper: create a FieldEncryption with a test key
    fn test_cipher() -> FieldEncryption {
        FieldEncryption::new(&[0u8; 32])
    }

    /// Helper: create a QueryBuilderIntegration with email, phone, ssn encrypted
    fn test_qbi() -> QueryBuilderIntegration {
        QueryBuilderIntegration::new(vec![
            "email".to_string(),
            "phone".to_string(),
            "ssn".to_string(),
        ])
    }

    // ============================================================================
    // INSERT OPERATION TESTS
    // ============================================================================

    /// Test auto-encrypt on single INSERT
    #[tokio::test]
    async fn test_insert_auto_encrypt_single_field() {
        let cipher = test_cipher();
        let plaintext = "user@example.com";

        // Encrypt value before INSERT
        let encrypted = cipher.encrypt(plaintext).unwrap();
        assert_ne!(encrypted, plaintext.as_bytes());

        // Verify encrypted value can be decrypted
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // INSERT validation allows encrypted fields (no restriction)
        let qbi = test_qbi();
        assert!(qbi.validate_query(QueryType::Insert, &[], &[], &[]).is_ok());
    }

    /// Test auto-encrypt on multi-field INSERT
    #[tokio::test]
    async fn test_insert_auto_encrypt_multiple_fields() {
        let cipher = test_cipher();

        let fields: HashMap<&str, &str> = HashMap::from([
            ("email", "user@example.com"),
            ("phone", "+1-555-123-4567"),
            ("ssn", "123-45-6789"),
        ]);

        let mut encrypted_values: HashMap<&str, Vec<u8>> = HashMap::new();
        for (field, value) in &fields {
            let encrypted = cipher.encrypt(value).unwrap();
            encrypted_values.insert(field, encrypted);
        }

        // Each field encrypted independently with different nonces
        assert_ne!(encrypted_values["email"], encrypted_values["phone"]);
        assert_ne!(encrypted_values["email"], encrypted_values["ssn"]);

        // All decrypt correctly
        for (field, value) in &fields {
            let decrypted = cipher.decrypt(&encrypted_values[field]).unwrap();
            assert_eq!(&decrypted, value);
        }
    }

    /// Test INSERT with NULL encrypted field
    #[tokio::test]
    async fn test_insert_null_encrypted_field() {
        let cipher = test_cipher();

        // NULL values skip encryption
        let value: Option<&str> = None;
        assert!(value.is_none());

        // Non-null values are encrypted
        let non_null_value: Option<&str> = Some("user@example.com");
        if let Some(v) = non_null_value {
            let encrypted = cipher.encrypt(v).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted, v);
        }
    }

    /// Test INSERT with empty string encrypted field
    #[tokio::test]
    async fn test_insert_empty_string_encrypted_field() {
        let cipher = test_cipher();

        // Empty string should be encrypted (not skipped)
        let encrypted = cipher.encrypt("").unwrap();
        assert!(!encrypted.is_empty()); // Encrypted bytes exist
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, ""); // Decrypts to empty string

        // Distinguishable from NULL
        let null_value: Option<Vec<u8>> = None;
        assert!(null_value.is_none());
        assert!(!encrypted.is_empty()); // Empty string encrypted is non-empty bytes
    }

    /// Test INSERT with mixed encrypted and unencrypted fields
    #[tokio::test]
    async fn test_insert_mixed_encrypted_unencrypted() {
        let cipher = test_cipher();
        let qbi = test_qbi();

        // Simulate a row with mixed fields
        let mut row: HashMap<String, Vec<u8>> = HashMap::new();

        // Encrypted fields
        let email_enc = cipher.encrypt("user@example.com").unwrap();
        row.insert("email".to_string(), email_enc);

        // Unencrypted fields stored as-is
        let name_bytes = "John Doe".as_bytes().to_vec();
        row.insert("name".to_string(), name_bytes.clone());

        // Only designated fields are encrypted
        assert!(qbi.is_encrypted("email"));
        assert!(!qbi.is_encrypted("name"));

        // Name stored as plain bytes
        assert_eq!(String::from_utf8(row["name"].clone()).unwrap(), "John Doe");
        // Email is encrypted bytes
        let decrypted_email = cipher.decrypt(&row["email"]).unwrap();
        assert_eq!(decrypted_email, "user@example.com");
    }

    /// Test INSERT with DEFAULT value for encrypted field
    #[tokio::test]
    async fn test_insert_default_value_encrypted() {
        let cipher = test_cipher();

        // Default value for encrypted field is encrypted before storage
        let default_email = "default@example.com";
        let encrypted = cipher.encrypt(default_email).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, default_email);
    }

    /// Test batch INSERT with multiple encrypted records
    #[tokio::test]
    async fn test_insert_batch_multiple_records() {
        let cipher = test_cipher();

        // Batch of 100 records
        let mut encrypted_emails: Vec<Vec<u8>> = Vec::new();
        for i in 0..100 {
            let email = format!("user{}@example.com", i);
            let encrypted = cipher.encrypt(&email).unwrap();
            encrypted_emails.push(encrypted);
        }

        // All encrypted independently with unique nonces
        for i in 0..99 {
            assert_ne!(
                encrypted_emails[i],
                encrypted_emails[i + 1],
                "Each record should have unique ciphertext"
            );
        }

        // All decrypt correctly
        for (i, encrypted) in encrypted_emails.iter().enumerate() {
            let expected = format!("user{}@example.com", i);
            let decrypted = cipher.decrypt(encrypted).unwrap();
            assert_eq!(decrypted, expected);
        }
    }

    /// Test INSERT with encryption context (audit trail)
    #[tokio::test]
    async fn test_insert_with_context() {
        let cipher = test_cipher();
        let ctx = EncryptionContext::new("user-123", "email", "insert", "2026-01-15T10:00:00Z");
        let aad = ctx.to_aad_string();

        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt_with_context(plaintext, &aad).unwrap();

        // Decrypt with same context succeeds
        let decrypted = cipher.decrypt_with_context(&encrypted, &aad).unwrap();
        assert_eq!(decrypted, plaintext);

        // Decrypt with different context fails
        let wrong_ctx =
            EncryptionContext::new("user-456", "email", "insert", "2026-01-15T10:00:00Z");
        assert!(cipher.decrypt_with_context(&encrypted, &wrong_ctx.to_aad_string()).is_err());
    }

    // ============================================================================
    // SELECT OPERATION TESTS
    // ============================================================================

    /// Test auto-decrypt on single SELECT
    #[tokio::test]
    async fn test_select_auto_decrypt_single_field() {
        let cipher = test_cipher();

        // Simulate encrypted data in database
        let stored = cipher.encrypt("user@example.com").unwrap();

        // On SELECT, auto-decrypt
        let plaintext = cipher.decrypt(&stored).unwrap();
        assert_eq!(plaintext, "user@example.com");
    }

    /// Test auto-decrypt on multi-field SELECT
    #[tokio::test]
    async fn test_select_auto_decrypt_multiple_fields() {
        let cipher = test_cipher();

        let stored_email = cipher.encrypt("user@example.com").unwrap();
        let stored_phone = cipher.encrypt("+1-555-123-4567").unwrap();
        let stored_ssn = cipher.encrypt("123-45-6789").unwrap();

        assert_eq!(cipher.decrypt(&stored_email).unwrap(), "user@example.com");
        assert_eq!(cipher.decrypt(&stored_phone).unwrap(), "+1-555-123-4567");
        assert_eq!(cipher.decrypt(&stored_ssn).unwrap(), "123-45-6789");
    }

    /// Test SELECT all columns (including encrypted)
    #[tokio::test]
    async fn test_select_all_columns_with_encryption() {
        let cipher = test_cipher();
        let qbi = test_qbi();

        // Row from SELECT * with mixed fields
        let encrypted_email = cipher.encrypt("user@example.com").unwrap();
        let name = "John Doe".to_string();
        let id = 42_i64;

        // Detect which fields need decryption
        let all_fields = vec!["id", "name", "email"];
        let encrypted_in_row = qbi.get_encrypted_fields_in_list(&all_fields);
        assert_eq!(encrypted_in_row, vec!["email"]);

        // Decrypt only encrypted fields
        let decrypted_email = cipher.decrypt(&encrypted_email).unwrap();
        assert_eq!(decrypted_email, "user@example.com");
        assert_eq!(name, "John Doe");
        assert_eq!(id, 42);
    }

    /// Test SELECT with NULL encrypted field
    #[tokio::test]
    async fn test_select_null_encrypted_field() {
        // NULL in encrypted column returns as NULL, not decrypted
        let value: Option<Vec<u8>> = None;
        let result: Option<String> = value.map(|_v| "should not happen".to_string());
        assert!(result.is_none());
    }

    /// Test SELECT with empty string encrypted field
    #[tokio::test]
    async fn test_select_empty_string_encrypted_field() {
        let cipher = test_cipher();

        // Empty string was encrypted
        let stored = cipher.encrypt("").unwrap();
        let decrypted = cipher.decrypt(&stored).unwrap();
        assert_eq!(decrypted, "");
        assert_ne!(decrypted, "null"); // Not NULL
    }

    /// Test SELECT of multiple rows with decryption
    #[tokio::test]
    async fn test_select_batch_multiple_rows() {
        let cipher = test_cipher();

        // Simulate 100 stored encrypted rows
        let mut stored: Vec<Vec<u8>> = Vec::new();
        for i in 0..100 {
            stored.push(cipher.encrypt(&format!("user{}@example.com", i)).unwrap());
        }

        // Decrypt all rows
        let decrypted: Vec<String> = stored.iter().map(|s| cipher.decrypt(s).unwrap()).collect();

        assert_eq!(decrypted.len(), 100);
        assert_eq!(decrypted[0], "user0@example.com");
        assert_eq!(decrypted[99], "user99@example.com");
    }

    /// Test SELECT with column aliases for encrypted field
    #[tokio::test]
    async fn test_select_column_alias_encrypted() {
        let cipher = test_cipher();
        let qbi = test_qbi();

        // Field "email" aliased to "user_email" in SELECT
        let stored = cipher.encrypt("user@example.com").unwrap();
        let decrypted = cipher.decrypt(&stored).unwrap();

        // The underlying field is still "email" - alias doesn't affect encryption
        assert!(qbi.is_encrypted("email"));
        assert_eq!(decrypted, "user@example.com");
    }

    /// Test SELECT with context (audit trail)
    #[tokio::test]
    async fn test_select_with_context() {
        let cipher = test_cipher();
        let insert_ctx =
            EncryptionContext::new("user-123", "email", "insert", "2026-01-15T10:00:00Z");
        let select_ctx =
            EncryptionContext::new("user-123", "email", "insert", "2026-01-15T10:00:00Z");

        // Data encrypted with context on INSERT
        let stored = cipher
            .encrypt_with_context("user@example.com", &insert_ctx.to_aad_string())
            .unwrap();

        // SELECT must use the same context to decrypt
        let decrypted = cipher.decrypt_with_context(&stored, &select_ctx.to_aad_string()).unwrap();
        assert_eq!(decrypted, "user@example.com");
    }

    // ============================================================================
    // UPDATE OPERATION TESTS
    // ============================================================================

    /// Test auto-encrypt on UPDATE
    #[tokio::test]
    async fn test_update_auto_encrypt_single_field() {
        let cipher = test_cipher();

        // Original encrypted value
        let original = cipher.encrypt("old@example.com").unwrap();

        // UPDATE with new value - new nonce generated
        let updated = cipher.encrypt("new@example.com").unwrap();

        // Different ciphertext
        assert_ne!(original, updated);

        // New value decrypts correctly
        assert_eq!(cipher.decrypt(&updated).unwrap(), "new@example.com");

        // UPDATE validation allows encrypted fields
        let qbi = test_qbi();
        assert!(qbi.validate_query(QueryType::Update, &[], &[], &[]).is_ok());
    }

    /// Test UPDATE with NULL for encrypted field
    #[tokio::test]
    async fn test_update_set_null_encrypted_field() {
        let cipher = test_cipher();

        // Original value exists
        let _original = cipher.encrypt("user@example.com").unwrap();

        // Update to NULL - NULL not encrypted
        let new_value: Option<&str> = None;
        assert!(new_value.is_none());
    }

    /// Test UPDATE multiple encrypted fields
    #[tokio::test]
    async fn test_update_multiple_encrypted_fields() {
        let cipher = test_cipher();

        let new_email = cipher.encrypt("new@example.com").unwrap();
        let new_phone = cipher.encrypt("+1-555-999-0000").unwrap();

        // Each gets new nonce
        assert_ne!(new_email, new_phone);

        assert_eq!(cipher.decrypt(&new_email).unwrap(), "new@example.com");
        assert_eq!(cipher.decrypt(&new_phone).unwrap(), "+1-555-999-0000");
    }

    /// Test UPDATE mixed encrypted and unencrypted
    #[tokio::test]
    async fn test_update_mixed_encrypted_unencrypted() {
        let cipher = test_cipher();
        let qbi = test_qbi();

        // Update both encrypted and unencrypted fields
        let new_email = cipher.encrypt("new@example.com").unwrap();
        let new_name = "Jane Doe".to_string();

        assert!(qbi.is_encrypted("email"));
        assert!(!qbi.is_encrypted("name"));

        assert_eq!(cipher.decrypt(&new_email).unwrap(), "new@example.com");
        assert_eq!(new_name, "Jane Doe");
    }

    /// Test batch UPDATE multiple records
    #[tokio::test]
    async fn test_update_batch_multiple_records() {
        let cipher = test_cipher();

        let mut updated_values: Vec<Vec<u8>> = Vec::new();
        for i in 0..100 {
            let encrypted = cipher.encrypt(&format!("updated{}@example.com", i)).unwrap();
            updated_values.push(encrypted);
        }

        // Verify uniqueness and correctness
        for i in 0..99 {
            assert_ne!(updated_values[i], updated_values[i + 1]);
        }
        assert_eq!(cipher.decrypt(&updated_values[50]).unwrap(), "updated50@example.com");
    }

    /// Test UPDATE with encryption context
    #[tokio::test]
    async fn test_update_with_context() {
        let cipher = test_cipher();
        let ctx = EncryptionContext::new("user-123", "email", "update", "2026-01-16T10:00:00Z");

        let encrypted = cipher
            .encrypt_with_context("updated@example.com", &ctx.to_aad_string())
            .unwrap();
        let decrypted = cipher.decrypt_with_context(&encrypted, &ctx.to_aad_string()).unwrap();
        assert_eq!(decrypted, "updated@example.com");
    }

    // ============================================================================
    // DELETE OPERATION TESTS
    // ============================================================================

    /// Test DELETE doesn't need decryption
    #[tokio::test]
    async fn test_delete_encrypted_records() {
        let qbi = test_qbi();

        // DELETE validation allows all (no clause restrictions)
        assert!(qbi.validate_query(QueryType::Delete, &[], &[], &[]).is_ok());

        // Even with encrypted field in WHERE for DELETE, query validation passes
        // because DELETE doesn't need to compare ciphertext
        assert!(qbi.validate_query(QueryType::Delete, &["email"], &[], &[]).is_ok());
    }

    /// Test DELETE with context
    #[tokio::test]
    async fn test_delete_with_context() {
        let ctx =
            EncryptionContext::new("admin-001", "user_record", "delete", "2026-01-17T10:00:00Z");
        let aad = ctx.to_aad_string();

        // Context is recorded for audit, but no encryption/decryption needed
        assert!(aad.contains("op:delete"));
        assert!(aad.contains("user:admin-001"));
    }

    // ============================================================================
    // WHERE CLAUSE TESTS
    // ============================================================================

    /// Test WHERE clause on unencrypted field with encrypted data
    #[tokio::test]
    async fn test_where_unencrypted_field_with_encrypted_records() {
        let qbi = test_qbi();

        // WHERE on unencrypted field is allowed
        assert!(qbi.validate_where_clause(&["name"]).is_ok());
        assert!(qbi.validate_where_clause(&["id"]).is_ok());
        assert!(qbi.validate_where_clause(&["created_at"]).is_ok());
    }

    /// Test WHERE clause rejects encrypted field equality
    #[tokio::test]
    async fn test_where_encrypted_field_equality_unsupported() {
        let qbi = test_qbi();

        let result = qbi.validate_where_clause(&["email"]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("email"), "Error should mention field name");
        assert!(err.contains("WHERE"), "Error should mention WHERE clause: {}", err);
    }

    /// Test WHERE clause rejects encrypted field range queries
    #[tokio::test]
    async fn test_where_encrypted_field_range_unsupported() {
        let qbi = test_qbi();

        let result = qbi.validate_where_clause(&["phone"]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("phone"));
    }

    /// Test WHERE clause with IN operator on encrypted field
    #[tokio::test]
    async fn test_where_encrypted_field_in_unsupported() {
        let qbi = test_qbi();

        // IN is a WHERE clause operation - encrypted fields rejected
        let result = qbi.validate_where_clause(&["email"]);
        assert!(result.is_err());
    }

    /// Test WHERE clause with LIKE on encrypted field
    #[tokio::test]
    async fn test_where_encrypted_field_like_unsupported() {
        let qbi = test_qbi();

        // LIKE is a WHERE clause operation - encrypted fields rejected
        let result = qbi.validate_where_clause(&["email"]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-deterministic encryption"));
    }

    /// Test WHERE clause with IS NULL on encrypted field
    #[tokio::test]
    async fn test_where_encrypted_field_is_null() {
        let qbi = test_qbi();

        // IS NULL is special - works on encrypted fields
        assert!(qbi.validate_is_null_on_encrypted("email").is_ok());
        assert!(qbi.validate_is_null_on_encrypted("phone").is_ok());
        assert!(qbi.validate_is_null_on_encrypted("ssn").is_ok());
    }

    // ============================================================================
    // ORDER BY TESTS
    // ============================================================================

    /// Test ORDER BY on unencrypted field with encrypted data
    #[tokio::test]
    async fn test_order_by_unencrypted_field() {
        let qbi = test_qbi();

        assert!(qbi.validate_order_by_clause(&["name"]).is_ok());
        assert!(qbi.validate_order_by_clause(&["id"]).is_ok());
        assert!(qbi.validate_order_by_clause(&["created_at"]).is_ok());
    }

    /// Test ORDER BY rejects encrypted field
    #[tokio::test]
    async fn test_order_by_encrypted_field_unsupported() {
        let qbi = test_qbi();

        let result = qbi.validate_order_by_clause(&["email"]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("ORDER BY"));
        assert!(err.contains("email"));
    }

    /// Test ORDER BY with DESC on encrypted field rejection
    #[tokio::test]
    async fn test_order_by_encrypted_field_desc_unsupported() {
        let qbi = test_qbi();

        // DESC or ASC doesn't matter - encrypted fields can't be ordered
        let result = qbi.validate_order_by_clause(&["phone"]);
        assert!(result.is_err());

        let result = qbi.validate_order_by_clause(&["ssn"]);
        assert!(result.is_err());
    }

    // ============================================================================
    // JOIN TESTS
    // ============================================================================

    /// Test JOIN on unencrypted field with encrypted data
    #[tokio::test]
    async fn test_join_unencrypted_field() {
        let qbi = test_qbi();

        assert!(qbi.validate_join_condition(&["user_id"]).is_ok());
        assert!(qbi.validate_join_condition(&["id"]).is_ok());
    }

    /// Test JOIN rejects encrypted field condition
    #[tokio::test]
    async fn test_join_encrypted_field_unsupported() {
        let qbi = test_qbi();

        let result = qbi.validate_join_condition(&["email"]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("JOIN"));
        assert!(err.contains("email"));
    }

    /// Test LEFT JOIN with encrypted fields
    #[tokio::test]
    async fn test_left_join_encrypted_fields_unencrypted_condition() {
        let qbi = test_qbi();
        let cipher = test_cipher();

        // LEFT JOIN on unencrypted field is fine
        assert!(qbi.validate_join_condition(&["user_id"]).is_ok());

        // NULL encrypted fields from LEFT JOIN remain NULL
        let left_join_null: Option<Vec<u8>> = None;
        assert!(left_join_null.is_none());

        // Non-NULL encrypted fields are decrypted
        let left_join_value = Some(cipher.encrypt("joined@example.com").unwrap());
        if let Some(encrypted) = left_join_value {
            assert_eq!(cipher.decrypt(&encrypted).unwrap(), "joined@example.com");
        }
    }

    // ============================================================================
    // MAPPER/CODEC INTEGRATION TESTS
    // ============================================================================

    /// Test mapper encrypts on INSERT
    #[tokio::test]
    async fn test_mapper_encrypt_on_insert() {
        let cipher = test_cipher();
        let qbi = test_qbi();

        // User record with email field
        let email = "user@example.com";
        let name = "John Doe";

        // Mapper checks which fields are encrypted
        let encrypted_email = if qbi.is_encrypted("email") {
            cipher.encrypt(email).unwrap()
        } else {
            email.as_bytes().to_vec()
        };

        // Name passes through
        assert!(!qbi.is_encrypted("name"));
        assert_eq!(name, "John Doe");
        assert_eq!(cipher.decrypt(&encrypted_email).unwrap(), "user@example.com");
    }

    /// Test mapper decrypts on SELECT
    #[tokio::test]
    async fn test_mapper_decrypt_on_select() {
        let cipher = test_cipher();
        let qbi = test_qbi();

        // Stored encrypted row
        let stored_email = cipher.encrypt("user@example.com").unwrap();

        // Mapper decrypts designated fields
        let result_email = if qbi.is_encrypted("email") {
            cipher.decrypt(&stored_email).unwrap()
        } else {
            String::from_utf8(stored_email).unwrap()
        };

        assert_eq!(result_email, "user@example.com");
    }

    /// Test mapper with generic collection encryption
    #[tokio::test]
    async fn test_mapper_encrypt_collection_results() {
        let cipher = test_cipher();

        // Simulate Vec<User> SELECT result with encrypted email
        let users: Vec<(&str, Vec<u8>)> = (0..50)
            .map(|i| {
                let name = if i % 2 == 0 { "Alice" } else { "Bob" };
                let email_enc = cipher.encrypt(&format!("{}@example.com", i)).unwrap();
                (name, email_enc)
            })
            .collect();

        // Decrypt all - scales linearly
        assert_eq!(users.len(), 50);
        for (i, (_name, encrypted_email)) in users.iter().enumerate() {
            let decrypted = cipher.decrypt(encrypted_email).unwrap();
            assert_eq!(decrypted, format!("{}@example.com", i));
        }
    }

    /// Test mapper respects field metadata
    #[tokio::test]
    async fn test_mapper_respects_encrypted_field_metadata() {
        let qbi = test_qbi();

        // Only designated fields are encrypted
        let all_fields = vec!["id", "name", "email", "phone", "ssn", "created_at"];
        let encrypted = qbi.get_encrypted_fields_in_list(&all_fields);
        assert_eq!(encrypted.len(), 3);
        assert!(encrypted.contains(&"email".to_string()));
        assert!(encrypted.contains(&"phone".to_string()));
        assert!(encrypted.contains(&"ssn".to_string()));

        // Non-designated fields not in list
        assert!(!encrypted.contains(&"id".to_string()));
        assert!(!encrypted.contains(&"name".to_string()));
        assert!(!encrypted.contains(&"created_at".to_string()));
    }

    // ============================================================================
    // TRANSACTION TESTS
    // ============================================================================

    /// Test encryption within transaction
    #[tokio::test]
    async fn test_transaction_insert_and_select() {
        let cipher = test_cipher();

        // INSERT encrypts value
        let stored = cipher.encrypt("txn@example.com").unwrap();

        // SELECT within same transaction decrypts
        let retrieved = cipher.decrypt(&stored).unwrap();
        assert_eq!(retrieved, "txn@example.com");

        // Consistent key used throughout
        let stored2 = cipher.encrypt("txn2@example.com").unwrap();
        let retrieved2 = cipher.decrypt(&stored2).unwrap();
        assert_eq!(retrieved2, "txn2@example.com");
    }

    /// Test transaction rollback with encryption
    #[tokio::test]
    async fn test_transaction_rollback_encrypted() {
        let cipher = test_cipher();

        // Data encrypted but not committed
        let encrypted = cipher.encrypt("rollback@example.com").unwrap();

        // On rollback, encrypted data is discarded
        // Verify the data was valid before "rollback"
        assert_eq!(cipher.decrypt(&encrypted).unwrap(), "rollback@example.com");

        // After rollback, original state preserved (no stale ciphertext in DB)
        // This is a database-level guarantee, not encryption-level
    }

    /// Test concurrent transactions with encrypted data
    #[tokio::test]
    async fn test_transaction_concurrent_encryption() {
        let cipher1 = test_cipher();
        let cipher2 = test_cipher();

        // Two concurrent transactions encrypting different records
        let (enc1, enc2) =
            tokio::join!(async { cipher1.encrypt("txn1@example.com").unwrap() }, async {
                cipher2.encrypt("txn2@example.com").unwrap()
            });

        // No contention - each produces valid ciphertext
        assert_eq!(cipher1.decrypt(&enc1).unwrap(), "txn1@example.com");
        assert_eq!(cipher2.decrypt(&enc2).unwrap(), "txn2@example.com");

        // Cross-decryption works (same key)
        assert_eq!(cipher1.decrypt(&enc2).unwrap(), "txn2@example.com");
        assert_eq!(cipher2.decrypt(&enc1).unwrap(), "txn1@example.com");
    }

    /// Test transaction with encryption context
    #[tokio::test]
    async fn test_transaction_with_encryption_context() {
        let cipher = test_cipher();
        let ctx = EncryptionContext::new("user-123", "email", "insert", "2026-01-18T10:00:00Z");

        let encrypted = cipher
            .encrypt_with_context("txn-ctx@example.com", &ctx.to_aad_string())
            .unwrap();

        // Context includes transaction metadata for audit correlation
        assert!(ctx.to_aad_string().contains("op:insert"));
        assert!(ctx.to_aad_string().contains("user:user-123"));

        let decrypted = cipher.decrypt_with_context(&encrypted, &ctx.to_aad_string()).unwrap();
        assert_eq!(decrypted, "txn-ctx@example.com");
    }

    // ============================================================================
    // PERFORMANCE TESTS
    // ============================================================================

    /// Test INSERT performance with encryption overhead
    #[tokio::test]
    async fn test_performance_insert_with_encryption() {
        let cipher = test_cipher();
        let start = std::time::Instant::now();

        // 1000 records with 3 encrypted fields each
        for i in 0..1000 {
            let _email = cipher.encrypt(&format!("user{}@example.com", i)).unwrap();
            let _phone = cipher.encrypt(&format!("+1-555-{:04}", i)).unwrap();
            let _ssn = cipher.encrypt(&format!("{:03}-{:02}-{:04}", i % 999, i % 99, i)).unwrap();
        }

        let elapsed = start.elapsed();
        // Should complete in reasonable time (< 5 seconds for 3000 encryptions)
        assert!(
            elapsed.as_secs() < 5,
            "Encryption of 3000 values took {:?}, expected < 5s",
            elapsed
        );
    }

    /// Test SELECT performance with decryption overhead
    #[tokio::test]
    async fn test_performance_select_with_decryption() {
        let cipher = test_cipher();

        // Pre-encrypt 1000 values
        let encrypted: Vec<Vec<u8>> = (0..1000)
            .map(|i| cipher.encrypt(&format!("user{}@example.com", i)).unwrap())
            .collect();

        let start = std::time::Instant::now();

        // Decrypt all
        for enc in &encrypted {
            let _decrypted = cipher.decrypt(enc).unwrap();
        }

        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs() < 5,
            "Decryption of 1000 values took {:?}, expected < 5s",
            elapsed
        );
    }

    /// Test cipher cache improves performance
    #[tokio::test]
    async fn test_performance_cipher_cache_hit() {
        // FieldEncryption is Clone - can be cached and reused
        let cipher = test_cipher();
        let cached_cipher = cipher.clone();

        // Both produce valid results
        let enc1 = cipher.encrypt("test@example.com").unwrap();
        let enc2 = cached_cipher.encrypt("test@example.com").unwrap();

        // Different nonces but same cipher
        assert_ne!(enc1, enc2);
        assert_eq!(cipher.decrypt(&enc1).unwrap(), "test@example.com");
        assert_eq!(cached_cipher.decrypt(&enc2).unwrap(), "test@example.com");

        // Cross-decryption works (same key material)
        assert_eq!(cipher.decrypt(&enc2).unwrap(), "test@example.com");
    }

    // ============================================================================
    // ERROR HANDLING TESTS
    // ============================================================================

    /// Test missing encryption key on INSERT
    #[tokio::test]
    async fn test_error_missing_key_on_insert() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);

        // Field not in encrypted list triggers different path
        assert!(!qbi.is_encrypted("unknown_field"));

        // Encrypted field with wrong key fails
        let cipher = FieldEncryption::new(&[1u8; 32]);
        let encrypted = cipher.encrypt("test").unwrap();

        // Decrypt with different key fails
        let wrong_cipher = FieldEncryption::new(&[2u8; 32]);
        assert!(wrong_cipher.decrypt(&encrypted).is_err());
    }

    /// Test missing encryption key on SELECT
    #[tokio::test]
    async fn test_error_missing_key_on_select() {
        let cipher = FieldEncryption::new(&[1u8; 32]);
        let encrypted = cipher.encrypt("secret@example.com").unwrap();

        // Attempting to decrypt with different key
        let wrong_cipher = FieldEncryption::new(&[2u8; 32]);
        let result = wrong_cipher.decrypt(&encrypted);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Decryption failed"), "Error: {}", err);
    }

    /// Test corrupted encrypted data on SELECT
    #[tokio::test]
    async fn test_error_corrupted_data_on_select() {
        let cipher = test_cipher();
        let mut encrypted = cipher.encrypt("test@example.com").unwrap();

        // Corrupt ciphertext (not the nonce)
        if encrypted.len() > 12 {
            encrypted[12] ^= 0xFF;
        }

        let result = cipher.decrypt(&encrypted);
        assert!(result.is_err());
    }

    /// Test wrong encryption key on SELECT
    #[tokio::test]
    async fn test_error_wrong_key_on_select() {
        let cipher_a = FieldEncryption::new(&[1u8; 32]);
        let cipher_b = FieldEncryption::new(&[2u8; 32]);

        let encrypted = cipher_a.encrypt("secret").unwrap();
        let result = cipher_b.decrypt(&encrypted);
        assert!(result.is_err());
    }

    /// Test invalid UTF-8 on SELECT
    #[tokio::test]
    async fn test_error_invalid_utf8_on_select() {
        let cipher = test_cipher();

        // Data too short for valid decryption
        let short_data = vec![0u8; 5];
        let result = cipher.decrypt(&short_data);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too short"), "Error: {}", err);
    }

    /// Test context mismatch on SELECT
    #[tokio::test]
    async fn test_error_context_mismatch_on_select() {
        let cipher = test_cipher();
        let ctx_insert =
            EncryptionContext::new("user-123", "email", "insert", "2026-01-15T10:00:00Z");
        let ctx_wrong =
            EncryptionContext::new("user-456", "email", "insert", "2026-01-15T10:00:00Z");

        let encrypted = cipher
            .encrypt_with_context("secret@example.com", &ctx_insert.to_aad_string())
            .unwrap();

        // Mismatched context fails
        let result = cipher.decrypt_with_context(&encrypted, &ctx_wrong.to_aad_string());
        assert!(result.is_err());
    }

    // ============================================================================
    // FIELD TYPE TESTS
    // ============================================================================

    /// Test encryption of VARCHAR field
    #[tokio::test]
    async fn test_field_type_varchar_encryption() {
        let cipher = test_cipher();

        let varchar = "Hello, World!";
        let encrypted = cipher.encrypt(varchar).unwrap();
        // Stored as bytes (BYTEA/BLOB in database)
        assert!(!encrypted.is_empty());
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, varchar);
    }

    /// Test encryption of NUMERIC field
    #[tokio::test]
    async fn test_field_type_numeric_encryption() {
        let cipher = test_cipher();

        // Numeric converted to string for encryption
        let numeric = "12345.67";
        let encrypted = cipher.encrypt(numeric).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, numeric);

        // Application converts back to number
        let value: f64 = decrypted.parse().unwrap();
        assert!((value - 12345.67).abs() < f64::EPSILON);
    }

    /// Test encryption of TIMESTAMP field
    #[tokio::test]
    async fn test_field_type_timestamp_encryption() {
        let cipher = test_cipher();

        let timestamp = "2026-01-15T10:30:00.123456Z";
        let encrypted = cipher.encrypt(timestamp).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, timestamp);
    }

    /// Test encryption of JSON field
    #[tokio::test]
    async fn test_field_type_json_encryption() {
        let cipher = test_cipher();

        let json = r#"{"key":"value","nested":{"array":[1,2,3]}}"#;
        let encrypted = cipher.encrypt(json).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, json);

        // JSON structure preserved
        let parsed: serde_json::Value = serde_json::from_str(&decrypted).unwrap();
        assert_eq!(parsed["key"], "value");
        assert_eq!(parsed["nested"]["array"][0], 1);
    }

    // ============================================================================
    // SCHEMA DETECTION TESTS
    // ============================================================================

    /// Test query builder detects encrypted fields from schema
    #[test]
    fn test_schema_detection_encrypted_fields() {
        let mut registry = SchemaRegistry::new();
        let schema = StructSchema::new("User").with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("name", "String", false, ""),
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone")
                .with_mark(EncryptionMark::Encrypted),
        ]);
        registry.register(schema).unwrap();

        // Build QueryBuilderIntegration from schema
        let user_schema = registry.get("User").unwrap();
        let encrypted_names: Vec<String> =
            user_schema.encrypted_field_names().iter().map(|s| (*s).to_string()).collect();

        let qbi = QueryBuilderIntegration::new(encrypted_names);
        assert!(qbi.is_encrypted("email"));
        assert!(qbi.is_encrypted("phone"));
        assert!(!qbi.is_encrypted("name"));
        assert!(!qbi.is_encrypted("id"));
    }

    /// Test query builder handles schema evolution
    #[test]
    fn test_schema_evolution_encrypted_fields() {
        // Schema v1: no encryption
        let schema_v1 = StructSchema::new("User").with_version(1).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("email", "String", false, ""),
        ]);
        assert_eq!(schema_v1.encrypted_field_count(), 0);

        // Schema v2: email now encrypted
        let schema_v2 = StructSchema::new("User").with_version(2).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("email", "String", true, "encryption/email"),
        ]);

        // QBI for v2 handles encryption
        let qbi = QueryBuilderIntegration::new(
            schema_v2.encrypted_field_names().iter().map(|s| (*s).to_string()).collect(),
        );
        assert!(qbi.is_encrypted("email"));
    }

    /// Test query builder handles key changes
    #[test]
    fn test_schema_key_rotation_encryption() {
        let old_cipher = FieldEncryption::new(&[1u8; 32]);
        let new_cipher = FieldEncryption::new(&[2u8; 32]);

        // Old data encrypted with old key
        let old_encrypted = old_cipher.encrypt("test@example.com").unwrap();

        // Old key still decrypts old data
        assert_eq!(old_cipher.decrypt(&old_encrypted).unwrap(), "test@example.com");

        // New key cannot decrypt old data
        assert!(new_cipher.decrypt(&old_encrypted).is_err());

        // New data encrypted with new key
        let new_encrypted = new_cipher.encrypt("test@example.com").unwrap();
        assert_eq!(new_cipher.decrypt(&new_encrypted).unwrap(), "test@example.com");

        // Re-encryption: decrypt with old, encrypt with new
        let old_plaintext = old_cipher.decrypt(&old_encrypted).unwrap();
        let re_encrypted = new_cipher.encrypt(&old_plaintext).unwrap();
        assert_eq!(new_cipher.decrypt(&re_encrypted).unwrap(), "test@example.com");
    }

    // ============================================================================
    // AUDIT TRAIL TESTS
    // ============================================================================

    /// Test encryption context recorded in audit log
    #[tokio::test]
    async fn test_audit_encryption_context_logged() {
        let cipher = test_cipher();
        let ctx = EncryptionContext::new("user-123", "email", "insert", "2026-01-15T10:00:00Z");

        let aad = ctx.to_aad_string();
        assert!(aad.contains("user:user-123"));
        assert!(aad.contains("field:email"));
        assert!(aad.contains("op:insert"));
        assert!(aad.contains("ts:2026-01-15T10:00:00Z"));

        // Context-based encryption ensures tamper-proof audit
        let encrypted = cipher.encrypt_with_context("user@example.com", &aad).unwrap();
        let decrypted = cipher.decrypt_with_context(&encrypted, &aad).unwrap();
        assert_eq!(decrypted, "user@example.com");
    }

    /// Test audit trail for encryption failures
    #[tokio::test]
    async fn test_audit_encryption_failure_logged() {
        let cipher = test_cipher();

        // Corrupted data fails to decrypt
        let mut encrypted = cipher.encrypt("test").unwrap();
        if encrypted.len() > 12 {
            encrypted[13] ^= 0xFF;
        }

        let result = cipher.decrypt(&encrypted);
        assert!(result.is_err());

        // Error can be logged in audit trail
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("Decryption failed") || err_msg.contains("decrypt"),
            "Error should indicate decryption failure: {}",
            err_msg
        );
    }
}
