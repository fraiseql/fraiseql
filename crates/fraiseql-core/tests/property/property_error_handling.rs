//! Property-based tests for error handling invariants.
//!
//! These properties verify that error formatting, audit logging, and
//! error sanitization behave correctly across all inputs.

use chrono::Utc;
use fraiseql_core::security::{AuditEntry, AuditLevel, ErrorFormatter};
use proptest::prelude::*;
use serde_json::json;

// ============================================================================
// ErrorFormatter Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Property: Production formatter never leaks SQL keywords.
    #[test]
    fn prop_production_hides_sql_keywords(
        prefix in "[a-zA-Z ]{0,30}",
        keyword in prop_oneof![
            Just("SELECT "),
            Just("INSERT "),
            Just("UPDATE "),
            Just("DELETE "),
            Just("DROP TABLE "),
            Just("ALTER TABLE "),
            Just("CREATE INDEX "),
            Just("TRUNCATE "),
        ],
        suffix in "[a-zA-Z0-9_ ]{0,30}",
    ) {
        let formatter = ErrorFormatter::production();
        let raw = format!("{}{}{}", prefix, keyword, suffix);
        let formatted = formatter.format_error(&raw);

        prop_assert!(
            !formatted.contains(keyword.trim()),
            "Production error must not contain SQL keyword '{}', got: {}",
            keyword.trim(), formatted
        );
    }

    /// Property: Production formatter never leaks database connection URLs.
    #[test]
    fn prop_production_hides_db_urls(
        user in "[a-zA-Z]{1,10}",
        host in "[a-zA-Z.]{1,15}",
        db_name in "[a-zA-Z_]{1,10}",
    ) {
        let formatter = ErrorFormatter::production();
        let raw = format!("Connection failed: postgresql://{}:secret@{}/{}", user, host, db_name);
        let formatted = formatter.format_error(&raw);

        prop_assert!(
            !formatted.contains("postgresql://"),
            "Production error must not contain database URL, got: {}", formatted
        );
        prop_assert!(
            !formatted.contains("secret"),
            "Production error must not contain passwords, got: {}", formatted
        );
    }

    /// Property: Production formatter never leaks file paths.
    #[test]
    fn prop_production_hides_file_paths(
        path in "/[a-z/]{1,30}",
        line in 1u32..10000,
    ) {
        let formatter = ErrorFormatter::production();
        let raw = format!("Error at {}:{}: something failed", path, line);
        let formatted = formatter.format_error(&raw);

        prop_assert!(
            !formatted.contains(&path),
            "Production error must not contain file path '{}', got: {}",
            path, formatted
        );
    }

    /// Property: Production formatter output is never empty.
    #[test]
    fn prop_production_never_empty(
        raw in ".{1,200}",
    ) {
        let formatter = ErrorFormatter::production();
        let formatted = formatter.format_error(&raw);

        prop_assert!(
            !formatted.is_empty(),
            "Production error must never be empty for input: {}", raw
        );
    }

    /// Property: Development formatter preserves the original error message.
    #[test]
    fn prop_development_preserves_message(
        raw in "[a-zA-Z0-9 .:_-]{1,100}",
    ) {
        let formatter = ErrorFormatter::development();
        let formatted = formatter.format_error(&raw);

        prop_assert!(
            formatted.contains(&raw),
            "Development error should contain original message '{}', got: {}",
            raw, formatted
        );
    }

    /// Property: Production formatter is idempotent — formatting twice gives same result.
    #[test]
    fn prop_production_idempotent(
        raw in ".{1,200}",
    ) {
        let formatter = ErrorFormatter::production();
        let once = formatter.format_error(&raw);
        let twice = formatter.format_error(&once);

        prop_assert_eq!(
            once, twice,
            "Production formatter should be idempotent"
        );
    }

    /// Property: Production formatter output length is bounded.
    #[test]
    fn prop_production_bounded_length(
        raw in ".{1,10000}",
    ) {
        let formatter = ErrorFormatter::production();
        let formatted = formatter.format_error(&raw);

        prop_assert!(
            formatted.len() <= 1000,
            "Production error should be bounded, got {} chars", formatted.len()
        );
    }
}

