#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
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
    assert!(!config.compression_enabled);
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
    assert_eq!(config.pool_max_size, 20);
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
    config.validate().unwrap_or_else(|e| panic!("expected Ok: {e}"));
}

#[test]
fn test_validate_metrics_enabled_without_token_fails() {
    let config = ServerConfig {
        metrics_enabled: true,
        metrics_token: None,
        ..ServerConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "expected Err, got: {result:?}");
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
    assert!(result.is_err(), "expected Err, got: {result:?}");
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
    config.validate().unwrap_or_else(|e| panic!("expected Ok: {e}"));
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
    assert!(result.is_err(), "expected Err, got: {result:?}");
    assert!(result.unwrap_err().contains("certificate file not found"));
}

#[test]
fn test_validate_tls_invalid_min_version() {
    // Create temp cert and key files that exist
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let cert_path = dir.path().join("test_cert.pem");
    let key_path = dir.path().join("test_key.pem");
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
    assert!(result.is_err(), "expected Err, got: {result:?}");
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
    assert!(result.is_err(), "expected Err, got: {result:?}");
    assert!(result.unwrap_err().contains("Invalid postgres_ssl_mode"));
}

#[test]
fn test_validate_tls_requires_client_ca() {
    // Create temp cert and key files that exist
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let cert_path = dir.path().join("test_cert.pem");
    let key_path = dir.path().join("test_key.pem");
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
    assert!(result.is_err(), "expected Err, got: {result:?}");
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
    assert!(result.is_err(), "expected Err, got: {result:?}");
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
    assert!(result.is_err(), "expected Err, got: {result:?}");
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
    config.validate().unwrap_or_else(|e| panic!("expected Ok: {e}"));
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
    config.validate().unwrap_or_else(|e| panic!("expected Ok: {e}"));
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
    config.validate().unwrap_or_else(|e| panic!("expected Ok: {e}"));
}

#[test]
fn test_validation_config_from_toml() {
    let toml_str = r"
        [validation]
        max_query_depth = 15
        max_query_complexity = 200
    ";
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let vc = config.validation.expect("validation section should be parsed");
    assert_eq!(vc.max_query_depth, Some(15));
    assert_eq!(vc.max_query_complexity, Some(200));
}

#[test]
fn test_validation_config_defaults_to_none() {
    let config = ServerConfig::default();
    assert!(config.validation.is_none(), "validation should default to None");
}

#[test]
fn test_validation_config_partial_override() {
    let toml_str = r"
        [validation]
        max_query_complexity = 500
    ";
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let vc = config.validation.expect("validation section should be parsed");
    assert_eq!(vc.max_query_depth, None, "unset depth should be None");
    assert_eq!(vc.max_query_complexity, Some(500));
}

#[test]
fn test_storage_and_files_default_to_empty() {
    let config = ServerConfig::default();
    assert!(config.storage.is_empty(), "storage should default to empty");
    assert!(config.files.is_empty(), "files should default to empty");
}

#[test]
fn test_storage_section_full_parses_from_toml() {
    let toml_str = r#"
        [storage.docs]
        backend = "s3"
        bucket = "fraiseql-docs"
        region = "us-east-1"
        endpoint = "http://minio:9000"
        access = "public_read"
        max_object_bytes = 10485760
        allowed_mime_types = ["image/png", "image/jpeg"]
        serve_inline = true
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let section = config.storage.get("docs").expect("storage.docs should be parsed");
    assert_eq!(section.backend, "s3");
    assert_eq!(section.bucket.as_deref(), Some("fraiseql-docs"));
    assert_eq!(section.region.as_deref(), Some("us-east-1"));
    assert_eq!(section.endpoint.as_deref(), Some("http://minio:9000"));
    assert_eq!(section.access.as_deref(), Some("public_read"));
    assert_eq!(section.max_object_bytes, Some(10_485_760));
    assert_eq!(
        section.allowed_mime_types.as_deref(),
        Some(["image/png".to_string(), "image/jpeg".to_string()].as_slice()),
    );
    assert_eq!(section.serve_inline, Some(true));
}

