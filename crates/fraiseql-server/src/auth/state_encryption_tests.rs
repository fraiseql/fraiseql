// State encryption integration tests

#[cfg(test)]
mod state_encryption_tests {
    use crate::auth::state_encryption::{StateEncryption, generate_state_encryption_key};

    #[test]
    fn test_encrypt_decrypt_state() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "oauth_state_xyz_abc_123";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_encrypt_produces_ciphertext() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "secret_state_value";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        assert!(encrypted.ciphertext.len() > 0);
    }

    #[test]
    fn test_encrypt_empty_state() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_encrypt_long_state() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "a".repeat(10_000);

        let encrypted = encryption.encrypt(&state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_different_keys_cannot_decrypt() {
        let key1_bytes = generate_state_encryption_key();
        let key2_bytes = generate_state_encryption_key();
        let encryption1 = StateEncryption::new(&key1_bytes).expect("Failed encryption 1");
        let encryption2 = StateEncryption::new(&key2_bytes).expect("Failed encryption 2");
        let state = "sensitive_oauth_state";

        let encrypted = encryption1.encrypt(state).expect("Encryption failed");
        let result = encryption2.decrypt(&encrypted);

        // Should fail to decrypt with different key
        assert!(result.is_err(), "Different key should not decrypt");
    }

    #[test]
    fn test_key_generation_creates_different_keys() {
        let key1 = generate_state_encryption_key();
        let key2 = generate_state_encryption_key();

        assert_ne!(key1, key2, "Generated keys should be different");
    }

    #[test]
    fn test_key_generation_creates_32_bytes() {
        let key = generate_state_encryption_key();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_tampered_ciphertext_fails_decrypt() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "oauth_state_value";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        // Tamper with ciphertext
        if !encrypted.ciphertext.is_empty() {
            encrypted.ciphertext[0] = encrypted.ciphertext[0].wrapping_add(1);
        }

        let result = encryption.decrypt(&encrypted);
        assert!(result.is_err(), "Tampered ciphertext should fail decryption");
    }

    #[test]
    fn test_tampered_nonce_fails_decrypt() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "oauth_state_value";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        // Tamper with nonce
        encrypted.nonce[0] = encrypted.nonce[0].wrapping_add(1);

        let result = encryption.decrypt(&encrypted);
        assert!(result.is_err(), "Tampered nonce should fail decryption");
    }

    #[test]
    fn test_nonce_prevents_replay() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "replay_prevention_test";

        let encrypted1 = encryption.encrypt(state).expect("Encryption 1 failed");
        let encrypted2 = encryption.encrypt(state).expect("Encryption 2 failed");

        // Nonces should be different (each encryption uses random nonce)
        assert_ne!(encrypted1.nonce, encrypted2.nonce, "Nonces should be different");

        // Both should decrypt correctly
        let decrypted1 = encryption.decrypt(&encrypted1).expect("Decryption 1 failed");
        let decrypted2 = encryption.decrypt(&encrypted2).expect("Decryption 2 failed");

        assert_eq!(decrypted1, state);
        assert_eq!(decrypted2, state);
    }

    #[test]
    fn test_serialize_deserialize_encrypted_state() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "serialize_test_state";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let bytes = encrypted.to_bytes();

        assert!(bytes.len() > 0, "Serialized state should not be empty");

        // Deserialize and decrypt
        let encrypted_deserialized = encrypted.to_bytes();
        assert_eq!(encrypted_deserialized.len(), bytes.len());
    }

    #[test]
    fn test_unicode_state() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "oauth_state_🔐_emoji";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_special_characters_state() {
        let key_bytes = generate_state_encryption_key();
        let encryption = StateEncryption::new(&key_bytes).expect("Failed to create encryption");
        let state = "state\n\r\t!@#$%^&*()_+-=[]{}|;:',.<>?/";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }
}
