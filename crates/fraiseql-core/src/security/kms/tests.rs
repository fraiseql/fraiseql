//! Tests for `security/kms/` modules.


mod base_tests {
    use crate::utils::clock::{Clock as _, SystemClock};

    #[test]
    fn test_system_clock_timestamp_is_positive() {
        assert!(SystemClock.now_secs_i64() > 0);
    }
}

mod models_tests {
    use crate::security::kms::*;

    #[test]
    fn test_key_reference_qualified_id() {
        let key_ref = KeyReference::new(
            "vault".to_string(),
            "my-key-123".to_string(),
            KeyPurpose::EncryptDecrypt,
            1_000_000,
        );
        assert_eq!(key_ref.qualified_id(), "vault:my-key-123");
    }

    #[test]
    fn test_key_reference_with_alias() {
        let key_ref = KeyReference::new(
            "vault".to_string(),
            "my-key-123".to_string(),
            KeyPurpose::EncryptDecrypt,
            1_000_000,
        )
        .with_alias("production-key".to_string());

        assert_eq!(key_ref.key_alias, Some("production-key".to_string()));
    }

    #[test]
    fn test_key_purpose_display() {
        assert_eq!(KeyPurpose::EncryptDecrypt.to_string(), "encrypt_decrypt");
        assert_eq!(KeyPurpose::SignVerify.to_string(), "sign_verify");
        assert_eq!(KeyPurpose::Mac.to_string(), "mac");
    }

    #[test]
    fn test_key_state_display() {
        assert_eq!(KeyState::Enabled.to_string(), "enabled");
        assert_eq!(KeyState::Disabled.to_string(), "disabled");
    }

    #[test]
    fn test_rotation_policy_new() {
        let policy = RotationPolicy::new(true, 90);
        assert!(policy.enabled);
        assert_eq!(policy.rotation_period_days, 90);
        assert_eq!(policy.last_rotation, None);
        assert_eq!(policy.next_rotation, None);
    }
}

mod vault_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use crate::security::kms::*;
    use crate::security::kms::vault::{VAULT_REQUEST_TIMEOUT, base64_encode, base64_decode};

    #[test]
    fn test_vault_config_api_url() {
        let config =
            VaultConfig::new("https://vault.example.com".to_string(), "token123".to_string());
        assert_eq!(
            config.api_url("encrypt/my-key"),
            "https://vault.example.com/v1/transit/encrypt/my-key"
        );
    }

    #[test]
    fn test_vault_config_custom_mount_path() {
        let config =
            VaultConfig::new("https://vault.example.com".to_string(), "token123".to_string())
                .with_mount_path("custom-transit".to_string());

        assert_eq!(
            config.api_url("encrypt/my-key"),
            "https://vault.example.com/v1/custom-transit/encrypt/my-key"
        );
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    // ── S25-H2: VaultKmsProvider client timeout ───────────────────────────────

    #[test]
    fn vault_request_timeout_is_set() {
        let secs = VAULT_REQUEST_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 120, "Vault timeout should be 1–120 s, got {secs}");
    }

    #[test]
    fn vault_provider_new_succeeds() {
        let config = VaultConfig::new("https://vault.example.com".to_string(), "token".to_string());
        let provider = VaultKmsProvider::new(config);
        assert!(provider.is_ok(), "VaultKmsProvider::new() must succeed with valid config");
    }
}
