#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

/// Test `FieldEncryption` creation
#[test]
fn test_field_encryption_creation() {
    let key = [0u8; KEY_SIZE];
    let _cipher = FieldEncryption::new(&key).unwrap();
}

/// Test basic encryption/decryption roundtrip
#[test]
fn test_field_encrypt_decrypt_roundtrip() {
    let key = [0u8; KEY_SIZE];
    let cipher = FieldEncryption::new(&key).unwrap();

    let plaintext = "user@example.com";
    let encrypted = cipher.encrypt(plaintext).unwrap();
    let decrypted = cipher.decrypt(&encrypted).unwrap();

    assert_eq!(plaintext, decrypted);
    assert_ne!(plaintext.as_bytes(), &encrypted[NONCE_SIZE..]);
}

/// Test that same plaintext produces different ciphertexts
#[test]
fn test_field_encrypt_random_nonce() {
    let key = [0u8; KEY_SIZE];
    let cipher = FieldEncryption::new(&key).unwrap();

    let plaintext = "sensitive@data.com";
    let encrypted1 = cipher.encrypt(plaintext).unwrap();
    let encrypted2 = cipher.encrypt(plaintext).unwrap();

    // Different random nonces produce different ciphertexts
    assert_ne!(encrypted1, encrypted2);

    // But both decrypt to same plaintext
    assert_eq!(cipher.decrypt(&encrypted1).unwrap(), plaintext);
    assert_eq!(cipher.decrypt(&encrypted2).unwrap(), plaintext);
}

/// Test encryption with context
#[test]
fn test_field_encrypt_decrypt_with_context() {
    let key = [0u8; KEY_SIZE];
    let cipher = FieldEncryption::new(&key).unwrap();

    let plaintext = "secret123";
    let context = "user:456:password";

    let encrypted = cipher.encrypt_with_context(plaintext, context).unwrap();
    let decrypted = cipher.decrypt_with_context(&encrypted, context).unwrap();

    assert_eq!(plaintext, decrypted);
}

/// Test context verification fails with wrong context
#[test]
fn test_field_decrypt_with_wrong_context_fails() {
    let key = [0u8; KEY_SIZE];
    let cipher = FieldEncryption::new(&key).unwrap();

    let plaintext = "secret123";
    let correct_context = "user:456:password";
    let wrong_context = "user:789:password";

    let encrypted = cipher.encrypt_with_context(plaintext, correct_context).unwrap();

    // Decryption with wrong context should fail
    let result = cipher.decrypt_with_context(&encrypted, wrong_context);
    assert!(
        matches!(result, Err(SecretsError::EncryptionError(_))),
        "expected EncryptionError for wrong context, got: {result:?}"
    );
}

/// Test various data types
#[test]
fn test_field_encrypt_various_types() {
    let key = [0u8; KEY_SIZE];
    let cipher = FieldEncryption::new(&key).unwrap();

    let test_cases = vec![
        "email@example.com",
        "+1-555-123-4567",
        "123-45-6789",
        "4532015112830366",
        "sk_live_abc123def456",
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
        "", // Empty string
        "with\nspecial\nchars\t!@#$%",
        "unicode: 你好世界 🔐",
    ];

    for plaintext in test_cases {
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }
}

/// Test invalid key size returns Err
#[test]
fn test_field_encryption_invalid_key_size_returns_err() {
    let invalid_key = [0u8; 16]; // Too short
    let result = FieldEncryption::new(&invalid_key);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "expected ValidationError for invalid key size, got: {result:?}"
    );
}

/// Test corrupted ciphertext fails to decrypt
#[test]
fn test_field_decrypt_corrupted_data_fails() {
    let key = [0u8; KEY_SIZE];
    let cipher = FieldEncryption::new(&key).unwrap();

    let plaintext = "data";
    let mut encrypted = cipher.encrypt(plaintext).unwrap();

    // Corrupt a byte in the ciphertext (not the nonce)
    if encrypted.len() > NONCE_SIZE {
        encrypted[NONCE_SIZE] ^= 0xFF;
    }

    let result = cipher.decrypt(&encrypted);
    assert!(
        matches!(result, Err(SecretsError::EncryptionError(_))),
        "expected EncryptionError for corrupted data, got: {result:?}"
    );
}

/// Test short ciphertext fails gracefully
#[test]
fn test_field_decrypt_short_data_fails() {
    let key = [0u8; KEY_SIZE];
    let cipher = FieldEncryption::new(&key).unwrap();

    let short_data = vec![0u8; 5]; // Too short for nonce
    let result = cipher.decrypt(&short_data);
    assert!(
        matches!(result, Err(SecretsError::EncryptionError(_))),
        "expected EncryptionError for short data, got: {result:?}"
    );
}

// =========================================================================
// Key management / VersionedFieldEncryption tests
// =========================================================================