// ============================================================================
// AuditEntry Hash Chain Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: calculate_hash produces a 64-character hex string (SHA-256).
    #[test]
    fn prop_audit_hash_is_sha256_hex(
        user_id in 1i64..1_000_000,
        operation in prop_oneof![Just("query"), Just("mutation")],
        query_str in "[a-zA-Z {}()]{1,100}",
    ) {
        let entry = AuditEntry {
            id: None,
            timestamp: Utc::now(),
            level: AuditLevel::INFO,
            user_id,
            tenant_id: 1,
            operation: operation.to_string(),
            query: query_str,
            variables: json!({}),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            error: None,
            duration_ms: Some(10),
            previous_hash: None,
            integrity_hash: None,
        };

        let hash = entry.calculate_hash();
        prop_assert_eq!(hash.len(), 64, "SHA-256 hash should be 64 hex chars, got: {}", hash);
        prop_assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should be hex, got: {}", hash
        );
    }

    /// Property: Same entry always produces the same hash (deterministic).
    #[test]
    fn prop_audit_hash_deterministic(
        user_id in 1i64..1_000_000,
        query_str in "[a-zA-Z {}()]{1,100}",
    ) {
        let entry = AuditEntry {
            id: None,
            timestamp: Utc::now(),
            level: AuditLevel::INFO,
            user_id,
            tenant_id: 1,
            operation: "query".to_string(),
            query: query_str,
            variables: json!({}),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            error: None,
            duration_ms: Some(10),
            previous_hash: None,
            integrity_hash: None,
        };

        let hash1 = entry.calculate_hash();
        let hash2 = entry.calculate_hash();
        prop_assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    /// Property: Different user IDs produce different hashes.
    #[test]
    fn prop_audit_hash_varies_with_user_id(
        user_id_a in 1i64..500_000,
        user_id_b in 500_001i64..1_000_000,
        query_str in "[a-zA-Z {}()]{1,50}",
    ) {
        let now = Utc::now();
        let make_entry = |uid| AuditEntry {
            id: None,
            timestamp: now,
            level: AuditLevel::INFO,
            user_id: uid,
            tenant_id: 1,
            operation: "query".to_string(),
            query: query_str.clone(),
            variables: json!({}),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            error: None,
            duration_ms: Some(10),
            previous_hash: None,
            integrity_hash: None,
        };

        let hash_a = make_entry(user_id_a).calculate_hash();
        let hash_b = make_entry(user_id_b).calculate_hash();
        prop_assert_ne!(hash_a, hash_b, "Different user IDs should produce different hashes");
    }

    /// Property: Different queries produce different hashes.
    #[test]
    fn prop_audit_hash_varies_with_query(
        query_a in "[a-z]{5,20}",
        query_b in "[A-Z]{5,20}",
    ) {
        prop_assume!(query_a != query_b);

        let now = Utc::now();
        let make_entry = |q: String| AuditEntry {
            id: None,
            timestamp: now,
            level: AuditLevel::INFO,
            user_id: 1,
            tenant_id: 1,
            operation: "query".to_string(),
            query: q,
            variables: json!({}),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            error: None,
            duration_ms: Some(10),
            previous_hash: None,
            integrity_hash: None,
        };

        let hash_a = make_entry(query_a).calculate_hash();
        let hash_b = make_entry(query_b).calculate_hash();
        prop_assert_ne!(hash_a, hash_b, "Different queries should produce different hashes");
    }

    /// Property: verify_integrity returns true when integrity_hash matches calculated hash.
    #[test]
    fn prop_audit_verify_integrity_correct_hash(
        user_id in 1i64..1_000_000,
        query_str in "[a-zA-Z {}()]{1,50}",
    ) {
        let mut entry = AuditEntry {
            id: None,
            timestamp: Utc::now(),
            level: AuditLevel::INFO,
            user_id,
            tenant_id: 1,
            operation: "query".to_string(),
            query: query_str,
            variables: json!({}),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            error: None,
            duration_ms: Some(10),
            previous_hash: None,
            integrity_hash: None,
        };

        entry.integrity_hash = Some(entry.calculate_hash());
        prop_assert!(entry.verify_integrity(), "Entry with correct hash should verify");
    }

    /// Property: verify_integrity returns false when integrity_hash is tampered.
    #[test]
    fn prop_audit_verify_integrity_detects_tampering(
        user_id in 1i64..1_000_000,
        query_str in "[a-zA-Z {}()]{1,50}",
        tampered_hash in "[0-9a-f]{64}",
    ) {
        let mut entry = AuditEntry {
            id: None,
            timestamp: Utc::now(),
            level: AuditLevel::INFO,
            user_id,
            tenant_id: 1,
            operation: "query".to_string(),
            query: query_str,
            variables: json!({}),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            error: None,
            duration_ms: Some(10),
            previous_hash: None,
            integrity_hash: None,
        };

        let correct_hash = entry.calculate_hash();
        prop_assume!(tampered_hash != correct_hash);

        entry.integrity_hash = Some(tampered_hash);
        prop_assert!(!entry.verify_integrity(), "Entry with tampered hash should not verify");
    }

    /// Property: Hash chain links — previous_hash affects current hash.
    #[test]
    fn prop_audit_hash_chain_links(
        query_str in "[a-zA-Z {}()]{1,50}",
        prev_hash in "[0-9a-f]{64}",
    ) {
        let now = Utc::now();
        let make_entry = |prev: Option<String>| AuditEntry {
            id: None,
            timestamp: now,
            level: AuditLevel::INFO,
            user_id: 1,
            tenant_id: 1,
            operation: "query".to_string(),
            query: query_str.clone(),
            variables: json!({}),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            error: None,
            duration_ms: Some(10),
            previous_hash: prev,
            integrity_hash: None,
        };

        let hash_without_prev = make_entry(None).calculate_hash();
        let hash_with_prev = make_entry(Some(prev_hash)).calculate_hash();

        prop_assert_ne!(
            hash_without_prev, hash_with_prev,
            "Previous hash should affect current hash"
        );
    }
}

