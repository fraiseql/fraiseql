#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

mod chain_tests {
    use super::super::chain::*;

    const TEST_SEED: [u8; 32] = *b"test-seed-32-bytes-exactly-here!";

    fn make_entry(action: &str, hasher: &mut ChainHasher) -> serde_json::Value {
        let mut entry = serde_json::json!({ "action": action, "user": "u1" });
        let hash = hasher.advance(&entry.to_string());
        entry["chain_hash"] = serde_json::Value::String(hash);
        entry
    }

    fn generate_chained_entries(n: usize, seed: [u8; 32]) -> Vec<serde_json::Value> {
        let mut hasher = ChainHasher::new(seed);
        (0..n).map(|i| make_entry(&format!("action-{i}"), &mut hasher)).collect()
    }

    #[test]
    fn test_chain_hash_is_deterministic() {
        let h1 = compute_chain_hash(&TEST_SEED, "entry-1");
        let h2 = compute_chain_hash(&TEST_SEED, "entry-1");
        assert_eq!(h1, h2, "same inputs must produce same hash");
    }

    #[test]
    fn test_chain_hash_changes_with_content() {
        let h1 = compute_chain_hash(&TEST_SEED, r#"{"action":"query"}"#);
        let h2 = compute_chain_hash(&TEST_SEED, r#"{"action":"mutation"}"#);
        assert_ne!(h1, h2, "different content must produce different hash");
    }

    #[test]
    fn test_chain_is_sequential() {
        let h1 = compute_chain_hash(&TEST_SEED, "entry-1");
        let h2 = compute_chain_hash(&h1, "entry-2");
        let h3 = compute_chain_hash(&h2, "entry-3");
        // Re-compute h3 skipping h2 — must differ.
        let h3_alt = compute_chain_hash(&h1, "entry-3");
        assert_ne!(h3, h3_alt, "sequential hashes must differ from skipped chain");
    }

    #[test]
    fn test_chain_hash_output_is_64_hex_chars() {
        let h = encode_chain_hash(&compute_chain_hash(&TEST_SEED, "entry"));
        assert_eq!(h.len(), 64, "hex-encoded SHA256 must be 64 characters");
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hasher_advance_changes_state() {
        let mut hasher = ChainHasher::new(TEST_SEED);
        let h1 = hasher.advance("entry-1");
        let h2 = hasher.advance("entry-1"); // same content, different state
        assert_ne!(h1, h2, "advancing changes internal state");
    }

    #[test]
    fn test_verify_valid_chain_passes() {
        let entries = generate_chained_entries(100, TEST_SEED);
        let result = verify_chain(entries, TEST_SEED);
        assert!(result.is_ok(), "valid chain must pass verification");
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_verify_detects_modified_entry() {
        let mut entries = generate_chained_entries(100, TEST_SEED);
        entries[50]["action"] = serde_json::Value::String("TAMPERED".to_string());
        let result = verify_chain(entries, TEST_SEED);
        assert!(
            matches!(
                result,
                Err(ChainVerifyError::BrokenLink {
                    entry_index: 50,
                    ..
                })
            ),
            "modified entry must break chain at that index"
        );
    }

    #[test]
    fn test_verify_detects_deleted_entry() {
        let mut entries = generate_chained_entries(100, TEST_SEED);
        entries.remove(50);
        let result = verify_chain(entries, TEST_SEED);
        assert!(
            matches!(
                result,
                Err(ChainVerifyError::BrokenLink {
                    entry_index: 50,
                    ..
                })
            ),
            "deleted entry must break chain at the deletion point"
        );
    }

    #[test]
    fn test_verify_empty_chain_passes() {
        let result = verify_chain([], TEST_SEED);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_verify_detects_missing_chain_hash() {
        let entries = vec![serde_json::json!({ "action": "query" })]; // no chain_hash
        let result = verify_chain(entries, TEST_SEED);
        assert!(matches!(result, Err(ChainVerifyError::MissingChainHash { entry_index: 0 })));
    }
}

mod logger_tests {
    use std::sync::Mutex;

    use super::super::logger::*;

    struct TestAuditLogger {
        entries: Mutex<Vec<AuditEntry>>,
    }

    impl TestAuditLogger {
        fn new() -> Self {
            Self {
                entries: Mutex::new(Vec::new()),
            }
        }

        fn get_entries(&self) -> Vec<AuditEntry> {
            self.entries.lock().unwrap().clone()
        }
    }

    impl AuditLogger for TestAuditLogger {
        fn log_entry(&self, entry: AuditEntry) {
            self.entries.lock().unwrap().push(entry);
        }
    }

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry {
            event_type: AuditEventType::JwtValidation,
            secret_type: SecretType::JwtToken,
            subject: Some("user123".to_string()),
            operation: "validate".to_string(),
            success: true,
            error_message: None,
            context: None,
            chain_hash: None,
        };

        assert_eq!(entry.event_type, AuditEventType::JwtValidation);
        assert_eq!(entry.subject, Some("user123".to_string()));
        assert!(entry.success);
    }

    #[test]
    fn test_audit_logger_logs_entry() {
        let logger = TestAuditLogger::new();

        logger.log_success(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123".to_string()),
            "validate",
        );

        let entries = logger.get_entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].success);
    }

    #[test]
    fn test_audit_logger_logs_failure() {
        let logger = TestAuditLogger::new();

        logger.log_failure(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123".to_string()),
            "validate",
            "Invalid signature",
        );

        let entries = logger.get_entries();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].success);
        assert_eq!(entries[0].error_message, Some("Invalid signature".to_string()));
    }

