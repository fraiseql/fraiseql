#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness
#![allow(clippy::field_reassign_with_default)] // Reason: test code builds structs incrementally for clarity

mod config_tests {
    use super::super::{security, *};

    #[test]
    fn test_default_config() {
        let config = TomlProjectConfig::default();
        assert_eq!(config.project.name, "my-fraiseql-app");
        assert_eq!(config.fraiseql.schema_file, "schema.json");
    }

    #[test]
    fn test_default_security_config() {
        let config = TomlProjectConfig::default();
        assert!(config.fraiseql.security.audit_logging.enabled);
        assert!(config.fraiseql.security.rate_limiting.enabled);
    }

    #[test]
    fn test_validation() {
        let config = TomlProjectConfig::default();
        config.validate().unwrap_or_else(|e| panic!("expected Ok from validate: {e:?}"));
    }

    #[test]
    fn test_role_definitions_default() {
        let config = TomlProjectConfig::default();
        assert!(config.fraiseql.security.role_definitions.is_empty());
        assert!(config.fraiseql.security.default_role.is_none());
    }

    #[test]
    fn test_naming_acronyms_default_empty() {
        let config = TomlProjectConfig::default();
        assert!(config.fraiseql.naming.acronyms.is_empty());
    }

    #[test]
    fn test_parse_naming_acronyms_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[fraiseql]
schema_file = "schema.json"

[fraiseql.naming]
acronyms = ["s3", "widget5"]
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.fraiseql.naming.acronyms, vec!["s3", "widget5"]);
    }

    #[test]
    fn test_cost_weights_default_empty() {
        let config = TomlProjectConfig::default();
        assert!(config.fraiseql.cost_weights.is_empty());
    }

    #[test]
    fn test_parse_cost_weights_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[fraiseql]
schema_file = "schema.json"

[fraiseql.cost_weights]
expensiveReport = 5000
searchUsers = 250
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.fraiseql.cost_weights.get("expensiveReport"), Some(&5000));
        assert_eq!(config.fraiseql.cost_weights.get("searchUsers"), Some(&250));
    }

    #[test]
    fn test_naming_convention_defaults_to_camel_case() {
        // Workflow-B compiles to a camelCase GraphQL surface by default, both when
        // [fraiseql.naming] is absent entirely and when it is present without a
        // `convention` key.
        let config = TomlProjectConfig::default();
        assert_eq!(config.fraiseql.naming.convention, NamingConvention::CamelCase);

        let toml_str = r#"
[project]
name = "test-app"

[fraiseql]
schema_file = "schema.json"

[fraiseql.naming]
acronyms = ["s3"]
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.fraiseql.naming.convention, NamingConvention::CamelCase);
    }

    #[test]
    fn test_parse_naming_convention_preserve_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[fraiseql]
schema_file = "schema.json"

[fraiseql.naming]
convention = "preserve"
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.fraiseql.naming.convention, NamingConvention::Preserve);
    }

    #[test]
    fn test_parse_naming_convention_camel_case_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[fraiseql]
schema_file = "schema.json"

[fraiseql.naming]
convention = "camelCase"
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.fraiseql.naming.convention, NamingConvention::CamelCase);
    }

    #[test]
    fn test_parse_role_definitions_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[fraiseql]
schema_file = "schema.json"

[[fraiseql.security.role_definitions]]
name = "viewer"
description = "Read-only access"
scopes = ["read:*"]

[[fraiseql.security.role_definitions]]
name = "admin"
description = "Full access"
scopes = ["admin:*"]

[fraiseql.security]
default_role = "viewer"
"#;

        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");

        assert_eq!(config.fraiseql.security.role_definitions.len(), 2);
        assert_eq!(config.fraiseql.security.role_definitions[0].name, "viewer");
        assert_eq!(config.fraiseql.security.role_definitions[0].scopes[0], "read:*");
        assert_eq!(config.fraiseql.security.role_definitions[1].name, "admin");
        assert_eq!(config.fraiseql.security.default_role, Some("viewer".to_string()));
    }

    #[test]
    fn test_security_config_validation_empty_role_name() {
        let toml_str = r#"
[[fraiseql.security.role_definitions]]
name = ""
scopes = ["read:*"]
"#;

        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty role name");
    }

    #[test]
    fn test_security_config_validation_empty_scopes() {
        let toml_str = r#"
[[fraiseql.security.role_definitions]]
name = "viewer"
scopes = []
"#;

        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty scopes");
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
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
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
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
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
            let config: TomlProjectConfig =
                toml::from_str(&expanded).expect("Failed to parse TOML");
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

[fraiseql]
schema_file = "schema.json"

[fraiseql.tenancy]
mode = "row"
tenant_claim = "tenant_id"
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(matches!(config.fraiseql.tenancy.mode, security::TenancyModeConfig::Row));
        assert_eq!(config.fraiseql.tenancy.tenant_claim, "tenant_id");
    }

    #[test]
    fn test_parse_tenancy_schema_mode_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[fraiseql.tenancy]