#[test]
fn test_storage_section_minimal_local_defaults_policy_to_none() {
    let toml_str = r#"
        [storage.uploads]
        backend = "local"
        path = "/var/lib/fraiseql/uploads"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let section = config.storage.get("uploads").expect("storage.uploads should be parsed");
    assert_eq!(section.backend, "local");
    assert_eq!(section.path.as_deref(), Some("/var/lib/fraiseql/uploads"));
    assert!(section.access.is_none(), "access policy should be unset");
    assert!(section.max_object_bytes.is_none());
    assert!(section.allowed_mime_types.is_none());
    assert!(section.serve_inline.is_none());
}

#[test]
fn test_files_section_is_parsed_for_warning() {
    let toml_str = r#"
        [files.avatars]
        storage = "uploads"
        max_size = "5MB"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let section = config.files.get("avatars").expect("files.avatars should be parsed");
    assert_eq!(section.storage.as_deref(), Some("uploads"));
    assert_eq!(section.max_size.as_deref(), Some("5MB"));
}

#[test]
fn resolve_storage_section_returns_none_when_unconfigured() {
    let config = ServerConfig::default();
    let resolved = resolve_storage_section(&config).expect("resolution should not error");
    assert!(resolved.is_none(), "no [storage] section should resolve to None");
}

#[test]
fn resolve_storage_section_maps_local_with_secure_defaults() {
    use fraiseql_storage::config::BucketAccess;

    let toml_str = r#"
        [storage.uploads]
        backend = "local"
        path = "/var/lib/fraiseql/uploads"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let resolved = resolve_storage_section(&config)
        .expect("resolution should not error")
        .expect("one section should resolve to Some");

    assert_eq!(resolved.backend.backend, "local");
    assert_eq!(resolved.backend.path.as_deref(), Some("/var/lib/fraiseql/uploads"));
    assert_eq!(resolved.bucket.name, "uploads", "bucket name is the section key");
    assert!(
        matches!(resolved.bucket.access, BucketAccess::Private),
        "access should default to the secure Private policy",
    );
    assert!(!resolved.bucket.serve_inline, "serve_inline should default to false");
    assert!(resolved.bucket.max_object_bytes.is_none());
    assert!(resolved.bucket.allowed_mime_types.is_none());
}

#[test]
fn resolve_storage_section_honors_public_read_and_policy_fields() {
    use fraiseql_storage::config::BucketAccess;

    let toml_str = r#"
        [storage.docs]
        backend = "local"
        path = "/tmp/docs"
        access = "public_read"
        max_object_bytes = 10485760
        allowed_mime_types = ["image/png", "image/*"]
        serve_inline = true
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let resolved = resolve_storage_section(&config).unwrap().unwrap();

    assert!(matches!(resolved.bucket.access, BucketAccess::PublicRead));
    assert_eq!(resolved.bucket.max_object_bytes, Some(10_485_760));
    assert_eq!(
        resolved.bucket.allowed_mime_types.as_deref(),
        Some(["image/png".to_string(), "image/*".to_string()].as_slice()),
    );
    assert!(resolved.bucket.serve_inline);
}

#[test]
fn resolve_storage_section_rejects_unknown_access() {
    let toml_str = r#"
        [storage.docs]
        backend = "local"
        path = "/tmp/docs"
        access = "open-to-the-world"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let err = resolve_storage_section(&config).expect_err("unknown access should error");
    assert!(err.contains("invalid storage access policy"), "got: {err}");
}

#[test]
fn resolve_storage_section_rejects_multiple_sections() {
    let toml_str = r#"
        [storage.docs]
        backend = "local"
        path = "/tmp/docs"

        [storage.media]
        backend = "local"
        path = "/tmp/media"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let err = resolve_storage_section(&config).expect_err("multiple sections should error");
    assert!(err.contains("single storage backend"), "got: {err}");
    // Both section names are reported, sorted.
    assert!(err.contains("docs") && err.contains("media"), "got: {err}");
}

#[test]
fn test_tenancy_runtime_defaults_to_disabled() {
    let config = ServerConfig::default();
    assert!(!config.tenancy.runtime.enabled, "tenancy runtime should default to off");
}