    #[test]
    fn test_event_type_strings() {
        assert_eq!(AuditEventType::JwtValidation.as_str(), "jwt_validation");
        assert_eq!(AuditEventType::OidcTokenExchange.as_str(), "oidc_token_exchange");
    }

    #[test]
    fn test_secret_type_strings() {
        assert_eq!(SecretType::JwtToken.as_str(), "jwt_token");
        assert_eq!(SecretType::ClientSecret.as_str(), "client_secret");
    }

    // Vulnerability #15: Audit logger bounds documentation tests
    #[test]
    fn test_bounds_constants_are_reasonable() {
        use crate::audit::logger::bounds;

        // Subject length should accommodate typical user IDs
        let max_subject = bounds::MAX_SUBJECT_LEN;
        assert!(max_subject >= 128, "Subject length too small");

        // Operation should cover all operation types
        let max_operation = bounds::MAX_OPERATION_LEN;
        assert!(max_operation >= 20, "Operation length too small");

        // Error messages should have room for context
        let max_error = bounds::MAX_ERROR_MESSAGE_LEN;
        assert!(max_error >= 512, "Error message length too small");

        // Context should allow some additional data
        let max_context = bounds::MAX_CONTEXT_LEN;
        assert!(max_context >= 1024, "Context length too small");

        // In-memory buffer should be large but not excessive
        let max_entries = bounds::MAX_ENTRIES_IN_MEMORY;
        assert!(max_entries >= 1000, "Max entries in memory too small");
        assert!(max_entries <= 100_000, "Max entries in memory too large");
    }

    #[test]
    fn test_bounds_constants_match_documentation() {
        use crate::audit::logger::bounds;

        // Verify documented bounds match constants
        assert_eq!(bounds::MAX_SUBJECT_LEN, 256, "Subject length bound mismatch");
        assert_eq!(bounds::MAX_OPERATION_LEN, 50, "Operation length bound mismatch");
        assert_eq!(bounds::MAX_ERROR_MESSAGE_LEN, 1024, "Error message length bound mismatch");
        assert_eq!(bounds::MAX_CONTEXT_LEN, 2048, "Context length bound mismatch");
        assert_eq!(bounds::MAX_ENTRIES_IN_MEMORY, 10_000, "Max entries in memory bound mismatch");
    }

