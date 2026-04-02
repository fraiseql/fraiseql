//! Property-based tests for fraiseql-secrets encryption invariants.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_secrets::{FieldEncryption, VersionedFieldEncryption};
use proptest::prelude::*;

// ── FieldEncryption Roundtrip Properties ──────────────────────────────────────

proptest! {
    /// Encrypt then decrypt always recovers the original plaintext.
    #[test]
    fn encrypt_decrypt_roundtrip(plaintext in "\\PC{1,1024}") {
        let key = [0x42u8; 32];
        let enc = FieldEncryption::new(&key).unwrap();

        let ciphertext = enc.encrypt(&plaintext).unwrap();
        let recovered = enc.decrypt(&ciphertext).unwrap();

        prop_assert_eq!(recovered, plaintext);
    }

    /// Different plaintexts produce different ciphertexts (due to random nonce).
    #[test]
    fn encrypt_different_inputs_differ(
        a in "[a-z]{10,50}",
        b in "[a-z]{10,50}",
    ) {
        prop_assume!(a != b);
        let key = [0x42u8; 32];
        let enc = FieldEncryption::new(&key).unwrap();

        let ct_a = enc.encrypt(&a).unwrap();
        let ct_b = enc.encrypt(&b).unwrap();

        // Ciphertexts should differ (random nonce ensures this with overwhelming probability)
        prop_assert_ne!(ct_a, ct_b);
    }

    /// Same plaintext encrypted twice produces different ciphertexts (nonce uniqueness).
    #[test]
    fn encrypt_same_input_different_nonces(plaintext in "[a-z]{10,50}") {
        let key = [0x42u8; 32];
        let enc = FieldEncryption::new(&key).unwrap();

        let ct1 = enc.encrypt(&plaintext).unwrap();
        let ct2 = enc.encrypt(&plaintext).unwrap();

        // Different nonces → different ciphertexts
        prop_assert_ne!(ct1, ct2);
    }

    /// Corrupted ciphertext always fails decryption (AEAD integrity).
    #[test]
    fn corrupted_ciphertext_fails(plaintext in "[a-z]{10,50}", flip_pos in 12usize..50) {
        let key = [0x42u8; 32];
        let enc = FieldEncryption::new(&key).unwrap();

        let mut ciphertext = enc.encrypt(&plaintext).unwrap();

        // Flip a byte after the nonce (in the ciphertext/tag area)
        if flip_pos < ciphertext.len() {
            ciphertext[flip_pos] ^= 0xFF;
            prop_assert!(enc.decrypt(&ciphertext).is_err());
        }
    }

    /// Wrong key always fails decryption.
    #[test]
    fn wrong_key_fails_decryption(plaintext in "[a-z]{10,50}") {
        let key_a = [0x42u8; 32];
        let key_b = [0x43u8; 32];
        let enc_a = FieldEncryption::new(&key_a).unwrap();
        let enc_b = FieldEncryption::new(&key_b).unwrap();

        let ciphertext = enc_a.encrypt(&plaintext).unwrap();
        prop_assert!(enc_b.decrypt(&ciphertext).is_err());
    }

    /// Ciphertext is always longer than plaintext (nonce + auth tag overhead).
    #[test]
    fn ciphertext_has_overhead(plaintext in "[a-z]{1,200}") {
        let key = [0x42u8; 32];
        let enc = FieldEncryption::new(&key).unwrap();

        let ciphertext = enc.encrypt(&plaintext).unwrap();

        // 12 bytes nonce + 16 bytes auth tag = 28 bytes overhead minimum
        prop_assert!(ciphertext.len() >= plaintext.len() + 28);
    }
}

// ── VersionedFieldEncryption Properties ───────────────────────────────────────

