#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

use std::path::PathBuf;

use super::*;

#[test]
fn test_default_config() {
    let config = ServerConfig::default();
    assert_eq!(config.schema_path, PathBuf::from("schema.compiled.json"));
    assert_eq!(config.database_url, "postgresql://localhost/fraiseql");
    assert_eq!(config.graphql_path, "/graphql");
    assert_eq!(config.health_path, "/health");
    assert_eq!(config.metrics_path, "/metrics");
    assert_eq!(config.metrics_json_path, "/metrics/json");
    assert!(config.cors_enabled);
    assert!(config.compression_enabled);
}

#[test]
fn test_default_config_metrics_disabled() {
    let config = ServerConfig::default();
    assert!(!config.metrics_enabled, "Metrics should be disabled by default for security");
    assert!(config.metrics_token.is_none());
}

#[test]
fn test_config_with_custom_database_url() {
    let config = ServerConfig {
        database_url: "postgresql://user:pass@db.example.com/mydb".to_string(),
        ..ServerConfig::default()
    };
    assert_eq!(config.database_url, "postgresql://user:pass@db.example.com/mydb");
}

#[test]
fn test_default_pool_config() {
    let config = ServerConfig::default();
    assert_eq!(config.pool_min_size, 5);
    assert_eq!(config.pool_max_size, 25);
    assert_eq!(config.pool_timeout_secs, 30);
}

#[test]
fn test_config_with_custom_pool_size() {
    let config = ServerConfig {
        pool_min_size: 2,
        pool_max_size: 50,
        pool_timeout_secs: 60,
        ..ServerConfig::default()
    };
    assert_eq!(config.pool_min_size, 2);
    assert_eq!(config.pool_max_size, 50);
    assert_eq!(config.pool_timeout_secs, 60);
}

