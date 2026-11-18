#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness
#![allow(clippy::field_reassign_with_default)] // Reason: test code builds structs incrementally for clarity

mod config_tests {
    use super::super::{security, *};

    #[test]
    fn test_default_config() {
        let config = TomlSchema::default();
        assert_eq!(config.project.name, "my-fraiseql-app");
    }

    #[test]
    fn test_default_security_config() {
        let config = TomlSchema::default();
        assert!(config.security.enterprise.audit_logging_enabled);
        assert!(config.security.rate_limiting.is_none());
    }

    #[test]
    fn test_validation() {
        let config = TomlSchema::default();
        config
            .validate_self_contained()
            .unwrap_or_else(|e| panic!("expected Ok from validate: {e:?}"));
    }

    #[test]
    fn test_role_definitions_default() {
        let config = TomlSchema::default();
        assert!(config.security.role_definitions.is_empty());
        assert!(config.security.default_role.is_none());
    }

    #[test]
    fn test_naming_acronyms_default_empty() {
        let config = TomlSchema::default();
        assert!(config.naming.acronyms.is_empty());
    }

    #[test]
    fn test_parse_naming_acronyms_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[naming]
acronyms = ["s3", "widget5"]
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.naming.acronyms, vec!["s3", "widget5"]);
    }

    #[test]
    fn test_cost_weights_default_empty() {
        let config = TomlSchema::default();
        assert!(config.cost_weights.is_empty());
    }

    #[test]
    fn test_parse_cost_weights_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[cost_weights]
expensiveReport = 5000
searchUsers = 250
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.cost_weights.get("expensiveReport"), Some(&5000));
        assert_eq!(config.cost_weights.get("searchUsers"), Some(&250));
    }

    #[test]
    fn test_naming_convention_defaults_to_camel_case() {
        // A `fraiseql.toml` compiles to a camelCase GraphQL surface by default, both when
        // [naming] is absent entirely and when it is present without a
        // `convention` key.
        let config = TomlSchema::default();
        assert_eq!(
            config.naming.convention_over(None),
            fraiseql_core::schema::NamingConvention::CamelCase
        );

        let toml_str = r#"
[project]
name = "test-app"

[naming]
acronyms = ["s3"]
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert_eq!(
            config.naming.convention_over(None),
            fraiseql_core::schema::NamingConvention::CamelCase
        );
    }

    #[test]
    fn test_parse_naming_convention_preserve_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[naming]
convention = "preserve"
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert_eq!(
            config.naming.convention_over(None),
            fraiseql_core::schema::NamingConvention::Preserve
        );
    }

    #[test]
    fn test_parse_naming_convention_camel_case_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[naming]
convention = "camelCase"
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert_eq!(
            config.naming.convention_over(None),
            fraiseql_core::schema::NamingConvention::CamelCase
        );
    }

    #[test]
    fn test_parse_role_definitions_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[[security.role_definitions]]
name = "viewer"
description = "Read-only access"
scopes = ["read:*"]

[[security.role_definitions]]
name = "admin"
description = "Full access"
scopes = ["admin:*"]