proptest! {
    /// Versioned encrypt/decrypt roundtrip works.
    #[test]
    fn versioned_roundtrip(plaintext in "\\PC{1,512}") {
        let key = [0x42u8; 32];
        let enc = VersionedFieldEncryption::new(1, &key).unwrap();

        let ciphertext = enc.encrypt(&plaintext).unwrap();
        let recovered = enc.decrypt(&ciphertext).unwrap();

        prop_assert_eq!(recovered, plaintext);
    }

    /// Version extraction matches the primary version used for encryption.
    #[test]
    fn versioned_extract_matches_primary(
        plaintext in "[a-z]{10,50}",
        version in 1u16..1000,
    ) {
        let key = [0x42u8; 32];
        let enc = VersionedFieldEncryption::new(version, &key).unwrap();

        let ciphertext = enc.encrypt(&plaintext).unwrap();
        let extracted = VersionedFieldEncryption::extract_version(&ciphertext).unwrap();

        prop_assert_eq!(extracted, version);
    }

    /// Fallback key can decrypt data from a previous version.
    #[test]
    fn versioned_fallback_decryption(plaintext in "[a-z]{10,50}") {
        let old_key = [0x42u8; 32];
        let new_key = [0x43u8; 32];

        // Encrypt with version 1
        let old_enc = VersionedFieldEncryption::new(1, &old_key).unwrap();
        let ciphertext = old_enc.encrypt(&plaintext).unwrap();

        // Decrypt with version 2 primary + version 1 fallback
        let new_enc = VersionedFieldEncryption::new(2, &new_key)
            .unwrap()
            .with_fallback(1, &old_key)
            .unwrap();

        let recovered = new_enc.decrypt(&ciphertext).unwrap();
        prop_assert_eq!(recovered, plaintext);
    }

    /// Re-encryption produces ciphertext with the new version.
    #[test]
    fn reencrypt_updates_version(plaintext in "[a-z]{10,50}") {
        let old_key = [0x42u8; 32];
        let new_key = [0x43u8; 32];

        // Encrypt with version 1
        let old_enc = VersionedFieldEncryption::new(1, &old_key).unwrap();
        let old_ct = old_enc.encrypt(&plaintext).unwrap();
        prop_assert_eq!(VersionedFieldEncryption::extract_version(&old_ct).unwrap(), 1);

        // Re-encrypt to version 2
        let new_enc = VersionedFieldEncryption::new(2, &new_key)
            .unwrap()
            .with_fallback(1, &old_key)
            .unwrap();
        let new_ct = new_enc.reencrypt_from_fallback(&old_ct).unwrap();

        // New ciphertext should have version 2
        prop_assert_eq!(VersionedFieldEncryption::extract_version(&new_ct).unwrap(), 2);

        // And decrypt to the same plaintext
        let recovered = new_enc.decrypt(&new_ct).unwrap();
        prop_assert_eq!(recovered, plaintext);
    }
}

// ── Context-Based Encryption Properties ───────────────────────────────────────

proptest! {
    /// Encrypt with context, decrypt with same context succeeds.
    #[test]
    fn context_roundtrip(
        plaintext in "[a-z]{10,50}",
        context in "[a-z]{5,20}",
    ) {
        let key = [0x42u8; 32];
        let enc = FieldEncryption::new(&key).unwrap();

        let ciphertext = enc.encrypt_with_context(&plaintext, &context).unwrap();
        let recovered = enc.decrypt_with_context(&ciphertext, &context).unwrap();

        prop_assert_eq!(recovered, plaintext);
    }

    /// Decrypt with wrong context fails (AAD mismatch).
    #[test]
    fn wrong_context_fails(
        plaintext in "[a-z]{10,50}",
        context_a in "[a-z]{5,20}",
        context_b in "[A-Z]{5,20}",
    ) {
        prop_assume!(context_a != context_b);
        let key = [0x42u8; 32];
        let enc = FieldEncryption::new(&key).unwrap();

        let ciphertext = enc.encrypt_with_context(&plaintext, &context_a).unwrap();
        prop_assert!(enc.decrypt_with_context(&ciphertext, &context_b).is_err());
    }
}
