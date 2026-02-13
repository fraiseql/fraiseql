//! Comprehensive test specifications for field-level encryption
//! testing AES-256-GCM encryption, database integration, and security properties

#[cfg(test)]
#[allow(clippy::module_inception)]
mod field_encryption_tests {
    use std::collections::HashSet;

    use crate::encryption::{
        FieldEncryption, database_adapter::EncryptionContext,
        query_builder::QueryBuilderIntegration,
    };

    const NONCE_SIZE: usize = 12;
    const TAG_SIZE: usize = 16;
    const KEY_SIZE: usize = 32;

    /// Helper: create a cipher with the zero key
    fn test_cipher() -> FieldEncryption {
        FieldEncryption::new(&[0u8; 32])
    }

    // ============================================================================
    // BASIC ENCRYPTION/DECRYPTION TESTS
    // ============================================================================

    /// Test basic field encryption roundtrip
    #[test]
    fn test_field_encrypt_decrypt_basic() {
        let cipher = test_cipher();
        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test encrypted data contains random nonce
    #[test]
    fn test_field_encryption_contains_nonce() {
        let cipher = test_cipher();
        let plaintext = "hello world";

        let enc1 = cipher.encrypt(plaintext).unwrap();
        let enc2 = cipher.encrypt(plaintext).unwrap();

        // The first 12 bytes are the nonce; they should differ between encryptions.
        let nonce1 = &enc1[..NONCE_SIZE];
        let nonce2 = &enc2[..NONCE_SIZE];
        assert_ne!(nonce1, nonce2, "nonces must differ between encryptions");

        // The full ciphertexts should also differ (different nonces produce different output).
        assert_ne!(enc1, enc2);

        // Both should decrypt to the same plaintext.
        assert_eq!(cipher.decrypt(&enc1).unwrap(), plaintext);
        assert_eq!(cipher.decrypt(&enc2).unwrap(), plaintext);
    }

    /// Test ciphertext is not plaintext
    #[test]
    fn test_field_encryption_output_not_plaintext() {
        let cipher = test_cipher();
        let plaintext = "sensitive data here";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Ciphertext (after nonce) should not contain the plaintext bytes.
        let ciphertext_portion = &encrypted[NONCE_SIZE..];
        assert_ne!(ciphertext_portion, plaintext.as_bytes());

        // Encrypted output includes nonce (12) + ciphertext (same length as plaintext) + tag (16),
        // so it should be longer than the plaintext.
        assert!(
            encrypted.len() > plaintext.len(),
            "encrypted output ({}) should be longer than plaintext ({})",
            encrypted.len(),
            plaintext.len()
        );

        // Verify exact expected length: NONCE_SIZE + plaintext.len() + TAG_SIZE
        assert_eq!(encrypted.len(), NONCE_SIZE + plaintext.len() + TAG_SIZE);

        // The full encrypted blob should not contain the plaintext as a substring.
        assert!(
            !encrypted.windows(plaintext.len()).any(|w| w == plaintext.as_bytes()),
            "encrypted data must not contain plaintext bytes"
        );
    }

    /// Test authenticated encryption prevents tampering
    #[test]
    fn test_field_encryption_detects_tampering() {
        let cipher = test_cipher();
        let plaintext = "do not tamper with me";
        let mut encrypted = cipher.encrypt(plaintext).unwrap();

        // Tamper with a byte in the ciphertext portion (after the nonce).
        let tamper_idx = NONCE_SIZE + 1;
        assert!(tamper_idx < encrypted.len());
        encrypted[tamper_idx] ^= 0xFF;

        let result = cipher.decrypt(&encrypted);
        assert!(result.is_err(), "decryption must fail after tampering");
    }

    // ============================================================================
    // SENSITIVE FIELD TYPE TESTS
    // ============================================================================

    /// Test email field encryption
    #[test]
    fn test_field_encrypt_email() {
        let cipher = test_cipher();
        let email = "john.doe+tag@example.co.uk";
        let encrypted = cipher.encrypt(email).unwrap();

        // Encrypted should be longer than email due to nonce + tag.
        assert!(encrypted.len() > email.len());
        assert_eq!(encrypted.len(), NONCE_SIZE + email.len() + TAG_SIZE);

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, email);
    }

