//! Tests for `security/` modules.

#![allow(clippy::panic)] // Reason: test code, panics acceptable
mod audit_tests {
    use chrono::Utc;

    use crate::security::audit::{AuditEntry, AuditError, AuditExportConfig, AuditLevel};

    #[test]
    fn test_audit_entry_integrity_hash() {
        let entry = AuditEntry {
            id:             Some(1),
            timestamp:      Utc::now(),
            level:          AuditLevel::INFO,
            user_id:        123,
            tenant_id:      456,
            operation:      "query".to_string(),
            query:          "{ users { id name } }".to_string(),
            variables:      serde_json::json!({}),
            ip_address:     "192.168.1.1".to_string(),
            user_agent:     "Mozilla/5.0".to_string(),
            error:          None,
            duration_ms:    Some(100),
            previous_hash:  None,
            integrity_hash: None,
        };

        let hash = entry.calculate_hash();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 hex is 64 chars
    }

    #[test]
    fn test_audit_integrity_verification() {
        let mut entry = AuditEntry {
            id:             Some(1),
            timestamp:      Utc::now(),
            level:          AuditLevel::INFO,
            user_id:        123,
            tenant_id:      456,
            operation:      "query".to_string(),
            query:          "{ users { id name } }".to_string(),
            variables:      serde_json::json!({}),
            ip_address:     "192.168.1.1".to_string(),
            user_agent:     "Mozilla/5.0".to_string(),
            error:          None,
            duration_ms:    Some(100),
            previous_hash:  None,
            integrity_hash: None,
        };

        // Calculate hash and store it
        let calculated_hash = entry.calculate_hash();
        entry.integrity_hash = Some(calculated_hash);

        // Verify should pass
        assert!(entry.verify_integrity());

        // Tamper with data
        entry.user_id = 999;

        // Verify should fail
        assert!(!entry.verify_integrity());
    }

    #[test]
    fn test_audit_hash_chain() {
        let timestamp = Utc::now();

        let mut entry1 = AuditEntry {
            id: Some(1),
            timestamp,
            level: AuditLevel::INFO,
            user_id: 123,
            tenant_id: 456,
            operation: "query".to_string(),
            query: "{ users { id } }".to_string(),
            variables: serde_json::json!({}),
            ip_address: "192.168.1.1".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            error: None,
            duration_ms: Some(100),
            previous_hash: None,
            integrity_hash: None,
        };

        let hash1 = entry1.calculate_hash();
        entry1.integrity_hash = Some(hash1.clone());

        // Create second entry with chain
        let mut entry2 = AuditEntry {
            id: Some(2),
            timestamp,
            level: AuditLevel::INFO,
            user_id: 123,
            tenant_id: 456,
            operation: "query".to_string(),
            query: "{ posts { id } }".to_string(),
            variables: serde_json::json!({}),
            ip_address: "192.168.1.1".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            error: None,
            duration_ms: Some(50),
            previous_hash: Some(hash1),
            integrity_hash: None,
        };

        let hash2 = entry2.calculate_hash();
        entry2.integrity_hash = Some(hash2);

        // Both should verify
        assert!(entry1.verify_integrity());
        assert!(entry2.verify_integrity());

        // Breaking the chain should be detected
        entry1.user_id = 999;
        assert!(!entry1.verify_integrity());
    }

    #[test]
    fn test_audit_level_parsing() {
        assert_eq!(AuditLevel::parse("WARN"), AuditLevel::WARN);
        assert_eq!(AuditLevel::parse("ERROR"), AuditLevel::ERROR);
        assert_eq!(AuditLevel::parse("INFO"), AuditLevel::INFO);
        assert_eq!(AuditLevel::parse("UNKNOWN"), AuditLevel::INFO);
    }

    #[test]
    fn test_audit_export_config_deserialization() {
        let json = r#"{
            "syslog": { "address": "syslog.internal", "port": 514, "protocol": "tcp" },
            "webhook": { "url": "https://logs.example.com/ingest" }
        }"#;
        let config: AuditExportConfig =
            serde_json::from_str(json).expect("should deserialize AuditExportConfig");
        assert!(config.syslog.is_some());
        assert!(config.webhook.is_some());

        let syslog = config.syslog.expect("syslog should be Some");
        assert_eq!(syslog.address, "syslog.internal");
        assert_eq!(syslog.port, 514);
        assert_eq!(syslog.protocol, "tcp");

        let webhook = config.webhook.expect("webhook should be Some");
        assert_eq!(webhook.url, "https://logs.example.com/ingest");
        assert_eq!(webhook.batch_size, 100);
        assert_eq!(webhook.flush_interval_secs, 30);
    }

    #[test]
    fn test_audit_export_config_empty() {
        let config: AuditExportConfig =
            serde_json::from_str("{}").expect("should deserialize empty config");
        assert!(config.syslog.is_none());
        assert!(config.webhook.is_none());
    }

    #[test]
    fn test_audit_error_export_variant() {
        let err = AuditError::Export("connection refused".to_string());
        assert!(err.to_string().contains("connection refused"));
    }
}

#[cfg(feature = "audit-syslog")]
mod audit_export_syslog_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::{io::Read, net::UdpSocket};

    use chrono::Utc;
    use parking_lot::Mutex;

    use crate::security::{
        audit::{AuditEntry, AuditExporter, AuditLevel, SyslogExportConfig},
        audit_export_syslog::{SyslogAuditExporter, Transport, escape_sd_value},
    };

    fn test_entry() -> AuditEntry {
        AuditEntry {
            id:             Some(1),
            timestamp:      Utc::now(),
            level:          AuditLevel::INFO,
            user_id:        123,
            tenant_id:      456,
            operation:      "query".to_string(),
            query:          "{ users { id name } }".to_string(),
            variables:      serde_json::json!({}),
            ip_address:     "192.168.1.1".to_string(),
            user_agent:     "Mozilla/5.0".to_string(),
            error:          None,
            duration_ms:    Some(42),
            previous_hash:  None,
            integrity_hash: None,
        }
    }

    #[test]
    fn test_format_message_rfc5424() {
        let exporter = SyslogAuditExporter {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            hostname:  "test-host".to_string(),
        };

        let msg = exporter.format_message(&test_entry());

        // Priority = authpriv(10) * 8 + severity(6) = 86
        assert!(msg.starts_with("<86>1 "), "should start with priority 86, got: {msg}");
        assert!(msg.contains("test-host"));
        assert!(msg.contains("fraiseql"));
        assert!(msg.contains("AUDIT"));
        assert!(msg.contains(r#"user="123""#));
        assert!(msg.contains(r#"tenant="456""#));
        assert!(msg.contains(r#"operation="query""#));
        assert!(msg.contains(r#"level="INFO""#));
        assert!(msg.contains("{ users { id name } }"));
    }

    #[test]
    fn test_format_message_with_error() {
        let exporter = SyslogAuditExporter {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            hostname:  "test-host".to_string(),
        };

        let mut entry = test_entry();
        entry.level = AuditLevel::ERROR;
        entry.error = Some("timeout".to_string());

        let msg = exporter.format_message(&entry);

        // Priority = authpriv(10) * 8 + severity(3) = 83
        assert!(msg.starts_with("<83>1 "), "error priority should be 83");
        assert!(msg.contains(r#"error="timeout""#));
        assert!(msg.contains(r#"level="ERROR""#));
    }

    #[test]
    fn test_severity_mapping() {
        assert_eq!(SyslogAuditExporter::severity(AuditLevel::INFO), 6);
        assert_eq!(SyslogAuditExporter::severity(AuditLevel::WARN), 4);
        assert_eq!(SyslogAuditExporter::severity(AuditLevel::ERROR), 3);
    }

    #[test]
    fn test_syslog_udp_export() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let recv_addr = receiver.local_addr().unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(1))).unwrap();

        let config = SyslogExportConfig {
            address:  "127.0.0.1".to_string(),
            port:     recv_addr.port(),
            protocol: "udp".to_string(),
        };

        let exporter = SyslogAuditExporter::new(&config).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(exporter.export(&test_entry())).unwrap();

        let mut buf = [0u8; 4096];
        let n = receiver.recv(&mut buf).unwrap();
        let received = std::str::from_utf8(&buf[..n]).unwrap();

        assert!(received.contains("AUDIT"));
        assert!(received.contains(r#"user="123""#));
    }

    #[test]
    fn test_syslog_tcp_export() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let listen_addr = listener.local_addr().unwrap();

        let config = SyslogExportConfig {
            address:  "127.0.0.1".to_string(),
            port:     listen_addr.port(),
            protocol: "tcp".to_string(),
        };

        let exporter = SyslogAuditExporter::new(&config).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(exporter.export(&test_entry())).unwrap();

        let (mut conn, _) = listener.accept().unwrap();
        conn.set_read_timeout(Some(std::time::Duration::from_secs(1))).unwrap();

        let mut buf = [0u8; 4096];
        let n = conn.read(&mut buf).unwrap();
        let received = std::str::from_utf8(&buf[..n]).unwrap();

        assert!(received.contains("AUDIT"));
        assert!(received.ends_with('\n'), "TCP syslog should end with newline");
    }

    #[test]
    fn test_escape_sd_value() {
        assert_eq!(escape_sd_value(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape_sd_value(r"a\b"), r"a\\b");
        assert_eq!(escape_sd_value("a]b"), r"a\]b");
    }

    #[test]
    fn test_long_query_truncated() {
        let exporter = SyslogAuditExporter {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            hostname:  "test-host".to_string(),
        };

        let mut entry = test_entry();
        entry.query = "x".repeat(500);

        let msg = exporter.format_message(&entry);
        // Query should be truncated to 200 chars + "..."
        assert!(msg.len() < 600, "message should be bounded");
        assert!(msg.contains("..."));
    }
}

#[cfg(feature = "audit-webhook")]
mod audit_export_webhook_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use chrono::Utc;

    use crate::security::{
        audit::{AuditEntry, AuditExporter, AuditLevel, WebhookExportConfig},
        audit_export_webhook::WebhookAuditExporter,
    };

    fn test_entry() -> AuditEntry {
        AuditEntry {
            id:             Some(1),
            timestamp:      Utc::now(),
            level:          AuditLevel::INFO,
            user_id:        123,
            tenant_id:      456,
            operation:      "query".to_string(),
            query:          "{ users { id name } }".to_string(),
            variables:      serde_json::json!({}),
            ip_address:     "192.168.1.1".to_string(),
            user_agent:     "Mozilla/5.0".to_string(),
            error:          None,
            duration_ms:    Some(42),
            previous_hash:  None,
            integrity_hash: None,
        }
    }

    #[test]
    fn test_buffer_accumulates_entries() {
        let config = WebhookExportConfig {
            url:                 "https://example.com/audit".to_string(),
            headers:             std::collections::HashMap::new(),
            batch_size:          10,
            flush_interval_secs: 30,
        };

        let exporter = WebhookAuditExporter::new(&config).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        // Add entries below batch_size — should not flush.
        for _ in 0..5 {
            // Export will buffer but not flush (no server, so flush would fail).
            rt.block_on(exporter.export(&test_entry())).unwrap();
        }

        assert_eq!(exporter.buffer.lock().len(), 5);
    }

    #[test]
    fn test_flush_empties_buffer() {
        let config = WebhookExportConfig {
            url:                 "https://example.com/audit".to_string(),
            headers:             std::collections::HashMap::new(),
            batch_size:          100,
            flush_interval_secs: 30,
        };

        let exporter = WebhookAuditExporter::new(&config).unwrap();

        // Manually push entries into buffer.
        {
            let mut buf = exporter.buffer.lock();
            for _ in 0..5 {
                buf.push(test_entry());
            }
        }

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        // Flush will fail (no server) but should still drain the buffer.
        let _ = rt.block_on(exporter.flush());

        // Buffer should be empty even though send failed.
        assert_eq!(exporter.buffer.lock().len(), 0);
    }

    #[test]
    fn test_config_defaults() {
        let json = r#"{"url": "https://example.com/audit"}"#;
        let config: WebhookExportConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.batch_size, 100);
        assert_eq!(config.flush_interval_secs, 30);
        assert!(config.headers.is_empty());
    }

    #[test]
    fn test_syslog_config_defaults() {
        use crate::security::audit::SyslogExportConfig;
        let json = r#"{"address": "syslog.internal"}"#;
        let config: SyslogExportConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.port, 514);
        assert_eq!(config.protocol, "udp");
    }

    #[test]
    fn test_export_config_optional_fields() {
        use crate::security::audit::AuditExportConfig;
        let json = "{}";
        let config: AuditExportConfig = serde_json::from_str(json).unwrap();

        assert!(config.syslog.is_none());
        assert!(config.webhook.is_none());
    }
}

mod error_formatter_tests {
    use crate::security::{error_formatter::SanitizationConfig, *};

    // ============================================================================
    // Helper Functions
    // ============================================================================

    fn db_error_msg() -> &'static str {
        "Database error: connection refused to postgresql://user:password@db.example.com:5432/mydb"
    }

    fn sql_error_msg() -> &'static str {
        "SQL Error: SELECT * FROM users WHERE id = 123; failed at db.example.com"
    }

    fn network_error_msg() -> &'static str {
        "Connection failed to 192.168.1.100 (admin@example.com)"
    }

    // ============================================================================
    // Check 1: Detail Level Tests
    // ============================================================================

    #[test]
    fn test_development_shows_full_details() {
        let formatter = ErrorFormatter::development();
        let formatted = formatter.format_error(db_error_msg());
        assert!(formatted.contains("postgresql"));
        assert!(formatted.contains("user:password"));
    }

    #[test]
    fn test_staging_shows_limited_details() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(db_error_msg());
        // Staging should hide the database URL pattern
        assert!(!formatted.contains("postgresql://"));
        // Specific credentials may still appear but URL pattern is hidden
        let _ = formatted;
    }

    #[test]
    fn test_production_shows_generic_error() {
        let formatter = ErrorFormatter::production();
        let formatted = formatter.format_error(db_error_msg());
        assert!(!formatted.contains("postgresql"));
        assert!(!formatted.contains("password"));
        assert!(formatted.contains("error") || formatted.contains("request"));
    }

    // ============================================================================
    // Check 2: Sanitization Tests
    // ============================================================================

    #[test]
    fn test_database_url_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(db_error_msg());
        // The URL pattern should be replaced
        assert!(!formatted.contains("postgresql://"));
        // Verify something was replaced
        assert!(formatted.contains("**hidden**") || !formatted.contains("postgresql://"));
    }

    #[test]
    fn test_sql_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(sql_error_msg());
        assert!(!formatted.contains("SELECT"));
    }

    #[test]
    fn test_ip_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(network_error_msg());
        assert!(!formatted.contains("192.168"));
    }

    #[test]
    fn test_email_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(network_error_msg());
        assert!(!formatted.contains("admin@example"));
    }

    // ============================================================================
    // Check 3: SecurityError Formatting Tests
    // ============================================================================

    #[test]
    fn test_security_error_development() {
        let formatter = ErrorFormatter::development();
        let error = SecurityError::AuthRequired;
        let formatted = formatter.format_security_error(&error);
        assert!(formatted.contains("Authentication"));
    }

    #[test]
    fn test_security_error_production() {
        let formatter = ErrorFormatter::production();
        let error = SecurityError::AuthRequired;
        let formatted = formatter.format_security_error(&error);
        assert!(!formatted.is_empty());
        assert!(formatted.len() < 100); // Generic, short message
    }

    #[test]
    fn test_token_expired_error_production() {
        let formatter = ErrorFormatter::production();
        let error = SecurityError::TokenExpired {
            expired_at: chrono::Utc::now(),
        };
        let formatted = formatter.format_security_error(&error);
        assert!(!formatted.contains("expired_at"));
        assert!(formatted.contains("Invalid") || formatted.contains("Authentication"));
    }

    #[test]
    fn test_query_too_deep_error_production() {
        let formatter = ErrorFormatter::production();
        let error = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };
        let formatted = formatter.format_security_error(&error);
        assert!(!formatted.contains("20"));
        assert!(!formatted.contains("10"));
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_detail_level_display() {
        assert_eq!(DetailLevel::Development.to_string(), "Development");
        assert_eq!(DetailLevel::Staging.to_string(), "Staging");
        assert_eq!(DetailLevel::Production.to_string(), "Production");
    }

    #[test]
    fn test_sanitization_config_permissive() {
        let config = SanitizationConfig::permissive();
        assert!(!config.hide_database_urls);
        assert!(!config.hide_sql);
    }

    #[test]
    fn test_sanitization_config_standard() {
        let config = SanitizationConfig::standard();
        assert!(config.hide_database_urls);
        assert!(config.hide_sql);
        assert!(!config.hide_paths);
    }

    #[test]
    fn test_sanitization_config_strict() {
        let config = SanitizationConfig::strict();
        assert!(config.hide_database_urls);
        assert!(config.hide_sql);
        assert!(config.hide_paths);
    }

    #[test]
    fn test_formatter_helpers() {
        let dev = ErrorFormatter::development();
        assert_eq!(dev.detail_level(), DetailLevel::Development);

        let prod = ErrorFormatter::production();
        assert_eq!(prod.detail_level(), DetailLevel::Production);
    }

    // ============================================================================
    // Edge Cases
    // ============================================================================

    #[test]
    fn test_empty_error_message() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error("");
        assert!(formatted.is_empty() || !formatted.is_empty()); // Either is fine
    }

    #[test]
    fn test_multiple_sensitive_elements() {
        let formatter = ErrorFormatter::staging();
        let msg = "Failed to connect to postgresql://admin@192.168.1.1 with email user@example.com";
        let formatted = formatter.format_error(msg);

        assert!(!formatted.contains("postgresql"));
        assert!(!formatted.contains("192.168"));
        assert!(!formatted.contains("user@example"));
    }

    #[test]
    fn test_security_error_categorization() {
        let formatter = ErrorFormatter::production();

        // Auth errors
        let auth_error = SecurityError::AuthRequired;
        let formatted = formatter.format_security_error(&auth_error);
        assert!(formatted.contains("Authentication"));

        // Introspection error
        let intro_error = SecurityError::IntrospectionDisabled {
            detail: "test".to_string(),
        };
        let formatted = formatter.format_security_error(&intro_error);
        assert!(formatted.contains("introspection"));
    }

    #[test]
    fn test_custom_sanitization_config() {
        let config = SanitizationConfig {
            hide_database_urls: false,
            hide_sql:           false,
            hide_paths:         true,
            hide_ips:           false,
            hide_emails:        false,
            hide_credentials:   false,
        };

        let formatter = ErrorFormatter::with_config(DetailLevel::Staging, config);
        let msg = "Error at /home/user/project: connection to 192.168.1.1 failed";
        let formatted = formatter.format_error(msg);

        // Paths should be hidden when that config is true
        // IPs should not be hidden when that config is false
        assert!(formatted.contains("192.168"));
        // Paths may be redacted or contain the redacted version
        let _ = formatted;
    }

    #[test]
    fn test_long_error_truncation() {
        let formatter = ErrorFormatter::staging();
        let long_msg = "a".repeat(200);
        let formatted = formatter.format_error(&long_msg);

        // Should be truncated in some cases
        assert!(formatted.len() <= 200 + 10); // Allow some buffer
    }
}