#[test]
fn test_tenancy_runtime_parses_from_toml() {
    let toml_str = r"
        [tenancy.runtime]
        enabled = true
    ";
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    assert!(config.tenancy.runtime.enabled, "[tenancy.runtime] enabled should be parsed");
}

#[test]
fn test_tenancy_absent_section_keeps_runtime_off() {
    let toml_str = r#"
        database_url = "postgres://localhost/db"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.tenancy.runtime.enabled);
}

#[test]
fn test_auth_hs256_defaults_to_none() {
    let config = ServerConfig::default();
    assert!(config.auth_hs256.is_none());
}

#[test]
fn test_auth_hs256_parses_from_toml() {
    let toml_str = r#"
        [auth_hs256]
        secret_env = "MY_TEST_HS256_SECRET"
        issuer = "test-suite"
        audience = "test-api"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let hs = config.auth_hs256.expect("auth_hs256 section should be parsed");
    assert_eq!(hs.secret_env, "MY_TEST_HS256_SECRET");
    assert_eq!(hs.issuer.as_deref(), Some("test-suite"));
    assert_eq!(hs.audience.as_deref(), Some("test-api"));
}

#[test]
fn test_auth_and_auth_hs256_are_mutually_exclusive() {
    use fraiseql_core::security::OidcConfig;

    let env_name = "FRAISEQL_TEST_HS256_MUTEX_EXCLUSIVE";
    temp_env::with_vars([(env_name, Some("secret-value-at-least-a-bit-long"))], || {
        let config = ServerConfig {
            auth: Some(OidcConfig::auth0("tenant.auth0.com", "my-api")),
            auth_hs256: Some(super::Hs256Config {
                secret_env: env_name.to_string(),
                issuer:     Some("test".to_string()),
                audience:   None,
            }),
            ..ServerConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("mutually exclusive") || err.contains("Pick one"),
            "unexpected error: {err}"
        );
    });
}

#[test]
fn test_auth_hs256_fails_when_secret_env_unset() {
    let env_name = "FRAISEQL_TEST_HS256_UNSET_XYZ";
    temp_env::with_vars([(env_name, None::<&str>)], || {
        let config = ServerConfig {
            auth_hs256: Some(super::Hs256Config {
                secret_env: env_name.to_string(),
                issuer:     None,
                audience:   None,
            }),
            ..ServerConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.contains("not set"), "expected 'not set' error, got: {err}");
    });
}

// ── observers_tests ───────────────────────────────────────────────────────────

#[cfg(feature = "observers")]
mod observers_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::super::observers::*;

    #[test]
    fn observer_pool_config_defaults_are_sensible() {
        let cfg = ObserverPoolConfig::default();
        assert!(cfg.min_connections >= 1, "observer pool needs at least 1 connection");
        assert!(
            cfg.max_connections >= cfg.min_connections,
            "max_connections ({}) must be >= min_connections ({})",
            cfg.max_connections,
            cfg.min_connections,
        );
        assert!(cfg.acquire_timeout_secs > 0, "acquire_timeout_secs should be > 0");
        // Observer pool should be smaller than a typical app pool.
        assert!(
            cfg.max_connections <= 10,
            "observer pool defaults should be small (<=10), got {}",
            cfg.max_connections,
        );
    }

    #[test]
    fn observer_config_with_pool_section_deserializes() {
        // Pool config lives under `[observers.runtime.pool]` since #342.
        let toml = r"
            enabled = true

            [runtime.pool]
            min_connections = 3
            max_connections = 8
            acquire_timeout_secs = 15
        ";
        let cfg: ObserverConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.runtime.pool.min_connections, 3);
        assert_eq!(cfg.runtime.pool.max_connections, 8);
        assert_eq!(cfg.runtime.pool.acquire_timeout_secs, 15);
    }

    #[test]
    fn observer_config_pool_defaults_when_section_absent() {
        let toml = r"enabled = true";
        let cfg: ObserverConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.runtime.pool.min_connections, 2, "default min_connections should be 2");
        assert_eq!(cfg.runtime.pool.max_connections, 5, "default max_connections should be 5");
        assert_eq!(
            cfg.runtime.pool.acquire_timeout_secs, 10,
            "default acquire_timeout_secs should be 10"
        );
    }
}
