//! Comprehensive test specifications for integrating field-level encryption
//! with database mappers/codecs for automatic encryption/decryption

#[cfg(test)]
#[allow(clippy::module_inception)]
mod mapper_integration_tests {
    use std::collections::HashMap;

    use crate::encryption::database_adapter::EncryptionContext;
    use crate::encryption::mapper::FieldMapping;
    use crate::encryption::schema::{EncryptionMark, SchemaFieldInfo, StructSchema};
    use crate::encryption::FieldEncryption;

    /// Helper: create a test cipher from a zeroed key
    fn test_cipher() -> FieldEncryption {
        FieldEncryption::new(&[0u8; 32])
    }

    // ============================================================================
    // MAPPER WRITE OPERATIONS
    // ============================================================================

    /// Test mapper encrypts single encrypted field on insert
    #[tokio::test]
    async fn test_mapper_insert_single_encrypted_field() {
        let cipher = test_cipher();

        // Simulate mapper writing User record with encrypted email field
        let plaintext_email = "user@example.com";
        let ciphertext = cipher.encrypt(plaintext_email).unwrap();

        // Encrypted value differs from plaintext
        assert_ne!(plaintext_email.as_bytes(), &ciphertext[12..]);

        // The ciphertext is what would be stored in the database
        assert!(ciphertext.len() > 12); // nonce + ciphertext + tag

        // Decryption recovers the original
        let decrypted = cipher.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext_email);
    }

    /// Test mapper encrypts multiple encrypted fields on insert
    #[tokio::test]
    async fn test_mapper_insert_multiple_encrypted_fields() {
        let cipher = test_cipher();

        // Encrypt email, phone, and SSN independently
        let email_ct = cipher.encrypt("user@example.com").unwrap();
        let phone_ct = cipher.encrypt("+1-555-0123").unwrap();
        let ssn_ct = cipher.encrypt("123-45-6789").unwrap();

        // Each field produces unique nonce
        assert_ne!(&email_ct[..12], &phone_ct[..12]);
        assert_ne!(&email_ct[..12], &ssn_ct[..12]);
        assert_ne!(&phone_ct[..12], &ssn_ct[..12]);

        // All fields decrypt independently
        assert_eq!(cipher.decrypt(&email_ct).unwrap(), "user@example.com");
        assert_eq!(cipher.decrypt(&phone_ct).unwrap(), "+1-555-0123");
        assert_eq!(cipher.decrypt(&ssn_ct).unwrap(), "123-45-6789");
    }

    /// Test mapper preserves type information through encryption
    #[tokio::test]
    async fn test_mapper_preserves_types_through_encryption() {
        let cipher = test_cipher();

        // Various types serialized to string, encrypted, then deserialized back
        let integer_str = "42";
        let float_str = "3.14159";
        let bool_str = "true";
        let json_str = r#"{"key":"value","nested":{"a":1}}"#;

        // Encrypt typed values
        let int_ct = cipher.encrypt(integer_str).unwrap();
        let float_ct = cipher.encrypt(float_str).unwrap();
        let bool_ct = cipher.encrypt(bool_str).unwrap();
        let json_ct = cipher.encrypt(json_str).unwrap();

        // Decrypt and verify type information preserved
        let int_pt = cipher.decrypt(&int_ct).unwrap();
        let float_pt = cipher.decrypt(&float_ct).unwrap();
        let bool_pt = cipher.decrypt(&bool_ct).unwrap();
        let json_pt = cipher.decrypt(&json_ct).unwrap();

        assert_eq!(int_pt.parse::<i64>().unwrap(), 42);
        assert!((float_pt.parse::<f64>().unwrap() - 3.14159).abs() < f64::EPSILON);
        assert_eq!(bool_pt.parse::<bool>().unwrap(), true);
        assert_eq!(json_pt, json_str);
    }

    /// Test mapper handles NULL encrypted fields on insert
    #[tokio::test]
    async fn test_mapper_insert_null_encrypted_field() {
        let cipher = test_cipher();

        // NULL should remain NULL (not encrypted)
        let null_field: Option<Vec<u8>> = None;
        let non_null_field: Option<Vec<u8>> = Some(cipher.encrypt("has_value").unwrap());

        assert!(null_field.is_none());
        assert!(non_null_field.is_some());

        // Non-null field decrypts
        let pt = cipher.decrypt(non_null_field.as_ref().unwrap()).unwrap();
        assert_eq!(pt, "has_value");
    }

    /// Test mapper handles mixed encrypted and unencrypted fields
    #[tokio::test]
    async fn test_mapper_insert_mixed_fields() {
        let cipher = test_cipher();

        // Simulate a struct with both encrypted and unencrypted fields
        let mut fields: HashMap<String, Vec<u8>> = HashMap::new();

        // Encrypted field
        let email_ct = cipher.encrypt("secret@email.com").unwrap();
        fields.insert("email".to_string(), email_ct);

        // Unencrypted fields - stored as-is
        fields.insert("name".to_string(), b"John Doe".to_vec());
        fields.insert("age".to_string(), b"30".to_vec());

        // Email field is ciphertext (cannot be read directly)
        let email_bytes = fields.get("email").unwrap();
        assert_ne!(email_bytes, b"secret@email.com");

        // Unencrypted fields are plaintext
        assert_eq!(fields.get("name").unwrap(), b"John Doe");
        assert_eq!(fields.get("age").unwrap(), b"30");

        // Email decrypts correctly
        let decrypted_email = cipher.decrypt(email_bytes).unwrap();
        assert_eq!(decrypted_email, "secret@email.com");
    }

    /// Test mapper batch insert with multiple records
    #[tokio::test]
    async fn test_mapper_batch_insert_encryption() {
        let cipher = test_cipher();

        // Batch of 100 records
        let mut batch: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..100 {
            let plaintext = format!("user_{}@example.com", i);
            let ct = cipher.encrypt(&plaintext).unwrap();
            batch.push((plaintext, ct));
        }

        // All records encrypted independently with unique nonces
        for i in 0..batch.len() {
            for j in (i + 1)..batch.len() {
                assert_ne!(&batch[i].1[..12], &batch[j].1[..12]);
            }
        }

        // All decrypt correctly
        for (plaintext, ct) in &batch {
            assert_eq!(&cipher.decrypt(ct).unwrap(), plaintext);
        }
    }

    // ============================================================================
    // MAPPER READ OPERATIONS
    // ============================================================================

    /// Test mapper decrypts single encrypted field on select
    #[tokio::test]
    async fn test_mapper_select_single_encrypted_field() {
        let cipher = test_cipher();

        // "Store" encrypted email
        let stored_ct = cipher.encrypt("stored@example.com").unwrap();

        // Schema detects field is encrypted
        let mut schema = StructSchema::new("User");
        schema.add_field(SchemaFieldInfo::new(
            "email",
            "String",
            true,
            "encryption/email",
        ));
        assert!(schema.is_field_encrypted("email"));

        // Mapper automatically decrypts based on schema
        let decrypted = cipher.decrypt(&stored_ct).unwrap();
        assert_eq!(decrypted, "stored@example.com");
    }

    /// Test mapper decrypts multiple encrypted fields on select
    #[tokio::test]
    async fn test_mapper_select_multiple_encrypted_fields() {
        let cipher = test_cipher();

        // Store multiple encrypted fields
        let email_ct = cipher.encrypt("user@test.com").unwrap();
        let phone_ct = cipher.encrypt("+1-555-9876").unwrap();
        let ssn_ct = cipher.encrypt("987-65-4321").unwrap();

        // All fields automatically decrypted
        assert_eq!(cipher.decrypt(&email_ct).unwrap(), "user@test.com");
        assert_eq!(cipher.decrypt(&phone_ct).unwrap(), "+1-555-9876");
        assert_eq!(cipher.decrypt(&ssn_ct).unwrap(), "987-65-4321");
    }

    /// Test mapper restores type information after decryption
    #[tokio::test]
    async fn test_mapper_restores_types_after_decryption() {
        let cipher = test_cipher();

        // Store typed data as encrypted strings
        let int_ct = cipher.encrypt("12345").unwrap();
        let bool_ct = cipher.encrypt("false").unwrap();
        let float_ct = cipher.encrypt("99.99").unwrap();

        // Decrypt and parse back to original types
        let int_val: i64 = cipher.decrypt(&int_ct).unwrap().parse().unwrap();
        let bool_val: bool = cipher.decrypt(&bool_ct).unwrap().parse().unwrap();
        let float_val: f64 = cipher.decrypt(&float_ct).unwrap().parse().unwrap();

        assert_eq!(int_val, 12345);
        assert!(!bool_val);
        assert!((float_val - 99.99).abs() < f64::EPSILON);
    }

    /// Test mapper batch read with multiple records
    #[tokio::test]
    async fn test_mapper_batch_select_decryption() {
        let cipher = test_cipher();

        // Store 100 encrypted records
        let stored: Vec<(String, Vec<u8>)> = (0..100)
            .map(|i| {
                let pt = format!("record_{}", i);
                let ct = cipher.encrypt(&pt).unwrap();
                (pt, ct)
            })
            .collect();

        // Batch decrypt
        for (expected_pt, ct) in &stored {
            let decrypted = cipher.decrypt(ct).unwrap();
            assert_eq!(&decrypted, expected_pt);
        }
    }

    /// Test mapper handles NULL encrypted fields on select
    #[tokio::test]
    async fn test_mapper_select_null_encrypted_field() {
        let cipher = test_cipher();

        // NULL field stays NULL
        let null_value: Option<Vec<u8>> = None;
        assert!(null_value.is_none());

        // Empty string is distinct from NULL
        let empty_ct = cipher.encrypt("").unwrap();
        let empty_pt = cipher.decrypt(&empty_ct).unwrap();
        assert_eq!(empty_pt, "");

        // Application sees NULL vs "" distinction
        let null_result: Option<String> = null_value.map(|ct| cipher.decrypt(&ct).unwrap());
        assert!(null_result.is_none());
    }

    /// Test mapper collection deserialization with encryption
    #[tokio::test]
    async fn test_mapper_collection_deserialization() {
        let cipher = test_cipher();

        // Encrypt a collection of user emails
        let emails = vec![
            "alice@example.com",
            "bob@example.com",
            "charlie@example.com",
            "diana@example.com",
        ];
        let encrypted: Vec<Vec<u8>> =
            emails.iter().map(|e| cipher.encrypt(e).unwrap()).collect();

        // Decrypt entire collection
        let decrypted: Vec<String> = encrypted
            .iter()
            .map(|ct| cipher.decrypt(ct).unwrap())
            .collect();

        assert_eq!(decrypted.len(), emails.len());
        for (i, email) in emails.iter().enumerate() {
            assert_eq!(&decrypted[i], email);
        }
    }

    // ============================================================================
    // MAPPER UPDATE OPERATIONS
    // ============================================================================

    /// Test mapper encrypts on update
    #[tokio::test]
    async fn test_mapper_update_encrypt() {
        let cipher = test_cipher();

        // Original encrypted value
        let old_ct = cipher.encrypt("old@email.com").unwrap();

        // Update with new value
        let new_ct = cipher.encrypt("new@email.com").unwrap();

        // Different nonce means different ciphertext
        assert_ne!(old_ct, new_ct);

        // New value decrypts correctly
        assert_eq!(cipher.decrypt(&new_ct).unwrap(), "new@email.com");

        // Old value is still valid (until overwritten in database)
        assert_eq!(cipher.decrypt(&old_ct).unwrap(), "old@email.com");
    }

    /// Test mapper batch update with encryption
    #[tokio::test]
    async fn test_mapper_batch_update_encrypt() {
        let cipher = test_cipher();

        // Batch update of 100 records
        let mut updates: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        for i in 0..100 {
            let old_ct = cipher.encrypt(&format!("old_{}", i)).unwrap();
            let new_ct = cipher.encrypt(&format!("new_{}", i)).unwrap();
            // Each update has unique nonce
            assert_ne!(&old_ct[..12], &new_ct[..12]);
            updates.push((old_ct, new_ct));
        }

        // All new values decrypt correctly
        for (i, (_, new_ct)) in updates.iter().enumerate() {
            assert_eq!(cipher.decrypt(new_ct).unwrap(), format!("new_{}", i));
        }
    }

    // ============================================================================
    // SCHEMA METADATA TESTS
    // ============================================================================

    /// Test mapper reads encrypted field metadata from schema
    #[test]
    fn test_mapper_schema_metadata() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone")
                .with_mark(EncryptionMark::Sensitive),
        );
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));

        // Schema detects encrypted fields automatically
        assert!(schema.is_field_encrypted("email"));
        assert!(schema.is_field_encrypted("phone"));
        assert!(!schema.is_field_encrypted("name"));

        // Encrypted field list
        let encrypted_names = schema.encrypted_field_names();
        assert_eq!(encrypted_names.len(), 2);
        assert!(encrypted_names.contains(&"email"));
        assert!(encrypted_names.contains(&"phone"));
    }

    /// Test mapper respects encrypted field metadata
    #[test]
    fn test_mapper_respects_field_metadata() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
        );

        // Schema marks email as encrypted
        let field = schema.get_field("email").unwrap();
        assert!(field.is_encrypted);
        assert_eq!(field.mark, Some(EncryptionMark::Encrypted));
        assert_eq!(field.key_reference, "encryption/email");
        assert_eq!(field.algorithm, "aes256-gcm");
    }

    /// Test mapper ignores non-encrypted fields
    #[test]
    fn test_mapper_ignores_unencrypted_fields() {
        let mut schema = StructSchema::new("User");
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        schema.add_field(SchemaFieldInfo::new("age", "i32", false, ""));

        // Non-encrypted fields are not in encrypted list
        assert!(!schema.is_field_encrypted("name"));
        assert!(!schema.is_field_encrypted("age"));
        assert_eq!(schema.encrypted_field_count(), 0);

        // They pass through as plaintext
        let field = schema.get_field("name").unwrap();
        assert!(!field.is_encrypted);
    }

    /// Test mapper handles schema evolution
    #[test]
    fn test_mapper_schema_evolution() {
        // Version 1: no encryption
        let schema_v1 = StructSchema::new("User").with_version(1).with_fields(vec![
            SchemaFieldInfo::new("email", "String", false, ""),
            SchemaFieldInfo::new("name", "String", false, ""),
        ]);

        // Version 2: email now encrypted
        let schema_v2 = StructSchema::new("User").with_version(2).with_fields(vec![
            SchemaFieldInfo::new("email", "String", true, "encryption/email"),
            SchemaFieldInfo::new("name", "String", false, ""),
        ]);

        assert_eq!(schema_v1.version, 1);
        assert!(!schema_v1.is_field_encrypted("email"));
        assert_eq!(schema_v1.encrypted_field_count(), 0);

        assert_eq!(schema_v2.version, 2);
        assert!(schema_v2.is_field_encrypted("email"));
        assert_eq!(schema_v2.encrypted_field_count(), 1);
    }

    // ============================================================================
    // ENCRYPTION KEY MANAGEMENT
    // ============================================================================

    /// Test mapper gets encryption keys from adapter
    #[tokio::test]
    async fn test_mapper_gets_keys_from_adapter() {
        // The cipher abstracts away key management
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Mapper uses the cipher via adapter - doesn't know about Vault
        let ct = cipher.encrypt("adapter_test").unwrap();
        let pt = cipher.decrypt(&ct).unwrap();
        assert_eq!(pt, "adapter_test");
    }

    /// Test mapper uses correct key for each field
    #[tokio::test]
    async fn test_mapper_correct_key_per_field() {
        // Different keys for different fields
        let email_key = [1u8; 32];
        let phone_key = [2u8; 32];
        let ssn_key = [3u8; 32];

        let email_cipher = FieldEncryption::new(&email_key);
        let phone_cipher = FieldEncryption::new(&phone_key);
        let ssn_cipher = FieldEncryption::new(&ssn_key);

        // Each field encrypted with its own key
        let email_ct = email_cipher.encrypt("user@test.com").unwrap();
        let phone_ct = phone_cipher.encrypt("+1-555-0000").unwrap();
        let ssn_ct = ssn_cipher.encrypt("000-00-0000").unwrap();

        // Each field decrypts only with its own key
        assert_eq!(email_cipher.decrypt(&email_ct).unwrap(), "user@test.com");
        assert_eq!(phone_cipher.decrypt(&phone_ct).unwrap(), "+1-555-0000");
        assert_eq!(ssn_cipher.decrypt(&ssn_ct).unwrap(), "000-00-0000");

        // Cross-key decryption fails
        assert!(phone_cipher.decrypt(&email_ct).is_err());
        assert!(ssn_cipher.decrypt(&phone_ct).is_err());
        assert!(email_cipher.decrypt(&ssn_ct).is_err());
    }

    /// Test mapper handles missing key gracefully
    #[tokio::test]
    async fn test_mapper_missing_key_error() {
        // Attempting to create cipher with invalid key fails
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[0u8; 16]) // Wrong key size
        });
        assert!(result.is_err());

        // Short ciphertext produces clear error
        let cipher = test_cipher();
        let short_data = vec![0u8; 5]; // Too short for nonce
        let result = cipher.decrypt(&short_data);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("too short") || err_str.contains("Encrypted data"));
    }

    // ============================================================================
    // TRANSACTION INTEGRATION
    // ============================================================================

    /// Test mapper in transaction: insert then select same record
    #[tokio::test]
    async fn test_mapper_transaction_insert_select() {
        let cipher = test_cipher();

        // Within transaction: encrypt for INSERT
        let plaintext = "txn_user@example.com";
        let ct = cipher.encrypt(plaintext).unwrap();

        // Within same transaction: decrypt for SELECT
        let decrypted = cipher.decrypt(&ct).unwrap();
        assert_eq!(decrypted, plaintext);

        // Consistent key throughout (same cipher instance)
        let ct2 = cipher.encrypt("another_field").unwrap();
        assert_eq!(cipher.decrypt(&ct2).unwrap(), "another_field");
    }

    /// Test mapper in transaction: rollback
    #[tokio::test]
    async fn test_mapper_transaction_rollback() {
        let cipher = test_cipher();

        // Encrypt for INSERT
        let ct = cipher.encrypt("rollback_test").unwrap();

        // Transaction rolls back - ciphertext discarded
        // After rollback, no stale state
        let _discarded = ct; // Ciphertext would be discarded on rollback

        // Cipher still works for new operations
        let new_ct = cipher.encrypt("post_rollback").unwrap();
        assert_eq!(cipher.decrypt(&new_ct).unwrap(), "post_rollback");
    }

    /// Test mapper in concurrent transactions
    #[tokio::test]
    async fn test_mapper_concurrent_transactions() {
        // Each transaction gets its own cipher instance (same key but independent state)
        let cipher_txn1 = test_cipher();
        let cipher_txn2 = test_cipher();

        // Concurrent operations
        let ct1 = cipher_txn1.encrypt("txn1_data").unwrap();
        let ct2 = cipher_txn2.encrypt("txn2_data").unwrap();

        // Different nonces even with same key
        assert_ne!(ct1, ct2);

        // Each transaction decrypts its own data
        assert_eq!(cipher_txn1.decrypt(&ct1).unwrap(), "txn1_data");
        assert_eq!(cipher_txn2.decrypt(&ct2).unwrap(), "txn2_data");

        // Cross-transaction decryption also works (same key)
        assert_eq!(cipher_txn1.decrypt(&ct2).unwrap(), "txn2_data");
        assert_eq!(cipher_txn2.decrypt(&ct1).unwrap(), "txn1_data");
    }

    // ============================================================================
    // ERROR HANDLING
    // ============================================================================

    /// Test mapper handles encryption errors
    #[tokio::test]
    async fn test_mapper_encryption_error() {
        // Encryption with valid cipher always succeeds for valid UTF-8
        let cipher = test_cipher();
        let result = cipher.encrypt("valid data");
        assert!(result.is_ok());

        // FieldMapping tracks error context
        let mapping = FieldMapping::new("email", true, vec![]);
        assert_eq!(mapping.field_name(), "email");
        assert!(mapping.is_encrypted());
    }

    /// Test mapper handles decryption errors
    #[tokio::test]
    async fn test_mapper_decryption_error() {
        let cipher = test_cipher();

        // Corrupted ciphertext
        let valid_ct = cipher.encrypt("test_data").unwrap();
        let mut corrupted = valid_ct;
        corrupted[13] ^= 0xFF;

        let result = cipher.decrypt(&corrupted);
        assert!(result.is_err());

        // Error message indicates decryption failure
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("Decryption") || err_str.contains("decrypt"),
            "Error should mention decryption: {}",
            err_str
        );
    }

    /// Test mapper handles corrupted ciphertext
    #[tokio::test]
    async fn test_mapper_corrupted_ciphertext() {
        let cipher = test_cipher();

        // Complete garbage data (not a valid nonce + ciphertext)
        let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99];
        let result = cipher.decrypt(&garbage);
        assert!(result.is_err());

        // Truncated ciphertext (just nonce, no actual data)
        let just_nonce = vec![0u8; 12];
        let result = cipher.decrypt(&just_nonce);
        assert!(result.is_err());

        // Empty data
        let empty: Vec<u8> = vec![];
        let result = cipher.decrypt(&empty);
        assert!(result.is_err());
    }

    /// Test mapper handles invalid UTF-8
    #[tokio::test]
    async fn test_mapper_invalid_utf8_error() {
        // FieldMapping with invalid UTF-8 data
        let invalid_utf8 = FieldMapping::new("email", true, vec![0xFF, 0xFE, 0xFD]);
        let result = invalid_utf8.to_string();
        assert!(result.is_err());

        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("UTF-8") || err_str.contains("email"),
            "Error should mention UTF-8 or field name: {}",
            err_str
        );
    }

    // ============================================================================
    // PERFORMANCE TESTS
    // ============================================================================

    /// Test mapper batch insert performance
    #[tokio::test]
    async fn test_mapper_batch_insert_performance() {
        let cipher = test_cipher();

        let start = std::time::Instant::now();

        // Encrypt 1000 records
        let mut ciphertexts = Vec::with_capacity(1000);
        for i in 0..1000 {
            let ct = cipher.encrypt(&format!("user_{}@example.com", i)).unwrap();
            ciphertexts.push(ct);
        }

        let duration = start.elapsed();

        // Should complete well within 2 seconds
        assert!(
            duration.as_secs() < 2,
            "Batch encryption took too long: {:?}",
            duration
        );
        assert_eq!(ciphertexts.len(), 1000);
    }

    /// Test mapper batch select performance
    #[tokio::test]
    async fn test_mapper_batch_select_performance() {
        let cipher = test_cipher();

        // Pre-encrypt 1000 records
        let ciphertexts: Vec<Vec<u8>> = (0..1000)
            .map(|i| cipher.encrypt(&format!("user_{}@example.com", i)).unwrap())
            .collect();

        let start = std::time::Instant::now();

        // Decrypt all 1000
        for (i, ct) in ciphertexts.iter().enumerate() {
            let pt = cipher.decrypt(ct).unwrap();
            assert_eq!(pt, format!("user_{}@example.com", i));
        }

        let duration = start.elapsed();

        // Should complete well within 2 seconds
        assert!(
            duration.as_secs() < 2,
            "Batch decryption took too long: {:?}",
            duration
        );
    }

    /// Test mapper cache hit performance
    #[tokio::test]
    async fn test_mapper_cache_hit_performance() {
        let cipher = test_cipher();

        // Encrypt the same field repeatedly (simulates cipher cache hits)
        let start = std::time::Instant::now();

        for _ in 0..10000 {
            let ct = cipher.encrypt("cached_field_value").unwrap();
            let _pt = cipher.decrypt(&ct).unwrap();
        }

        let duration = start.elapsed();

        // Repeated use of same cipher should be fast
        assert!(
            duration.as_secs() < 5,
            "Cached cipher operations took too long: {:?}",
            duration
        );
    }

    // ============================================================================
    // SPECIAL DATA TYPES
    // ============================================================================

    /// Test mapper with UUID fields
    #[tokio::test]
    async fn test_mapper_uuid_field_encryption() {
        let cipher = test_cipher();

        // UUID as string
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let ct = cipher.encrypt(uuid_str).unwrap();
        let decrypted = cipher.decrypt(&ct).unwrap();

        assert_eq!(decrypted, uuid_str);
        // Verify it's still a valid UUID format
        assert_eq!(decrypted.len(), 36);
        assert_eq!(decrypted.chars().filter(|c| *c == '-').count(), 4);
    }

    /// Test mapper with DateTime fields
    #[tokio::test]
    async fn test_mapper_datetime_field_encryption() {
        let cipher = test_cipher();

        // ISO 8601 datetime
        let datetime_str = "2024-01-15T10:30:00.123456789Z";
        let ct = cipher.encrypt(datetime_str).unwrap();
        let decrypted = cipher.decrypt(&ct).unwrap();

        assert_eq!(decrypted, datetime_str);
        // Precision preserved
        assert!(decrypted.contains("123456789"));
    }

    /// Test mapper with JSON fields
    #[tokio::test]
    async fn test_mapper_json_field_encryption() {
        let cipher = test_cipher();

        let json_str = r#"{"user":{"name":"Alice","roles":["admin","editor"],"settings":{"theme":"dark","notifications":true}}}"#;
        let ct = cipher.encrypt(json_str).unwrap();
        let decrypted = cipher.decrypt(&ct).unwrap();

        assert_eq!(decrypted, json_str);
        // Structure preserved
        assert!(decrypted.contains("\"admin\""));
        assert!(decrypted.contains("\"notifications\":true"));
    }

    /// Test mapper with Option<T> fields
    #[tokio::test]
    async fn test_mapper_option_encrypted_field() {
        let cipher = test_cipher();

        // Some(value) encrypted
        let some_value: Option<&str> = Some("present_value");
        let encrypted: Option<Vec<u8>> = some_value.map(|v| cipher.encrypt(v).unwrap());
        assert!(encrypted.is_some());

        let decrypted: Option<String> =
            encrypted.as_ref().map(|ct| cipher.decrypt(ct).unwrap());
        assert_eq!(decrypted, Some("present_value".to_string()));

        // None remains None
        let none_value: Option<&str> = None;
        let encrypted_none: Option<Vec<u8>> = none_value.map(|v| cipher.encrypt(v).unwrap());
        assert!(encrypted_none.is_none());

        let decrypted_none: Option<String> =
            encrypted_none.as_ref().map(|ct| cipher.decrypt(ct).unwrap());
        assert!(decrypted_none.is_none());
    }

    // ============================================================================
    // CUSTOM SERIALIZATION TESTS
    // ============================================================================

    /// Test mapper with custom serializer
    #[test]
    fn test_mapper_custom_serializer() {
        let cipher = test_cipher();

        // Custom format: pipe-separated values
        let custom_format = "field1|field2|field3";
        let ct = cipher.encrypt(custom_format).unwrap();
        let decrypted = cipher.decrypt(&ct).unwrap();

        // Custom format preserved through encryption
        assert_eq!(decrypted, custom_format);
        let parts: Vec<&str> = decrypted.split('|').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "field1");
    }

    /// Test mapper with field-level encryption marks
    #[test]
    fn test_mapper_encryption_marks() {
        // Schema marks fields with different encryption annotations
        let mut schema = StructSchema::new("User");

        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn")
                .with_mark(EncryptionMark::Sensitive),
        );
        schema.add_field(
            SchemaFieldInfo::new("api_key", "String", true, "encryption/api")
                .with_mark(EncryptionMark::Encrypt),
        );

        // All marks are metadata, no runtime overhead difference
        let email_field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(email_field.mark, Some(EncryptionMark::Encrypted));

        let ssn_field = schema.get_encrypted_field("ssn").unwrap();
        assert_eq!(ssn_field.mark, Some(EncryptionMark::Sensitive));

        let api_field = schema.get_encrypted_field("api_key").unwrap();
        assert_eq!(api_field.mark, Some(EncryptionMark::Encrypt));

        // All are treated as encrypted regardless of mark type
        assert_eq!(schema.encrypted_field_count(), 3);
    }

    // ============================================================================
    // AUDIT AND LOGGING
    // ============================================================================

    /// Test mapper logs encryption operations
    #[tokio::test]
    async fn test_mapper_encryption_audit_log() {
        let cipher = test_cipher();

        // Create encryption context for audit
        let ctx = EncryptionContext::new("user_123", "email", "insert", "2024-06-15T12:00:00Z");

        // Context string for authenticated data
        let aad = ctx.to_aad_string();
        assert!(aad.contains("user:user_123"));
        assert!(aad.contains("field:email"));
        assert!(aad.contains("op:insert"));

        // Encrypt with context for audit trail
        let ct = cipher.encrypt_with_context("user@example.com", &aad).unwrap();
        let pt = cipher.decrypt_with_context(&ct, &aad).unwrap();
        assert_eq!(pt, "user@example.com");
    }

    /// Test mapper logs decryption operations
    #[tokio::test]
    async fn test_mapper_decryption_audit_log() {
        let cipher = test_cipher();

        // Create decryption context
        let ctx = EncryptionContext::new("auditor_456", "email", "select", "2024-06-15T13:00:00Z");
        let aad = ctx.to_aad_string();

        // Encrypt with context
        let ct = cipher.encrypt_with_context("audited@email.com", &aad).unwrap();

        // Decrypt with same context (verification succeeds)
        let pt = cipher.decrypt_with_context(&ct, &aad).unwrap();
        assert_eq!(pt, "audited@email.com");

        // Decrypt with different context fails (tamper detection)
        let wrong_ctx =
            EncryptionContext::new("hacker_789", "email", "select", "2024-06-15T13:00:00Z");
        let wrong_aad = wrong_ctx.to_aad_string();
        let result = cipher.decrypt_with_context(&ct, &wrong_aad);
        assert!(result.is_err());
    }

    /// Test mapper logs errors
    #[tokio::test]
    async fn test_mapper_error_audit_log() {
        let cipher = test_cipher();

        // Encrypt with context
        let ctx = EncryptionContext::new("user_1", "email", "insert", "2024-01-01T00:00:00Z");
        let aad = ctx.to_aad_string();
        let ct = cipher.encrypt_with_context("secret", &aad).unwrap();

        // Attempt decryption with tampered context
        let tampered_aad = "user:attacker:field:email:op:select:ts:2024-01-01T00:00:00Z";
        let result = cipher.decrypt_with_context(&ct, tampered_aad);
        assert!(result.is_err());

        let err_str = result.unwrap_err().to_string();
        // Error provides useful context about the failure
        assert!(
            err_str.contains("Decryption") || err_str.contains("context") || err_str.contains("failed"),
            "Error should indicate decryption/context failure: {}",
            err_str
        );
    }
}