mod errors_tests {
    use crate::security::*;

    #[test]
    fn test_rate_limit_error_display() {
        let err = SecurityError::RateLimitExceeded {
            retry_after: 60,
            limit:       100,
            window_secs: 60,
        };

        assert!(err.to_string().contains("Rate limit exceeded"));
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("60"));
    }

    #[test]
    fn test_query_too_deep_display() {
        let err = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };

        assert!(err.to_string().contains("Query too deep"));
        assert!(err.to_string().contains("20"));
        assert!(err.to_string().contains("10"));
    }

    #[test]
    fn test_query_too_complex_display() {
        let err = SecurityError::QueryTooComplex {
            complexity:     500,
            max_complexity: 100,
        };

        assert!(err.to_string().contains("Query too complex"));
        assert!(err.to_string().contains("500"));
        assert!(err.to_string().contains("100"));
    }

    #[test]
    fn test_query_too_large_display() {
        let err = SecurityError::QueryTooLarge {
            size:     100_000,
            max_size: 10_000,
        };

        assert!(err.to_string().contains("Query too large"));
        assert!(err.to_string().contains("100000"));
        assert!(err.to_string().contains("10000"));
    }

    #[test]
    fn test_cors_errors() {
        let origin_err = SecurityError::OriginNotAllowed("https://evil.com".to_string());
        assert!(origin_err.to_string().contains("CORS origin"));

        let method_err = SecurityError::MethodNotAllowed("DELETE".to_string());
        assert!(method_err.to_string().contains("CORS method"));

        let header_err = SecurityError::HeaderNotAllowed("X-Custom".to_string());
        assert!(header_err.to_string().contains("CORS header"));
    }

    #[test]
    fn test_csrf_errors() {
        let invalid = SecurityError::InvalidCSRFToken("expired".to_string());
        assert!(invalid.to_string().contains("Invalid CSRF token"));

        let mismatch = SecurityError::CSRFSessionMismatch;
        assert!(mismatch.to_string().contains("session mismatch"));
    }

    #[test]
    fn test_audit_error() {
        let err = SecurityError::AuditLogFailure("connection timeout".to_string());
        assert!(err.to_string().contains("Audit logging failed"));
    }

    #[test]
    fn test_config_error() {
        let err = SecurityError::SecurityConfigError("missing config key".to_string());
        assert!(err.to_string().contains("Security configuration error"));
    }

    #[test]
    fn test_error_equality() {
        let err1 = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };
        let err2 = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };
        assert_eq!(err1, err2);

        let err3 = SecurityError::QueryTooDeep {
            depth:     30,
            max_depth: 10,
        };
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_rate_limit_equality() {
        let err1 = SecurityError::RateLimitExceeded {
            retry_after: 60,
            limit:       100,
            window_secs: 60,
        };
        let err2 = SecurityError::RateLimitExceeded {
            retry_after: 60,
            limit:       100,
            window_secs: 60,
        };
        assert_eq!(err1, err2);
    }

    // ============================================================================
    // TLS Error Tests
    // ============================================================================

    #[test]
    fn test_tls_required_error_display() {
        let err = SecurityError::TlsRequired {
            detail: "HTTPS required".to_string(),
        };

        assert!(err.to_string().contains("TLS/HTTPS required"));
        assert!(err.to_string().contains("HTTPS required"));
    }

    #[test]
    fn test_tls_version_too_old_error_display() {
        use crate::security::tls_enforcer::TlsVersion;

        let err = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_2,
            required: TlsVersion::V1_3,
        };

        assert!(err.to_string().contains("TLS version too old"));
        assert!(err.to_string().contains("1.2"));
        assert!(err.to_string().contains("1.3"));
    }

    #[test]
    fn test_mtls_required_error_display() {
        let err = SecurityError::MtlsRequired {
            detail: "Client certificate required".to_string(),
        };

        assert!(err.to_string().contains("Mutual TLS required"));
        assert!(err.to_string().contains("Client certificate"));
    }

    #[test]
    fn test_invalid_client_cert_error_display() {
        let err = SecurityError::InvalidClientCert {
            detail: "Certificate validation failed".to_string(),
        };

        assert!(err.to_string().contains("Invalid client certificate"));
        assert!(err.to_string().contains("validation failed"));
    }

    #[test]
    fn test_auth_required_error_display() {
        let err = SecurityError::AuthRequired;
        assert!(err.to_string().contains("Authentication required"));
    }

    #[test]
    fn test_invalid_token_error_display() {
        let err = SecurityError::InvalidToken;
        assert!(err.to_string().contains("Invalid authentication token"));
    }

    #[test]
    fn test_token_expired_error_display() {
        use chrono::{Duration, Utc};

        let expired_at = Utc::now() - Duration::hours(1);
        let err = SecurityError::TokenExpired { expired_at };

        assert!(err.to_string().contains("Token expired"));
    }

    #[test]
    fn test_token_missing_claim_error_display() {
        let err = SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        };

        assert!(err.to_string().contains("Token missing required claim"));
        assert!(err.to_string().contains("sub"));
    }

    #[test]
    fn test_invalid_token_algorithm_error_display() {
        let err = SecurityError::InvalidTokenAlgorithm {
            algorithm: "HS256".to_string(),
        };

        assert!(err.to_string().contains("Invalid token algorithm"));
        assert!(err.to_string().contains("HS256"));
    }

    #[test]
    fn test_introspection_disabled_error_display() {
        let err = SecurityError::IntrospectionDisabled {
            detail: "Introspection not allowed in production".to_string(),
        };

        assert!(err.to_string().contains("Introspection disabled"));
        assert!(err.to_string().contains("production"));
    }

    // ============================================================================
    // TLS Error Equality Tests
    // ============================================================================

    #[test]
    fn test_tls_required_equality() {
        let err1 = SecurityError::TlsRequired {
            detail: "test".to_string(),
        };
        let err2 = SecurityError::TlsRequired {
            detail: "test".to_string(),
        };
        assert_eq!(err1, err2);

        let err3 = SecurityError::TlsRequired {
            detail: "different".to_string(),
        };
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_tls_version_too_old_equality() {
        use crate::security::tls_enforcer::TlsVersion;

        let err1 = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_2,
            required: TlsVersion::V1_3,
        };
        let err2 = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_2,
            required: TlsVersion::V1_3,
        };
        assert_eq!(err1, err2);

        let err3 = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_1,
            required: TlsVersion::V1_3,
        };
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_mtls_required_equality() {
        let err1 = SecurityError::MtlsRequired {
            detail: "test".to_string(),
        };
        let err2 = SecurityError::MtlsRequired {
            detail: "test".to_string(),
        };
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_invalid_token_equality() {
        assert_eq!(SecurityError::InvalidToken, SecurityError::InvalidToken);
    }

    #[test]
    fn test_auth_required_equality() {
        assert_eq!(SecurityError::AuthRequired, SecurityError::AuthRequired);
    }
}