/// Versioned encryption: same inputs produce different ciphertexts due to random nonce
#[test]
fn test_versioned_encrypt_not_deterministic() {
    let key = [1u8; KEY_SIZE];
    let ve = VersionedFieldEncryption::new(1, &key).unwrap();

    let ct1 = ve.encrypt("secret").unwrap();
    let ct2 = ve.encrypt("secret").unwrap();
    assert_ne!(ct1, ct2, "Versioned encryption must produce non-deterministic output");
}

/// Versioned encryption roundtrip with primary key
#[test]
fn test_versioned_encrypt_decrypt_roundtrip() {
    let key = [2u8; KEY_SIZE];
    let ve = VersionedFieldEncryption::new(1, &key).unwrap();

    let plaintext = "sensitive@example.com";
    let ct = ve.encrypt(plaintext).unwrap();
    let decrypted = ve.decrypt(&ct).unwrap();
    assert_eq!(decrypted, plaintext, "Versioned roundtrip must restore original plaintext");
}

/// Different key versions produce blobs with different version prefix
#[test]
fn test_versioned_different_versions_different_prefixes() {
    let key_v1 = [1u8; KEY_SIZE];
    let key_v2 = [2u8; KEY_SIZE];
    let ve1 = VersionedFieldEncryption::new(1, &key_v1).unwrap();
    let ve2 = VersionedFieldEncryption::new(2, &key_v2).unwrap();

    let ct1 = ve1.encrypt("data").unwrap();
    let ct2 = ve2.encrypt("data").unwrap();

    let ver1 = VersionedFieldEncryption::extract_version(&ct1).unwrap();
    let ver2 = VersionedFieldEncryption::extract_version(&ct2).unwrap();

    assert_ne!(ver1, ver2, "Different key versions must produce different version prefixes");
    assert_eq!(ver1, 1u16);
    assert_eq!(ver2, 2u16);
}

/// Fallback key allows decrypting data encrypted with old key version
#[test]
fn test_versioned_fallback_key_decrypts_old_data() {
    let key_v1 = [1u8; KEY_SIZE];
    let key_v2 = [2u8; KEY_SIZE];

    // Encrypt with v1
    let ve_old = VersionedFieldEncryption::new(1, &key_v1).unwrap();
    let old_ct = ve_old.encrypt("legacy data").unwrap();

    // Now switch primary to v2, keep v1 as fallback
    let ve_new = VersionedFieldEncryption::new(2, &key_v2)
        .unwrap()
        .with_fallback(1, &key_v1)
        .unwrap();

    // Can decrypt old ciphertext via fallback
    let decrypted = ve_new.decrypt(&old_ct).unwrap();
    assert_eq!(decrypted, "legacy data", "Fallback key must decrypt old ciphertexts");
}

/// Empty key material returns an error
#[test]
fn test_versioned_empty_key_returns_error() {
    let result = VersionedFieldEncryption::new(1, &[]);
    assert!(result.is_err(), "Empty key must return an error");
}

/// Key length too short (16 bytes instead of 32) must fail
#[test]
fn test_versioned_short_key_returns_error() {
    let short_key = [0u8; 16];
    let result = VersionedFieldEncryption::new(1, &short_key);
    assert!(result.is_err(), "Short key must return an error");
}

/// Derived key is not an identity function (output != input key)
#[test]
fn test_versioned_encrypt_is_not_identity() {
    let key = [5u8; KEY_SIZE];
    let ve = VersionedFieldEncryption::new(1, &key).unwrap();
    let ct = ve.encrypt("hello").unwrap();

    // The ciphertext must not equal the plaintext
    assert_ne!(ct, b"hello", "Encrypted output must differ from plaintext");
}

/// Reencrypt migrates ciphertext from fallback key to primary key
#[test]
fn test_versioned_reencrypt_from_fallback() {
    let key_v1 = [10u8; KEY_SIZE];
    let key_v2 = [20u8; KEY_SIZE];

    let ve_old = VersionedFieldEncryption::new(1, &key_v1).unwrap();
    let old_ct = ve_old.encrypt("migrate me").unwrap();

    let ve_new = VersionedFieldEncryption::new(2, &key_v2)
        .unwrap()
        .with_fallback(1, &key_v1)
        .unwrap();

    let new_ct = ve_new.reencrypt_from_fallback(&old_ct).unwrap();

    // New ciphertext uses version 2
    let ver = VersionedFieldEncryption::extract_version(&new_ct).unwrap();
    assert_eq!(ver, 2u16, "Re-encrypted blob must use the primary key version");

    // Plaintext is preserved
    let decrypted = ve_new.decrypt(&new_ct).unwrap();
    assert_eq!(decrypted, "migrate me", "Plaintext must be preserved after re-encryption");
}