[security]
default_role = "viewer"
"#;

        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");

        assert_eq!(config.security.role_definitions.len(), 2);
        assert_eq!(config.security.role_definitions[0].name, "viewer");
        assert_eq!(config.security.role_definitions[0].scopes[0], "read:*");
        assert_eq!(config.security.role_definitions[1].name, "admin");
        assert_eq!(config.security.default_role, Some("viewer".to_string()));
    }

    #[test]
    fn test_security_config_validation_empty_role_name() {
        let toml_str = r#"
[[security.role_definitions]]
name = ""
scopes = ["read:*"]
"#;

        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert!(config.validate_self_contained().is_err(), "Should fail with empty role name");
    }

    #[test]
    fn test_security_config_validation_empty_scopes() {
        let toml_str = r#"
[[security.role_definitions]]
name = "viewer"
scopes = []
"#;

        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert!(config.validate_self_contained().is_err(), "Should fail with empty scopes");
    }

    #[test]
    fn test_fraiseql_config_parses_server_section() {
        let toml_str = r#"
[server]
host = "127.0.0.1"
port = 9000

[server.cors]
origins = ["https://example.com"]
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.cors.origins, ["https://example.com"]);
    }

    #[test]
    fn test_fraiseql_config_parses_database_section() {
        let toml_str = r#"
[database]
url      = "postgresql://localhost/testdb"
pool_min = 3
pool_max = 15
ssl_mode = "require"
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.database.url, Some("postgresql://localhost/testdb".to_string()));
        assert_eq!(config.database.pool_min, 3);
        assert_eq!(config.database.pool_max, 15);
        assert_eq!(config.database.ssl_mode.as_deref(), Some("require"));
    }

    #[test]
    fn test_env_var_expansion_in_fraiseql_config() {
        temp_env::with_var("TEST_DB_URL", Some("postgres://test/db"), || {
            let toml_str = r#"
[database]
url = "${TEST_DB_URL}"
"#;
            let expanded = expand_env_vars(toml_str).unwrap();
            let config = TomlSchema::parse_toml(&expanded).expect("Failed to parse TOML");
            assert_eq!(config.database.url, Some("postgres://test/db".to_string()));
        });
    }

    #[test]
    fn test_env_var_expansion_unknown_var_passthrough() {
        // Unknown variables should be left as-is, not panic
        let toml_str = r#"url = "${NONEXISTENT_VAR_XYZ123}""#;
        let expanded = expand_env_vars(toml_str).unwrap();
        assert_eq!(expanded, toml_str, "Unknown vars must be left unchanged");
    }

    #[test]
    fn test_env_var_expansion_multiple_occurrences() {
        temp_env::with_var("FRAISEQL_TEST_HOST", Some("db.example.com"), || {
            let toml_str = r#"primary = "${FRAISEQL_TEST_HOST}" replica = "${FRAISEQL_TEST_HOST}""#;
            let expanded = expand_env_vars(toml_str).unwrap();
            assert_eq!(expanded, r#"primary = "db.example.com" replica = "db.example.com""#);
        });
    }

    // ── Tenancy TOML parsing ────────────────────────────────────────────

    #[test]
    fn test_parse_tenancy_row_mode_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[tenancy]
mode = "row"
tenant_claim = "tenant_id"
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert!(matches!(config.tenancy.mode, security::TenancyModeConfig::Row));
        assert_eq!(config.tenancy.tenant_claim, "tenant_id");
    }

    #[test]
    fn test_parse_tenancy_schema_mode_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[tenancy]
mode = "schema"
tenant_claim = "org_id"
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert!(matches!(config.tenancy.mode, security::TenancyModeConfig::Schema));
        assert_eq!(config.tenancy.tenant_claim, "org_id");
    }

    #[test]
    fn test_parse_tenancy_defaults_when_absent() {
        let toml_str = r#"
[project]
name = "test-app"
"#;
        let config = TomlSchema::parse_toml(toml_str).expect("Failed to parse TOML");
        assert!(matches!(config.tenancy.mode, security::TenancyModeConfig::None));
        assert_eq!(config.tenancy.tenant_claim, "tenant_id");
    }

    #[test]
    fn test_parse_tenancy_invalid_mode_rejected() {
        let toml_str = r#"
[tenancy]
mode = "invalid"
"#;
        let result = TomlSchema::parse_toml(toml_str);
        assert!(result.is_err(), "invalid tenancy mode should be rejected");
    }
}

mod runtime_tests {
    use super::super::runtime::*;

    // ── ServerRuntimeConfig defaults ─────────────────────────────────────────