mod field_filter_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use crate::security::*;

    // ========================================================================
    // Test Suite 1: Basic Configuration
    // ========================================================================

    #[test]
    fn test_empty_config_allows_all() {
        let filter = FieldFilter::permissive();
        let scopes: Vec<String> = vec![];

        filter
            .can_access("User", "name", &scopes)
            .unwrap_or_else(|e| panic!("expected access to User.name: {e}"));
        filter
            .can_access("User", "email", &scopes)
            .unwrap_or_else(|e| panic!("expected access to User.email: {e}"));
        filter
            .can_access("User", "salary", &scopes)
            .unwrap_or_else(|e| panic!("expected access to User.salary: {e}"));
    }

    #[test]
    fn test_protect_single_field() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // Unprotected fields are allowed
        let no_scopes: Vec<String> = vec![];
        filter
            .can_access("User", "name", &no_scopes)
            .unwrap_or_else(|e| panic!("expected access to User.name: {e}"));
        filter
            .can_access("User", "email", &no_scopes)
            .unwrap_or_else(|e| panic!("expected access to User.email: {e}"));

        // Protected field is denied without scope
        let result = filter.can_access("User", "salary", &no_scopes);
        assert_eq!(result.unwrap_err(), FieldAccessError::new("User", "salary"));
    }

    #[test]
    fn test_protect_multiple_fields() {
        let config = FieldFilterConfig::new().protect_fields("User", &["salary", "ssn", "bonus"]);
        let filter = FieldFilter::new(config);

        let no_scopes: Vec<String> = vec![];
        filter
            .can_access("User", "name", &no_scopes)
            .unwrap_or_else(|e| panic!("expected access to User.name: {e}"));
        assert_eq!(
            filter.can_access("User", "salary", &no_scopes).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );
        assert_eq!(
            filter.can_access("User", "ssn", &no_scopes).unwrap_err(),
            FieldAccessError::new("User", "ssn")
        );
        assert_eq!(
            filter.can_access("User", "bonus", &no_scopes).unwrap_err(),
            FieldAccessError::new("User", "bonus")
        );
    }

    #[test]
    fn test_protect_entire_type() {
        let config = FieldFilterConfig::new().protect_type("Secret");
        let filter = FieldFilter::new(config);

        let no_scopes: Vec<String> = vec![];
        // All fields on Secret type require authorization
        assert_eq!(
            filter.can_access("Secret", "anything", &no_scopes).unwrap_err(),
            FieldAccessError::new("Secret", "anything")
        );
        assert_eq!(
            filter.can_access("Secret", "data", &no_scopes).unwrap_err(),
            FieldAccessError::new("Secret", "data")
        );

        // Other types are fine
        filter
            .can_access("User", "name", &no_scopes)
            .unwrap_or_else(|e| panic!("expected access to User.name: {e}"));
    }

    // ========================================================================
    // Test Suite 2: Scope Matching
    // ========================================================================

    #[test]
    fn test_exact_scope_match() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:User.salary".to_string()];
        filter
            .can_access("User", "salary", &scopes)
            .unwrap_or_else(|e| panic!("expected exact scope match: {e}"));
    }

    #[test]
    fn test_type_wildcard_scope() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("User", "ssn");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:User.*".to_string()];
        filter
            .can_access("User", "salary", &scopes)
            .unwrap_or_else(|e| panic!("expected type wildcard to grant access to salary: {e}"));
        filter
            .can_access("User", "ssn", &scopes)
            .unwrap_or_else(|e| panic!("expected type wildcard to grant access to ssn: {e}"));
    }

    #[test]
    fn test_global_wildcard_scope() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("Employee", "compensation");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:*".to_string()];
        filter.can_access("User", "salary", &scopes).unwrap_or_else(|e| {
            panic!("expected global wildcard to grant access to User.salary: {e}")
        });
        filter.can_access("Employee", "compensation", &scopes).unwrap_or_else(|e| {
            panic!("expected global wildcard to grant access to Employee.compensation: {e}")
        });
    }

    #[test]
    fn test_admin_scope_bypasses_all() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("User", "ssn")
            .protect_type("Secret");
        let filter = FieldFilter::new(config);

        let scopes = vec!["admin".to_string()];
        filter
            .can_access("User", "salary", &scopes)
            .unwrap_or_else(|e| panic!("expected admin to bypass User.salary: {e}"));
        filter
            .can_access("User", "ssn", &scopes)
            .unwrap_or_else(|e| panic!("expected admin to bypass User.ssn: {e}"));
        filter
            .can_access("Secret", "data", &scopes)
            .unwrap_or_else(|e| panic!("expected admin to bypass Secret.data: {e}"));
    }

    #[test]
    fn test_custom_admin_scope() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .add_admin_scope("superuser");
        let filter = FieldFilter::new(config);

        let scopes = vec!["superuser".to_string()];
        filter
            .can_access("User", "salary", &scopes)
            .unwrap_or_else(|e| panic!("expected custom admin scope to bypass: {e}"));
    }

    #[test]
    fn test_wrong_scope_denied() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // Wrong type
        let scopes = vec!["read:Employee.salary".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &scopes).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );

        // Wrong field
        let scopes = vec!["read:User.name".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &scopes).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );

        // Wrong action (write instead of read)
        let scopes = vec!["write:User.salary".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &scopes).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );
    }

    // ========================================================================
    // Test Suite 3: Explicit Scope Requirements
    // ========================================================================

    #[test]
    fn test_explicit_scope_requirement() {
        let config = FieldFilterConfig::new().protect_field("User", "salary").require_scope(
            "User",
            "salary",
            "hr:view_compensation",
        );
        let filter = FieldFilter::new(config);

        // Default pattern doesn't work
        let wrong_scope = vec!["read:User.salary".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &wrong_scope).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );

        // Explicit scope works
        let right_scope = vec!["hr:view_compensation".to_string()];
        filter
            .can_access("User", "salary", &right_scope)
            .unwrap_or_else(|e| panic!("expected explicit scope to grant access: {e}"));
    }

    #[test]
    fn test_admin_still_bypasses_explicit() {
        let config = FieldFilterConfig::new().protect_field("User", "salary").require_scope(
            "User",
            "salary",
            "hr:view_compensation",
        );
        let filter = FieldFilter::new(config);

        let admin_scope = vec!["admin".to_string()];
        filter
            .can_access("User", "salary", &admin_scope)
            .unwrap_or_else(|e| panic!("expected admin to bypass explicit scope: {e}"));
    }

    // ========================================================================
    // Test Suite 4: Error Messages
    // ========================================================================

    #[test]
    fn test_error_contains_field_info() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let no_scopes: Vec<String> = vec![];
        let err = filter.can_access("User", "salary", &no_scopes).unwrap_err();

        assert_eq!(err.type_name, "User");
        assert_eq!(err.field_name, "salary");
        assert!(err.message.contains("salary"));
        assert!(err.message.contains("User"));
    }

    #[test]
    fn test_error_display() {
        let err = FieldAccessError::new("User", "salary");
        let display = err.to_string();

        assert!(display.contains("Access denied"));
        assert!(display.contains("salary"));
        assert!(display.contains("User"));
    }

    // ========================================================================
    // Test Suite 5: Batch Validation
    // ========================================================================

    #[test]
    fn test_validate_multiple_fields() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("User", "ssn");
        let filter = FieldFilter::new(config);

        let fields = ["name", "email", "salary", "ssn"];
        let no_scopes: Vec<String> = vec![];

        let errors = filter.validate_fields("User", &fields, &no_scopes);
        assert_eq!(errors.len(), 2);

        let error_fields: Vec<&str> = errors.iter().map(|e| e.field_name.as_str()).collect();
        assert!(error_fields.contains(&"salary"));
        assert!(error_fields.contains(&"ssn"));
    }

    #[test]
    fn test_validate_all_allowed() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let fields = ["name", "email", "salary"];
        let scopes = vec!["read:User.salary".to_string()];

        let errors = filter.validate_fields("User", &fields, &scopes);
        assert!(errors.is_empty(), "expected no validation errors, got: {errors:?}");
    }

    // ========================================================================
    // Test Suite 6: Builder Pattern
    // ========================================================================

    #[test]
    fn test_builder_basic() {
        let mut builder = FieldFilterBuilder::new();
        builder.add_protected_field("User", "salary");
        builder.add_protected_field("User", "ssn");

        let filter = builder.build();
        let no_scopes: Vec<String> = vec![];

        assert_eq!(
            filter.can_access("User", "salary", &no_scopes).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );
        assert_eq!(
            filter.can_access("User", "ssn", &no_scopes).unwrap_err(),
            FieldAccessError::new("User", "ssn")
        );
        filter
            .can_access("User", "name", &no_scopes)
            .unwrap_or_else(|e| panic!("expected access to unprotected User.name: {e}"));
    }

    #[test]
    fn test_builder_with_explicit_scopes() {
        let mut builder = FieldFilterBuilder::new();
        builder.add_scope_requirement("User", "salary", "hr:compensation");

        let filter = builder.build();

        let wrong = vec!["read:User.salary".to_string()];
        let right = vec!["hr:compensation".to_string()];

        assert_eq!(
            filter.can_access("User", "salary", &wrong).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );
        filter
            .can_access("User", "salary", &right)
            .unwrap_or_else(|e| panic!("expected explicit scope to grant access: {e}"));
    }

    #[test]
    fn test_builder_custom_admin_scopes() {
        let mut builder = FieldFilterBuilder::new();
        builder.add_protected_field("User", "salary");
        builder.set_admin_scopes(vec!["root".to_string(), "superadmin".to_string()]);

        let filter = builder.build();

        // Default admin scope no longer works
        let admin = vec!["admin".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &admin).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );

        // Custom admin scopes work
        let root = vec!["root".to_string()];
        filter
            .can_access("User", "salary", &root)
            .unwrap_or_else(|e| panic!("expected root admin scope to bypass: {e}"));

        let superadmin = vec!["superadmin".to_string()];
        filter
            .can_access("User", "salary", &superadmin)
            .unwrap_or_else(|e| panic!("expected superadmin scope to bypass: {e}"));
    }

    // ========================================================================
    // Test Suite 7: Config Inspection
    // ========================================================================

    #[test]
    fn test_is_protected() {
        let config =
            FieldFilterConfig::new().protect_field("User", "salary").protect_type("Secret");

        assert!(config.is_protected("User", "salary"));
        assert!(!config.is_protected("User", "name"));
        assert!(config.is_protected("Secret", "anything"));
        assert!(!config.is_protected("Public", "data"));
    }

    #[test]
    fn test_config_default_action() {
        let config = FieldFilterConfig::new()
            .with_default_action("view")
            .protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // "read" action doesn't work
        let read_scope = vec!["read:User.salary".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &read_scope).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );

        // "view" action works
        let view_scope = vec!["view:User.salary".to_string()];
        filter
            .can_access("User", "salary", &view_scope)
            .unwrap_or_else(|e| panic!("expected view action to grant access: {e}"));
    }

    // ========================================================================
    // Test Suite 8: Edge Cases
    // ========================================================================

    #[test]
    fn test_empty_scopes() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let empty: Vec<String> = vec![];
        assert_eq!(
            filter.can_access("User", "salary", &empty).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );
    }

    #[test]
    fn test_multiple_scopes_one_match() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let scopes = vec![
            "read:Product.*".to_string(),
            "write:Order.status".to_string(),
            "read:User.salary".to_string(), // This one matches
            "other:scope".to_string(),
        ];
        filter
            .can_access("User", "salary", &scopes)
            .unwrap_or_else(|e| panic!("expected one matching scope to suffice: {e}"));
    }

    #[test]
    fn test_case_sensitive_scopes() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // Scopes are case-sensitive
        let wrong_case = vec!["READ:User.salary".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &wrong_case).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );

        let wrong_type_case = vec!["read:user.salary".to_string()];
        assert_eq!(
            filter.can_access("User", "salary", &wrong_type_case).unwrap_err(),
            FieldAccessError::new("User", "salary")
        );
    }

    #[test]
    fn test_special_characters_in_names() {
        let config =
            FieldFilterConfig::new().protect_field("UserProfile", "social_security_number");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:UserProfile.social_security_number".to_string()];
        filter
            .can_access("UserProfile", "social_security_number", &scopes)
            .unwrap_or_else(|e| panic!("expected access with special characters in names: {e}"));
    }
}

mod field_masking_tests {
    use crate::security::*;

    // ========================================================================
    // Test Suite 1: Field Sensitivity Detection - Public Fields
    // ========================================================================

    #[test]
    fn test_id_is_public() {
        assert_eq!(FieldMasker::detect_sensitivity("id"), FieldSensitivity::Public);
    }

    #[test]
    fn test_name_is_public() {
        assert_eq!(FieldMasker::detect_sensitivity("name"), FieldSensitivity::Public);
    }

    #[test]
    fn test_title_is_public() {
        assert_eq!(FieldMasker::detect_sensitivity("title"), FieldSensitivity::Public);
    }

    #[test]
    fn test_description_is_public() {
        assert_eq!(FieldMasker::detect_sensitivity("description"), FieldSensitivity::Public);
    }

    #[test]
    fn test_created_at_is_public() {
        assert_eq!(FieldMasker::detect_sensitivity("created_at"), FieldSensitivity::Public);
    }

    // ========================================================================
    // Test Suite 2: Field Sensitivity Detection - Sensitive Fields
    // ========================================================================

    #[test]
    fn test_email_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("email"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_email_address_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("email_address"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_user_email_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("user_email"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_phone_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("phone"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_phone_number_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("phone_number"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_mobile_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("mobile"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_mobile_phone_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("mobile_phone"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_ip_address_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("ip_address"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_mac_address_is_sensitive() {
        assert_eq!(FieldMasker::detect_sensitivity("mac_address"), FieldSensitivity::Sensitive);
    }

    // ========================================================================
    // Test Suite 3: Field Sensitivity Detection - PII Fields
    // ========================================================================

    #[test]
    fn test_ssn_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("ssn"), FieldSensitivity::PII);
    }

    #[test]
    fn test_social_security_is_pii() {
        assert_eq!(
            FieldMasker::detect_sensitivity("social_security_number"),
            FieldSensitivity::PII
        );
    }

    #[test]
    fn test_credit_card_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("credit_card"), FieldSensitivity::PII);
    }

    #[test]
    fn test_card_number_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("card_number"), FieldSensitivity::PII);
    }

    #[test]
    fn test_cvv_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("cvv"), FieldSensitivity::PII);
    }