#[test]
fn test_validate_metrics_disabled_ok() {
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_metrics_enabled_without_token_fails() {
    let config = ServerConfig {
        metrics_enabled: true,
        metrics_token: None,
        ..ServerConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("metrics_token is not set"));
}

#[test]
fn test_validate_metrics_enabled_with_short_token_fails() {
    let config = ServerConfig {
        metrics_enabled: true,
        metrics_token: Some("short".to_string()), // < 16 chars
        ..ServerConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("at least 16 characters"));
}

#[test]
fn test_validate_metrics_enabled_with_valid_token_ok() {
    let config = ServerConfig {
        metrics_enabled: true,
        metrics_token: Some("a-secure-token-that-is-long-enough".to_string()),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_default_subscription_config() {
    let config = ServerConfig::default();
    assert_eq!(config.subscription_path, "/ws");
    assert!(config.subscriptions_enabled);
}

#[test]
fn test_subscription_config_with_custom_path() {
    let config = ServerConfig {
        subscription_path: "/subscriptions".to_string(),
        ..ServerConfig::default()
    };
    assert_eq!(config.subscription_path, "/subscriptions");
    assert!(config.subscriptions_enabled);
}

#[test]
fn test_subscriptions_can_be_disabled() {
    let config = ServerConfig {
        subscriptions_enabled: false,
        ..ServerConfig::default()
    };
    assert!(!config.subscriptions_enabled);
    assert_eq!(config.subscription_path, "/ws");
}

#[test]
fn test_subscription_path_serialization() {
    let config = ServerConfig::default();
    let json = serde_json::to_string(&config).expect(
        "ServerConfig derives Serialize with serializable fields; serialization is infallible",
    );
    let restored: ServerConfig = serde_json::from_str(&json)
        .expect("ServerConfig roundtrip: deserialization of just-serialized data is infallible");

    assert_eq!(restored.subscription_path, config.subscription_path);
    assert_eq!(restored.subscriptions_enabled, config.subscriptions_enabled);
}

#[test]
fn test_subscription_config_with_partial_toml() {
    let toml_str = r#"
        subscription_path = "/graphql-ws"
        subscriptions_enabled = false
    "#;

    let decoded: ServerConfig = toml::from_str(toml_str).expect(
        "TOML config parsing: valid TOML syntax with expected fields deserializes correctly",
    );
    assert_eq!(decoded.subscription_path, "/graphql-ws");
    assert!(!decoded.subscriptions_enabled);
}

#[test]
fn test_tls_config_defaults() {
    let config = ServerConfig::default();
    assert!(config.tls.is_none());
    assert!(config.database_tls.is_none());
}

#[test]
fn test_database_tls_config_defaults() {
    let db_tls = DatabaseTlsConfig {
        postgres_ssl_mode:   "prefer".to_string(),
        redis_ssl:           false,
        clickhouse_https:    false,
        elasticsearch_https: false,
        verify_certificates: true,
        ca_bundle_path:      None,
    };

    assert_eq!(db_tls.postgres_ssl_mode, "prefer");
    assert!(!db_tls.redis_ssl);
    assert!(!db_tls.clickhouse_https);
    assert!(!db_tls.elasticsearch_https);
    assert!(db_tls.verify_certificates);
}

#[test]
fn test_tls_server_config_fields() {
    let tls = TlsServerConfig {
        enabled:             true,
        cert_path:           PathBuf::from("/etc/fraiseql/cert.pem"),
        key_path:            PathBuf::from("/etc/fraiseql/key.pem"),
        require_client_cert: false,
        client_ca_path:      None,
        min_version:         "1.3".to_string(),
    };

    assert!(tls.enabled);
    assert_eq!(tls.cert_path, PathBuf::from("/etc/fraiseql/cert.pem"));
    assert_eq!(tls.key_path, PathBuf::from("/etc/fraiseql/key.pem"));
    assert!(!tls.require_client_cert);
    assert_eq!(tls.min_version, "1.3");
}

#[test]
fn test_validate_tls_enabled_without_cert() {
    let config = ServerConfig {
        tls: Some(TlsServerConfig {
            enabled:             true,
            cert_path:           PathBuf::from("/nonexistent/cert.pem"),
            key_path:            PathBuf::from("/etc/fraiseql/key.pem"),
            require_client_cert: false,
            client_ca_path:      None,
            min_version:         "1.2".to_string(),
        }),
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("certificate file not found"));
}

#[test]
fn test_validate_tls_invalid_min_version() {
    // Create temp cert and key files that exist
    let cert_path = PathBuf::from("/tmp/test_cert.pem");
    let key_path = PathBuf::from("/tmp/test_key.pem");
    std::fs::write(&cert_path, "test").ok();
    std::fs::write(&key_path, "test").ok();

    let config = ServerConfig {
        tls: Some(TlsServerConfig {
            enabled: true,
            cert_path,
            key_path,
            require_client_cert: false,
            client_ca_path: None,
            min_version: "1.1".to_string(),
        }),
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("min_version must be"));
}

#[test]
fn test_validate_database_tls_invalid_postgres_ssl_mode() {
    let config = ServerConfig {
        database_tls: Some(DatabaseTlsConfig {
            postgres_ssl_mode:   "invalid_mode".to_string(),
            redis_ssl:           false,
            clickhouse_https:    false,
            elasticsearch_https: false,
            verify_certificates: true,
            ca_bundle_path:      None,
        }),
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid postgres_ssl_mode"));
}

#[test]
fn test_validate_tls_requires_client_ca() {
    // Create temp cert and key files that exist
    let cert_path = PathBuf::from("/tmp/test_cert2.pem");
    let key_path = PathBuf::from("/tmp/test_key2.pem");
    std::fs::write(&cert_path, "test").ok();
    std::fs::write(&key_path, "test").ok();

    let config = ServerConfig {
        tls: Some(TlsServerConfig {
            enabled: true,
            cert_path,
            key_path,
            require_client_cert: true,
            client_ca_path: None,
            min_version: "1.3".to_string(),
        }),
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("client_ca_path is not set"));
}

#[test]
fn test_database_tls_serialization() {
    let db_tls = DatabaseTlsConfig {
        postgres_ssl_mode:   "require".to_string(),
        redis_ssl:           true,
        clickhouse_https:    true,
        elasticsearch_https: true,
        verify_certificates: true,
        ca_bundle_path:      Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")),
    };

    let json = serde_json::to_string(&db_tls).expect(
        "DatabaseTlsConfig derives Serialize with serializable fields; serialization is infallible",
    );
    let restored: DatabaseTlsConfig = serde_json::from_str(&json).expect(
        "DatabaseTlsConfig roundtrip: deserialization of just-serialized data is infallible",
    );

    assert_eq!(restored.postgres_ssl_mode, db_tls.postgres_ssl_mode);
    assert_eq!(restored.redis_ssl, db_tls.redis_ssl);
    assert_eq!(restored.clickhouse_https, db_tls.clickhouse_https);
    assert_eq!(restored.elasticsearch_https, db_tls.elasticsearch_https);
    assert_eq!(restored.ca_bundle_path, db_tls.ca_bundle_path);
}

#[test]
fn test_admin_api_disabled_by_default() {
    let config = ServerConfig::default();
    assert!(
        !config.admin_api_enabled,
        "Admin API should be disabled by default for security"
    );
    assert!(config.admin_token.is_none());
}

#[test]
fn test_validate_admin_api_enabled_without_token_fails() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: None,
        ..ServerConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("admin_token is not set"));
}

#[test]
fn test_validate_admin_api_enabled_with_short_token_fails() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("short".to_string()), // < 32 chars
        ..ServerConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("at least 32 characters"));
}