// ============================================================================
// AuditLevel Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Property: AuditLevel::parse roundtrips through as_str.
    #[test]
    fn prop_audit_level_roundtrip(
        level in prop_oneof![Just("INFO"), Just("WARN"), Just("ERROR")],
    ) {
        let parsed = AuditLevel::parse(level);
        let back = parsed.as_str();
        prop_assert_eq!(back, level, "AuditLevel should roundtrip through parse/as_str");
    }

    /// Property: AuditLevel::parse returns INFO for unknown strings.
    #[test]
    fn prop_audit_level_unknown_defaults(
        unknown in "[a-z]{1,10}",
    ) {
        prop_assume!(unknown != "info" && unknown != "warn" && unknown != "error");
        let parsed = AuditLevel::parse(&unknown);
        prop_assert_eq!(parsed.as_str(), "INFO", "Unknown level should default to INFO");
    }
}

// ============================================================================
// Error Recovery and Resilience Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(250))]

    /// Property: ErrorFormatter survives formatting very long error messages.
    #[test]
    fn prop_formatter_handles_long_messages(
        long_msg in ".{100,5000}",
    ) {
        let formatter = ErrorFormatter::production();
        let formatted = formatter.format_error(&long_msg);

        // Should always succeed (not panic) and return something
        prop_assert!(!formatted.is_empty(), "Should produce some output");
    }

    /// Property: Error formatter handles special characters safely.
    #[test]
    fn prop_formatter_handles_special_chars(
        message in "[a-zA-Z0-9 .,:;!?-]{0,200}",
    ) {
        let formatter = ErrorFormatter::production();
        // Should never panic on any input
        let formatted = formatter.format_error(&message);
        let _ = formatted;
    }

    /// Property: Production and Development formatters output consistent structure.
    #[test]
    fn prop_formatters_consistent_structure(
        raw in "[a-zA-Z0-9 ]{1,100}",
    ) {
        let prod = ErrorFormatter::production();
        let dev = ErrorFormatter::development();

        let prod_out = prod.format_error(&raw);
        let dev_out = dev.format_error(&raw);

        // Both should be non-empty
        prop_assert!(!prod_out.is_empty(), "Production output should be non-empty");
        prop_assert!(!dev_out.is_empty(), "Development output should be non-empty");

        // Both should be valid strings
        prop_assert!(prod_out.chars().all(|c| !c.is_control() || c == '\n'),
            "Production output should have valid characters");
    }
}