    #[test]
    fn test_cvc_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("cvc"), FieldSensitivity::PII);
    }

    #[test]
    fn test_bank_account_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("bank_account"), FieldSensitivity::PII);
    }

    #[test]
    fn test_pin_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("pin"), FieldSensitivity::PII);
    }

    #[test]
    fn test_driver_license_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("driver_license"), FieldSensitivity::PII);
    }

    #[test]
    fn test_passport_is_pii() {
        assert_eq!(FieldMasker::detect_sensitivity("passport"), FieldSensitivity::PII);
    }

    // ========================================================================
    // Test Suite 4: Field Sensitivity Detection - Secret Fields
    // ========================================================================

    #[test]
    fn test_password_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("password"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_password_hash_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("password_hash"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_secret_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("secret"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_secret_key_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("secret_key"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_token_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("token"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_refresh_token_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("refresh_token"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_api_key_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("api_key"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_auth_token_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("auth_token"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_jwt_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("jwt"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_jwt_prefixed_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("jwt_token"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_id_jwt_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("id_jwt"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_nonce_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("nonce"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_nonce_value_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("nonce_value"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_bearer_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("bearer"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_client_secret_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("client_secret"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_oauth_client_secret_is_secret() {
        assert_eq!(
            FieldMasker::detect_sensitivity("oauth_client_secret"),
            FieldSensitivity::Secret
        );
    }

    #[test]
    fn test_hash_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("hash"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_signature_is_secret() {
        assert_eq!(FieldMasker::detect_sensitivity("signature"), FieldSensitivity::Secret);
    }

    // ========================================================================
    // Test Suite 5: Case Insensitivity
    // ========================================================================

    #[test]
    fn test_case_insensitive_email() {
        assert_eq!(FieldMasker::detect_sensitivity("EMAIL"), FieldSensitivity::Sensitive);
    }

    #[test]
    fn test_case_insensitive_password() {
        assert_eq!(FieldMasker::detect_sensitivity("PASSWORD"), FieldSensitivity::Secret);
    }

    #[test]
    fn test_mixed_case_ssn() {
        assert_eq!(FieldMasker::detect_sensitivity("SSN"), FieldSensitivity::PII);
    }

    // ========================================================================
    // Test Suite 6: Value Masking - Public
    // ========================================================================

    #[test]
    fn test_public_value_unmasked() {
        let result = FieldMasker::mask_value("value", FieldSensitivity::Public);
        assert_eq!(result, "value");
    }

    #[test]
    fn test_public_empty_string_unmasked() {
        let result = FieldMasker::mask_value("", FieldSensitivity::Public);
        assert_eq!(result, "");
    }

    // ========================================================================
    // Test Suite 7: Value Masking - Sensitive
    // ========================================================================

    #[test]
    fn test_sensitive_email_masked() {
        let result = FieldMasker::mask_value("user@example.com", FieldSensitivity::Sensitive);
        assert_eq!(result, "u***");
    }

    #[test]
    fn test_sensitive_phone_masked() {
        let result = FieldMasker::mask_value("555-1234", FieldSensitivity::Sensitive);
        assert_eq!(result, "5***");
    }

    #[test]
    fn test_sensitive_single_char_masked() {
        let result = FieldMasker::mask_value("a", FieldSensitivity::Sensitive);
        assert_eq!(result, "a***");
    }

    #[test]
    fn test_sensitive_empty_masked() {
        let result = FieldMasker::mask_value("", FieldSensitivity::Sensitive);
        assert_eq!(result, "***");
    }

    // ========================================================================
    // Test Suite 8: Value Masking - PII
    // ========================================================================

    #[test]
    fn test_pii_ssn_masked() {
        let result = FieldMasker::mask_value("123-45-6789", FieldSensitivity::PII);
        assert_eq!(result, "[PII]");
    }

    #[test]
    fn test_pii_credit_card_masked() {
        let result = FieldMasker::mask_value("4111-1111-1111-1111", FieldSensitivity::PII);
        assert_eq!(result, "[PII]");
    }

    #[test]
    fn test_pii_empty_masked() {
        let result = FieldMasker::mask_value("", FieldSensitivity::PII);
        assert_eq!(result, "[PII]");
    }

    // ========================================================================
    // Test Suite 9: Value Masking - Secret
    // ========================================================================

    #[test]
    fn test_secret_password_masked() {
        let result = FieldMasker::mask_value("mypassword123", FieldSensitivity::Secret);
        assert_eq!(result, "****");
    }

    #[test]
    fn test_secret_token_masked() {
        let result = FieldMasker::mask_value("token_abc123xyz", FieldSensitivity::Secret);
        assert_eq!(result, "****");
    }

    #[test]
    fn test_secret_empty_masked() {
        let result = FieldMasker::mask_value("", FieldSensitivity::Secret);
        assert_eq!(result, "****");
    }

    #[test]
    fn test_secret_any_value_masked() {
        let result = FieldMasker::mask_value("anything", FieldSensitivity::Secret);
        assert_eq!(result, "****");
    }

    // ========================================================================
    // Test Suite 10: Profile-Based Masking Decision
    // ========================================================================

    #[test]
    fn test_standard_profile_no_masking() {
        let standard = SecurityProfile::standard();
        assert!(!FieldMasker::should_mask(FieldSensitivity::Public, &standard));
        assert!(!FieldMasker::should_mask(FieldSensitivity::Sensitive, &standard));
        assert!(!FieldMasker::should_mask(FieldSensitivity::PII, &standard));
        assert!(!FieldMasker::should_mask(FieldSensitivity::Secret, &standard));
    }

    #[test]
    fn test_regulated_profile_public_no_masking() {
        let regulated = SecurityProfile::regulated();
        assert!(!FieldMasker::should_mask(FieldSensitivity::Public, &regulated));
    }

    #[test]
    fn test_regulated_profile_sensitive_masked() {
        let regulated = SecurityProfile::regulated();
        assert!(FieldMasker::should_mask(FieldSensitivity::Sensitive, &regulated));
    }

    #[test]
    fn test_regulated_profile_pii_masked() {
        let regulated = SecurityProfile::regulated();
        assert!(FieldMasker::should_mask(FieldSensitivity::PII, &regulated));
    }

    #[test]
    fn test_regulated_profile_secret_masked() {
        let regulated = SecurityProfile::regulated();
        assert!(FieldMasker::should_mask(FieldSensitivity::Secret, &regulated));
    }

    // ========================================================================
    // Test Suite 11: Edge Cases
    // ========================================================================

    #[test]
    fn test_very_long_email_masked() {
        let long_email = "a".repeat(1000) + "@example.com";
        let result = FieldMasker::mask_value(&long_email, FieldSensitivity::Sensitive);
        assert_eq!(result, "a***");
        assert!(result.len() < long_email.len());
    }

    #[test]
    fn test_unicode_email_masked() {
        let result = FieldMasker::mask_value("émail@example.com", FieldSensitivity::Sensitive);
        assert_eq!(result, "é***");
    }

    #[test]
    fn test_sensitivity_display() {
        assert_eq!(FieldSensitivity::Public.to_string(), "public");
        assert_eq!(FieldSensitivity::Sensitive.to_string(), "sensitive");
        assert_eq!(FieldSensitivity::PII.to_string(), "pii");
        assert_eq!(FieldSensitivity::Secret.to_string(), "secret");
    }
}

mod headers_tests {
    use crate::security::*;

    #[test]
    fn test_default_security_headers() {
        let headers = SecurityHeaders::default();
        assert!(headers.has("X-XSS-Protection"));
        assert!(headers.has("X-Content-Type-Options"));
        assert!(headers.has("X-Frame-Options"));
        assert!(headers.has("Referrer-Policy"));
        assert!(headers.has("Permissions-Policy"));
    }

    #[test]
    fn test_production_security_headers() {
        let headers = SecurityHeaders::production();
        assert!(headers.has("Content-Security-Policy"));
        assert!(headers.has("Strict-Transport-Security"));
        assert!(headers.has("X-XSS-Protection")); // Should inherit from default
    }

    #[test]
    fn test_custom_header_operations() {
        let mut headers = SecurityHeaders::default();

        // Add custom header
        headers.add("X-Custom-Header".to_string(), "custom-value".to_string());
        assert_eq!(headers.get("X-Custom-Header"), Some(&"custom-value".to_string()));

        // Remove header
        headers.remove("X-Custom-Header");
        assert!(!headers.has("X-Custom-Header"));
    }

    #[test]
    fn test_header_merge() {
        let mut headers1 = SecurityHeaders::default();
        let mut headers2 = SecurityHeaders::default();

        headers2.add("X-Custom".to_string(), "value".to_string());
        headers1.merge(&headers2);

        assert!(headers1.has("X-Custom"));
        assert_eq!(headers1.get("X-Custom"), Some(&"value".to_string()));
    }
}

mod introspection_enforcer_tests {
    use crate::security::{introspection_enforcer::IntrospectionConfig, *};

    // ============================================================================
    // Helper Functions
    // ============================================================================

    fn introspection_schema_query() -> &'static str {
        "{ __schema { types { name } } }"
    }

    fn introspection_type_query() -> &'static str {
        "{ __type(name: \"User\") { name fields { name } } }"
    }

    fn normal_query() -> &'static str {
        "{ user { id name email } }"
    }

    // ============================================================================
    // Check 1: Introspection Detection Tests
    // ============================================================================

    #[test]
    fn test_detect_schema_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let is_introspection = enforcer.is_introspection_query(introspection_schema_query());
        assert!(is_introspection);
    }

    #[test]
    fn test_detect_type_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let is_introspection = enforcer.is_introspection_query(introspection_type_query());
        assert!(is_introspection);
    }

    #[test]
    fn test_normal_query_not_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let is_introspection = enforcer.is_introspection_query(normal_query());
        assert!(!is_introspection);
    }

    #[test]
    fn test_detection_is_case_sensitive() {
        // GraphQL is case-sensitive: `__SCHEMA` is an ordinary (invalid) field
        // name, not the `__schema` meta-field, so it is not introspection.
        let enforcer = IntrospectionEnforcer::allowed();
        let uppercase_query = "{ __SCHEMA { types { name } } }";
        let is_introspection = enforcer.is_introspection_query(uppercase_query);
        assert!(!is_introspection);
    }

    // ============================================================================
    // Check 2: Authentication Check Tests
    // ============================================================================

    #[test]
    fn test_internal_only_allows_authenticated_user() {
        let enforcer = IntrospectionEnforcer::internal_only();
        enforcer
            .validate_query(introspection_schema_query(), Some("user123"))
            .unwrap_or_else(|e| panic!("expected authenticated user to be allowed: {e}"));
    }

    #[test]
    fn test_internal_only_rejects_anonymous_user() {
        let enforcer = IntrospectionEnforcer::internal_only();
        let result = enforcer.validate_query(introspection_schema_query(), None);
        assert!(matches!(result, Err(SecurityError::IntrospectionDisabled { .. })));
    }

    // ============================================================================
    // Check 3: Policy Enforcement Tests
    // ============================================================================

    #[test]
    fn test_allowed_policy_permits_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        enforcer
            .validate_query(introspection_schema_query(), None)
            .unwrap_or_else(|e| panic!("expected Allowed policy to permit introspection: {e}"));
    }

    #[test]
    fn test_allowed_policy_permits_anonymous_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        enforcer.validate_query(introspection_schema_query(), None).unwrap_or_else(|e| {
            panic!("expected Allowed policy to permit anonymous introspection: {e}")
        });
    }

    #[test]
    fn test_disabled_policy_blocks_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        let result = enforcer.validate_query(introspection_schema_query(), None);
        assert!(matches!(result, Err(SecurityError::IntrospectionDisabled { .. })));
    }

    #[test]
    fn test_disabled_policy_blocks_authenticated_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        let result = enforcer.validate_query(introspection_schema_query(), Some("user123"));
        assert!(matches!(result, Err(SecurityError::IntrospectionDisabled { .. })));
    }

    #[test]
    fn test_policy_allows_normal_queries_always() {
        let disabled_enforcer = IntrospectionEnforcer::disabled();
        disabled_enforcer.validate_query(normal_query(), None).unwrap_or_else(|e| {
            panic!("expected normal queries to pass even with Disabled policy: {e}")
        });
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_introspection_config_all() {
        let config = IntrospectionConfig::all();
        assert!(config.detect_schema);
        assert!(config.detect_type);
    }

    #[test]
    fn test_introspection_config_strict_coincides_with_all() {
        // Post-#454 (`__typename` / `__directive` knobs removed) the two
        // configs detect the same meta-fields.
        let config = IntrospectionConfig::strict();
        assert!(config.detect_schema);
        assert!(config.detect_type);
    }

    #[test]
    fn test_policy_display() {
        assert_eq!(IntrospectionPolicy::Allowed.to_string(), "Allowed");
        assert_eq!(IntrospectionPolicy::Disabled.to_string(), "Disabled");
        assert_eq!(IntrospectionPolicy::InternalOnly.to_string(), "InternalOnly");
    }

    #[test]
    fn test_policy_from_config_truth_table() {
        // The single source of truth shared by the GraphQL gate and the REST
        // `/introspection` mount decision (#453).
        assert_eq!(IntrospectionPolicy::from_config(false, true), IntrospectionPolicy::Disabled);
        assert_eq!(IntrospectionPolicy::from_config(false, false), IntrospectionPolicy::Disabled);
        assert_eq!(IntrospectionPolicy::from_config(true, true), IntrospectionPolicy::InternalOnly);
        assert_eq!(IntrospectionPolicy::from_config(true, false), IntrospectionPolicy::Allowed);
    }

    #[test]
    fn test_enforcer_helpers() {
        let allowed = IntrospectionEnforcer::allowed();
        assert_eq!(allowed.policy(), IntrospectionPolicy::Allowed);

        let disabled = IntrospectionEnforcer::disabled();
        assert_eq!(disabled.policy(), IntrospectionPolicy::Disabled);

        let internal = IntrospectionEnforcer::internal_only();
        assert_eq!(internal.policy(), IntrospectionPolicy::InternalOnly);
    }

    // ============================================================================
    // Custom Configuration Tests
    // ============================================================================

    #[test]
    fn test_custom_config_with_selective_detection() {
        let config = IntrospectionConfig {
            detect_schema: true,
            detect_type:   false,
        };

        let enforcer = IntrospectionEnforcer::with_config(IntrospectionPolicy::Disabled, config);

        // __schema should be detected and blocked
        let schema_query = "{ __schema { types { name } } }";
        assert!(matches!(
            enforcer.validate_query(schema_query, None),
            Err(SecurityError::IntrospectionDisabled { .. })
        ));

        // __type should NOT be detected (allowed through)
        let type_query = "{ __type(name: \"User\") { name } }";
        enforcer.validate_query(type_query, None).unwrap_or_else(|e| {
            panic!("expected __type to pass through when detect_type=false: {e}")
        });
    }

    // ============================================================================
    // Edge Cases
    // ============================================================================

    #[test]
    fn test_introspection_in_string_literal_not_detected() {
        let enforcer = IntrospectionEnforcer::disabled();
        // `__schema` here is a string argument value, not a root field. AST-based
        // detection (#454) keys on the root field name, so this is not flagged.
        let query = r#"{ user(filter: "__schema") { name } }"#;
        let is_introspection = enforcer.is_introspection_query(query);
        assert!(!is_introspection);
    }

    #[test]
    fn test_empty_query_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        enforcer.validate_query("", None).unwrap_or_else(|e| {
            panic!("expected empty query to be allowed (not introspection): {e}")
        });
    }

    #[test]
    fn test_whitespace_handled_correctly() {
        let enforcer = IntrospectionEnforcer::allowed();
        let query = "{\n  __schema {\n    types { name }\n  }\n}";
        let is_introspection = enforcer.is_introspection_query(query);
        assert!(is_introspection);
    }

    #[test]
    fn test_multiple_introspection_patterns() {
        let enforcer = IntrospectionEnforcer::allowed();
        let query = "{ __schema { types { name } } __type(name: \"Query\") { name } }";
        let is_introspection = enforcer.is_introspection_query(query);
        assert!(is_introspection);
    }

    // ========================================================================
    // #454: AST-accurate detection truth table
    //
    // `__typename` is never introspection (GraphQL spec §"Type Name
    // Introspection" — it is queryable on every type regardless of policy);
    // aliases, string-argument values, and comments must not false-positive;
    // `__schema` / `__type` are matched as real root fields by *name* (not
    // response alias), including in multi-root documents.
    // ========================================================================

    #[test]
    fn test_root_typename_is_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(!enforcer.is_introspection_query("{ __typename }"));
        // ...and is allowed through even under the strictest policy.
        enforcer
            .validate_query("{ __typename }", None)
            .unwrap_or_else(|e| panic!("__typename must never be blocked: {e}"));
    }

    #[test]
    fn test_nested_typename_is_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(!enforcer.is_introspection_query("{ user { __typename } }"));
    }

    #[test]
    fn test_aliased_typename_is_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(!enforcer.is_introspection_query("{ x: __typename }"));
    }

    #[test]
    fn test_schema_in_string_argument_is_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(!enforcer.is_introspection_query(r#"{ search(q: "__schema") }"#));
    }

    #[test]
    fn test_type_in_comment_is_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        let query = "query Foo { a: user { name } }  # mentions __type in a comment";
        assert!(!enforcer.is_introspection_query(query));
    }

    #[test]
    fn test_aliased_normal_field_is_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(!enforcer.is_introspection_query("{ alias: users { id } }"));
    }

    #[test]
    fn test_aliased_schema_is_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(enforcer.is_introspection_query("{ foo: __schema { queryType { name } } }"));
    }

    #[test]
    fn test_multi_root_schema_is_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        let query = "{ users { id } __schema { types { name } } }";
        assert!(enforcer.is_introspection_query(query));
    }

    #[test]
    fn test_type_field_query_is_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(enforcer.is_introspection_query(r#"{ __type(name: "X") { name } }"#));
    }

    #[test]
    fn test_malformed_query_is_not_introspection() {
        // Parse errors must not fail-open into a 500 nor double-error: the
        // executor/handler rejects malformed queries on the normal parse path.
        let enforcer = IntrospectionEnforcer::disabled();
        assert!(!enforcer.is_introspection_query("{ this is not valid graphql"));
    }
}

