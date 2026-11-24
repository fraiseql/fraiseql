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

// #839: overview.md shipped a production config whose keys sat in [server]/[database]
// grouping tables that ServerConfig does not have. serde silently discarded both tables,
// so the server booted on 127.0.0.1:8000 with a default database URL while the operator
// believed they had configured 0.0.0.0:4000 and pool sizing. An unknown top-level key
// must refuse to parse, naming the key.
#[test]
fn unknown_top_level_table_is_rejected_not_discarded() {
    let toml_str = r#"
        [server]
        bind_addr = "0.0.0.0:4000"
        schema_path = "schema.compiled.json"

        [database]
        url_env = "DATABASE_URL"
        pool_min_size = 5
        pool_max_size = 20
    "#;

    let err = toml::from_str::<ServerConfig>(toml_str)
        .expect_err("a [server] grouping table is not a ServerConfig key and must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("server") || msg.contains("unknown field"),
        "the error must name the unknown key so the operator can fix the file; got: {msg}"
    );
}

// #909: `[cache] response_cache_enabled = true` used to be accepted — by
// `fraiseql_core::config::FraiseQLConfig`, a parallel config tree nothing outside its
// own module read. That type is gone, so the section is now refused; this asserts the
// refusal is *usable*, naming the keys that do the job instead of leaving the operator
// to find `cache_enabled` in a hundred-name serde field list.
#[test]
fn a_removed_grouping_table_names_the_working_knob() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("server.toml");
    std::fs::write(
        &path,
        "[cache]\nresponse_cache_enabled = true\nresponse_cache_ttl_secs = 300\n",
    )
    .expect("write config");

    let err = ServerConfig::from_file(&path)
        .expect_err("[cache] is not a ServerConfig key and must be refused");
    // Not just `err.contains("cache_enabled")`: serde's deny_unknown_fields message
    // already lists every valid field, so that assertion passes with the note removed.
    // The note itself is what has to be there.
    assert!(
        err.contains("there is no `[cache]` table"),
        "the refusal must say the section does not exist (#909); got: {err}"
    );
    assert!(
        err.contains("Use `cache_enabled` (query result cache), `apq_enabled`"),
        "the refusal must name the keys that do the job (#909); got: {err}"
    );
}

// The whole removed tree, not just [cache] — every section of the deleted
// FraiseQLConfig gets a note. Driven through the helper so it runs under every
// feature set.
#[test]
fn every_removed_section_gets_a_replacement_note() {
    for (section, expected) in [
        ("server", "`bind_addr`"),
        ("database", "`database_url`"),
        ("cors", "`cors_enabled`"),
        ("rate_limit", "`[rate_limiting]`"),
        ("cache", "`cache_enabled`"),
        ("collation", "never wired"),
    ] {
        let content = format!("[{section}]\nx = 1\n");
        let msg = super::methods::enrich_parse_error(
            &[],
            &content,
            format!("Invalid TOML config: unknown field `{section}`"),
        )
        .0;
        assert!(msg.contains(expected), "[{section}] must point at {expected}; got: {msg}");
    }
}

// A section compiled out of a build must be refused with an error that names the
// missing build feature, not a bare serde "unknown field" (the operator's fix is a
// rebuild or a config edit — the message must say which). Tested through the
// parameterized helper so it runs under EVERY feature set — a cfg(not(feature))
// gate would make it a never-run test in the all-features CI leg.
#[test]
fn compiled_out_section_error_names_the_build_feature() {
    let sections: &[(&str, &str, bool)] = &[
        ("observers", "observers", false),
        ("sources", "sources", true),
    ];
    let content = "[observers]\nenabled = true\n[sources]\nenabled = true\n";

    let msg = super::methods::enrich_parse_error(
        sections,
        content,
        "Invalid TOML config: unknown field `observers`".to_string(),
    )
    .0;

    assert!(
        msg.contains("`observers` feature"),
        "a compiled-out section must get a build-feature hint; got: {msg}"
    );
    assert!(
        !msg.contains("`sources` feature"),
        "a compiled-in section must NOT get a hint; got: {msg}"
    );
}

// End-to-end twin of the helper test, exercising the real cfg! table through
// from_file. Only compiled when `observers` is off (default local build); the
// helper test above is the gate of record in the all-features CI leg.
#[cfg(not(feature = "observers"))]
#[test]
fn from_file_names_the_build_feature_for_a_compiled_out_section() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("server.toml");
    std::fs::write(&path, "[observers]\nenabled = true\n").expect("write config");

    let err = ServerConfig::from_file(&path)
        .expect_err("[observers] without the observers feature must refuse to parse");
    assert!(
        err.contains("`observers` feature"),
        "the error must name the missing build feature; got: {err}"
    );
}

/// The pre-#342 flat `[observers]` tuning layout is refused by the loader itself, so every
/// reader of a config file refuses it: the binary, `fraiseql run` and an embedder.
#[cfg(feature = "observers")]
#[test]
fn from_file_refuses_observer_tuning_outside_observers_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("server.toml");
    std::fs::write(&path, "[observers]\npoll_interval_ms = 200\n").expect("write config");

    let err = ServerConfig::from_file(&path)
        .expect_err("a flat [observers] tuning key must be refused, not ignored");
    assert!(
        err.contains("poll_interval_ms") && err.contains("[observers.runtime]"),
        "the error must name the key and where it moved; got: {err}"
    );
}

#[test]
fn unknown_scalar_key_is_rejected_not_discarded() {
    let err = toml::from_str::<ServerConfig>("bind_adr = \"0.0.0.0:4000\"\n")
        .expect_err("a typoed key must be refused, not silently dropped");
    assert!(
        err.to_string().contains("bind_adr") || err.to_string().contains("unknown field"),
        "got: {err}"
    );
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
        postgres_ssl_mode:   Some("prefer".to_string()),
        verify_certificates: true,
        ca_bundle_path:      None,
        redis_ssl:           None,
        clickhouse_https:    None,
        elasticsearch_https: None,
    };

    assert_eq!(db_tls.postgres_ssl_mode.as_deref(), Some("prefer"));
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
            postgres_ssl_mode:   Some("invalid_mode".to_string()),
            verify_certificates: true,
            ca_bundle_path:      None,
            redis_ssl:           None,
            clickhouse_https:    None,
            elasticsearch_https: None,
        }),
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(result.is_err(), "expected Err, got: {result:?}");
    assert!(result.unwrap_err().contains("unknown postgres ssl mode"));
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
        postgres_ssl_mode:   Some("require".to_string()),
        verify_certificates: true,
        ca_bundle_path:      Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")),
        redis_ssl:           None,
        clickhouse_https:    None,
        elasticsearch_https: None,
    };

    let json = serde_json::to_string(&db_tls).expect(
        "DatabaseTlsConfig derives Serialize with serializable fields; serialization is infallible",
    );
    let restored: DatabaseTlsConfig = serde_json::from_str(&json).expect(
        "DatabaseTlsConfig roundtrip: deserialization of just-serialized data is infallible",
    );

    assert_eq!(restored.postgres_ssl_mode, db_tls.postgres_ssl_mode);
    assert_eq!(restored.ca_bundle_path, db_tls.ca_bundle_path);
    assert_eq!(restored.verify_certificates, db_tls.verify_certificates);
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