    #[test]
    fn test_server_runtime_config_default() {
        let cfg = ServerRuntimeConfig::default();
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.port, 8080);
        assert!(!cfg.tls.enabled);
        assert!(cfg.cors.origins.is_empty());
    }

    #[test]
    fn test_server_runtime_config_validate_ok() {
        ServerRuntimeConfig::default()
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok from validate: {e:?}"));
    }

    #[test]
    fn test_server_runtime_config_validate_port_zero() {
        let cfg = ServerRuntimeConfig {
            port: 0,
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("port"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_validate_tls_missing_cert() {
        let cfg = ServerRuntimeConfig {
            tls: TlsRuntimeConfig {
                enabled:     true,
                cert_file:   String::new(),
                key_file:    "key.pem".to_string(),
                min_version: "1.2".to_string(),
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("cert_file"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_validate_tls_missing_key() {
        let cfg = ServerRuntimeConfig {
            tls: TlsRuntimeConfig {
                enabled:     true,
                cert_file:   "cert.pem".to_string(),
                key_file:    String::new(),
                min_version: "1.2".to_string(),
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("key_file"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_validate_bad_tls_version() {
        let cfg = ServerRuntimeConfig {
            tls: TlsRuntimeConfig {
                enabled:     true,
                cert_file:   "cert.pem".to_string(),
                key_file:    "key.pem".to_string(),
                min_version: "1.0".to_string(),
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("min_version"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_parses_toml() {
        let toml_str = r#"
host = "127.0.0.1"
port = 9000

[cors]
origins = ["https://example.com"]

[tls]
enabled = false
"#;
        let cfg: ServerRuntimeConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.cors.origins, ["https://example.com"]);
        assert!(!cfg.tls.enabled);
    }

    // ── DatabaseRuntimeConfig ────────────────────────────────────────────────

    #[test]
    fn test_database_runtime_config_default() {
        let cfg = DatabaseRuntimeConfig::default();
        assert!(cfg.url.is_none());
        assert_eq!(cfg.pool_min, 5);
        assert_eq!(cfg.pool_max, 20);
        assert_eq!(cfg.connect_timeout_ms, 30_000);
        // Unset, so the connection URL decides — see `DatabaseRuntimeConfig::ssl_mode`.
        assert_eq!(cfg.ssl_mode, None);
    }

    #[test]
    fn test_database_runtime_config_validate_ok() {
        DatabaseRuntimeConfig::default()
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok from validate: {e:?}"));
    }

    #[test]
    fn test_database_runtime_config_validate_pool_range() {
        let cfg = DatabaseRuntimeConfig {
            pool_min: 10,
            pool_max: 5,
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("pool_min"), "got: {err}");
    }

    #[test]
    fn test_database_runtime_config_validate_ssl_mode() {
        let cfg = DatabaseRuntimeConfig {
            ssl_mode: Some("bogus".to_string()),
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("ssl_mode"), "got: {err}");
    }

    #[test]
    fn test_database_runtime_config_parses_toml() {
        let toml_str = r#"
url      = "postgresql://localhost/mydb"
pool_min = 5
pool_max = 50
ssl_mode = "require"
"#;
        let cfg: DatabaseRuntimeConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.url, Some("postgresql://localhost/mydb".to_string()));
        assert_eq!(cfg.pool_min, 5);
        assert_eq!(cfg.pool_max, 50);
        assert_eq!(cfg.ssl_mode.as_deref(), Some("require"));
    }
}

mod security_tests {
    use super::super::{security::*, toml_schema::security::SecuritySettings};

    /// `settings`, with a default `[tenancy]`, lowered to the compiled `security` block.
    fn lowered(settings: &SecuritySettings) -> serde_json::Value {
        serde_json::to_value(settings.to_compiled(&TenancyTomlConfig::default())).unwrap()
    }

    /// #1151: a `fraiseql.toml` that never mentions `[security.state_encryption]` must not
    /// compile an artifact declaring it enabled.
    ///
    /// When the absent table's default was `enabled: true`, the lowering emitted it into the
    /// compiled artifact, and the server refused to boot:
    ///
    /// ```text
    /// Error: Configuration error: state_encryption enabled but key env var
    /// 'STATE_ENCRYPTION_KEY' failed: env var STATE_ENCRYPTION_KEY not set
    /// ```
    ///
    /// The assertion that matters is on the **lowered artifact**, not the struct field.
    #[test]
    fn state_encryption_is_off_unless_the_operator_asks_for_it() {
        let lowered = lowered(&SecuritySettings::default());
        let enabled = lowered
            .get("state_encryption")
            .and_then(|se| se.get("enabled"))
            .and_then(serde_json::Value::as_bool);

        assert_ne!(
            enabled,
            Some(true),
            "a config that never mentions state_encryption compiled an artifact enabling it; \
             the server then refuses to boot without STATE_ENCRYPTION_KEY (#1151). Lowered: {lowered}"
        );
    }

    #[test]
    fn test_default_security_config() {
        let config = SecuritySettings::default();
        assert!(config.enterprise.audit_logging_enabled);
        // Unwritten, these are the server's to decide.
        assert!(config.error_sanitization.is_none());
        assert!(config.rate_limiting.is_none());
        // State encryption demands a key, and demanding one the operator never configured
        // stops the server from booting.
        assert!(
            config.state_encryption.is_none(),
            "state encryption must stay off until asked for"
        );
    }

    /// #983: the four `[security.error_sanitization]` keys that reached no runtime shape are
    /// gone, so `deny_unknown_fields` refuses them by name.
    #[test]
    fn error_sanitization_refuses_the_keys_that_never_had_a_consumer() {
        for key in [
            "generic_messages = true",
            "internal_logging = true",
            "leak_sensitive_details = true",
            "user_facing_format = \"detailed\"",
        ] {
            let toml = format!("[error_sanitization]\nenabled = true\n{key}\n");
            let err = toml::from_str::<SecuritySettings>(&toml)
                .expect_err("a key with no consumer must not parse");
            assert!(
                err.to_string().contains("unknown field"),
                "the refusal must name the key; got: {err}"
            );
        }
    }

    /// `[security.rate_limiting]` with `body`, validated as every `fraiseql.toml` is.
    fn rate_limiting(body: &str) -> anyhow::Result<()> {
        let toml = format!("[security.rate_limiting]\n{body}\n[project]\nname = \"app\"\n");
        crate::config::TomlSchema::parse_toml(&toml)
            .expect("parses")
            .validate_self_contained()
    }

    #[test]
    fn test_rate_limiting_validation() {
        rate_limiting("").unwrap_or_else(|e| panic!("expected Ok for default config: {e}"));
        assert!(
            rate_limiting("auth_start_window_secs = 0").is_err(),
            "expected Err when auth_start_window_secs=0, got Ok"
        );
    }

    #[test]
    fn test_rate_limiting_zero_max_requests_rejected() {
        let err = rate_limiting("auth_start_max_requests = 0").unwrap_err();
        assert!(
            err.to_string().contains("auth_start_max_requests"),
            "error should name the field: {err}"
        );
        assert!(
            err.to_string().contains("blocks all requests"),
            "error should explain the impact: {err}"
        );
    }

    #[test]
    fn test_rate_limiting_one_max_requests_accepted() {
        rate_limiting("auth_start_max_requests = 1")
            .unwrap_or_else(|e| panic!("expected Ok for max_requests=1: {e}"));
    }

    #[test]
    fn test_rate_limiting_callback_zero_max_requests_rejected() {
        assert!(
            rate_limiting("auth_callback_max_requests = 0").is_err(),
            "expected Err when auth_callback_max_requests=0, got Ok"
        );
    }

    /// `[security.state_encryption]`, parsed as every `fraiseql.toml` reads it.
    fn state_encryption(body: &str) -> Result<SecuritySettings, toml::de::Error> {
        toml::from_str::<SecuritySettings>(&format!("[state_encryption]\n{body}"))
    }

    /// #983 — nonce and key sizes are fixed by the algorithm, so there was nothing
    /// for these keys to size. Refused by name rather than validated into a range
    /// that had no effect.
    #[test]
    fn state_encryption_refuses_the_keys_that_never_had_a_consumer() {
        for key in [
            "key_rotation_enabled = true",
            "nonce_size = 12",
            "key_size = 32",
        ] {
            let err = state_encryption(&format!("enabled = true\n{key}\n"))
                .expect_err("a key with no consumer must not parse");
            assert!(
                err.to_string().contains("unknown field"),
                "the refusal must name the key; got: {err}"
            );
        }
    }

    #[test]
    fn test_state_encryption_unsupported_algorithm_rejected() {
        let err = state_encryption("algorithm = \"rot13\"\n").unwrap_err();
        assert!(err.to_string().contains("rot13"), "error should name the bad algorithm: {err}");
        assert!(
            err.to_string().contains("chacha20-poly1305"),
            "error should list supported algorithms: {err}"
        );
    }

    #[test]
    fn test_state_encryption_aes_256_gcm_accepted() {
        let config = state_encryption("algorithm = \"aes-256-gcm\"\n")
            .unwrap_or_else(|e| panic!("expected Ok for aes-256-gcm: {e}"));
        assert_eq!(
            config.state_encryption.unwrap().algorithm,
            fraiseql_core::schema::EncryptionAlgorithm::Aes256Gcm
        );
    }

    #[test]
    fn test_state_encryption_chacha20_poly1305_accepted() {
        let config = state_encryption("algorithm = \"chacha20-poly1305\"\n")
            .unwrap_or_else(|e| panic!("expected Ok for chacha20-poly1305: {e}"));
        assert_eq!(
            config.state_encryption.unwrap().algorithm,
            fraiseql_core::schema::EncryptionAlgorithm::Chacha20Poly1305
        );
    }

    /// `key_env` names the variable the key is read from, as in a TOML schema.
    #[test]
    fn state_encryption_key_env_reaches_the_compiled_section() {
        let config = state_encryption("enabled = true\nkey_env = \"OAUTH_STATE_KEY\"\n").unwrap();
        assert_eq!(lowered(&config)["state_encryption"]["key_env"], "OAUTH_STATE_KEY");
    }

    #[test]
    fn test_security_config_serialization() {
        let config: SecuritySettings =
            toml::from_str("[rate_limiting]\n[state_encryption]\n").unwrap();
        let json = lowered(&config);
        // `enterprise` carries the key the runtime reads (#977).
        assert!(json["enterprise"]["audit_logging_enabled"].is_boolean());
        assert!(json["state_encryption"]["algorithm"].is_string());

        // Flat, not `rate_limiting.authStart.maxRequests`. This assertion used to pin
        // the nested camelCase shape, which matched no reader anywhere — so the test
        // passed for the whole time a project TOML's rate-limit configuration was
        // reaching nothing (#893). `security_seam_tests` covers the rest of the seam.
        assert!(json["rate_limiting"]["auth_start_max_requests"].is_number());
        assert!(json["rate_limiting"]["requests_per_second"].is_number());
    }

    // ── TenancyTomlConfig ───────────────────────────────────────────────

    #[test]
    fn test_tenancy_default_mode_none() {
        let config = TenancyTomlConfig::default();
        assert!(matches!(config.mode, TenancyModeConfig::None));
        assert_eq!(config.tenant_claim, "tenant_id");
    }

    #[test]
    fn test_tenancy_to_runtime_row_mode() {
        let config = TenancyTomlConfig {
            mode:         TenancyModeConfig::Row,
            tenant_claim: "tenant_id".to_string(),
        };
        let json = serde_json::to_value(config.to_runtime()).unwrap();
        assert_eq!(json["mode"], "row");
        // `tenant_claim`, the name the runtime `TenancyConfig` declares (#757).
        assert_eq!(json["tenant_claim"], "tenant_id");
    }

    #[test]
    fn test_tenancy_to_runtime_schema_mode() {
        let config = TenancyTomlConfig {
            mode:         TenancyModeConfig::Schema,
            tenant_claim: "org_id".to_string(),
        };
        let json = serde_json::to_value(config.to_runtime()).unwrap();
        assert_eq!(json["mode"], "schema");
        assert_eq!(json["tenant_claim"], "org_id");
    }

    #[test]
    fn test_tenancy_validate_empty_claim_with_mode_fails() {
        let config = TenancyTomlConfig {
            mode:         TenancyModeConfig::Row,
            tenant_claim: String::new(),
        };
        assert!(config.validate().is_err());
    }

    /// #1542: the claim is compiled in every mode, so an empty one is refused in every mode.
    #[test]
    fn test_tenancy_validate_empty_claim_with_none_refused() {
        let config = TenancyTomlConfig {
            mode:         TenancyModeConfig::None,
            tenant_claim: String::new(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_tenancy_mode_invalid_variant_rejected() {
        let result: Result<TenancyModeConfig, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
    }
}

/// #1341: `[project] database_target`, the key `fraiseql init` writes into every new
/// `fraiseql.toml`, refuses a backend the engine removed.
mod project_database_target_tests {
    use super::super::*;

    fn project_targeting(target: Option<&str>) -> TomlSchema {
        let key = target.map_or_else(String::new, |t| format!("database_target = \"{t}\"\n"));
        TomlSchema::parse_toml(&format!("[project]\nname = \"x\"\nversion = \"0.1.0\"\n{key}"))
            .expect("the key parses so that it can be refused with a sentence")
    }

    #[test]
    fn a_removed_backend_is_refused_naming_the_section_and_the_doc() {
        for target in ["mysql", "sqlite", "sqlserver", "MySQL", "oracle"] {
            let err = project_targeting(Some(target))
                .validate_self_contained()
                .expect_err(target)
                .to_string();
            assert!(err.contains("[project] database_target"), "{target}: {err}");
            assert!(err.contains(&format!("{target:?}")), "{target}: {err}");
            assert!(err.contains("docs/database-compatibility.md"), "{target}: {err}");
        }
    }

    #[test]
    fn postgresql_and_an_absent_key_are_accepted() {
        project_targeting(Some("postgresql")).validate_self_contained().unwrap();
        project_targeting(Some("PostgreSQL")).validate_self_contained().unwrap();
        project_targeting(None).validate_self_contained().unwrap();
    }
}

/// `fraiseql.toml` keys that `fraiseql run` never applied. Each is refused when it is written,
/// naming what to do instead, so a project that relied on one learns it never worked.
mod refused_key_tests {
    use super::super::TomlSchema;

    /// Load `contents` as a `fraiseql.toml`, through the loader `run` and `compile` share.
    fn load(contents: &str) -> anyhow::Result<TomlSchema> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fraiseql.toml");
        std::fs::write(&path, contents).unwrap();
        TomlSchema::from_project_file(path.to_str().unwrap())
    }

    #[test]
    fn every_key_run_never_applied_is_refused_with_what_to_do_instead() {
        let cases = [
            (
                "[server]\nrequest_timeout_ms = 5_000\n",
                "[server] request_timeout_ms",
                "request_timeout_secs",
            ),
            (
                "[server]\nkeep_alive_secs = 30\n",
                "[server] keep_alive_secs",
                "no keep-alive setting",
            ),
            (
                "[server.cors]\ncredentials = true\n",
                "[server.cors] credentials",
                "cannot send the session cookie",
            ),
            (
                "[database]\nidle_timeout_ms = 60_000\n",
                "[database] idle_timeout_ms",
                "no idle timeout",
            ),
        ];
        for (toml, key, instead) in cases {
            let err = load(toml).err().unwrap_or_else(|| panic!("{key} was accepted"));
            let message = format!("{err:#}");
            assert!(message.contains(key), "{key}: {message}");
            assert!(message.contains(instead), "{key}: {message}");
        }
    }

    /// The server timeout lives in the server configuration file, so the refusal says where.
    #[test]
    fn the_request_timeout_refusal_names_the_server_configuration_file() {
        let err = load("[server]\nrequest_timeout_ms = 5_000\n").unwrap_err();
        assert!(format!("{err:#}").contains("FRAISEQL_CONFIG"), "{err:#}");
    }

    /// Whoever sets `credentials = true` wants cross-origin cookies; the refusal says why
    /// that cannot work and what does.
    #[test]
    fn the_credentials_refusal_names_the_cookie_alternatives() {
        let err = format!("{:#}", load("[server.cors]\ncredentials = true\n").unwrap_err());
        assert!(err.contains("same origin"), "{err}");
        assert!(err.contains("bearer token"), "{err}");
    }

    /// A key set to its old default is refused too: it never had an effect either way.
    #[test]
    fn a_refused_key_set_to_its_old_default_is_still_refused() {
        assert!(load("[server.cors]\ncredentials = false\n").is_err());
    }

    /// The four keys' sections still load without them.
    #[test]
    fn the_sections_load_without_the_refused_keys() {
        let config = load(
            "[server]\nport = 9000\n[server.cors]\norigins = [\"https://example.com\"]\n\
             [database]\npool_max = 5\n",
        )
        .unwrap();
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.database.pool_max, 5);
    }
}