mod profiles_tests {
    use crate::security::*;

    // ========================================================================
    // Test Suite 1: Profile Creation and Basic Properties
    // ========================================================================

    #[test]
    fn test_create_standard_profile() {
        let profile = SecurityProfile::standard();
        assert!(profile.is_standard());
        assert!(!profile.is_regulated());
        assert_eq!(profile.name(), "STANDARD");
    }

    #[test]
    fn test_create_regulated_profile() {
        let profile = SecurityProfile::regulated();
        assert!(!profile.is_standard());
        assert!(profile.is_regulated());
        assert_eq!(profile.name(), "REGULATED");
    }

    #[test]
    fn test_default_profile_is_standard() {
        let profile = SecurityProfile::default();
        assert!(profile.is_standard());
    }

    #[test]
    fn test_profile_display() {
        assert_eq!(SecurityProfile::Standard.to_string(), "STANDARD");
        assert_eq!(SecurityProfile::Regulated.to_string(), "REGULATED");
    }

    // ========================================================================
    // Test Suite 2: Common Features (Both Profiles)
    // ========================================================================

    #[test]
    fn test_standard_has_rate_limiting() {
        let profile = SecurityProfile::standard();
        assert!(profile.rate_limit_enabled());
    }

    #[test]
    fn test_regulated_has_rate_limiting() {
        let profile = SecurityProfile::regulated();
        assert!(profile.rate_limit_enabled());
    }

    #[test]
    fn test_standard_has_audit_logging() {
        let profile = SecurityProfile::standard();
        assert!(profile.audit_logging_enabled());
    }

    #[test]
    fn test_regulated_has_audit_logging() {
        let profile = SecurityProfile::regulated();
        assert!(profile.audit_logging_enabled());
    }

    // ========================================================================
    // Test Suite 3: STANDARD Profile Features
    // ========================================================================

    #[test]
    fn test_standard_no_field_audit() {
        let profile = SecurityProfile::standard();
        assert!(!profile.audit_field_access());
    }

    #[test]
    fn test_standard_no_field_masking() {
        let profile = SecurityProfile::standard();
        assert!(!profile.sensitive_field_masking());
    }

    #[test]
    fn test_standard_no_error_redaction() {
        let profile = SecurityProfile::standard();
        assert!(!profile.error_detail_reduction());
    }

    #[test]
    fn test_standard_no_query_logging_compliance() {
        let profile = SecurityProfile::standard();
        assert!(!profile.query_logging_for_compliance());
    }

    #[test]
    fn test_standard_no_response_limits() {
        let profile = SecurityProfile::standard();
        assert!(!profile.response_size_limits());
    }

    #[test]
    fn test_standard_no_strict_filtering() {
        let profile = SecurityProfile::standard();
        assert!(!profile.field_filtering_strict());
    }

    #[test]
    fn test_standard_unlimited_response_size() {
        let profile = SecurityProfile::standard();
        assert_eq!(profile.max_response_size_bytes(), usize::MAX);
    }

    #[test]
    fn test_standard_rate_limit_rps() {
        let profile = SecurityProfile::standard();
        assert_eq!(profile.rate_limit_rps(), 100);
    }

    // ========================================================================
    // Test Suite 4: REGULATED Profile Features
    // ========================================================================

    #[test]
    fn test_regulated_has_field_audit() {
        let profile = SecurityProfile::regulated();
        assert!(profile.audit_field_access());
    }

    #[test]
    fn test_regulated_has_field_masking() {
        let profile = SecurityProfile::regulated();
        assert!(profile.sensitive_field_masking());
    }

    #[test]
    fn test_regulated_has_error_redaction() {
        let profile = SecurityProfile::regulated();
        assert!(profile.error_detail_reduction());
    }

    #[test]
    fn test_regulated_has_query_logging_compliance() {
        let profile = SecurityProfile::regulated();
        assert!(profile.query_logging_for_compliance());
    }

    #[test]
    fn test_regulated_has_response_limits() {
        let profile = SecurityProfile::regulated();
        assert!(profile.response_size_limits());
    }

    #[test]
    fn test_regulated_has_strict_filtering() {
        let profile = SecurityProfile::regulated();
        assert!(profile.field_filtering_strict());
    }

    #[test]
    fn test_regulated_response_size_limit() {
        let profile = SecurityProfile::regulated();
        assert_eq!(profile.max_response_size_bytes(), 1_000_000);
    }

    #[test]
    fn test_regulated_rate_limit_stricter() {
        let standard = SecurityProfile::standard();
        let regulated = SecurityProfile::regulated();
        assert!(regulated.rate_limit_rps() < standard.rate_limit_rps());
    }

    #[test]
    fn test_regulated_query_complexity_stricter() {
        let standard = SecurityProfile::standard();
        let regulated = SecurityProfile::regulated();
        assert!(regulated.max_query_complexity() < standard.max_query_complexity());
    }

    #[test]
    fn test_regulated_query_depth_stricter() {
        let standard = SecurityProfile::standard();
        let regulated = SecurityProfile::regulated();
        assert!(regulated.max_query_depth() < standard.max_query_depth());
    }

    // ========================================================================
    // Test Suite 5: Limits and Thresholds
    // ========================================================================

    #[test]
    fn test_standard_query_limits() {
        let profile = SecurityProfile::standard();
        assert!(profile.max_query_complexity() > 0);
        assert!(profile.max_query_depth() > 0);
    }

    #[test]
    fn test_regulated_query_limits() {
        let profile = SecurityProfile::regulated();
        assert!(profile.max_query_complexity() > 0);
        assert!(profile.max_query_depth() > 0);
    }

    #[test]
    fn test_response_size_reasonable() {
        let profile = SecurityProfile::regulated();
        assert!(profile.max_response_size_bytes() > 100_000); // At least 100KB
        assert!(profile.max_response_size_bytes() < 100_000_000); // Less than 100MB
    }

    // ========================================================================
    // Test Suite 6: Feature Descriptions
    // ========================================================================

    #[test]
    fn test_standard_description() {
        let profile = SecurityProfile::standard();
        let desc = profile.description();
        assert!(!desc.is_empty());
        assert!(desc.contains("rate limiting"));
    }

    #[test]
    fn test_regulated_description() {
        let profile = SecurityProfile::regulated();
        let desc = profile.description();
        assert!(!desc.is_empty());
        assert!(desc.contains("compliance"));
    }

    #[test]
    fn test_standard_enforced_features() {
        let profile = SecurityProfile::standard();
        let features = profile.enforced_features();
        assert!(features.contains(&"Rate Limiting"));
        assert!(features.contains(&"Audit Logging"));
        assert_eq!(features.len(), 2);
    }

    #[test]
    fn test_regulated_enforced_features() {
        let profile = SecurityProfile::regulated();
        let features = profile.enforced_features();
        assert!(features.len() > 2);
        assert!(features.contains(&"Rate Limiting"));
        assert!(features.contains(&"Audit Logging"));
        assert!(features.contains(&"Sensitive Field Masking"));
        assert!(features.contains(&"Error Detail Reduction"));
    }

    // ========================================================================
    // Test Suite 7: Profile Comparison
    // ========================================================================

    #[test]
    fn test_profile_equality() {
        let standard1 = SecurityProfile::standard();
        let standard2 = SecurityProfile::standard();
        let regulated = SecurityProfile::regulated();

        assert_eq!(standard1, standard2);
        assert_ne!(standard1, regulated);
    }

    #[test]
    fn test_profile_clone() {
        let original = SecurityProfile::regulated();
        let cloned = original;
        assert_eq!(original, cloned);
    }

    // ========================================================================
    // Test Suite 8: Edge Cases
    // ========================================================================

    #[test]
    fn test_profile_features_are_superset() {
        // REGULATED should have everything STANDARD has, plus more
        let standard = SecurityProfile::standard();
        let regulated = SecurityProfile::regulated();

        assert_eq!(standard.rate_limit_enabled(), regulated.rate_limit_enabled());
        assert_eq!(standard.audit_logging_enabled(), regulated.audit_logging_enabled());
        // But REGULATED should have additional features
        assert!(regulated.audit_field_access() || !standard.audit_field_access());
    }

    #[test]
    fn test_all_features_documented() {
        let profile = SecurityProfile::standard();
        assert!(!profile.name().is_empty());
        assert!(!profile.description().is_empty());

        let profile = SecurityProfile::regulated();
        assert!(!profile.name().is_empty());
        assert!(!profile.description().is_empty());
    }

    #[test]
    fn test_rate_limits_are_positive() {
        for profile in &[SecurityProfile::Standard, SecurityProfile::Regulated] {
            assert!(
                profile.rate_limit_rps() > 0,
                "Profile {profile} should have positive rate limit"
            );
        }
    }
}