mode = "schema"
tenant_claim = "org_id"
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(matches!(config.fraiseql.tenancy.mode, security::TenancyModeConfig::Schema));
        assert_eq!(config.fraiseql.tenancy.tenant_claim, "org_id");
    }

    #[test]
    fn test_parse_tenancy_defaults_when_absent() {
        let toml_str = r#"
[project]
name = "test-app"
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(matches!(config.fraiseql.tenancy.mode, security::TenancyModeConfig::None));
        assert_eq!(config.fraiseql.tenancy.tenant_claim, "tenant_id");
    }

    #[test]
    fn test_parse_tenancy_invalid_mode_rejected() {
        let toml_str = r#"
[fraiseql.tenancy]
mode = "invalid"
"#;
        let result: Result<TomlProjectConfig, _> = toml::from_str(toml_str);
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
        assert_eq!(cfg.request_timeout_ms, 30_000);
        assert_eq!(cfg.keep_alive_secs, 75);
        assert!(!cfg.tls.enabled);
        assert!(cfg.cors.origins.is_empty());
        assert!(!cfg.cors.credentials);
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
host               = "127.0.0.1"
port               = 9000
request_timeout_ms = 60_000

[cors]
origins     = ["https://example.com"]
credentials = true

[tls]
enabled = false
"#;
        let cfg: ServerRuntimeConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.request_timeout_ms, 60_000);
        assert_eq!(cfg.cors.origins, ["https://example.com"]);
        assert!(cfg.cors.credentials);
        assert!(!cfg.tls.enabled);
    }

    // ── DatabaseRuntimeConfig ────────────────────────────────────────────────

    #[test]
    fn test_database_runtime_config_default() {
        let cfg = DatabaseRuntimeConfig::default();
        assert!(cfg.url.is_none());
        assert_eq!(cfg.pool_min, 2);
        assert_eq!(cfg.pool_max, 20);
        assert_eq!(cfg.connect_timeout_ms, 5_000);
        assert_eq!(cfg.idle_timeout_ms, 600_000);
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
    use super::super::security::*;

    /// #1151 — a project that never mentions `[fraiseql.security.state_encryption]` must not
    /// compile an artifact declaring it enabled.
    ///
    /// This struct's `state_encryption` field is **not** an `Option` and the struct carries
    /// `#[serde(default)]`, so an absent table becomes `StateEncryptionConfig::default()`. When
    /// that default was `enabled: true`, `to_json()` lowered `enabled: true` into the compiled
    /// artifact, and the server refused to boot:
    ///
    /// ```text
    /// Error: Configuration error: state_encryption enabled but key env var
    /// 'STATE_ENCRYPTION_KEY' failed: env var STATE_ENCRYPTION_KEY not set
    /// ```
    ///
    /// The value had been `true` since before 2.14, but the compiled key was emitted as
    /// `stateEncryption` while the runtime read `state_encryption`, so nothing ever read it.
    /// Fixing that casing (#893/#977/#983) turned a silent no-op into a hard boot failure — so
    /// the assertion that matters is on the **lowered artifact**, not just the struct field.
    #[test]
    fn state_encryption_is_off_unless_the_operator_asks_for_it() {
        let lowered = SecurityConfig::default().to_json();
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

    /// #1151 — the two structs named `StateEncryptionConfig` must agree on the default.
    ///
    /// `fraiseql-core`'s is the documented contract ("off unless asked for"); this crate's is the
    /// one that reaches the artifact. They disagreed silently, which is what made the divergence
    /// survive: each looked correct on its own.
    #[test]
    fn state_encryption_default_agrees_with_fraiseql_core() {
        assert_eq!(
            StateEncryptionConfig::default().enabled,
            fraiseql_core::schema::StateEncryptionConfig::default().enabled,
            "fraiseql-cli and fraiseql-core disagree on the state_encryption default; the cli's \
             is the one compiled into the artifact (#1151)"
        );
    }

    #[test]
    fn test_default_security_config() {
        let config = SecurityConfig::default();
        assert!(config.audit_logging.enabled);
        assert!(config.error_sanitization.enabled);
        assert!(config.rate_limiting.enabled);
        assert!(config.constant_time.enabled);
        // state_encryption is deliberately NOT in the on-by-default list. This test used to
        // assert it was, which is how the divergence from fraiseql-core survived review: the
        // wrong default had a test pinning it in place (#1151). The controls above are inert
        // without further configuration; state encryption is not — enabling it demands a key,
        // and demanding a key the operator never configured stops the server from booting.
        assert!(
            !config.state_encryption.enabled,
            "state encryption must stay off until asked for — see #1151"
        );
    }

    /// #983 — the four `[fraiseql.security.error_sanitization]` keys that reached no
    /// runtime shape are gone, so `deny_unknown_fields` refuses them by name. The
    /// old test asserted that `leak_sensitive_details = true` was rejected as "a
    /// security risk"; it switched nothing either way, which is the more alarming
    /// half of that sentence.
    #[test]
    fn error_sanitization_refuses_the_keys_that_never_had_a_consumer() {
        ErrorSanitizationConfig::default()
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for default config: {e}"));

        for key in [
            "generic_messages = true",
            "internal_logging = true",
            "leak_sensitive_details = true",
            "user_facing_format = \"detailed\"",
        ] {
            let toml = format!("enabled = true\n{key}\n");
            let err = toml::from_str::<ErrorSanitizationConfig>(&toml)
                .expect_err("a key with no consumer must not parse");
            assert!(
                err.to_string().contains("unknown field"),
                "the refusal must name the key; got: {err}"
            );
        }
    }

    #[test]
    fn test_rate_limiting_validation() {
        let mut config = RateLimitConfig::default();
        config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for default config: {e}"));

        config.auth_start_window_secs = 0;
        assert!(config.validate().is_err(), "expected Err when auth_start_window_secs=0, got Ok");
    }

    #[test]
    fn test_rate_limiting_zero_max_requests_rejected() {
        let mut config = RateLimitConfig::default();
        config.auth_start_max_requests = 0;
        let err = config.validate().unwrap_err();
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
        let mut config = RateLimitConfig::default();
        config.auth_start_max_requests = 1;
        config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for max_requests=1: {e}"));
    }

    #[test]
    fn test_rate_limiting_callback_zero_max_requests_rejected() {
        let mut config = RateLimitConfig::default();
        config.auth_callback_max_requests = 0;
        assert!(
            config.validate().is_err(),
            "expected Err when auth_callback_max_requests=0, got Ok"
        );
    }

    /// #983 — nonce and key sizes are fixed by the algorithm, so there was nothing
    /// for these keys to size. Refused by name rather than validated into a range
    /// that had no effect.
    #[test]
    fn state_encryption_refuses_the_keys_that_never_had_a_consumer() {
        StateEncryptionConfig::default()
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for default config: {e}"));

        for key in [
            "key_rotation_enabled = true",
            "nonce_size = 12",
            "key_size = 32",
        ] {
            let toml = format!("enabled = true\n{key}\n");
            let err = toml::from_str::<StateEncryptionConfig>(&toml)
                .expect_err("a key with no consumer must not parse");
            assert!(
                err.to_string().contains("unknown field"),
                "the refusal must name the key; got: {err}"
            );
        }
    }

    #[test]
    fn test_state_encryption_unsupported_algorithm_rejected() {
        let mut config = StateEncryptionConfig::default();
        config.algorithm = "rot13".to_string();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("rot13"), "error should name the bad algorithm: {err}");
        assert!(
            err.to_string().contains("chacha20-poly1305"),
            "error should list supported algorithms: {err}"
        );
    }

    #[test]
    fn test_state_encryption_aes_256_gcm_accepted() {
        let mut config = StateEncryptionConfig::default();
        config.algorithm = "aes-256-gcm".to_string();
        config.validate().unwrap_or_else(|e| panic!("expected Ok for aes-256-gcm: {e}"));
    }

    #[test]
    fn test_state_encryption_chacha20_poly1305_accepted() {
        let config = StateEncryptionConfig::default();
        // default is "chacha20-poly1305"
        config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for default chacha20-poly1305: {e}"));
    }

    #[test]
    fn test_security_config_serialization() {
        let config = SecurityConfig::default();
        let json = config.to_json();
        // `audit_logging` lowers onto the key the runtime reads (#977).
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
    fn test_tenancy_to_json_row_mode() {
        let config = TenancyTomlConfig {
            mode:         TenancyModeConfig::Row,
            tenant_claim: "tenant_id".to_string(),
        };
        let json = config.to_json();
        assert_eq!(json["mode"], "row");
        // `tenant_claim`, the name the runtime `TenancyConfig` declares (#757).
        assert_eq!(json["tenant_claim"], "tenant_id");
    }

    #[test]
    fn test_tenancy_to_json_schema_mode() {
        let config = TenancyTomlConfig {
            mode:         TenancyModeConfig::Schema,
            tenant_claim: "org_id".to_string(),
        };
        let json = config.to_json();
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

    #[test]
    fn test_tenancy_validate_empty_claim_with_none_ok() {
        let config = TenancyTomlConfig {
            mode:         TenancyModeConfig::None,
            tenant_claim: String::new(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tenancy_mode_invalid_variant_rejected() {
        let result: Result<TenancyModeConfig, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
    }
}