    /// Test phone number encryption
    #[test]
    fn test_field_encrypt_phone_number() {
        let cipher = test_cipher();
        let phone_formats = [
            "+1-555-123-4567",
            "5551234567",
            "(555) 123-4567",
            "+44 20 7946 0958",
        ];

        for phone in &phone_formats {
            let encrypted = cipher.encrypt(phone).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(&decrypted, phone, "phone format must be preserved exactly");
        }
    }

    /// Test SSN/tax ID encryption
    #[test]
    fn test_field_encrypt_ssn() {
        let cipher = test_cipher();
        let ssn = "123-45-6789";
        let encrypted = cipher.encrypt(ssn).unwrap();

        // Ciphertext must not leak the SSN format (dashes, digit pattern).
        assert!(
            !encrypted.windows(ssn.len()).any(|w| w == ssn.as_bytes()),
            "ciphertext must not contain SSN plaintext"
        );

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, ssn, "SSN format must be preserved after decrypt");
    }

    /// Test credit card encryption
    #[test]
    fn test_field_encrypt_credit_card() {
        let cipher = test_cipher();
        let card_numbers = [
            "4532015112830366",    // 16 digits, no separators
            "4532 0151 1283 0366", // with spaces
            "4532-0151-1283-0366", // with dashes
            "3530111333300000",    // 16-digit JCB
            "4222222222222",       // 13-digit Visa
            "6011111111111111117", // 19-digit Discover
        ];

        for card in &card_numbers {
            let encrypted = cipher.encrypt(card).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(&decrypted, card, "credit card format must be preserved exactly");
        }
    }

    /// Test API key encryption
    #[test]
    fn test_field_encrypt_api_key() {
        let cipher = test_cipher();
        let api_keys = [
            // NOTE: Using obviously fake test keys to prevent accidental secret exposure
            "sk_test_XXXXXXXXXXXXXXXXXXXXXXXXXXXX",
            "pk_test_0000000000000000000000000000",
            &"a".repeat(128), // 128-character key
            &"x".repeat(256), // 256-character key
        ];

        for key in &api_keys {
            let encrypted = cipher.encrypt(key).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(&decrypted, key, "API key content must be preserved");
        }
    }

    /// Test OAuth token encryption
    #[test]
    fn test_field_encrypt_oauth_token() {
        let cipher = test_cipher();
        // JWT format: header.payload.signature
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
                   eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
                   SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let opaque = "ya29.a0AfH6SMBx4bKmnT2rZqyRRRRRRRRRR-AAAAAAAAAA_bbbbbbbbb";

        for token in &[jwt, opaque] {
            let encrypted = cipher.encrypt(token).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(&decrypted, token, "OAuth token must be preserved exactly");
        }
    }

    /// Test empty string encryption
    #[test]
    fn test_field_encrypt_empty_string() {
        let cipher = test_cipher();
        let encrypted = cipher.encrypt("").unwrap();

        // Even empty plaintext should produce nonce + tag.
        assert_eq!(encrypted.len(), NONCE_SIZE + TAG_SIZE);

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "");
    }

    /// Test special characters
    #[test]
    fn test_field_encrypt_special_characters() {
        let cipher = test_cipher();
        let special = "!@#$%^&*()_+-=[]{}|;':\",./<>?\\\n\t\r\0";
        let encrypted = cipher.encrypt(special).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, special);
    }

    /// Test unicode support
    #[test]
    fn test_field_encrypt_unicode() {
        let cipher = test_cipher();
        let test_strings = [
            "\u{4f60}\u{597d}\u{4e16}\u{754c}",                 // Chinese
            "\u{041f}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}", // Russian (Cyrillic)
            "\u{1f512}\u{1f510}\u{1f511}",                      // Lock emoji
            "\u{0639}\u{0631}\u{0628}\u{064a}",                 // Arabic
            "caf\u{00e9} na\u{00ef}ve r\u{00e9}sum\u{00e9}",    // Latin diacritics
            "\u{1f600}\u{1f60d}\u{1f525}\u{1f4a5}",             // More emoji
        ];

        for text in &test_strings {
            let encrypted = cipher.encrypt(text).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(
                decrypted.as_bytes(),
                text.as_bytes(),
                "unicode must be preserved byte-for-byte for: {}",
                text
            );
        }
    }

    // ============================================================================
    // CONTEXT-BASED ENCRYPTION TESTS
    // ============================================================================

    /// Test encryption with context
    #[test]
    fn test_field_encrypt_with_context() {
        let cipher = test_cipher();
        let plaintext = "sensitive@email.com";
        let context = "user:123:email";

        let encrypted = cipher.encrypt_with_context(plaintext, context).unwrap();
        let decrypted = cipher.decrypt_with_context(&encrypted, context).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    /// Test context verification prevents wrong context
    #[test]
    fn test_field_context_verification_strict() {
        let cipher = test_cipher();
        let plaintext = "my secret value";
        let correct_context = "user:100:password";
        let wrong_context = "user:999:password";

        let encrypted = cipher.encrypt_with_context(plaintext, correct_context).unwrap();

        // Decryption with wrong context must fail.
        let result = cipher.decrypt_with_context(&encrypted, wrong_context);
        assert!(result.is_err(), "decryption with wrong context must fail");

        // Ensure correct context still works.
        let decrypted = cipher.decrypt_with_context(&encrypted, correct_context).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test context supports audit trail use cases
    #[test]
    fn test_field_context_audit_information() {
        let cipher = test_cipher();
        let plaintext = "audit-test-value";

        let ctx1 = EncryptionContext::new("user1", "email", "insert", "2024-01-01T00:00:00Z");
        let ctx2 = EncryptionContext::new("user2", "email", "insert", "2024-01-01T00:00:01Z");

        let aad1 = ctx1.to_aad_string();
        let aad2 = ctx2.to_aad_string();

        // Context strings should differ.
        assert_ne!(aad1, aad2);

        // Same plaintext + different context = different authenticated data.
        let enc1 = cipher.encrypt_with_context(plaintext, &aad1).unwrap();
        let enc2 = cipher.encrypt_with_context(plaintext, &aad2).unwrap();

        // Each can only be decrypted with its own context.
        assert_eq!(cipher.decrypt_with_context(&enc1, &aad1).unwrap(), plaintext);
        assert_eq!(cipher.decrypt_with_context(&enc2, &aad2).unwrap(), plaintext);

        // Cross-context decryption must fail.
        assert!(cipher.decrypt_with_context(&enc1, &aad2).is_err());
        assert!(cipher.decrypt_with_context(&enc2, &aad1).is_err());

        // Ciphertext size should not increase from adding context (AAD is not stored).
        assert_eq!(enc1.len(), enc2.len());
        assert_eq!(enc1.len(), NONCE_SIZE + plaintext.len() + TAG_SIZE);
    }

    // ============================================================================
    // ERROR HANDLING TESTS
    // ============================================================================

    /// Test invalid key size
    #[test]
    fn test_field_invalid_key_size() {
        // 16 bytes (AES-128) should panic.
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[0u8; 16]);
        });
        assert!(result.is_err(), "16-byte key must panic");

        // 0 bytes should panic.
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[]);
        });
        assert!(result.is_err(), "empty key must panic");

        // 31 bytes should panic.
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[0u8; 31]);
        });
        assert!(result.is_err(), "31-byte key must panic");

        // 33 bytes should panic.
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[0u8; 33]);
        });
        assert!(result.is_err(), "33-byte key must panic");

        // Exactly 32 bytes should succeed.
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[0u8; 32]);
        });
        assert!(result.is_ok(), "32-byte key must succeed");
    }

    /// Test decryption of corrupted data
    #[test]
    fn test_field_corrupted_ciphertext_error() {
        let cipher = test_cipher();
        let plaintext = "corruption target";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Corrupt each section of the ciphertext and verify decryption fails.
        // Corrupt a byte in the ciphertext body.
        let mut corrupted = encrypted.clone();
        corrupted[NONCE_SIZE + 2] ^= 0x42;
        let result = cipher.decrypt(&corrupted);
        assert!(result.is_err());

        // Corrupt the last byte (part of the authentication tag).
        let mut corrupted = encrypted.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0x01;
        let result = cipher.decrypt(&corrupted);
        assert!(result.is_err());

        // Original should still work.
        assert_eq!(cipher.decrypt(&encrypted).unwrap(), plaintext);
    }

    /// Test decryption of truncated data
    #[test]
    fn test_field_truncated_ciphertext_error() {
        let cipher = test_cipher();

        // Too short to even contain a nonce.
        let result = cipher.decrypt(&[0u8; 5]);
        assert!(result.is_err());

        // Exactly nonce size but no ciphertext or tag.
        let result = cipher.decrypt(&[0u8; NONCE_SIZE]);
        assert!(result.is_err());

        // One byte short of the minimum valid message (nonce + tag).
        let result = cipher.decrypt(&[0u8; NONCE_SIZE + TAG_SIZE - 1]);
        assert!(result.is_err());

        // Empty slice.
        let result = cipher.decrypt(&[]);
        assert!(result.is_err());
    }

    /// Test decryption with wrong key
    #[test]
    fn test_field_decrypt_wrong_key_error() {
        let cipher1 = FieldEncryption::new(&[0u8; 32]);
        let cipher2 = FieldEncryption::new(&[1u8; 32]);

        let plaintext = "key-mismatch test";
        let encrypted = cipher1.encrypt(plaintext).unwrap();

        // Decrypting with a different key must fail.
        let result = cipher2.decrypt(&encrypted);
        assert!(result.is_err(), "decryption with wrong key must fail");

        // Original key should still work.
        assert_eq!(cipher1.decrypt(&encrypted).unwrap(), plaintext);
    }

    /// Test invalid UTF-8 handling
    #[test]
    fn test_field_invalid_utf8_error() {
        let cipher = test_cipher();

        // Craft a valid ciphertext that decrypts to invalid UTF-8 bytes.
        // We use raw AES-GCM to encrypt non-UTF-8 bytes, then try to decrypt
        // through FieldEncryption which expects UTF-8 output.
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };

        let key = [0u8; 32];
        let raw_cipher = Aes256Gcm::new_from_slice(&key).unwrap();

        // Invalid UTF-8 sequence.
        let invalid_bytes: &[u8] = &[0xFF, 0xFE, 0x80, 0x81, 0xC0, 0xC1];
        let nonce_bytes = [42u8; NONCE_SIZE];
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = raw_cipher.encrypt(nonce, invalid_bytes).unwrap();

        // Construct the format FieldEncryption expects: [nonce || ciphertext].
        let mut encrypted = nonce_bytes.to_vec();
        encrypted.extend_from_slice(&ciphertext);

        // Attempting to decrypt should fail due to invalid UTF-8.
        let result = cipher.decrypt(&encrypted);
        assert!(result.is_err(), "invalid UTF-8 decryption must return error");
    }

    // ============================================================================
    // DATABASE INTEGRATION TESTS
    // ============================================================================

    /// Test encrypted field in database storage
    #[tokio::test]
    async fn test_field_database_storage() {
        let cipher = test_cipher();
        let plaintext = "stored@database.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Verify encrypted output is raw bytes suitable for BYTEA/BLOB storage.
        assert_eq!(encrypted.len(), NONCE_SIZE + plaintext.len() + TAG_SIZE);

        // Verify it is NOT valid UTF-8 (so it cannot be accidentally stored as text).
        // Note: it *might* happen to be valid UTF-8 by chance, so we just verify
        // the bytes differ from the plaintext.
        assert_ne!(&encrypted[NONCE_SIZE..], plaintext.as_bytes());

        // Verify round-trip.
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test encrypting before database insert
    #[tokio::test]
    async fn test_field_encrypt_before_insert() {
        let cipher = test_cipher();
        let fields = [
            ("email", "alice@example.com"),
            ("phone", "+1-555-000-1234"),
            ("ssn", "999-88-7777"),
        ];

        // Simulate encrypting each field before INSERT.
        let mut encrypted_row: Vec<(&str, Vec<u8>)> = Vec::new();
        for (name, value) in &fields {
            let enc = cipher.encrypt(value).unwrap();
            encrypted_row.push((name, enc));
        }

        // Verify none of the encrypted values contain the original plaintext.
        for ((name, original), (_, encrypted)) in fields.iter().zip(encrypted_row.iter()) {
            assert!(
                !encrypted.windows(original.len()).any(|w| w == original.as_bytes()),
                "field '{}' ciphertext must not contain plaintext",
                name
            );
        }
    }

    /// Test decrypting after database retrieval
    #[tokio::test]
    async fn test_field_decrypt_after_select() {
        let cipher = test_cipher();

        // Simulate multiple rows with encrypted data.
        let rows = ["row1@example.com", "row2@example.com", "row3@example.com"];

        let encrypted_rows: Vec<Vec<u8>> =
            rows.iter().map(|r| cipher.encrypt(r).unwrap()).collect();

        // Decrypt each row independently.
        for (i, encrypted) in encrypted_rows.iter().enumerate() {
            let decrypted = cipher.decrypt(encrypted).unwrap();
            assert_eq!(decrypted, rows[i]);
        }
    }

    /// Test multiple encrypted fields in single row
    #[tokio::test]
    async fn test_field_multiple_encrypted_fields() {
        // Use different keys for each field.
        let email_cipher = FieldEncryption::new(&[1u8; 32]);
        let phone_cipher = FieldEncryption::new(&[2u8; 32]);
        let ssn_cipher = FieldEncryption::new(&[3u8; 32]);

        let email = "multi@test.com";
        let phone = "+1-555-999-0000";
        let ssn = "111-22-3333";

        let enc_email = email_cipher.encrypt(email).unwrap();
        let enc_phone = phone_cipher.encrypt(phone).unwrap();
        let enc_ssn = ssn_cipher.encrypt(ssn).unwrap();

        // Each field has an independent nonce.
        assert_ne!(&enc_email[..NONCE_SIZE], &enc_phone[..NONCE_SIZE]);

        // Decrypt each with its own cipher.
        assert_eq!(email_cipher.decrypt(&enc_email).unwrap(), email);
        assert_eq!(phone_cipher.decrypt(&enc_phone).unwrap(), phone);
        assert_eq!(ssn_cipher.decrypt(&enc_ssn).unwrap(), ssn);

        // Cross-key decryption must fail.
        assert!(email_cipher.decrypt(&enc_phone).is_err());
        assert!(phone_cipher.decrypt(&enc_ssn).is_err());
        assert!(ssn_cipher.decrypt(&enc_email).is_err());
    }

    /// Test encrypted field in UPDATE operations
    #[tokio::test]
    async fn test_field_database_update() {
        let cipher = test_cipher();

        // Original value.
        let original = "old@example.com";
        let enc_original = cipher.encrypt(original).unwrap();

        // Updated value.
        let updated = "new@example.com";
        let enc_updated = cipher.encrypt(updated).unwrap();

        // The two ciphertexts should differ.
        assert_ne!(enc_original, enc_updated);

        // Even re-encrypting the same updated value produces a different ciphertext (new nonce).
        let enc_updated2 = cipher.encrypt(updated).unwrap();
        assert_ne!(enc_updated, enc_updated2);

        // Both decrypt to the updated plaintext.
        assert_eq!(cipher.decrypt(&enc_updated).unwrap(), updated);
        assert_eq!(cipher.decrypt(&enc_updated2).unwrap(), updated);
    }

    /// Test encrypted field in WHERE clauses (not supported)
    #[tokio::test]
    async fn test_field_cannot_query_encrypted() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);

        // WHERE on encrypted field should be rejected.
        let result = qbi.validate_where_clause(&["email"]);
        assert!(result.is_err());

        // ORDER BY on encrypted field should be rejected.
        let result = qbi.validate_order_by_clause(&["phone"]);
        assert!(result.is_err());

        // Unencrypted fields should pass.
        let result = qbi.validate_where_clause(&["id", "name"]);
        assert!(result.is_ok());

        // `is_encrypted` check.
        assert!(qbi.is_encrypted("email"));
        assert!(qbi.is_encrypted("phone"));
        assert!(!qbi.is_encrypted("name"));
    }

    // ============================================================================
    // PERFORMANCE AND SCALABILITY TESTS
    // ============================================================================

    /// Test encryption throughput
    #[tokio::test]
    async fn test_field_encryption_throughput() {
        let cipher = test_cipher();
        let plaintext = "performance-test@example.com";

        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = cipher.encrypt(plaintext).unwrap();
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 5000,
            "1000 encryptions took {}ms, expected <5000ms",
            elapsed.as_millis()
        );
    }

    /// Test decryption throughput
    #[tokio::test]
    async fn test_field_decryption_throughput() {
        let cipher = test_cipher();
        let plaintext = "throughput-test@example.com";

        // Pre-encrypt 1000 values.
        let encrypted: Vec<Vec<u8>> =
            (0..1000).map(|_| cipher.encrypt(plaintext).unwrap()).collect();

        let start = std::time::Instant::now();
        for enc in &encrypted {
            let _ = cipher.decrypt(enc).unwrap();
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 5000,
            "1000 decryptions took {}ms, expected <5000ms",
            elapsed.as_millis()
        );
    }

    /// Test large plaintext encryption
    #[test]
    fn test_field_large_plaintext() {
        let cipher = test_cipher();

        // 1MB string.
        let large = "A".repeat(1_000_000);

        let encrypted = cipher.encrypt(&large).unwrap();
        assert_eq!(encrypted.len(), NONCE_SIZE + large.len() + TAG_SIZE);

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted.len(), large.len());
        assert_eq!(decrypted, large);
    }

    // ============================================================================
    // KEY MANAGEMENT TESTS
    // ============================================================================

    /// Test key derivation requirements
    #[test]
    fn test_field_key_must_be_32_bytes() {
        // Various wrong sizes.
        for size in [0, 1, 15, 16, 24, 31, 33, 48, 64, 128] {
            let key = vec![0u8; size];
            let result = std::panic::catch_unwind(|| {
                FieldEncryption::new(&key);
            });
            assert!(result.is_err(), "key of size {} must be rejected", size);
        }

        // Exactly 32 bytes must succeed.
        let result = std::panic::catch_unwind(|| {
            FieldEncryption::new(&[0u8; 32]);
        });
        assert!(result.is_ok(), "32-byte key must be accepted");
    }

    /// Test key reuse across many encryptions
    #[test]
    fn test_field_key_reuse_with_random_nonce() {
        let cipher = test_cipher();
        let plaintext = "reuse-key-test";

        let mut ciphertexts = Vec::new();
        let mut nonces = HashSet::new();

        for _ in 0..100 {
            let enc = cipher.encrypt(plaintext).unwrap();
            let nonce: [u8; NONCE_SIZE] = enc[..NONCE_SIZE].try_into().unwrap();
            nonces.insert(nonce);
            ciphertexts.push(enc);
        }

        // All 100 nonces should be unique (collision probability is astronomically low).
        assert_eq!(nonces.len(), 100, "all nonces must be unique");

        // All ciphertexts should be different.
        for i in 0..ciphertexts.len() {
            for j in (i + 1)..ciphertexts.len() {
                assert_ne!(ciphertexts[i], ciphertexts[j]);
            }
        }

        // All should decrypt to the same plaintext.
        for enc in &ciphertexts {
            assert_eq!(cipher.decrypt(enc).unwrap(), plaintext);
        }
    }

    /// Test independent cipher instances
    #[test]
    fn test_field_cipher_instances_independent() {
        let key = [42u8; 32];
        let cipher_a = FieldEncryption::new(&key);
        let cipher_b = FieldEncryption::new(&key);

        let plaintext = "cross-instance test";

        // Encrypt with cipher_a, decrypt with cipher_b (same key).
        let encrypted = cipher_a.encrypt(plaintext).unwrap();
        let decrypted = cipher_b.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Encrypt with cipher_b, decrypt with cipher_a.
        let encrypted = cipher_b.encrypt(plaintext).unwrap();
        let decrypted = cipher_a.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ============================================================================
    // SECURITY PROPERTIES TESTS
    // ============================================================================

    /// Test IND-CPA security (indistinguishability under chosen plaintext)
    #[test]
    fn test_field_ind_cpa_property() {
        let cipher = test_cipher();

        // Encrypt the same plaintext many times.
        let plaintext = "ind-cpa-test";
        let encryptions: Vec<Vec<u8>> =
            (0..50).map(|_| cipher.encrypt(plaintext).unwrap()).collect();

        // No two ciphertexts should be identical.
        for i in 0..encryptions.len() {
            for j in (i + 1)..encryptions.len() {
                assert_ne!(encryptions[i], encryptions[j], "IND-CPA: ciphertexts must be distinct");
            }
        }

        // The ciphertext portions (after nonce) should also all differ.
        let ct_portions: Vec<&[u8]> = encryptions.iter().map(|e| &e[NONCE_SIZE..]).collect();
        for i in 0..ct_portions.len() {
            for j in (i + 1)..ct_portions.len() {
                assert_ne!(
                    ct_portions[i], ct_portions[j],
                    "IND-CPA: ciphertext bodies must differ"
                );
            }
        }
    }

    /// Test authenticated encryption (prevents undetected modifications)
    #[test]
    fn test_field_authenticated_encryption() {
        let cipher = test_cipher();
        let plaintext = "authenticated-encryption-test";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Try flipping every single byte in the encrypted data; each should
        // cause decryption to fail.
        for i in NONCE_SIZE..encrypted.len() {
            let mut tampered = encrypted.clone();
            tampered[i] ^= 0x01;
            assert!(
                cipher.decrypt(&tampered).is_err(),
                "flipping byte at index {} must cause decryption failure",
                i
            );
        }
    }

    /// Test nonce reuse protection
    #[test]
    fn test_field_nonce_uniqueness_requirement() {
        let cipher = test_cipher();

        // Encrypt two different plaintexts; nonces should be different.
        let enc1 = cipher.encrypt("plaintext A").unwrap();
        let enc2 = cipher.encrypt("plaintext B").unwrap();

        let nonce1 = &enc1[..NONCE_SIZE];
        let nonce2 = &enc2[..NONCE_SIZE];

        assert_ne!(nonce1, nonce2, "different encryptions must use different nonces");

        // Encrypt many times and check all nonces are unique.
        let mut nonces = HashSet::new();
        for _ in 0..200 {
            let enc = cipher.encrypt("nonce-uniqueness").unwrap();
            let nonce: [u8; NONCE_SIZE] = enc[..NONCE_SIZE].try_into().unwrap();
            assert!(nonces.insert(nonce), "duplicate nonce detected among 200 encryptions");
        }
    }

    /// Test no key recovery from ciphertext
    #[test]
    fn test_field_key_not_recoverable() {
        // With known plaintext-ciphertext pairs, XOR of plaintext and ciphertext
        // should NOT reveal the key. Verify that the XOR does not equal the key.
        let key = [0xABu8; 32];
        let cipher = FieldEncryption::new(&key);

        let plaintext = "known plaintext for key recovery test!";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Extract just the ciphertext (after nonce, before tag end).
        let ct_body = &encrypted[NONCE_SIZE..];

        // XOR plaintext bytes with ciphertext bytes (up to shorter length).
        let min_len = plaintext.len().min(ct_body.len());
        let xor_result: Vec<u8> = plaintext.as_bytes()[..min_len]
            .iter()
            .zip(&ct_body[..min_len])
            .map(|(a, b)| a ^ b)
            .collect();

        // The XOR should NOT equal the key bytes.
        assert_ne!(
            &xor_result[..KEY_SIZE.min(xor_result.len())],
            &key[..KEY_SIZE.min(xor_result.len())],
            "XOR of plaintext and ciphertext must not reveal key"
        );
    }

    // ============================================================================
    // INTEROPERABILITY TESTS
    // ============================================================================

    /// Test ciphertext format stability
    #[test]
    fn test_field_ciphertext_format() {
        let cipher = test_cipher();
        let plaintext = "format-stability-test";

        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Format: [12-byte nonce][ciphertext][16-byte tag]
        // Total = NONCE_SIZE + plaintext.len() + TAG_SIZE
        assert_eq!(
            encrypted.len(),
            NONCE_SIZE + plaintext.len() + TAG_SIZE,
            "ciphertext format: [12-byte nonce][ciphertext][16-byte tag]"
        );

        // The first 12 bytes should be the nonce.
        let nonce_portion = &encrypted[..NONCE_SIZE];
        assert_eq!(nonce_portion.len(), 12);

        // The rest is ciphertext + tag.
        let ct_and_tag = &encrypted[NONCE_SIZE..];
        assert_eq!(ct_and_tag.len(), plaintext.len() + TAG_SIZE);

        // Verify decryption still works (format is correct).
        assert_eq!(cipher.decrypt(&encrypted).unwrap(), plaintext);
    }

    /// Test different aes-gcm implementations compatibility
    #[test]
    fn test_field_aes_gcm_standard_compliance() {
        // Verify our implementation uses 12-byte nonce (96-bit) as per NIST SP 800-38D.
        assert_eq!(NONCE_SIZE, 12, "nonce must be 96 bits (12 bytes) per NIST SP 800-38D");
        assert_eq!(KEY_SIZE, 32, "key must be 256 bits (32 bytes) for AES-256");
        assert_eq!(TAG_SIZE, 16, "tag must be 128 bits (16 bytes) for GCM");

        // Encrypt with our FieldEncryption, then decrypt with raw aes-gcm to prove compatibility.
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };

        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let raw = Aes256Gcm::new_from_slice(&key).unwrap();

        let plaintext = "standard compliance";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        // Extract nonce and ciphertext.
        let nonce = Nonce::from_slice(&encrypted[..NONCE_SIZE]);
        let ct = &encrypted[NONCE_SIZE..];

        // Decrypt with raw aes-gcm.
        let decrypted_bytes = raw.decrypt(nonce, ct).unwrap();
        let decrypted = String::from_utf8(decrypted_bytes).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ============================================================================
    // EDGE CASE TESTS
    // ============================================================================

    /// Test all zero key
    #[test]
    fn test_field_all_zero_key() {
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let plaintext = "zero-key-test";

        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    /// Test all zero plaintext
    #[test]
    fn test_field_all_zero_plaintext() {
        let cipher = test_cipher();
        // Use null bytes as "all zero plaintext".
        let plaintext = "\0\0\0\0\0\0\0\0\0\0";

        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
        assert_eq!(decrypted.len(), 10);
    }

    /// Test very long plaintext
    #[test]
    fn test_field_very_long_plaintext() {
        let cipher = test_cipher();

        // 10MB plaintext.
        let large = "B".repeat(10_000_000);

        let encrypted = cipher.encrypt(&large).unwrap();
        assert_eq!(encrypted.len(), NONCE_SIZE + large.len() + TAG_SIZE);

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted.len(), large.len());
        assert_eq!(decrypted, large);
    }

    /// Test single character
    #[test]
    fn test_field_single_character() {
        let cipher = test_cipher();

        for ch in ['a', 'Z', '0', ' ', '\n', '\0'] {
            let plaintext = ch.to_string();
            let encrypted = cipher.encrypt(&plaintext).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(
                decrypted,
                plaintext,
                "single character '{}' must round-trip",
                ch.escape_debug()
            );
        }
    }
}