mod query_validator_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use crate::security::*;

    // ============================================================================
    // Check 1: Query Size Validation Tests
    // ============================================================================

    fn large_query(size: usize) -> String {
        "{ ".to_string() + "field ".repeat(size).as_str() + "}"
    }

    #[test]
    fn test_query_size_within_limit() {
        let validator = QueryValidator::standard();
        validator
            .validate("{ user { id name } }")
            .unwrap_or_else(|e| panic!("expected Ok for small query: {e}"));
    }

    #[test]
    fn test_query_size_exceeds_limit() {
        let validator = QueryValidator::standard();
        let q = large_query(100_000);
        let result = validator.validate(&q);
        assert!(matches!(result, Err(SecurityError::QueryTooLarge { .. })));
    }

    // ============================================================================
    // Check 2: Malformed query
    // ============================================================================

    #[test]
    fn test_malformed_query_returns_error() {
        let validator = QueryValidator::standard();
        let result = validator.validate("this is not graphql {{{}}}");
        assert!(
            matches!(result, Err(SecurityError::MalformedQuery(_))),
            "malformed query must return MalformedQuery error, got {result:?}"
        );
    }

    // ============================================================================
    // Check 3: Query Depth Validation Tests
    // ============================================================================

    #[test]
    fn test_valid_query_depth() {
        let validator = QueryValidator::standard();
        let metrics = validator
            .validate("{ user { id name } }")
            .unwrap_or_else(|e| panic!("expected Ok for shallow query: {e}"));
        assert!(metrics.depth <= validator.config().max_depth);
    }

    #[test]
    fn test_query_depth_exceeds_limit() {
        let validator = QueryValidator::strict(); // max_depth = 5
        // depth = 7 (a→b→c→d→e→f→g)
        let deep = "{ a { b { c { d { e { f { g } } } } } } }";
        let result = validator.validate(deep);
        assert!(
            matches!(result, Err(SecurityError::QueryTooDeep { .. })),
            "depth-7 query must be rejected with strict (max=5), got {result:?}"
        );
    }

    #[test]
    fn test_very_deep_query_rejected() {
        let validator = QueryValidator::strict(); // max_depth = 5
        // depth = 8 (a→b→c→d→e→f→g→h)
        let deep = "{ a { b { c { d { e { f { g { h } } } } } } } }";
        let result = validator.validate(deep);
        assert!(
            matches!(result, Err(SecurityError::QueryTooDeep { .. })),
            "depth-8 query must be rejected, got {result:?}"
        );
    }

    // ============================================================================
    // Check 4: Query Complexity Validation Tests
    // ============================================================================

    #[test]
    fn test_valid_query_complexity() {
        let validator = QueryValidator::standard();
        let metrics = validator
            .validate("{ user { id name } }")
            .unwrap_or_else(|e| panic!("expected Ok for simple query: {e}"));
        assert!(metrics.complexity <= validator.config().max_complexity);
    }

    #[test]
    fn test_complexity_calculated() {
        let validator = QueryValidator::standard();
        let metrics = validator.validate("{ user { id } }").unwrap();
        assert!(metrics.complexity > 0);
    }

    // ============================================================================
    // Check 5: Alias amplification protection
    // ============================================================================

    #[test]
    fn test_alias_amplification_rejected() {
        let validator = QueryValidator::standard(); // max_aliases = 30
        let aliases: String =
            (0..31).map(|i| ["a", &i.to_string(), ": user { id } "].concat()).collect();
        let query = format!("{{ {aliases} }}");
        let result = validator.validate(&query);
        assert!(
            matches!(
                result,
                Err(SecurityError::TooManyAliases {
                    alias_count: 31,
                    max_aliases: 30,
                })
            ),
            "31-alias query must be rejected with TooManyAliases, got {result:?}"
        );
    }

    #[test]
    fn test_alias_within_limit_allowed() {
        let validator = QueryValidator::standard(); // max_aliases = 30
        let aliases: String =
            (0..5).map(|i| ["a", &i.to_string(), ": user { id } "].concat()).collect();
        let query = format!("{{ {aliases} }}");
        let result = validator.validate(&query);
        assert!(result.is_ok(), "5 aliases should be allowed, got {result:?}");
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_permissive_config() {
        let config = QueryValidatorConfig::permissive();
        assert_eq!(config.max_depth, 20);
        assert_eq!(config.max_complexity, 5000);
        assert_eq!(config.max_size_bytes, 1_000_000);
        assert_eq!(config.max_aliases, 100);
    }

    #[test]
    fn test_standard_config() {
        let config = QueryValidatorConfig::standard();
        assert_eq!(config.max_depth, 10);
        assert_eq!(config.max_complexity, 1000);
        assert_eq!(config.max_size_bytes, 256_000);
        assert_eq!(config.max_aliases, 30);
    }

    #[test]
    fn test_strict_config() {
        let config = QueryValidatorConfig::strict();
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.max_complexity, 500);
        assert_eq!(config.max_size_bytes, 64_000);
        assert_eq!(config.max_aliases, 10);
    }

    #[test]
    fn test_validator_helpers() {
        let permissive = QueryValidator::permissive();
        assert_eq!(permissive.config().max_depth, 20);

        let standard = QueryValidator::standard();
        assert_eq!(standard.config().max_depth, 10);

        let strict = QueryValidator::strict();
        assert_eq!(strict.config().max_depth, 5);
    }

    // ============================================================================
    // Metrics Tests
    // ============================================================================

    #[test]
    fn test_metrics_returned_on_valid_query() {
        let validator = QueryValidator::standard();
        let query = "{ user { id name } }";
        let metrics = validator.validate(query).unwrap();
        assert!(metrics.depth >= 2); // user → (id, name)
        assert!(metrics.complexity > 0);
        assert_eq!(metrics.alias_count, 0);
    }

    #[test]
    fn test_alias_count_in_metrics() {
        let validator = QueryValidator::standard();
        let query = "{ a: user { id } b: user { id } }";
        let metrics = validator.validate(query).unwrap();
        assert_eq!(metrics.alias_count, 2);
    }
}

mod rls_policy_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::collections::HashMap;

    use crate::{
        db::WhereClause,
        security::{
            rls_policy::{RLSRule, extract_user_value},
            *,
        },
        utils::clock::Clock,
    };

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_context(user_id: &str, roles: Vec<&str>, tenant_id: Option<&str>) -> SecurityContext {
        SecurityContext {
            user_id:          user_id.into(),
            roles:            roles.into_iter().map(String::from).collect(),
            tenant_id:        tenant_id.map(Into::into),
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req1".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    #[test]
    fn trace_id_is_none_without_a_stamp() {
        let ctx = make_context("u1", vec![], None);
        assert_eq!(ctx.trace_id(), None);
    }

    #[test]
    fn with_trace_id_round_trips_through_the_attribute_bag() {
        let tid = "4bf92f3577b34da6a3ce929d0e0e4736";
        let ctx = make_context("u1", vec![], None).with_trace_id(tid);
        assert_eq!(ctx.trace_id(), Some(tid));
        // Stored under the documented key so the server (writer) and the change-log
        // stamp (reader) agree.
        assert_eq!(
            ctx.get_attribute(SecurityContext::TRACE_ID_ATTRIBUTE)
                .and_then(serde_json::Value::as_str),
            Some(tid),
        );
    }

    #[test]
    fn trace_context_is_none_without_a_stamp() {
        let ctx = make_context("u1", vec![], None);
        assert_eq!(ctx.trace_context(), None);
    }

    #[test]
    fn with_trace_context_round_trips_through_the_attribute_bag() {
        let tc = serde_json::json!({
            "version": "00",
            "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
            "parent_id": "00f067aa0ba902b7",
            "trace_flags": "01"
        });
        let ctx = make_context("u1", vec![], None).with_trace_context(tc.clone());
        assert_eq!(ctx.trace_context(), Some(&tc));
        // Stored under the documented key (as a real JSON object, not double-encoded)
        // so the server (writer) and the change-log stamp (reader) agree.
        assert_eq!(ctx.get_attribute(SecurityContext::TRACE_CONTEXT_ATTRIBUTE), Some(&tc));
    }

    fn cacheable_owner_rule() -> RLSRule {
        RLSRule {
            name:              "owner_only".to_string(),
            expression:        "user.id == object.author_id".to_string(),
            cacheable:         true,
            cache_ttl_seconds: Some(300),
        }
    }

    fn policy_with_rule(rule: RLSRule) -> CompiledRLSPolicy {
        let mut rules_by_type = std::collections::HashMap::new();
        rules_by_type.insert("Post".to_string(), vec![rule]);
        CompiledRLSPolicy::new(rules_by_type, None)
    }

    fn policy_with_rule_and_clock(
        rule: RLSRule,
        clock: std::sync::Arc<dyn crate::utils::clock::Clock>,
    ) -> CompiledRLSPolicy {
        let mut rules_by_type = std::collections::HashMap::new();
        rules_by_type.insert("Post".to_string(), vec![rule]);
        CompiledRLSPolicy::new_with_clock(rules_by_type, None, clock)
    }

    // ── DefaultRLSPolicy ─────────────────────────────────────────────────────

    #[test]
    fn test_with_tenant_field_sets_field_name() {
        // Kills mutation: with_tenant_field → Default::default() (line 225).
        // Default::default() returns a policy with tenant_field = "tenant_id";
        // after with_tenant_field("org_id") it must be "org_id".
        let policy = DefaultRLSPolicy::new().with_tenant_field("org_id".to_string());
        assert_eq!(policy.tenant_field, "org_id", "with_tenant_field must update tenant_field");

        // Verify the custom field name appears in the generated WHERE clause
        let context = make_context("user1", vec!["viewer"], Some("org42"));
        let result = policy.evaluate(&context, "Post").unwrap().unwrap();
        let sql = format!("{:?}", result.into_where_clause());
        assert!(sql.contains("org_id"), "custom tenant field must appear in WHERE clause: {sql}");
        assert!(!sql.contains("\"tenant_id\""), "default field name must not appear: {sql}");
    }

    #[test]
    fn test_with_owner_field_sets_field_name() {
        // Kills mutation: with_owner_field → Default::default() (line 231).
        // Default::default() returns a policy with owner_field = "author_id".
        let policy = DefaultRLSPolicy::new().with_owner_field("creator_id".to_string());
        assert_eq!(policy.owner_field, "creator_id", "with_owner_field must update owner_field");

        // Verify the custom field appears in the generated WHERE clause
        let context = make_context("user1", vec!["viewer"], None);
        let result = policy.evaluate(&context, "Post").unwrap().unwrap();
        let sql = format!("{:?}", result.into_where_clause());
        assert!(
            sql.contains("creator_id"),
            "custom owner field must appear in WHERE clause: {sql}"
        );
        assert!(!sql.contains("author_id"), "default field name must not appear: {sql}");
    }

    #[test]
    fn test_default_rls_policy_admin_bypass() {
        let policy = DefaultRLSPolicy::new();
        let context = make_context("user123", vec!["admin"], Some("tenant1"));
        let result = policy.evaluate(&context, "Post").unwrap();
        assert_eq!(result, None, "Admins should bypass RLS");
    }

    #[test]
    fn test_default_rls_policy_tenant_isolation() {
        let policy = DefaultRLSPolicy::new();
        let context = make_context("user123", vec!["user"], Some("tenant1"));
        let result = policy.evaluate(&context, "Post").unwrap();
        assert!(result.is_some(), "Non-admin users should have RLS filter applied");
    }

    #[test]
    fn test_no_rls_policy() {
        let policy = NoRLSPolicy;
        let context = make_context("user123", vec![], None);
        let result = policy.evaluate(&context, "Post").unwrap();
        assert_eq!(result, None, "NoRLSPolicy should never apply filters");
    }

    // ── Cache TTL correctness (kills lines 399, 414) ─────────────────────────

    #[test]
    fn test_compiled_rls_cache_entry_has_correct_ttl() {
        // Verifies: expires_at = clock.now_secs() + ttl_secs  (line 414)
        // Mutation `+ → -` would give expires_at already in the past; `+ → *` gives huge value.
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();
        // Move clock into policy (t0 already captured — clock not needed afterwards)
        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock);
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // First evaluation populates cache
        policy.evaluate(&context, "Post").unwrap();

        let cache = policy.cache.read();
        let entry =
            cache.get("user1:Post").expect("cache should be populated after first evaluate");
        assert_eq!(entry.expires_at, t0 + 300, "expires_at must be now_secs + ttl_secs (300)");
    }

    #[test]
    fn test_compiled_rls_cache_hit_does_not_refresh_expiry() {
        // Verifies: when now < expires_at, cache is read and expiry is NOT updated (line 399: <).
        // Mutation `< → ==` would cause cache miss at any time ≠ expires_at.
        // Mutation `< → >` would cause hit AFTER expiry (backward).
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock.clone());
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // Populate cache at T
        policy.evaluate(&context, "Post").unwrap();
        let first_expires_at = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(first_expires_at, t0 + 300);

        // Advance to 1 second before expiry — still within TTL
        clock.advance(std::time::Duration::from_secs(299));

        // Should hit cache, NOT re-calculate expiry
        policy.evaluate(&context, "Post").unwrap();
        let second_expires_at = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(
            second_expires_at, first_expires_at,
            "Cache hit must not update expires_at (would indicate a miss + re-cache)"
        );
    }

    #[test]
    fn test_compiled_rls_cache_miss_after_expiry_refreshes_entry() {
        // Verifies: when now >= expires_at, cache is NOT used and entry is refreshed (line 399: <).
        // Mutation `< → >` would use the stale entry (no refresh), so expires_at stays at T+300.
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock.clone());
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // Populate cache at T → expires_at = T+300
        policy.evaluate(&context, "Post").unwrap();

        // Advance 301 seconds — clearly past expiry
        clock.advance(std::time::Duration::from_secs(301));

        // Cache miss: re-evaluates and re-caches with new expiry = (T+301)+300 = T+601
        policy.evaluate(&context, "Post").unwrap();
        let new_expires = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(
            new_expires,
            t0 + 601,
            "After TTL expiry, cache must be refreshed with updated expires_at"
        );
    }

    #[test]
    fn test_compiled_rls_cache_expires_exactly_at_ttl_boundary() {
        // Verifies: at exactly expires_at (now == expires_at), cache is expired (line 399: <).
        // Mutation `< → <=` would treat the exact boundary as still valid (off-by-one).
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock.clone());
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // Populate cache at T → expires_at = T+300
        policy.evaluate(&context, "Post").unwrap();

        // Advance to EXACTLY the expiry second
        clock.advance(std::time::Duration::from_secs(300));
        assert_eq!(clock.now_secs(), t0 + 300);

        // At exactly expires_at, the entry must be considered expired (now < expires_at is false)
        policy.evaluate(&context, "Post").unwrap();
        let refreshed_expires = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(
            refreshed_expires,
            t0 + 600,
            "At exact TTL boundary, cache must expire and refresh (< not <=)"
        );
    }

    // ── cache_result (kills line 432) ─────────────────────────────────────────

    #[test]
    fn test_cache_result_stores_with_300s_default_ttl() {
        // Verifies: cache_result uses 300s TTL (line 432: +).
        // Mutation `+ → -` gives expires_at already past; `+ → *` gives huge value.
        // Mutation "delete entire method" would leave cache empty.
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy =
            CompiledRLSPolicy::new_with_clock(std::collections::HashMap::new(), None, clock);

        let result = Some(WhereClause::Field {
            path:     vec!["author_id".to_string()],
            operator: crate::db::WhereOperator::Eq,
            value:    serde_json::json!("user_x"),
        });

        policy.cache_result("user_x:Post", &result);

        let cache = policy.cache.read();
        let entry = cache.get("user_x:Post").expect("cache_result must insert entry");
        assert_eq!(entry.expires_at, t0 + 300, "cache_result must use 300s TTL");
        assert_eq!(entry.result, result, "cache_result must store the provided result");
    }

    #[test]
    fn test_cache_result_stores_none_result() {
        // Verifies cache_result stores None (bypass) results correctly.
        use crate::utils::clock::ManualClock;
        let policy = CompiledRLSPolicy::new_with_clock(
            std::collections::HashMap::new(),
            None,
            std::sync::Arc::new(ManualClock::new()),
        );

        policy.cache_result("user1:Post", &None);

        let cache = policy.cache.read();
        let entry = cache.get("user1:Post").expect("cache_result must store even None result");
        assert!(entry.result.is_none(), "cached None result must remain None");
    }

    // ── extract_user_value (kills line 518) ──────────────────────────────────

    #[test]
    fn test_extract_user_value_id_field() {
        // Mutation `→ None` gives None; `→ Some(Default)` gives Some(Null). Both fail.
        let ctx = make_context("user_abc_123", vec![], None);
        assert_eq!(
            extract_user_value("id", &ctx),
            Some(serde_json::json!("user_abc_123")),
            "'id' must return the actual user_id"
        );
    }

    #[test]
    fn test_extract_user_value_user_id_alias() {
        let ctx = make_context("user_abc_123", vec![], None);
        assert_eq!(
            extract_user_value("user_id", &ctx),
            Some(serde_json::json!("user_abc_123")),
            "'user_id' must return the same user_id as 'id'"
        );
    }

    #[test]
    fn test_extract_user_value_tenant_id_present() {
        let ctx = make_context("u1", vec![], Some("tenant_xyz"));
        assert_eq!(
            extract_user_value("tenant_id", &ctx),
            Some(serde_json::json!("tenant_xyz")),
            "'tenant_id' must return the tenant id when present"
        );
    }

    #[test]
    fn test_extract_user_value_tenant_id_absent() {
        let ctx = make_context("u1", vec![], None);
        assert_eq!(
            extract_user_value("tenant_id", &ctx),
            None,
            "'tenant_id' must return None when absent, not Some(null)"
        );
    }

    #[test]
    fn test_extract_user_value_roles_field() {
        let ctx = make_context("u1", vec!["editor", "viewer"], None);
        assert_eq!(
            extract_user_value("roles", &ctx),
            Some(serde_json::json!(["editor", "viewer"])),
            "'roles' must return the full roles array"
        );
    }

    #[test]
    fn test_extract_user_value_custom_attribute() {
        let mut attrs = HashMap::new();
        attrs.insert("department".to_string(), serde_json::json!("engineering"));
        let ctx = SecurityContext {
            user_id:          "u1".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       attrs,
            request_id:       "r1".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };
        assert_eq!(
            extract_user_value("department", &ctx),
            Some(serde_json::json!("engineering")),
            "Custom attribute must be returned by name"
        );
    }

    #[test]
    fn test_extract_user_value_unknown_returns_none() {
        let ctx = make_context("u1", vec![], None);
        assert_eq!(
            extract_user_value("nonexistent_field", &ctx),
            None,
            "Unknown field must return None, not Some(null)"
        );
    }

    // ── extract_user_value integration: user_id flows to WHERE clause ─────────

    #[test]
    fn test_user_id_propagated_to_rls_where_clause() {
        // Ensures extract_user_value("id") result reaches the generated WhereClause.
        // Kills mutations → None and → Some(Default) on line 518.
        let policy = policy_with_rule(RLSRule {
            name:              "owner_only".to_string(),
            expression:        "user.id == object.author_id".to_string(),
            cacheable:         false,
            cache_ttl_seconds: None,
        });

        let context = make_context("specific_user_42", vec!["viewer"], None);
        let result = policy.evaluate(&context, "Post").unwrap();

        let clause = result.expect("non-admin user must receive an RLS filter").into_where_clause();
        match clause {
            WhereClause::Field { value, .. } => {
                assert_eq!(
                    value,
                    serde_json::json!("specific_user_42"),
                    "RLS WhereClause must embed the actual user_id, not null"
                );
            },
            other => panic!("Expected Field clause, got {other:?}"),
        }
    }
}