/// `admin_token` gates the RBAC, API-key, SAML-IdP, SCIM-token, identity and suppression
/// routers whenever it is set, `admin_api_enabled` or not. Its strength rule must hold
/// whenever it does.
#[test]
fn a_short_admin_token_is_refused_even_with_the_admin_api_off() {
    let config = ServerConfig {
        admin_api_enabled: false,
        admin_token: Some("p16-scim-admin-token".to_string()), // 20 chars
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let err = config.validate().expect_err("a short admin_token must be refused");
    assert!(err.contains("admin_token must be at least 32 characters"), "got: {err}");
}

#[test]
fn a_weak_readonly_token_is_refused_even_with_the_admin_api_off() {
    let strong = "an-admin-token-that-is-long-enough-123".to_string();
    let short = ServerConfig {
        admin_api_enabled: false,
        admin_token: Some(strong.clone()),
        admin_readonly_token: Some("short".to_string()),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let err = short.validate().expect_err("a short admin_readonly_token must be refused");
    assert!(err.contains("admin_readonly_token must be at least 32"), "got: {err}");

    let same = ServerConfig {
        admin_api_enabled: false,
        admin_token: Some(strong.clone()),
        admin_readonly_token: Some(strong),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let err = same
        .validate()
        .expect_err("a read-only token equal to the admin token is refused");
    assert!(err.contains("must differ from admin_token"), "got: {err}");
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

/// #369/#370: the two per-bucket keys the resumable-upload and render
/// surfaces read must actually reach `BucketConfig` — a key the runtime never
/// sees is the P06 defect.
#[test]
#[cfg(feature = "storage-transforms")]
fn resolve_storage_section_maps_upload_ttl_and_transform_presets() {
    let toml_str = r#"
        [storage.media]
        backend = "local"
        path = "/tmp/media"
        upload_ttl_secs = 3600
        default_resize_mode = "fill"
        transform_presets = [{ name = "thumb", width = 200, format = "jpeg", quality = 80, resize_mode = "fit", gravity = "smart" }]
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let resolved = resolve_storage_section(&config).unwrap().unwrap();

    assert_eq!(resolved.bucket.upload_ttl_secs, Some(3600));
    assert_eq!(resolved.bucket.default_resize_mode.as_deref(), Some("fill"));
    let presets = resolved.bucket.transform_presets.expect("presets reach the bucket config");
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].name, "thumb");
    assert_eq!(presets[0].width, Some(200));
    assert_eq!(presets[0].format.as_deref(), Some("jpeg"));
    assert_eq!(presets[0].quality, Some(80));
    // #973: the two new preset keys reach the bucket too.
    assert_eq!(presets[0].resize_mode.as_deref(), Some("fit"));
    assert_eq!(presets[0].gravity.as_deref(), Some("smart"));
}

/// #973: every render spelling is validated at BOOT. A misspelt mode or
/// gravity would otherwise render something the operator did not ask for on
/// every request, and a quality paired with a losslessly-encoded format is a
/// parameter that can never take effect.
#[test]
#[cfg(feature = "storage-transforms")]
fn misconfigured_render_keys_refuse_to_boot() {
    let cases = [
        (
            r#"transform_presets = [{ name = "t", width = 8, resize_mode = "fil" }]"#,
            "resize_mode",
        ),
        (
            r#"transform_presets = [{ name = "t", width = 8, gravity = "northwest" }]"#,
            "gravity",
        ),
        (
            r#"transform_presets = [{ name = "t", width = 8, format = "webp", quality = 80 }]"#,
            "losslessly",
        ),
        (
            r#"transform_presets = [{ name = "t", width = 8, format = "png", quality = 80 }]"#,
            "losslessly",
        ),
        (r#"default_resize_mode = "cover""#, "default_resize_mode"),
        (r#"watermark_font = "/nonexistent/font.ttf""#, "watermark_font"),
    ];
    for (line, expected) in cases {
        let toml_str =
            format!("[storage.media]\nbackend = \"local\"\npath = \"/tmp/media\"\n{line}\n");
        let config: ServerConfig = toml::from_str(&toml_str).unwrap();
        let err =
            resolve_storage_section(&config).expect_err(&format!("{line} must be a startup error"));
        assert!(err.contains(expected), "{line}: refusal must name {expected}, got {err}");
    }
}

/// #973: a bucket name becomes the first segment of every object key, so a
/// bucket named after one of FraiseQL's own namespaces would put caller objects
/// inside the upload staging area or the render cache.
#[test]
fn a_reserved_bucket_name_refuses_to_boot() {
    for reserved in fraiseql_storage::config::RESERVED_BUCKET_NAMES {
        let toml_str =
            format!("[storage.\"{reserved}\"]\nbackend = \"local\"\npath = \"/tmp/media\"\n");
        let config: ServerConfig = toml::from_str(&toml_str).unwrap();
        let err = resolve_storage_section(&config)
            .expect_err("a reserved bucket name must be a startup error");
        assert!(err.contains("reserves"), "{reserved}: {err}");
    }
}

/// #973's render keys are the #370 class: accepted by the parser, servable
/// only by a binary carrying the render endpoint.
#[test]
#[cfg(not(feature = "storage-transforms"))]
fn render_keys_without_the_feature_refuse_to_boot() {
    for line in [
        "default_resize_mode = \"fill\"",
        "watermark_font = \"/tmp/f.ttf\"",
    ] {
        let toml_str =
            format!("[storage.media]\nbackend = \"local\"\npath = \"/tmp/media\"\n{line}\n");
        let config: ServerConfig = toml::from_str(&toml_str).unwrap();
        let err = resolve_storage_section(&config)
            .expect_err(&format!("{line} without the serving feature must be a startup error"));
        assert!(err.contains("storage-transforms"), "{line}: {err}");
    }
}

/// #370: presets configured into a binary that cannot serve them must REFUSE
/// TO BOOT. Silently accepting them would leave an operator believing renders
/// are configured while `/storage/v1/render/...` 404s.
///
/// Runs only in builds WITHOUT `storage-transforms` — the guard it pins is
/// itself `cfg(not(...))`, so an all-features leg can never execute it.
#[test]
#[cfg(not(feature = "storage-transforms"))]
fn transform_presets_without_the_feature_refuse_to_boot() {
    let toml_str = r#"
        [storage.media]
        backend = "local"
        path = "/tmp/media"
        transform_presets = [{ name = "thumb", width = 200 }]
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let err = resolve_storage_section(&config)
        .expect_err("presets without the serving feature must be a startup error");
    assert!(
        err.contains("storage-transforms"),
        "the refusal must name the missing feature: {err}"
    );
}

/// #371: a bucket policy reaches `BucketConfig` parsed, in order.
#[test]
fn resolve_storage_section_parses_bucket_policies() {
    use fraiseql_storage::{PolicyMethod, PolicyPrincipal};

    let toml_str = r#"
        [storage.docs]
        backend = "local"
        path = "/tmp/docs"

        [[storage.docs.policies]]
        methods = ["read"]
        principal = "role:auditor"
        key_prefix = "reports/"

        [[storage.docs.policies]]
        methods = ["read", "write", "delete"]
        principal = "owner"
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let resolved = resolve_storage_section(&config).unwrap().unwrap();

    let policy = resolved.bucket.policies.expect("policies reach the bucket config");
    assert_eq!(policy.rules.len(), 2);
    assert_eq!(policy.rules[0].methods, vec![PolicyMethod::Read]);
    assert_eq!(policy.rules[0].principal, PolicyPrincipal::Role("auditor".to_string()));
    assert_eq!(policy.rules[0].key_prefix.as_deref(), Some("reports/"));
    assert_eq!(
        policy.rules[1].methods,
        vec![
            PolicyMethod::Read,
            PolicyMethod::Write,
            PolicyMethod::Delete
        ]
    );
    assert_eq!(policy.rules[1].principal, PolicyPrincipal::Owner);
}

/// #1099: the TOML door reaches `set_metadata` and `require_metadata`. Both are
/// carried by one spec type shared with the admin API, so this asserts what an
/// operator actually writes rather than what the evaluator accepts — the door
/// where a `deny_unknown_fields` mismatch would have refused the boot outright.
#[test]
fn resolve_storage_section_parses_metadata_grants_and_conditions() {
    use fraiseql_storage::PolicyMethod;

    let toml_str = r#"
        [storage.docs]
        backend = "local"
        path = "/tmp/docs"

        [[storage.docs.policies]]
        methods = ["set_metadata"]
        principal = "role:curator"

        [[storage.docs.policies]]
        methods = ["read"]
        principal = "authenticated"
        require_metadata = { classification = "public" }
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let resolved = resolve_storage_section(&config).unwrap().unwrap();
    let policy = resolved.bucket.policies.expect("policies reach the bucket config");

    assert_eq!(policy.rules[0].methods, vec![PolicyMethod::SetMetadata]);
    assert_eq!(
        policy.rules[1].require_metadata.get("classification").map(String::as_str),
        Some("public"),
        "require_metadata must survive the TOML door, not just the admin API"
    );
}

/// #1099: the one policy shape that cannot exist. `require_metadata` is answered
/// by asking whether the caller may SET metadata, so a rule granting
/// `set_metadata` that also carries the condition would decide itself. Refused
/// at boot, like every other malformed policy.
#[test]
fn a_self_deciding_metadata_rule_refuses_to_boot() {
    let toml_str = r#"
        [storage.docs]
        backend = "local"
        path = "/tmp/docs"
        [[storage.docs.policies]]
        methods = ["set_metadata"]
        principal = "authenticated"
        require_metadata = { classification = "public" }
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    let err = resolve_storage_section(&config).expect_err("a self-deciding rule must not boot");
    assert!(err.contains("decide itself"), "{err}");
}

/// #371: a policy that does not parse REFUSES TO BOOT. Accepting a rule with
/// an unknown method or principal would either deny everything silently or —
/// worse, in a multi-rule policy — drop the narrowing rule while the
/// permitting ones stand.
#[test]
fn unparseable_policies_refuse_to_boot() {
    let cases = [
        (
            r#"
            [storage.docs]
            backend = "local"
            path = "/tmp/docs"
            [[storage.docs.policies]]
            methods = ["reed"]
            principal = "owner"
            "#,
            "unknown policy method",
        ),
        (
            r#"
            [storage.docs]
            backend = "local"
            path = "/tmp/docs"
            [[storage.docs.policies]]
            methods = ["read"]
            principal = "everyone"
            "#,
            "unknown policy principal",
        ),
        (
            r#"
            [storage.docs]
            backend = "local"
            path = "/tmp/docs"
            [[storage.docs.policies]]
            methods = []
            principal = "owner"
            "#,
            "lists no methods",
        ),
    ];
    for (toml_str, expected) in cases {
        let config: ServerConfig = toml::from_str(toml_str).unwrap();
        let err = resolve_storage_section(&config)
            .expect_err("an unparseable policy must refuse to boot");
        assert!(err.contains(expected), "expected {expected:?} in: {err}");
    }

    // An unknown *field* inside a rule is refused by serde itself, before
    // resolution — a typo'd key must never be silently dropped.
    let with_typo = r#"
        [storage.docs]
        backend = "local"
        path = "/tmp/docs"
        [[storage.docs.policies]]
        methods = ["read"]
        principal = "owner"
        keyprefix = "reports/"
    "#;
    assert!(
        toml::from_str::<ServerConfig>(with_typo).is_err(),
        "an unknown policy-rule field must refuse to parse"
    );
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
fn an_auth_hs256_section_is_refused_as_an_unknown_field() {
    // The shared-secret path is gone: a config that still declares it must fail to load,
    // not boot with no authentication at all.
    let toml_str = r#"
        [auth_hs256]
        secret_env = "MY_TEST_SECRET"
        audience = "test-api"
    "#;
    let err = toml::from_str::<ServerConfig>(toml_str).unwrap_err().to_string();
    assert!(err.contains("auth_hs256"), "the refusal names the section: {err}");

    let msg = super::methods::enrich_parse_error(&[], toml_str, err).0;
    assert!(
        msg.contains("`[auth_hs256]` is no longer supported") && msg.contains("`[auth_issuer]`"),
        "the refusal points at what replaced it: {msg}"
    );
}

/// The refusal `ServerConfig::from_file` gives for a file holding `toml`.
fn load_refusal(toml: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.toml");
    std::fs::write(&path, toml).unwrap();
    ServerConfig::from_file(&path).expect_err("the file must be refused")
}

/// A setting both files make, written with `fraiseql.toml`'s spelling, is refused with a
/// note naming the server file's spelling. Serde's unknown-field list names neither file.
#[test]
fn a_fraiseql_toml_spelling_is_refused_naming_the_server_files_spelling() {
    for (toml, spelling, instead) in [
        (
            "[tls]\nenabled = true\ncert_file = \"/c\"\nkey_path = \"/k\"\n",
            "[tls] cert_file",
            "`cert_path`",
        ),
        (
            "[tls]\nenabled = true\ncert_path = \"/c\"\nkey_file = \"/k\"\n",
            "[tls] key_file",
            "`key_path`",
        ),
        (
            "[database_tls]\nssl_mode = \"require\"\n",
            "[database_tls] ssl_mode",
            "`postgres_ssl_mode`",
        ),
        ("[server]\nhost = \"0.0.0.0\"\n", "[server]", "`bind_addr`"),
        ("[database]\nurl = \"postgres://db/app\"\n", "[database]", "`database_url`"),
        (
            "[database.transaction_retry]\nmax_attempts = 3\n",
            "[database]",
            "`[transaction_retry]`",
        ),
    ] {
        let msg = load_refusal(toml);
        assert!(msg.contains(&format!("`{spelling}`")), "{spelling}: {msg}");
        assert!(msg.contains("fraiseql.toml"), "{spelling}: names no other file: {msg}");
        assert!(msg.contains(instead), "{spelling}: does not name {instead}: {msg}");
    }
}

/// `[tenancy]` names a different table in each file: `fraiseql.toml`'s keys are refused here
/// naming that file, which compiles them into the schema.
#[test]
fn fraiseql_tomls_tenancy_keys_are_refused_naming_that_file() {
    for (toml, spelling) in [
        ("[tenancy]\nmode = \"schema\"\n", "[tenancy] mode"),
        ("[tenancy]\ntenant_claim = \"org\"\n", "[tenancy] tenant_claim"),
    ] {
        let msg = load_refusal(toml);
        assert!(msg.contains(&format!("`{spelling}`")), "{spelling}: {msg}");
        assert!(msg.contains("compiled"), "{spelling}: {msg}");
        assert!(msg.contains("fraiseql.toml"), "{spelling}: {msg}");
    }
}

/// `ServerConfig::from_file` over a file holding `toml`.
fn load(toml: &str) -> Result<ServerConfig, String> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.toml");
    std::fs::write(&path, toml).unwrap();
    ServerConfig::from_file(&path)
}

/// `${VAR}` expands in the server's file as in `fraiseql.toml`.
#[test]
fn an_environment_variable_expands_in_the_server_file() {
    temp_env::with_var("FQ_SRV_DB_URL", Some("postgres://db.internal/app"), || {
        let config = load("database_url = \"${FQ_SRV_DB_URL}\"\n").unwrap();
        assert_eq!(config.database_url, "postgres://db.internal/app");
    });
}

/// An unset variable refuses the file. Kept literal, `admin_token = "${ADMIN_TOKEN}"` would
/// boot with the placeholder as the token.
#[test]
fn an_unset_environment_variable_refuses_the_server_file() {
    temp_env::with_var_unset("FQ_SRV_UNSET_TOKEN", || {
        let err = load("admin_token = \"${FQ_SRV_UNSET_TOKEN}\"\n").unwrap_err();
        assert!(err.contains("FQ_SRV_UNSET_TOKEN"), "names the variable: {err}");
        assert!(err.contains("not set"), "{err}");
    });
}

/// A value that would break out of its TOML string is refused, as in `fraiseql.toml`.
#[test]
fn an_environment_value_with_toml_metacharacters_refuses_the_server_file() {
    temp_env::with_var("FQ_SRV_BAD", Some("x\"\nadmin_api_enabled = true"), || {
        let err = load("admin_token = \"${FQ_SRV_BAD}\"\n").unwrap_err();
        assert!(err.contains("FQ_SRV_BAD"), "names the variable: {err}");
    });
}

/// Each value that could end the TOML string it is placed in, or start a line of its own,
/// refuses the file: expansion runs on the text before it is parsed. One case per refusal,
/// each through the server's loader, and each would otherwise have set `admin_api_enabled`.
#[test]
fn a_value_that_could_escape_its_string_refuses_the_server_file() {
    for (file, value) in [
        ("admin_token = \"${FQ_SRV_ESC}\"\n", "x\" admin_api_enabled = true #"),
        ("admin_token = \"${FQ_SRV_ESC}\"\n", "x\nadmin_api_enabled = true"),
        ("admin_token = \"${FQ_SRV_ESC}\"\n", "x\radmin_api_enabled = true"),
        ("admin_token = '${FQ_SRV_ESC}'\n", "x' admin_api_enabled = true #"),
        (
            "admin_token = \"\"\"${FQ_SRV_ESC}\"\"\"\n",
            "x\"\"\" admin_api_enabled = true #",
        ),
        ("admin_token = \"${FQ_SRV_ESC}\"\n", "x\\"),
    ] {
        temp_env::with_var("FQ_SRV_ESC", Some(value), || {
            let err = load(file).expect_err(value);
            assert!(err.contains("FQ_SRV_ESC"), "{value:?}: names the variable: {err}");
        });
    }
}

/// A parse error in a file whose variables were expanded does not echo the expanded line:
/// it may hold a secret.
#[test]
fn a_parse_error_after_expansion_does_not_print_the_expanded_value() {
    temp_env::with_var("FQ_SRV_SECRET", Some("s3cr3t-token-value"), || {
        let err = load("admin_token = \"${FQ_SRV_SECRET}\" bogus\n").unwrap_err();
        assert!(!err.contains("s3cr3t-token-value"), "{err}");
        assert!(err.contains("line 1"), "still says where: {err}");
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

/// #874: the Arrow Flight gRPC surface must default to loopback like the HTTP
/// surface (`default_bind_addr`), not the 0.0.0.0 wildcard; the built-in
/// default no longer consults the environment; env overrides flow through
/// `ServerArgs`, where a malformed value is a hard startup error instead of a
/// silent fallback to the wildcard.
///
/// The malformed-value case is pinned in `cli::env_tests`, which reads the real
/// `FRAISEQL_FLIGHT_BIND_ADDR` in a child process, racing no test that parses `Cli`.
#[cfg(feature = "arrow")]
mod flight_bind_addr_874 {
    use super::*;

    #[test]
    fn default_is_loopback_not_wildcard() {
        let config: ServerConfig = toml::from_str("").expect("empty config parses");
        assert_eq!(
            config.flight_bind_addr.to_string(),
            "127.0.0.1:50051",
            "Flight must default to loopback (parity with default_bind_addr)"
        );
    }

    #[cfg(feature = "cli")]
    #[test]
    fn env_override_applies_over_config_file_value() {
        // A VALID value on the real var name (a concurrent clap parse reading
        // it is harmless; garbage here would race those tests).
        std::env::set_var("FRAISEQL_FLIGHT_BIND_ADDR", "127.0.0.1:60051");
        let args = crate::ServerArgs::from_env();
        std::env::remove_var("FRAISEQL_FLIGHT_BIND_ADDR");
        let args = args.expect("valid flight addr parses");
        assert_eq!(args.flight_bind_addr.map(|a| a.to_string()), Some("127.0.0.1:60051".into()));

        let mut config: ServerConfig = toml::from_str("flight_bind_addr = \"10.0.0.9:50051\"")
            .expect("config with flight_bind_addr parses");
        args.apply_to_config(&mut config);
        assert_eq!(
            config.flight_bind_addr.to_string(),
            "127.0.0.1:60051",
            "documented precedence: env must override the config-file value"
        );
    }
}

/// #874 (5.1): the `[rate_limiting]` example in `ServerConfig`'s own rustdoc must parse,
/// and a partial table starts from the documented defaults. This pins the exact block.
#[test]
fn rustdoc_rate_limiting_example_parses() {
    let toml_str = r"
        [rate_limiting]
        requests_per_second = 100            # per client IP
        requests_per_second_per_user = 1000  # per authenticated user
        burst_size = 200
    ";
    let config: ServerConfig =
        toml::from_str(toml_str).expect("the documented [rate_limiting] example must parse");
    let rl = config.rate_limiting.expect("section present");
    assert!(rl.policy.enabled);
    assert_eq!(rl.policy.requests_per_second, 100);
    assert_eq!(rl.policy.requests_per_second_per_user, Some(1000));
    assert_eq!(rl.policy.burst_size, 200);
}

/// An unknown key in the server's `[rate_limiting]` stays an error, as it is in the compiled
/// section: the table is read by hand, and dropping `deny_unknown_fields` there would accept
/// a typo silently.
#[test]
fn an_unknown_rate_limiting_key_is_refused() {
    let err = toml::from_str::<ServerConfig>("[rate_limiting]\nrequests_per_secnd = 5\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains("requests_per_secnd"), "{err}");
}

mod read_replicas_407 {
    use super::*;

    #[test]
    fn defaults_to_no_replicas() {
        let config = ServerConfig::default();
        assert!(config.read_replica_urls.is_empty());
        assert!(config.read_replica_pin_after_write_ms.is_none());
        assert!(config.read_replicas().is_none(), "no replica config must lower to None");
    }

    #[test]
    fn lowers_urls_with_the_default_pin_window() {
        let config = ServerConfig {
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            ..ServerConfig::default()
        };
        let rc = config.read_replicas().expect("configured replicas must lower to Some");
        assert_eq!(rc.urls, vec!["postgres://replica1/db".to_string()]);
        assert_eq!(
            rc.pin_after_write,
            std::time::Duration::from_secs(5),
            "the pin default is 5000 ms and lives in this one seam"
        );
    }

    #[test]
    fn lowers_an_explicit_pin_window() {
        let config = ServerConfig {
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            read_replica_pin_after_write_ms: Some(250),
            ..ServerConfig::default()
        };
        let rc = config.read_replicas().expect("configured replicas must lower to Some");
        assert_eq!(rc.pin_after_write, std::time::Duration::from_millis(250));
    }

    #[test]
    fn pin_without_urls_is_refused_as_inert() {
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_pin_after_write_ms: Some(5000),
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("an inert pin setting must be refused");
        assert!(
            err.contains("read_replica_pin_after_write_ms"),
            "the refusal must name the inert key; got: {err}"
        );
    }

    #[test]
    fn empty_url_entries_are_refused() {
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_urls: vec!["postgres://replica1/db".to_string(), "  ".to_string()],
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("a blank replica URL must be refused");
        assert!(err.contains("read_replica_urls"), "got: {err}");
    }

    #[test]
    fn replica_config_with_urls_validates() {
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            read_replica_pin_after_write_ms: Some(10_000),
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok(), "a well-formed replica config must validate");
    }

    #[test]
    fn toml_round_trip() {
        let toml = r#"
            database_url = "postgres://primary/db"
            read_replica_urls = ["postgres://replica1/db", "postgres://replica2/db"]
            read_replica_pin_after_write_ms = 2500
        "#;
        let config: ServerConfig = toml::from_str(toml).expect("replica keys must deserialize");
        assert_eq!(config.read_replica_urls.len(), 2);
        assert_eq!(config.read_replica_pin_after_write_ms, Some(2500));
        let rc = config.read_replicas().unwrap();
        assert_eq!(rc.urls.len(), 2);
    }
}

mod bounded_staleness_957 {
    use super::*;

    #[test]
    fn probing_is_on_by_default_and_lag_gating_is_not() {
        let config = ServerConfig {
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            ..ServerConfig::default()
        };
        let rc = config.read_replicas().expect("configured replicas must lower to Some");
        assert_eq!(
            rc.max_lag, None,
            "lag-based routing is opt-in: without a budget, replicas serve reads however \
             far behind they are, exactly as before this option existed"
        );
        assert_eq!(
            rc.health_probe_interval,
            std::time::Duration::from_secs(1),
            "probing is NOT opt-in — a failover that promotes a replica after boot is \
             invisible to the one-shot boot health check"
        );
    }

    #[test]
    fn lowers_an_explicit_budget_and_cadence() {
        let config = ServerConfig {
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            read_replica_max_lag_ms: Some(2000),
            read_replica_health_probe_interval_ms: Some(250),
            ..ServerConfig::default()
        };
        let rc = config.read_replicas().expect("configured replicas must lower to Some");
        assert_eq!(rc.max_lag, Some(std::time::Duration::from_secs(2)));
        assert_eq!(rc.health_probe_interval, std::time::Duration::from_millis(250));
    }

    #[test]
    fn a_budget_without_urls_is_refused_as_inert() {
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_max_lag_ms: Some(2000),
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("an inert staleness budget must be refused");
        assert!(
            err.contains("read_replica_max_lag_ms"),
            "the refusal must name the inert key; got: {err}"
        );
    }

    #[test]
    fn a_probe_interval_without_urls_is_refused_as_inert() {
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_health_probe_interval_ms: Some(500),
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("an inert probe interval must be refused");
        assert!(
            err.contains("read_replica_health_probe_interval_ms"),
            "the refusal must name the inert key; got: {err}"
        );
    }

    #[test]
    fn a_budget_no_larger_than_the_probe_interval_is_refused() {
        // Eligibility ages the last probe, so a budget of one probe period would
        // drop a fully caught-up replica out of rotation for part of every cycle
        // — replica routing that silently mostly does not happen.
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            read_replica_max_lag_ms: Some(1000),
            read_replica_health_probe_interval_ms: Some(1000),
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("budget <= probe interval must be refused");
        assert!(
            err.contains("read_replica_max_lag_ms")
                && err.contains("read_replica_health_probe_interval_ms"),
            "the refusal must name both keys so the operator can see the relation; got: {err}"
        );
    }

    #[test]
    fn a_budget_against_the_default_probe_interval_is_checked_too() {
        // The interval is defaulted, not written down, so the comparison has to
        // use the same default `read_replicas()` lowers — otherwise the check
        // passes here and the adapter refuses at boot instead.
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            read_replica_max_lag_ms: Some(500),
            ..ServerConfig::default()
        };
        let err = config
            .validate()
            .expect_err("a budget below the DEFAULT probe interval must be refused");
        assert!(
            err.contains("1000"),
            "the refusal must state the effective interval; got: {err}"
        );
    }

    #[test]
    fn a_zero_probe_interval_is_refused() {
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            read_replica_health_probe_interval_ms: Some(0),
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("a zero probe interval must be refused");
        assert!(err.contains("read_replica_health_probe_interval_ms"), "got: {err}");
    }

    #[test]
    fn a_well_formed_bounded_staleness_config_validates() {
        let config = ServerConfig {
            cors_enabled: false,
            read_replica_urls: vec!["postgres://replica1/db".to_string()],
            read_replica_max_lag_ms: Some(2000),
            read_replica_health_probe_interval_ms: Some(500),
            ..ServerConfig::default()
        };
        assert!(
            config.validate().is_ok(),
            "a well-formed bounded-staleness config must validate"
        );
    }

    #[test]
    fn toml_round_trip() {
        let toml = r#"
            database_url = "postgres://primary/db"
            read_replica_urls = ["postgres://replica1/db"]
            read_replica_max_lag_ms = 1500
            read_replica_health_probe_interval_ms = 250
        "#;
        let config: ServerConfig =
            toml::from_str(toml).expect("bounded-staleness keys must deserialize");
        assert_eq!(config.read_replica_max_lag_ms, Some(1500));
        assert_eq!(config.read_replica_health_probe_interval_ms, Some(250));
        let rc = config.read_replicas().unwrap();
        assert_eq!(rc.max_lag, Some(std::time::Duration::from_millis(1500)));
        assert_eq!(rc.health_probe_interval, std::time::Duration::from_millis(250));
    }
}

mod graphql_sse_387 {
    use super::*;

    #[test]
    fn disabled_by_default() {
        let config = ServerConfig::default();
        assert!(!config.enable_graphql_incremental, "SSE transport must be opt-in");
        assert!(config.graphql_incremental_batch_size.is_none());
        assert_eq!(config.graphql_incremental_batch_size(), 100, "the batch-size default is 100");
    }

    #[test]
    fn batch_size_without_enable_is_refused_as_inert() {
        let config = ServerConfig {
            cors_enabled: false,
            graphql_incremental_batch_size: Some(50),
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("an inert batch size must be refused");
        assert!(err.contains("graphql_incremental_batch_size"), "got: {err}");
    }

    #[test]
    fn zero_batch_size_is_refused() {
        let config = ServerConfig {
            cors_enabled: false,
            enable_graphql_incremental: true,
            graphql_incremental_batch_size: Some(0),
            ..ServerConfig::default()
        };
        let err = config.validate().expect_err("a zero batch size must be refused");
        assert!(err.contains("at least 1"), "got: {err}");
    }

    #[test]
    fn enabled_with_batch_size_validates_and_lowers() {
        let config = ServerConfig {
            cors_enabled: false,
            enable_graphql_incremental: true,
            graphql_incremental_batch_size: Some(25),
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok());
        assert_eq!(config.graphql_incremental_batch_size(), 25);
    }

    #[test]
    fn toml_round_trip() {
        let toml = r"
            enable_graphql_incremental = true
            graphql_incremental_batch_size = 10
        ";
        let config: ServerConfig = toml::from_str(toml).expect("SSE keys must deserialize");
        assert!(config.enable_graphql_incremental);
        assert_eq!(config.graphql_incremental_batch_size(), 10);
    }
}

/// The SQL console's boot refusals (#962).
///
/// The console executes SQL an operator typed, so every condition that could
/// make it reachable is refused by name rather than by quietly not mounting. An
/// operator who configured a console and got a 404 cannot tell "not mounted" from
/// "wrong URL"; an operator who gets a boot error naming the missing piece can.
mod admin_sql_console {
    use super::*;

    const TOKEN: &str = "admin-token-at-least-32-characters-long";
    const READONLY: &str = "readonly-token-at-least-32-characters-x";

    fn enabled_console() -> AdminSqlConfig {
        AdminSqlConfig {
            enabled: true,
            ..AdminSqlConfig::default()
        }
    }

    fn base(admin_sql: Option<AdminSqlConfig>) -> ServerConfig {
        ServerConfig {
            cors_enabled: false,
            admin_api_enabled: true,
            admin_token: Some(TOKEN.to_string()),
            admin_readonly_token: Some(READONLY.to_string()),
            admin_sql,
            ..ServerConfig::default()
        }
    }

    /// The default is off, and an absent section validates.
    #[test]
    fn absent_and_disabled_are_both_fine() {
        assert!(!AdminSqlConfig::default().enabled);
        assert!(base(None).validate().is_ok());
        assert!(base(Some(AdminSqlConfig::default())).validate().is_ok());
    }

    /// The `admin-sql` cargo feature is the outer gate. In a build that has it
    /// the fully-configured console validates; in one that does not, `enabled =
    /// true` is refused and the message names the feature.
    ///
    /// Written as one test over `cfg!` rather than two `#[cfg]`-gated ones
    /// because a `#[cfg(not(feature))]` test never runs in the all-features leg
    /// (the #1082 asymmetry) — this arm always runs and always asserts.
    #[test]
    fn the_cargo_feature_is_the_outer_gate() {
        let result = base(Some(enabled_console())).validate();
        if cfg!(feature = "admin-sql") {
            assert!(result.is_ok(), "a fully-configured console must validate: {result:?}");
        } else {
            let err = result.expect_err("without the feature the console must be refused");
            assert!(err.contains("admin-sql"), "the refusal must name the feature: {err}");
        }
    }

    /// The feature gate is outermost, so in a build without it *that* is the
    /// refusal — asserted per build rather than assumed, because a test that
    /// expects the inner message would fail only in the narrow-feature leg (the
    /// shape `make preflight`'s `--all-features` structurally cannot see).
    #[test]
    fn enabled_without_the_admin_api_is_refused() {
        let config = ServerConfig {
            admin_api_enabled: false,
            ..base(Some(enabled_console()))
        };
        let err = config.validate().expect_err("must be refused");
        let expected = if cfg!(feature = "admin-sql") {
            "admin_api_enabled"
        } else {
            "admin-sql"
        };
        assert!(err.contains(expected), "got: {err}");
    }

    /// The console cannot exist without `admin_token` — and not because
    /// `[admin_sql]` checks for it: `admin_api_enabled` already requires it, and
    /// the mount sits inside the `Some(admin_token)` branch. This pins the
    /// property at the level that actually enforces it.
    #[test]
    fn the_admin_api_already_refuses_to_enable_without_a_token() {
        let config = ServerConfig {
            admin_token: None,
            admin_readonly_token: None,
            ..base(Some(enabled_console()))
        };
        let err = config.validate().expect_err("must be refused");
        assert!(err.contains("admin_token"), "got: {err}");
    }

    /// A zero timeout looks like the strictest possible setting and is the
    /// opposite: PostgreSQL reads `statement_timeout = 0` as *no* timeout.
    #[test]
    fn a_zero_bound_is_refused_rather_than_accepted_as_a_limit() {
        let zero_timeout = base(Some(AdminSqlConfig {
            statement_timeout_ms: 0,
            ..enabled_console()
        }));
        let err = zero_timeout.validate().expect_err("must be refused");
        assert!(err.contains("statement_timeout_ms"), "got: {err}");

        let zero_rows = base(Some(AdminSqlConfig {
            max_rows: 0,
            ..enabled_console()
        }));
        let err = zero_rows.validate().expect_err("must be refused");
        assert!(err.contains("max_rows"), "got: {err}");
    }

    /// A disabled section is not validated for its bounds — an operator may
    /// leave a tuning they are not currently using.
    #[test]
    fn a_disabled_section_is_not_bound_checked() {
        let config = base(Some(AdminSqlConfig {
            enabled:              false,
            statement_timeout_ms: 0,
            max_rows:             0,
            allow_commit:         true,
        }));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn toml_round_trip() {
        let toml = r"
            [admin_sql]
            enabled = true
            statement_timeout_ms = 5000
            max_rows = 250
            allow_commit = false
        ";
        let config: ServerConfig = toml::from_str(toml).expect("[admin_sql] must deserialize");
        let section = config.admin_sql.expect("section present");
        assert!(section.enabled);
        assert_eq!(section.statement_timeout_ms, 5_000);
        assert_eq!(section.max_rows, 250);
        assert!(!section.allow_commit);
    }

    /// The defaults an operator gets by writing only `enabled = true`.
    #[test]
    fn defaults_are_the_conservative_ones() {
        let toml = r"
            [admin_sql]
            enabled = true
        ";
        let config: ServerConfig = toml::from_str(toml).expect("[admin_sql] must deserialize");
        let section = config.admin_sql.expect("section present");
        assert_eq!(section.statement_timeout_ms, 30_000);
        assert_eq!(section.max_rows, 1_000);
        assert!(section.allow_commit, "the acceptance is an endpoint that can commit on request");
    }
}

// #1577: the cross-server feed is opt-in, spelled as the docs spell it.
#[test]
fn the_cache_feed_is_off_unless_asked_for() {
    let config: ServerConfig = toml::from_str("").unwrap();
    assert_eq!(config.cache_cross_instance, CacheCrossInstance::Off);
    assert_eq!(ServerConfig::default().cache_cross_instance, CacheCrossInstance::Off);

    let config: ServerConfig = toml::from_str("cache_cross_instance = \"notify\"\n").unwrap();
    assert_eq!(config.cache_cross_instance, CacheCrossInstance::Notify);

    assert!(toml::from_str::<ServerConfig>("cache_cross_instance = \"redis\"\n").is_err());
}

/// The service SDL's auth switch is read from TOML and from
/// `FRAISEQL_SERVICE_SDL_REQUIRE_AUTH`, and the environment wins.
mod service_sdl_require_auth {
    use super::*;

    #[test]
    fn it_is_read_from_toml() {
        let config: ServerConfig =
            toml::from_str("service_sdl_require_auth = false").expect("config parses");
        assert_eq!(config.service_sdl_require_auth, Some(false));
        let config: ServerConfig = toml::from_str("").expect("empty config parses");
        assert_eq!(config.service_sdl_require_auth, None);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_environment_overrides_the_config_file() {
        // A valid value on the real name: a concurrent clap parse reading it is harmless.
        std::env::set_var("FRAISEQL_SERVICE_SDL_REQUIRE_AUTH", "true");
        let args = crate::ServerArgs::from_env();
        std::env::remove_var("FRAISEQL_SERVICE_SDL_REQUIRE_AUTH");
        let args = args.expect("a boolean parses");
        assert_eq!(args.service_sdl_require_auth, Some(true));

        let mut config: ServerConfig =
            toml::from_str("service_sdl_require_auth = false").expect("config parses");
        args.apply_to_config(&mut config);
        assert_eq!(config.service_sdl_require_auth, Some(true));
    }
}

mod suggestions_enabled {
    use fraiseql_guard::deployment::Deployment;

    use super::*;

    #[test]
    fn it_is_read_from_toml() {
        let config: ServerConfig =
            toml::from_str("suggestions_enabled = true").expect("config parses");
        assert_eq!(config.suggestions_enabled, Some(true));
        let config: ServerConfig = toml::from_str("").expect("empty config parses");
        assert_eq!(config.suggestions_enabled, None);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_environment_overrides_the_config_file() {
        // A valid value on the real name: a concurrent clap parse reading it is harmless.
        std::env::set_var("FRAISEQL_SUGGESTIONS_ENABLED", "false");
        let args = crate::ServerArgs::from_env();
        std::env::remove_var("FRAISEQL_SUGGESTIONS_ENABLED");
        let args = args.expect("a boolean parses");
        assert_eq!(args.suggestions_enabled, Some(false));

        let mut config: ServerConfig =
            toml::from_str("suggestions_enabled = true").expect("config parses");
        args.apply_to_config(&mut config);
        assert_eq!(config.suggestions_enabled, Some(false));
    }

    /// #1717: unset, a production server withholds hints and a development one gives them.
    #[test]
    fn an_unset_switch_follows_the_deployment() {
        let config = ServerConfig::default();
        assert!(!config.suggestions_enabled_in(Deployment::Production));
        assert!(config.suggestions_enabled_in(Deployment::Development));
    }

    /// #1717: set, it is honoured in either deployment.
    #[test]
    fn a_set_switch_is_honoured_in_either_deployment() {
        for (set, deployment) in [
            (true, Deployment::Production),
            (true, Deployment::Development),
            (false, Deployment::Production),
            (false, Deployment::Development),
        ] {
            let config = ServerConfig {
                suggestions_enabled: Some(set),
                ..ServerConfig::default()
            };
            assert_eq!(config.suggestions_enabled_in(deployment), set, "{deployment:?}");
        }
    }
}

mod transaction_retry {
    use std::time::Duration;

    use fraiseql_core::db::postgres::{RetryEvent, TransactionRetry};

    use super::*;

    /// `toml` over a configuration `validate` accepts.
    fn parsed(toml: &str) -> ServerConfig {
        let config: ServerConfig = toml::from_str(toml).unwrap();
        ServerConfig {
            cors_enabled: false,
            ..config
        }
    }

    #[test]
    fn without_the_section_a_mutation_runs_once() {
        let policy = ServerConfig::default().transaction_retry().unwrap();
        assert_eq!(policy.max_attempts(), 1);
    }

    #[test]
    fn the_section_sets_the_policy() {
        let config = parsed("[transaction_retry]\nmax_attempts = 4\nbase_ms = 5\nmax_ms = 50\n");
        let policy = config.transaction_retry().unwrap();
        assert_eq!(policy.max_attempts(), 4);
        assert_eq!(
            format!("{policy:?}"),
            format!("{:?}", TransactionRetry::new(4, 5, 50).unwrap())
        );
    }

    /// An empty section opts in with the documented defaults.
    #[test]
    fn an_empty_section_opts_in_with_the_defaults() {
        let policy = parsed("[transaction_retry]\n").transaction_retry().unwrap();
        assert_eq!(
            format!("{policy:?}"),
            format!("{:?}", TransactionRetry::new(3, 10, 200).unwrap())
        );
    }

    #[test]
    fn a_misspelt_key_is_refused() {
        let err = toml::from_str::<ServerConfig>("[transaction_retry]\nmax_attempt = 4\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("max_attempt"), "{err}");
    }

    #[test]
    fn each_bound_is_refused_by_name_at_validation() {
        for (section, named) in [
            ("max_attempts = 0", "max_attempts"),
            ("base_ms = 300\nmax_ms = 200", "base_ms"),
            ("max_ms = 60000", "max_ms"),
        ] {
            let config = parsed(&format!("[transaction_retry]\n{section}\n"));
            let valid = ServerConfig {
                transaction_retry: None,
                ..config.clone()
            };
            valid.validate().unwrap();
            let err = config.validate().expect_err(named);
            assert!(err.contains("transaction_retry") && err.contains(named), "{named}: {err}");
        }
    }

    #[test]
    fn a_retry_and_an_exhaustion_count_on_their_own_counters_by_function_and_sqlstate() {
        let labels = |function: &str, sqlstate: &str| {
            [
                ("function", function.to_string()),
                ("sqlstate", sqlstate.to_string()),
            ]
        };
        assert_eq!(
            retry_metric(RetryEvent::Retrying {
                function:  "app.fn_create_post",
                sql_state: "40P01",
                attempt:   1,
                backoff:   Duration::from_millis(3),
            }),
            (
                "fraiseql_mutation_transaction_retries_total",
                labels("app.fn_create_post", "40P01")
            )
        );
        assert_eq!(
            retry_metric(RetryEvent::Exhausted {
                function:  "app.fn_create_post",
                sql_state: "40001",
                attempts:  3,
            }),
            (
                "fraiseql_mutation_transaction_retries_exhausted_total",
                labels("app.fn_create_post", "40001")
            )
        );
    }
}

/// The refused spelling in a server file is data on the loader's error, the file named;
/// the message is the one `from_file` gives.
#[test]
fn a_refused_spelling_in_the_server_file_carries_its_diagnostic() {
    use fraiseql_core::utils::config_diagnostic::DiagnosticCode;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.toml");
    std::fs::write(&path, "[tls]\nkey_file = \"key.pem\"\n").unwrap();
    let refused = ServerConfig::load_file(&path).expect_err("a fraiseql.toml spelling");
    let diagnostic = refused.diagnostic().expect("a refused spelling");
    assert_eq!(diagnostic.code, DiagnosticCode::Misplaced);
    assert_eq!(
        (diagnostic.table.as_str(), diagnostic.key.as_deref()),
        ("tls", Some("key_file"))
    );
    assert_eq!(diagnostic.replacement.as_deref(), Some("`key_path`"));
    assert_eq!(diagnostic.file.as_deref(), Some(path.display().to_string().as_str()));
    assert_eq!(refused.to_string(), ServerConfig::from_file(&path).unwrap_err());
}

/// A file that is not a refused spelling (here, not TOML at all) carries no diagnostic.
#[test]
fn a_server_file_that_does_not_parse_carries_no_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.toml");
    std::fs::write(&path, "bind_addr = \n").unwrap();
    let refused = ServerConfig::load_file(&path).expect_err("not TOML");
    assert!(refused.diagnostic().is_none(), "{refused}");
}

/// The flat `[observers]` tuning keys are a misplaced spelling too: `[observers.runtime]`.
#[cfg(feature = "observers")]
#[test]
fn a_flat_observers_key_carries_its_diagnostic() {
    use fraiseql_core::utils::config_diagnostic::DiagnosticCode;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.toml");
    std::fs::write(&path, "[observers]\npoll_interval_ms = 200\n").unwrap();
    let refused = ServerConfig::load_file(&path).expect_err("a flat observers key");
    let diagnostic = refused.diagnostic().expect("a refused spelling");
    assert_eq!(diagnostic.code, DiagnosticCode::Misplaced);
    assert_eq!(
        (diagnostic.table.as_str(), diagnostic.key.as_deref()),
        ("observers", Some("poll_interval_ms"))
    );
    assert_eq!(diagnostic.replacement.as_deref(), Some("`[observers.runtime]`"));
    assert_eq!(diagnostic.file.as_deref(), Some(path.display().to_string().as_str()));
}

/// A `fraiseql.toml` setting this file does not make at all is refused as belonging in
/// `fraiseql.toml`; one it spells otherwise is fixed here, with its spelling.
#[test]
fn a_fraiseql_toml_setting_in_the_server_file_names_where_it_belongs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.toml");
    std::fs::write(&path, "[tenancy]\nmode = \"row\"\n").unwrap();
    let refused = ServerConfig::load_file(&path).expect_err("a fraiseql.toml setting");
    let diagnostic = refused.diagnostic().expect("a refused spelling");
    assert_eq!(diagnostic.belongs_in.as_deref(), Some("fraiseql.toml"));
    assert_eq!(diagnostic.replacement, None);

    std::fs::write(&path, "[tls]\ncert_file = \"c.pem\"\n").unwrap();
    let refused = ServerConfig::load_file(&path).expect_err("a fraiseql.toml spelling");
    let diagnostic = refused.diagnostic().expect("a refused spelling");
    assert_eq!(diagnostic.belongs_in, None);
    assert_eq!(diagnostic.replacement.as_deref(), Some("`cert_path`"));
}

/// A key the server's file does not take is refused as `E-CONFIG-UNKNOWN-KEY`, naming its
/// table and the accepted spelling nearest to it, the file named.
#[test]
fn an_unknown_key_in_the_server_file_carries_its_diagnostic() {
    use fraiseql_core::utils::config_diagnostic::DiagnosticCode;

    for (written, table, key, replacement) in [
        ("bind_adr = \"127.0.0.1:8080\"\n", "", "bind_adr", Some("`bind_addr`")),
        ("[tls]\ncert_pth = \"c.pem\"\n", "tls", "cert_pth", Some("`cert_path`")),
        ("[tls]\nzzzzzzzz = 1\n", "tls", "zzzzzzzz", None),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.toml");
        std::fs::write(&path, written).unwrap();
        let refused = ServerConfig::load_file(&path).expect_err(written);
        assert!(refused.diagnostic().is_some(), "{written}: {refused}");
        let diagnostic = refused.diagnostic().unwrap();
        assert_eq!(diagnostic.code, DiagnosticCode::UnknownKey, "{written}");
        assert_eq!((diagnostic.table.as_str(), diagnostic.key.as_deref()), (table, Some(key)));
        assert_eq!(diagnostic.replacement.as_deref(), replacement, "{written}");
        assert_eq!(diagnostic.file.as_deref(), Some(path.display().to_string().as_str()));
        assert!(refused.to_string().contains(&diagnostic.message), "{refused}");
    }
}

/// A section this build compiled out keeps its feature note, and is not called unknown.
#[cfg(not(feature = "observers"))]
#[test]
fn a_compiled_out_section_is_not_an_unknown_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.toml");
    std::fs::write(&path, "[observers]\nenabled = true\n").unwrap();
    let refused = ServerConfig::load_file(&path).expect_err("compiled out");
    assert!(refused.diagnostic().is_none(), "{refused}");
    assert!(refused.to_string().contains("`observers` feature"), "{refused}");
}

/// A `[rate_limiting]` key renamed to `[security.rate_limiting]`'s spelling, and a flat
/// `[observers]` key that moved to `[observers.runtime]`, are refused as misplaced with the
/// real spelling, not as unknown keys with the nearest guess.
#[test]
fn a_renamed_or_moved_key_is_refused_with_its_real_spelling() {
    use fraiseql_core::utils::config_diagnostic::DiagnosticCode;

    let renamed = crate::middleware::rate_limit::RENAMED_SERVER_KEYS.iter().map(|(old, new)| {
        (
            format!("[rate_limiting]\n{old} = 50\n"),
            "rate_limiting",
            *old,
            format!("`{new}`"),
        )
    });
    #[cfg(feature = "observers")]
    let moved = super::observers::LEGACY_RUNTIME_KEYS.iter().map(|key| {
        (
            format!("[observers]\n{key} = 1\n"),
            "observers",
            *key,
            "`[observers.runtime]`".into(),
        )
    });
    #[cfg(not(feature = "observers"))]
    let moved = std::iter::empty();
    let cases: Vec<(String, &str, &str, String)> = renamed.chain(moved).collect();
    let dir = tempfile::tempdir().unwrap();
    for (written, table, key, replacement) in &cases {
        let path = dir.path().join("server.toml");
        std::fs::write(&path, written).unwrap();
        let refused = ServerConfig::load_file(&path).expect_err(written);
        assert!(refused.diagnostic().is_some(), "{written}: {refused}");
        let diagnostic = refused.diagnostic().unwrap();
        assert_eq!(diagnostic.code, DiagnosticCode::Misplaced, "{written}: {refused}");
        assert_eq!((diagnostic.table.as_str(), diagnostic.key.as_deref()), (*table, Some(*key)));
        assert_eq!(diagnostic.replacement.as_deref(), Some(replacement.as_str()), "{written}");
        assert_eq!(
            refused.to_string().matches(diagnostic.message.as_str()).count(),
            1,
            "the refusal is given once: {refused}"
        );
    }
}

/// Refusals `validate` gives in every deployment: a file a TLS setting names must exist,
/// and a section whose credential could not be issued or checked does not boot.
mod refused_in_every_deployment {
    use super::*;

    /// A config with nothing switched on but the setting under test. CORS is off so the
    /// production check on its origins cannot be the refusal a test reads.
    fn quiet(config: ServerConfig) -> ServerConfig {
        ServerConfig {
            cors_enabled: false,
            ..config
        }
    }

    fn tls(dir: &std::path::Path, key: &str, client_ca: Option<&str>) -> TlsServerConfig {
        let cert_path = dir.join("cert.pem");
        std::fs::write(&cert_path, "cert").expect("the cert file is written");
        std::fs::write(dir.join("key.pem"), "key").expect("the key file is written");
        std::fs::write(dir.join("ca.pem"), "ca").expect("the CA file is written");
        TlsServerConfig {
            enabled: true,
            cert_path,
            key_path: dir.join(key),
            require_client_cert: client_ca.is_some(),
            client_ca_path: client_ca.map(|name| dir.join(name)),
            min_version: "1.2".to_string(),
        }
    }

    #[test]
    fn a_tls_key_file_that_does_not_exist_is_refused() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let present = quiet(ServerConfig {
            tls: Some(tls(dir.path(), "key.pem", None)),
            ..ServerConfig::default()
        });
        assert!(present.validate().is_ok(), "{:?}", present.validate());
        let missing = quiet(ServerConfig {
            tls: Some(tls(dir.path(), "missing-key.pem", None)),
            ..ServerConfig::default()
        });
        let err = missing.validate().expect_err("a missing key file must be refused");
        assert!(err.contains("key file not found"), "got: {err}");
    }

    #[test]
    fn a_client_ca_file_that_does_not_exist_is_refused() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let present = quiet(ServerConfig {
            tls: Some(tls(dir.path(), "key.pem", Some("ca.pem"))),
            ..ServerConfig::default()
        });
        assert!(present.validate().is_ok(), "{:?}", present.validate());
        let missing = quiet(ServerConfig {
            tls: Some(tls(dir.path(), "key.pem", Some("missing-ca.pem"))),
            ..ServerConfig::default()
        });
        let err = missing.validate().expect_err("a missing client CA file must be refused");
        assert!(err.contains("Client CA file not found"), "got: {err}");
    }

    #[test]
    fn a_database_ca_bundle_that_does_not_exist_is_refused() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(dir.path().join("bundle.pem"), "ca").expect("the bundle is written");
        let with_bundle = |name: &str| {
            quiet(ServerConfig {
                database_tls: Some(DatabaseTlsConfig {
                    postgres_ssl_mode:   Some("verify-full".to_string()),
                    verify_certificates: true,
                    ca_bundle_path:      Some(dir.path().join(name)),
                    redis_ssl:           None,
                    clickhouse_https:    None,
                    elasticsearch_https: None,
                }),
                ..ServerConfig::default()
            })
        };
        let present = with_bundle("bundle.pem");
        assert!(present.validate().is_ok(), "{:?}", present.validate());
        let err = with_bundle("missing-bundle.pem")
            .validate()
            .expect_err("a missing CA bundle must be refused");
        assert!(err.contains("CA bundle file not found"), "got: {err}");
    }

    #[cfg(feature = "auth-saml")]
    #[test]
    fn saml_without_an_auth_issuer_is_refused() {
        let config: ServerConfig =
            toml::from_str("cors_enabled = false\n[saml]\nstore_enabled = true\n")
                .expect("config parses");
        assert!(config.saml.is_some() && config.auth_issuer.is_none());
        let err = config.validate().expect_err("[saml] without [auth_issuer] must be refused");
        assert!(err.contains("[saml] requires [auth_issuer]"), "got: {err}");
    }

    #[test]
    fn scim_enabled_without_an_admin_token_is_refused() {
        let scim = |enabled: bool| {
            quiet(ServerConfig {
                scim: Some(ScimServerConfig {
                    enabled,
                    ..ScimServerConfig::default()
                }),
                ..ServerConfig::default()
            })
        };
        assert!(scim(false).validate().is_ok(), "{:?}", scim(false).validate());
        let err = scim(true).validate().expect_err("SCIM without an admin token must be refused");
        assert!(err.contains("[scim] enabled = true requires admin_token"), "got: {err}");
    }

    /// Empty CORS origins are refused in production and allowed in development: the
    /// deployment, not the setting alone, decides.
    #[test]
    fn empty_cors_origins_are_refused_in_production() {
        let config = ServerConfig {
            cors_enabled: true,
            cors_origins: Vec::new(),
            playground_enabled: false,
            ..ServerConfig::default()
        };
        let validate_in = |env: Option<&str>| {
            temp_env::with_vars(
                [
                    ("FRAISEQL_ENV", env),
                    ("FRAISEQL_PROFILE", None),
                    ("KUBERNETES_SERVICE_HOST", None),
                ],
                || config.validate(),
            )
        };
        assert!(
            validate_in(Some("development")).is_ok(),
            "{:?}",
            validate_in(Some("development"))
        );
        let err = validate_in(Some("production"))
            .expect_err("empty CORS origins must be refused in production");
        assert!(err.contains("cors_origins is empty in production mode"), "got: {err}");
    }
}