#[test]
fn test_validate_admin_api_enabled_with_valid_token_ok() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("a-very-secure-admin-token-that-is-long-enough".to_string()),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    assert!(config.validate().is_ok());
}

// --- admin_readonly_token validation tests (S10-1) ---

#[test]
fn test_validate_admin_readonly_token_short_fails() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("a-very-secure-admin-token-that-is-long-enough".to_string()),
        admin_readonly_token: Some("short".to_string()),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let err = config.validate().unwrap_err();
    assert!(
        err.contains("admin_readonly_token must be at least 32"),
        "expected length error, got: {err}"
    );
}

#[test]
fn test_validate_admin_readonly_token_same_as_admin_token_fails() {
    let token = "a-very-secure-admin-token-that-is-long-enough".to_string();
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some(token.clone()),
        admin_readonly_token: Some(token),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let err = config.validate().unwrap_err();
    assert!(
        err.contains("must differ from admin_token"),
        "expected differ error, got: {err}"
    );
}

#[test]
fn test_validate_admin_readonly_token_valid_passes() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("admin-write-token-that-is-long-enough-1234".to_string()),
        admin_readonly_token: Some("admin-readonly-token-that-is-long-enough-5678".to_string()),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_admin_readonly_token_without_admin_enabled_is_ignored() {
    // admin_readonly_token with admin_api_enabled=false — validation skipped entirely.
    let config = ServerConfig {
        admin_api_enabled: false,
        admin_token: None,
        admin_readonly_token: Some("short".to_string()), // would fail if admin_api_enabled=true
        cors_enabled: false,
        ..ServerConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_toml_rejects_unknown_top_level_keys() {
    let toml_str = r#"
[server]
port = 4001
bind = "0.0.0.0"
"#;
    let result: Result<ServerConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err(), "Nested [server] section should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown field"), "Error should mention unknown field: {err}");
}

#[test]
fn test_toml_rejects_nested_database_section() {
    let toml_str = r#"
[database]
url = "postgresql://localhost/mydb"
"#;
    let result: Result<ServerConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err(), "Nested [database] section should be rejected");
}

#[test]
fn test_toml_accepts_valid_flat_config() {
    let toml_str = r#"
database_url = "postgresql://localhost/mydb"
bind_addr = "127.0.0.1:9000"
"#;
    let config: ServerConfig = toml::from_str(toml_str).expect("Valid flat config should parse");
    assert_eq!(config.database_url, "postgresql://localhost/mydb");
    assert_eq!(config.bind_addr.port(), 9000);
}