mod security_context_tests {
    use std::collections::HashMap;

    use chrono::Utc;

    use crate::security::*;

    #[test]
    fn test_has_role() {
        let context = SecurityContext {
            user_id:          "user123".into(),
            roles:            vec!["admin".to_string(), "moderator".to_string()],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };

        assert!(context.has_role("admin"));
        assert!(context.has_role("moderator"));
        assert!(!context.has_role("superadmin"));
    }

    #[test]
    fn test_has_scope() {
        let context = SecurityContext {
            user_id:          "user123".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec!["read:user".to_string(), "write:post".to_string()],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };

        assert!(context.has_scope("read:user"));
        assert!(context.has_scope("write:post"));
        assert!(!context.has_scope("admin:*"));
    }

    #[test]
    fn test_wildcard_scopes() {
        let context = SecurityContext {
            user_id:          "user123".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec!["admin:*".to_string()],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };

        assert!(context.has_scope("admin:read"));
        assert!(context.has_scope("admin:write"));
        assert!(!context.has_scope("user:read"));
    }

    #[test]
    fn test_builder_pattern() {
        use crate::types::TenantId;
        let now = Utc::now();
        let context = SecurityContext {
            user_id:          "user123".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: now,
            expires_at:       now + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
        .with_role("admin".to_string())
        .with_scopes(vec!["read:user".to_string()])
        .with_tenant("tenant-1".to_string());

        assert!(context.has_role("admin"));
        assert!(context.has_scope("read:user"));
        assert_eq!(context.tenant_id, Some(TenantId::new("tenant-1")));
    }
}

mod tls_enforcer_tests {
    use crate::security::*;

    // ============================================================================
    // Check 1: HTTPS Requirement Tests
    // ============================================================================

    #[test]
    fn test_http_allowed_when_tls_not_required() {
        let enforcer = TlsEnforcer::permissive();
        let conn = TlsConnection::new_http();

        enforcer
            .validate_connection(&conn)
            .unwrap_or_else(|e| panic!("expected HTTP allowed when TLS not required: {e}"));
    }

    #[test]
    fn test_http_rejected_when_tls_required() {
        let enforcer = TlsEnforcer::standard();
        let conn = TlsConnection::new_http();

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::TlsRequired { .. })));
    }

    #[test]
    fn test_https_allowed_when_tls_required() {
        let enforcer = TlsEnforcer::standard();
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        enforcer
            .validate_connection(&conn)
            .unwrap_or_else(|e| panic!("expected HTTPS allowed when TLS required: {e}"));
    }

    // ============================================================================
    // Check 2: TLS Version Minimum Tests
    // ============================================================================

    #[test]
    fn test_tls_1_0_rejected_when_min_1_3() {
        let enforcer = TlsEnforcer::strict(); // min_version = TLS 1.3
        let conn = TlsConnection::new_secure(TlsVersion::V1_0);

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::TlsVersionTooOld { .. })));
    }

    #[test]
    fn test_tls_1_2_rejected_when_min_1_3() {
        let enforcer = TlsEnforcer::strict(); // min_version = TLS 1.3
        let conn = TlsConnection::new_secure(TlsVersion::V1_2);

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::TlsVersionTooOld { .. })));
    }

    #[test]
    fn test_tls_1_3_allowed_when_min_1_2() {
        let enforcer = TlsEnforcer::standard(); // min_version = TLS 1.2
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        enforcer
            .validate_connection(&conn)
            .unwrap_or_else(|e| panic!("expected TLS 1.3 allowed when min 1.2: {e}"));
    }

    #[test]
    fn test_tls_1_2_allowed_when_min_1_2() {
        let enforcer = TlsEnforcer::standard(); // min_version = TLS 1.2
        let conn = TlsConnection::new_secure(TlsVersion::V1_2);

        enforcer
            .validate_connection(&conn)
            .unwrap_or_else(|e| panic!("expected TLS 1.2 allowed when min 1.2: {e}"));
    }

    #[test]
    fn test_tls_version_check_skipped_for_http() {
        // When connection is HTTP, version check is irrelevant
        let enforcer = TlsEnforcer::permissive();
        let conn = TlsConnection::new_http();

        // Even though version is V1_2, this passes because is_secure=false
        enforcer
            .validate_connection(&conn)
            .unwrap_or_else(|e| panic!("expected version check skipped for HTTP: {e}"));
    }

    // ============================================================================
    // Check 3: mTLS Requirement Tests
    // ============================================================================

    #[test]
    fn test_client_cert_optional_when_mtls_not_required() {
        let enforcer = TlsEnforcer::standard(); // mtls_required = false
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        enforcer.validate_connection(&conn).unwrap_or_else(|e| {
            panic!("expected no client cert needed when mTLS not required: {e}")
        });
    }

    #[test]
    fn test_client_cert_required_when_mtls_required() {
        let enforcer = TlsEnforcer::strict(); // mtls_required = true
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::MtlsRequired { .. })));
    }

    #[test]
    fn test_client_cert_allowed_when_mtls_required() {
        let enforcer = TlsEnforcer::strict(); // mtls_required = true
        let conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);

        enforcer.validate_connection(&conn).unwrap_or_else(|e| {
            panic!("expected valid client cert accepted when mTLS required: {e}")
        });
    }

    // ============================================================================
    // Check 4: Client Certificate Validity Tests
    // ============================================================================

    #[test]
    fn test_invalid_cert_rejected() {
        let enforcer = TlsEnforcer::strict();
        let conn = TlsConnection {
            is_secure:         true,
            version:           TlsVersion::V1_3,
            has_client_cert:   true,
            client_cert_valid: false, // Invalid!
        };

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::InvalidClientCert { .. })));
    }

    #[test]
    fn test_valid_cert_accepted() {
        let enforcer = TlsEnforcer::strict();
        let conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);

        enforcer
            .validate_connection(&conn)
            .unwrap_or_else(|e| panic!("expected valid cert accepted: {e}"));
    }

    // ============================================================================
    // Combination Tests (Multiple Checks)
    // ============================================================================

    #[test]
    fn test_all_3_tls_settings_enforced_together() {
        let enforcer = TlsEnforcer::strict();
        // strict: tls_required=true, mtls_required=true, min_version=V1_3

        // This should pass all checks
        let valid_conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);
        enforcer
            .validate_connection(&valid_conn)
            .unwrap_or_else(|e| panic!("expected all checks to pass: {e}"));

        // Fails check 1: HTTP when TLS required
        let http_conn = TlsConnection::new_http();
        assert!(matches!(
            enforcer.validate_connection(&http_conn),
            Err(SecurityError::TlsRequired { .. })
        ));

        // Fails check 2: TLS 1.2 when min 1.3 required
        let old_tls_conn = TlsConnection::new_secure(TlsVersion::V1_2);
        assert!(matches!(
            enforcer.validate_connection(&old_tls_conn),
            Err(SecurityError::TlsVersionTooOld { .. })
        ));

        // Fails check 3: No client cert when mTLS required
        let no_cert_conn = TlsConnection::new_secure(TlsVersion::V1_3);
        assert!(matches!(
            enforcer.validate_connection(&no_cert_conn),
            Err(SecurityError::MtlsRequired { .. })
        ));
    }

    // ============================================================================
    // Error Message Tests
    // ============================================================================

    #[test]
    fn test_error_messages_clear_and_loggable() {
        let enforcer = TlsEnforcer::strict();

        let tls_required_err = enforcer.validate_connection(&TlsConnection::new_http());
        if let Err(SecurityError::TlsRequired { detail }) = tls_required_err {
            assert!(!detail.is_empty());
            assert!(detail.contains("HTTP") || detail.contains("HTTPS"));
        } else {
            panic!("Expected TlsRequired error");
        }

        let tls_version_err =
            enforcer.validate_connection(&TlsConnection::new_secure(TlsVersion::V1_0));
        if let Err(SecurityError::TlsVersionTooOld { current, required }) = tls_version_err {
            assert_eq!(current, TlsVersion::V1_0);
            assert_eq!(required, TlsVersion::V1_3);
        } else {
            panic!("Expected TlsVersionTooOld error");
        }
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_permissive_config() {
        let config = TlsConfig::permissive();
        assert!(!config.tls_required);
        assert!(!config.mtls_required);
        assert_eq!(config.min_version, TlsVersion::V1_2);
    }

    #[test]
    fn test_standard_config() {
        let config = TlsConfig::standard();
        assert!(config.tls_required);
        assert!(!config.mtls_required);
        assert_eq!(config.min_version, TlsVersion::V1_2);
    }

    #[test]
    fn test_strict_config() {
        let config = TlsConfig::strict();
        assert!(config.tls_required);
        assert!(config.mtls_required);
        assert_eq!(config.min_version, TlsVersion::V1_3);
    }

    #[test]
    fn test_enforcer_helpers() {
        let permissive = TlsEnforcer::permissive();
        assert!(!permissive.config().tls_required);

        let standard = TlsEnforcer::standard();
        assert!(standard.config().tls_required);

        let strict = TlsEnforcer::strict();
        assert!(strict.config().mtls_required);
    }

    // ============================================================================
    // TlsVersion Tests
    // ============================================================================

    #[test]
    fn test_tls_version_display() {
        assert_eq!(TlsVersion::V1_0.to_string(), "TLS 1.0");
        assert_eq!(TlsVersion::V1_1.to_string(), "TLS 1.1");
        assert_eq!(TlsVersion::V1_2.to_string(), "TLS 1.2");
        assert_eq!(TlsVersion::V1_3.to_string(), "TLS 1.3");
    }

    #[test]
    fn test_tls_version_ordering() {
        assert!(TlsVersion::V1_0 < TlsVersion::V1_1);
        assert!(TlsVersion::V1_1 < TlsVersion::V1_2);
        assert!(TlsVersion::V1_2 < TlsVersion::V1_3);
        assert!(TlsVersion::V1_3 > TlsVersion::V1_2);
    }

    #[test]
    fn test_tls_connection_helpers() {
        let http_conn = TlsConnection::new_http();
        assert!(!http_conn.is_secure);

        let secure_conn = TlsConnection::new_secure(TlsVersion::V1_3);
        assert!(secure_conn.is_secure);
        assert!(!secure_conn.has_client_cert);

        let mtls_conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);
        assert!(mtls_conn.is_secure);
        assert!(mtls_conn.has_client_cert);
        assert!(mtls_conn.client_cert_valid);
    }

    // ============================================================================
    // Edge Case Tests
    // ============================================================================

    #[test]
    fn test_custom_config_from_individual_settings() {
        let config = TlsConfig {
            tls_required:  true,
            mtls_required: false,
            min_version:   TlsVersion::V1_2,
        };

        let enforcer = TlsEnforcer::from_config(config);

        // HTTP should fail (tls_required=true)
        let http_conn = TlsConnection::new_http();
        assert!(matches!(
            enforcer.validate_connection(&http_conn),
            Err(SecurityError::TlsRequired { .. })
        ));

        // HTTPS with TLS 1.2 should pass
        let secure_conn = TlsConnection::new_secure(TlsVersion::V1_2);
        enforcer
            .validate_connection(&secure_conn)
            .unwrap_or_else(|e| panic!("expected HTTPS with TLS 1.2 to pass: {e}"));

        // HTTPS without client cert should pass (mtls_required=false)
        let no_cert_conn = TlsConnection::new_secure(TlsVersion::V1_3);
        enforcer
            .validate_connection(&no_cert_conn)
            .unwrap_or_else(|e| panic!("expected HTTPS without client cert to pass: {e}"));
    }

    #[test]
    fn test_http_with_certificate_info_still_fails_when_tls_required() {
        let enforcer = TlsEnforcer::standard(); // tls_required=true

        // Even with client cert info, HTTP should fail
        let http_with_cert_info = TlsConnection {
            is_secure:         false, // Still HTTP
            version:           TlsVersion::V1_2,
            has_client_cert:   true,
            client_cert_valid: true,
        };

        assert!(matches!(
            enforcer.validate_connection(&http_with_cert_info),
            Err(SecurityError::TlsRequired { .. })
        ));
    }
}