    #[test]
    fn test_memory_per_entry_constant_is_reasonable() {
        use crate::audit::logger::bounds;

        // Memory per entry should be sensible
        let bytes_per_entry = bounds::BYTES_PER_ENTRY;
        let max_entries = bounds::MAX_ENTRIES_IN_MEMORY;
        let total_memory_mb = (bytes_per_entry * max_entries) / (1024 * 1024);

        // Total memory for full buffer should be reasonable (< 100 MB)
        assert!(
            total_memory_mb < 100,
            "Total memory for full buffer too large: {} MB",
            total_memory_mb
        );

        // But not absurdly small (> 10 MB for safety margin)
        assert!(
            total_memory_mb > 10,
            "Total memory for full buffer too small: {} MB",
            total_memory_mb
        );
    }

    #[test]
    fn test_audit_entry_field_sizes_within_bounds() {
        use crate::audit::logger::bounds;

        let entry = AuditEntry {
            event_type: AuditEventType::JwtValidation,
            secret_type: SecretType::JwtToken,
            subject: Some("a".repeat(bounds::MAX_SUBJECT_LEN)),
            operation: "validate".to_string(),
            success: true,
            error_message: None,
            context: None,
            chain_hash: None,
        };

        // Verify subject fits within bounds
        assert!(entry.subject.as_ref().unwrap().len() <= bounds::MAX_SUBJECT_LEN);
    }

    #[test]
    fn test_error_message_bound_accommodates_typical_errors() {
        use crate::audit::logger::bounds;

        // Typical security errors should fit
        let error_messages = vec![
            "Invalid signature",
            "Token expired",
            "User not authorized",
            "Failed to decrypt payload: AES-256-GCM decryption returned InvalidTag",
            "Database connection timeout after 30 seconds waiting for available connection",
        ];

        for msg in error_messages {
            assert!(
                msg.len() <= bounds::MAX_ERROR_MESSAGE_LEN,
                "Error message too long: {} bytes for: {}",
                msg.len(),
                msg
            );
        }
    }

    #[test]
    fn test_operation_bound_covers_all_audit_operations() {
        use crate::audit::logger::bounds;

        // All documented operation names should fit
        let operations = vec![
            "validate", "create", "revoke", "refresh", "exchange", "logout",
        ];

        for op in operations {
            assert!(
                op.len() <= bounds::MAX_OPERATION_LEN,
                "Operation name too long: {} bytes for: {}",
                op.len(),
                op
            );
        }
    }

    #[test]
    fn test_global_audit_logger_is_singleton() {
        // Verify that the global audit logger can only be initialized once
        let logger1 = get_audit_logger();
        let logger2 = get_audit_logger();

        // Both should return the same instance
        assert_eq!(
            std::sync::Arc::as_ptr(&logger1),
            std::sync::Arc::as_ptr(&logger2),
            "Audit loggers are not the same singleton instance"
        );
    }

    #[test]
    fn test_audit_entry_sizes_reasonable_for_serialization() {
        use crate::audit::logger::bounds;

        // Create a maximum-size entry
        let max_entry = AuditEntry {
            event_type: AuditEventType::JwtValidation,
            secret_type: SecretType::JwtToken,
            subject: Some("a".repeat(bounds::MAX_SUBJECT_LEN)),
            operation: "validate".to_string(),
            success: false,
            error_message: Some("e".repeat(bounds::MAX_ERROR_MESSAGE_LEN)),
            context: Some("c".repeat(bounds::MAX_CONTEXT_LEN)),
            chain_hash: None,
        };

        // Serialize to JSON to estimate size
        let json = serde_json::to_string(&max_entry);
        assert!(json.is_ok(), "Failed to serialize maximum-size entry");

        let json_size = json.unwrap().len();
        // JSON should be reasonable size (with overhead, but < 2x fields)
        assert!(
            json_size < bounds::BYTES_PER_ENTRY * 2,
            "JSON serialization too large: {} bytes",
            json_size
        );
    }

    #[test]
    fn test_authorization_denied_variant_exists_and_has_stable_string() {
        let event_type = AuditEventType::AuthorizationDenied;
        assert_eq!(
            event_type.as_str(),
            "authorization_denied",
            "AuthorizationDenied must serialize to 'authorization_denied' for compliance audit trails"
        );
    }
}