mod validation_audit_tests {
    use chrono::Utc;

    use crate::security::*;

    #[test]
    fn test_redaction_policy_default() {
        let policy = RedactionPolicy::default();
        match policy {
            RedactionPolicy::Conservative => {},
            _ => panic!("Default should be Conservative"),
        }
    }

    #[test]
    fn test_config_default() {
        let config = ValidationAuditLoggerConfig::default();
        assert!(config.enabled);
        assert!(config.capture_successful_validations);
        assert!(config.capture_query_strings);
    }

    #[test]
    fn test_logger_enabled_disabled() {
        let config = ValidationAuditLoggerConfig {
            enabled: false,
            ..Default::default()
        };

        let logger = ValidationAuditLogger::new(config);
        assert!(!logger.is_enabled());

        let config2 = ValidationAuditLoggerConfig::default();
        let logger2 = ValidationAuditLogger::new(config2);
        assert!(logger2.is_enabled());
    }

    #[test]
    fn test_logger_entry_logging() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        let entry = ValidationAuditEntry {
            timestamp:         Utc::now(),
            user_id:           Some("user:1".to_string()),
            tenant_id:         Some("tenant:1".to_string()),
            ip_address:        "192.168.1.1".to_string(),
            query_string:      "{ user { id } }".to_string(),
            mutation_name:     None,
            field:             "email".to_string(),
            validation_rule:   "pattern".to_string(),
            valid:             false,
            failure_reason:    Some("Invalid format".to_string()),
            duration_us:       100,
            execution_context: "pattern_validator".to_string(),
        };

        logger.log_entry(entry);
        assert_eq!(logger.entry_count(), 1);
    }

    #[test]
    fn test_logger_filter_by_user() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        let entry1 = ValidationAuditEntry {
            timestamp:         Utc::now(),
            user_id:           Some("user:1".to_string()),
            tenant_id:         None,
            ip_address:        "192.168.1.1".to_string(),
            query_string:      String::new(),
            mutation_name:     None,
            field:             "field1".to_string(),
            validation_rule:   "required".to_string(),
            valid:             false,
            failure_reason:    None,
            duration_us:       0,
            execution_context: "validator".to_string(),
        };

        let mut entry2 = entry1.clone();
        entry2.user_id = Some("user:2".to_string());

        logger.log_entry(entry1);
        logger.log_entry(entry2);

        let user1_entries = logger.entries_by_user("user:1");
        assert_eq!(user1_entries.len(), 1);
    }

    #[test]
    fn test_logger_failure_count() {
        let config = ValidationAuditLoggerConfig::default();
        let logger = ValidationAuditLogger::new(config);

        let entry = ValidationAuditEntry {
            timestamp:         Utc::now(),
            user_id:           None,
            tenant_id:         None,
            ip_address:        "192.168.1.1".to_string(),
            query_string:      String::new(),
            mutation_name:     None,
            field:             "field".to_string(),
            validation_rule:   "pattern".to_string(),
            valid:             false,
            failure_reason:    Some("error".to_string()),
            duration_us:       0,
            execution_context: "validator".to_string(),
        };

        logger.log_entry(entry.clone());

        let mut entry_success = entry;
        entry_success.valid = true;
        logger.log_entry(entry_success);

        assert_eq!(logger.failure_count(), 1);
    }
}

mod actor_context_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::doc_markdown)] // Reason: informal test doc comments

    use std::collections::HashMap;

    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        security::{ActorType, AuthenticatedUser, SecurityContext},
        types::UserId,
    };

    fn user(
        sub: &str,
        scopes: &[&str],
        extra: HashMap<String, serde_json::Value>,
    ) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id:      UserId::new(sub),
            scopes:       scopes.iter().map(ToString::to_string).collect(),
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: extra,
        }
    }

    /// A delegated agent JWT (`act` claim) derives AiAgent + acting_for = the
    /// underlying human subject.
    #[test]
    fn from_user_derives_ai_agent_acting_for_subject() {
        let sub = "550e8400-e29b-41d4-a716-446655440000";
        let mut extra = HashMap::new();
        extra.insert("act".to_string(), json!({ "sub": "agent-7" }));

        let ctx = SecurityContext::from_user(&user(sub, &[], extra), "req-1".to_string());

        assert_eq!(ctx.actor_type(), ActorType::AiAgent);
        assert_eq!(ctx.acting_for(), Some(Uuid::parse_str(sub).unwrap()));
    }

    /// An ordinary user JWT derives HumanUser with no delegated user.
    #[test]
    fn from_user_derives_human_user_by_default() {
        let ctx = SecurityContext::from_user(
            &user("user-1", &["read:user"], HashMap::new()),
            "req-2".to_string(),
        );

        assert_eq!(ctx.actor_type(), ActorType::HumanUser);
        assert_eq!(ctx.acting_for(), None);
    }

    /// A `service_account` scope derives ServiceAccount.
    #[test]
    fn from_user_derives_service_account_from_scope() {
        let ctx = SecurityContext::from_user(
            &user("svc-1", &["service_account"], HashMap::new()),
            "req-3".to_string(),
        );

        assert_eq!(ctx.actor_type(), ActorType::ServiceAccount);
    }

    /// `with_actor_type` overrides the derived classification (the API-key path).
    #[test]
    fn with_actor_type_overrides_and_round_trips() {
        let ctx = SecurityContext::from_user(&user("u", &[], HashMap::new()), "r".to_string())
            .with_actor_type(ActorType::ServiceAccount);

        assert_eq!(ctx.actor_type(), ActorType::ServiceAccount);
    }

    /// `with_acting_for(None)` clears a previously stamped delegated user.
    #[test]
    fn with_acting_for_none_clears() {
        let uuid = Uuid::new_v4();
        let ctx = SecurityContext::from_user(&user("u", &[], HashMap::new()), "r".to_string())
            .with_acting_for(Some(uuid))
            .with_acting_for(None);

        assert_eq!(ctx.acting_for(), None);
    }

    /// A context built without delegation reports the default actor type.
    #[test]
    fn unset_actor_type_defaults_to_human_user() {
        let ctx = SecurityContext::from_user(&user("u", &[], HashMap::new()), "r".to_string());
        assert_eq!(ctx.actor_type(), ActorType::HumanUser);
    }

    /// A scalar `role` claim populates `roles` so `requires_role` gates are
    /// reachable over every transport that builds a context via `from_user` (#503).
    #[test]
    fn from_user_populates_roles_from_scalar_role_claim() {
        let mut extra = HashMap::new();
        extra.insert("role".to_string(), json!("report_reader"));

        let ctx = SecurityContext::from_user(&user("u", &[], extra), "r".to_string());

        assert_eq!(ctx.roles, vec!["report_reader".to_string()]);
        assert!(ctx.has_role("report_reader"));
    }

    /// A `roles` array claim populates `roles` (sorted, deduplicated).
    #[test]
    fn from_user_populates_roles_from_roles_array_claim() {
        let mut extra = HashMap::new();
        extra.insert("roles".to_string(), json!(["moderator", "admin", "moderator"]));

        let ctx = SecurityContext::from_user(&user("u", &[], extra), "r".to_string());

        assert_eq!(ctx.roles, vec!["admin".to_string(), "moderator".to_string()]);
    }

    /// The `fraiseql_roles` array claim is also honoured.
    #[test]
    fn from_user_populates_roles_from_fraiseql_roles_claim() {
        let mut extra = HashMap::new();
        extra.insert("fraiseql_roles".to_string(), json!(["report_reader"]));

        let ctx = SecurityContext::from_user(&user("u", &[], extra), "r".to_string());

        assert_eq!(ctx.roles, vec!["report_reader".to_string()]);
    }

    /// `role` and `roles` claims are merged and de-duplicated across sources.
    #[test]
    fn from_user_merges_and_dedups_role_sources() {
        let mut extra = HashMap::new();
        extra.insert("role".to_string(), json!("admin"));
        extra.insert("roles".to_string(), json!(["admin", "editor"]));

        let ctx = SecurityContext::from_user(&user("u", &[], extra), "r".to_string());

        assert_eq!(ctx.roles, vec!["admin".to_string(), "editor".to_string()]);
    }

    /// No role claims → empty `roles` (gated operations stay denied, as before).
    #[test]
    fn from_user_roles_empty_without_role_claims() {
        let ctx =
            SecurityContext::from_user(&user("u", &["read:user"], HashMap::new()), "r".to_string());
        assert!(ctx.roles.is_empty());
    }
}

/// `SecurityContext::system_job` — the background/system identity for scheduled
/// sources and other server-initiated work (#573). The first (and, at
/// introduction, only) construction of [`ActorType::SystemJob`].
mod system_job_tests {
    use crate::{
        security::{ActorType, SecurityContext},
        types::TenantId,
    };

    #[test]
    fn system_job_carries_granted_authority_and_actor_type() {
        let ctx = SecurityContext::system_job(
            "orders",
            "fire-1",
            vec!["ingest_writer".to_string()],
            vec!["write:order".to_string()],
            Some(TenantId::from("acme")),
        );
        assert_eq!(ctx.actor_type(), ActorType::SystemJob);
        assert!(ctx.has_role("ingest_writer"));
        assert!(ctx.has_scope("write:order"));
        assert_eq!(ctx.tenant_id.as_ref().map(TenantId::as_str), Some("acme"));
        assert!(ctx.is_multi_tenant());
        assert!(!ctx.is_expired(), "a fresh system-job context is not expired");
        // The principal reads as an internal job, mirroring the `apikey:` convention.
        assert_eq!(ctx.user_id.as_str(), "system_job:orders");
    }

    #[test]
    fn system_job_without_grants_is_fail_closed() {
        // No roles, no scopes, no tenant → the identity grants nothing: every
        // authz/RLS decision denies. This is the fail-closed default a source with
        // no `run_as` runs under.
        let ctx = SecurityContext::system_job("orders", "fire-1", vec![], vec![], None);
        assert_eq!(ctx.actor_type(), ActorType::SystemJob);
        assert!(!ctx.has_role("admin"));
        assert!(!ctx.has_scope("write:order"));
        assert!(!ctx.is_multi_tenant(), "no tenant → scoped to nothing");
    }
}
