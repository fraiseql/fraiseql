#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::time::Duration;

use fraiseql_error::ConfigError;

use super::*;
use crate::config::{
    env::{parse_duration, parse_size, resolve_env_value},
    validation::ConfigValidator,
};

#[test]
fn test_parse_minimal_config() {
    temp_env::with_vars([("DATABASE_URL", Some("postgres://localhost/test"))], || {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = "DATABASE_URL"
        "#;

        let config: RuntimeConfig = toml::from_str(toml).unwrap();

        assert_eq!(config.server.port, 4000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.database.url_env, "DATABASE_URL");
        assert_eq!(config.database.pool_size, 10);
    });
}

#[test]
fn test_parse_size() {
    assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
    assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
    assert_eq!(parse_size("500KB").unwrap(), 500 * 1024);
    assert_eq!(parse_size("1000").unwrap(), 1000);
    assert_eq!(parse_size("100B").unwrap(), 100);
}

#[test]
fn test_parse_size_invalid() {
    assert!(
        parse_size("abc").is_err(),
        "expected Err for invalid size string, got: {:?}",
        parse_size("abc")
    );
}

#[test]
fn test_parse_duration() {
    assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    assert_eq!(parse_duration("100ms").unwrap(), Duration::from_millis(100));
    assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
}

#[test]
fn test_parse_duration_invalid() {
    assert!(
        parse_duration("30").is_err(),
        "expected Err for duration without unit, got: {:?}",
        parse_duration("30")
    ); // Missing unit
    assert!(
        parse_duration("abc").is_err(),
        "expected Err for non-numeric duration, got: {:?}",
        parse_duration("abc")
    );
}

#[test]
fn test_env_resolution_with_default() {
    temp_env::with_vars([("NONEXISTENT_VAR", None::<&str>)], || {
        let result = resolve_env_value("${NONEXISTENT_VAR:-default_value}").unwrap();
        assert_eq!(result, "default_value");
    });
}

#[test]
fn test_env_resolution_without_default() {
    temp_env::with_vars([("EXISTING_VAR", Some("actual_value"))], || {
        let result = resolve_env_value("${EXISTING_VAR:-default}").unwrap();
        assert_eq!(result, "actual_value");
    });
}

#[test]
fn test_validation_missing_env_var() {
    temp_env::with_vars([("NONEXISTENT_DB_URL", None::<&str>)], || {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = "NONEXISTENT_DB_URL"
        "#;

        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();

        assert!(result.is_err(), "expected Err for missing env var but got no errors");
        assert!(result.errors.iter().any(|e| matches!(e, ConfigError::MissingEnvVar { .. })));
    });
}

#[test]
fn test_validation_cross_field() {
    temp_env::with_vars([("DATABASE_URL", Some("postgres://localhost/test"))], || {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = "DATABASE_URL"

            [observers.test]
            entity = "users"
            events = ["insert"]

            [[observers.test.actions]]
            type = "email"
            template = "welcome"
        "#;

        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();

        // Should fail because email action requires notifications config
        assert!(
            result.is_err(),
            "expected Err because email action requires notifications config but got no errors"
        );
    });
}

// ── env_tests ─────────────────────────────────────────────────────────────────

mod env_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::super::env::*;
    use std::time::Duration;

    // ─── resolve_env_value ──────────────────────────────────────────────────

    #[test]
    fn literal_value_returned_unchanged() {
        assert_eq!(resolve_env_value("hello").unwrap(), "hello");
    }

    #[test]
    fn env_var_reference_resolves() {
        temp_env::with_var("FRAISEQL_TEST_ENV_RS", Some("resolved"), || {
            assert_eq!(resolve_env_value("${FRAISEQL_TEST_ENV_RS}").unwrap(), "resolved");
        });
    }

    #[test]
    fn missing_env_var_returns_error() {
        temp_env::with_var("FRAISEQL_MISSING_VAR", None::<&str>, || {
            let err = resolve_env_value("${FRAISEQL_MISSING_VAR}").unwrap_err();
            assert!(
                matches!(err, EnvError::MissingVar { ref name } if name == "FRAISEQL_MISSING_VAR"),
                "expected MissingVar, got: {err:?}"
            );
        });
    }

    #[test]
    fn default_syntax_uses_fallback_when_absent() {
        temp_env::with_var("FRAISEQL_ABSENT", None::<&str>, || {
            assert_eq!(resolve_env_value("${FRAISEQL_ABSENT:-fallback}").unwrap(), "fallback");
        });
    }

    #[test]
    fn default_syntax_uses_real_value_when_present() {
        temp_env::with_var("FRAISEQL_PRESENT", Some("real"), || {
            assert_eq!(resolve_env_value("${FRAISEQL_PRESENT:-fallback}").unwrap(), "real");
        });
    }

    #[test]
    fn required_with_message_syntax_errors_with_message() {
        temp_env::with_var("FRAISEQL_REQUIRED", None::<&str>, || {
            let err = resolve_env_value("${FRAISEQL_REQUIRED:?must be set}").unwrap_err();
            assert!(
                matches!(
                    err,
                    EnvError::MissingVarWithMessage { ref name, ref message }
                    if name == "FRAISEQL_REQUIRED" && message == "must be set"
                ),
                "expected MissingVarWithMessage, got: {err:?}"
            );
        });
    }

    #[test]
    fn required_with_message_syntax_resolves_when_present() {
        temp_env::with_var("FRAISEQL_REQUIRED_OK", Some("value"), || {
            assert_eq!(resolve_env_value("${FRAISEQL_REQUIRED_OK:?must be set}").unwrap(), "value");
        });
    }

    // ─── get_env_value ──────────────────────────────────────────────────────

    #[test]
    fn get_env_value_returns_value_when_set() {
        temp_env::with_var("FRAISEQL_GET_TEST", Some("got_it"), || {
            assert_eq!(get_env_value("FRAISEQL_GET_TEST").unwrap(), "got_it");
        });
    }

    #[test]
    fn get_env_value_returns_error_when_missing() {
        temp_env::with_var("FRAISEQL_GET_MISSING", None::<&str>, || {
            assert!(get_env_value("FRAISEQL_GET_MISSING").is_err());
        });
    }

    // ─── parse_size edge cases ──────────────────────────────────────────────

    #[test]
    fn parse_size_overflow_returns_error() {
        // usize::MAX GB would overflow
        let result = parse_size(&format!("{}GB", usize::MAX));
        assert!(result.is_err(), "overflow must return Err");
    }

    #[test]
    fn parse_size_whitespace_trimmed() {
        assert_eq!(parse_size("  10MB  ").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_case_insensitive() {
        assert_eq!(parse_size("10mb").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("10Mb").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_zero_is_valid() {
        assert_eq!(parse_size("0MB").unwrap(), 0);
    }

    // ─── parse_duration edge cases ──────────────────────────────────────────

    #[test]
    fn parse_duration_zero_is_valid() {
        assert_eq!(parse_duration("0s").unwrap(), Duration::from_secs(0));
    }

    #[test]
    fn parse_duration_whitespace_trimmed() {
        assert_eq!(parse_duration("  30s  ").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn parse_duration_missing_unit_returns_error() {
        let err = parse_duration("42").unwrap_err();
        assert!(matches!(err, ParseError::InvalidDuration { .. }));
    }

    #[test]
    fn parse_duration_non_numeric_returns_error() {
        let err = parse_duration("xyzs").unwrap_err();
        assert!(matches!(err, ParseError::InvalidDuration { .. }));
    }
}

// ── error_sanitization_tests ──────────────────────────────────────────────────

mod error_sanitization_tests {
    use super::super::error_sanitization::*;
    use crate::error::{ErrorCode, ErrorExtensions, GraphQLError};

    fn enabled_sanitizer() -> ErrorSanitizer {
        ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled:                     true,
            hide_implementation_details: true,
            sanitize_database_errors:    true,
            custom_error_message:        None,
        })
    }

    fn disabled_sanitizer() -> ErrorSanitizer {
        ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: false,
            ..ErrorSanitizationConfig::default()
        })
    }

    #[test]
    fn test_sanitizer_strips_db_error_when_enabled() {
        let s = enabled_sanitizer();
        let err = GraphQLError::database(r#"ERROR: relation "tb_users" does not exist"#);
        let out = s.sanitize(err);
        assert_eq!(out.message, "An internal error occurred");
    }

    #[test]
    fn test_sanitizer_passes_through_when_disabled() {
        let s = disabled_sanitizer();
        let original = r#"ERROR: relation "tb_users" does not exist"#;
        let err = GraphQLError::database(original);
        let out = s.sanitize(err);
        assert_eq!(out.message, original);
    }

    #[test]
    fn test_sanitizer_preserves_user_facing_errors() {
        let s = enabled_sanitizer();
        let cases = [
            (ErrorCode::ValidationError, "field is required"),
            (ErrorCode::Unauthenticated, "Authentication required"),
            (ErrorCode::Forbidden, "Access denied"),
            (ErrorCode::NotFound, "Resource not found"),
        ];
        for (code, msg) in cases {
            let err = GraphQLError::new(msg, code);
            let out = s.sanitize(err);
            assert_eq!(out.message, msg, "code {code:?} should not be sanitized");
        }
    }

    #[test]
    fn test_sanitizer_custom_message() {
        let s = ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            custom_error_message: Some("Contact support".to_string()),
            ..ErrorSanitizationConfig::default()
        });
        let err = GraphQLError::database("pg error detail");
        assert_eq!(s.sanitize(err).message, "Contact support");
    }

    #[test]
    fn test_sanitizer_strips_extensions_detail_when_hide_impl() {
        let s = enabled_sanitizer();
        let mut err = GraphQLError::internal("internal");
        err.extensions = Some(ErrorExtensions {
            category:         None,
            status:           None,
            request_id:       None,
            retry_after_secs: None,
            detail:           Some("panic at line 42".to_string()),
        });
        let out = s.sanitize(err);
        assert!(
            out.extensions.as_ref().and_then(|e| e.detail.as_ref()).is_none(),
            "detail should be stripped when hide_implementation_details = true"
        );
    }

    #[test]
    fn test_sanitize_database_errors_false_allows_db_message_through() {
        let s = ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            sanitize_database_errors: false,
            ..ErrorSanitizationConfig::default()
        });
        let err = GraphQLError::database("duplicate key value");
        assert_eq!(s.sanitize(err).message, "duplicate key value");
    }
}

// ── pool_tuning_tests ─────────────────────────────────────────────────────────

mod pool_tuning_tests {
    #[allow(clippy::wildcard_imports)] // Reason: test module — wildcard import keeps test boilerplate minimal
    use super::super::pool_tuning::*;

    #[test]
    fn test_default_config_is_disabled() {
        let cfg = PoolPressureMonitorConfig::default();
        assert!(!cfg.enabled, "pool pressure monitoring should be off by default");
    }

    #[test]
    fn test_default_bounds_are_sensible() {
        let cfg = PoolPressureMonitorConfig::default();
        assert!(cfg.min_pool_size < cfg.max_pool_size);
        assert!(cfg.scale_up_step > 0);
        assert!(cfg.scale_down_step > 0);
        assert!(cfg.tuning_interval_ms >= 1000);
    }

    #[test]
    fn test_validate_passes_for_defaults() {
        PoolPressureMonitorConfig::default()
            .validate()
            .unwrap_or_else(|e| panic!("default pool monitor config should pass validation: {e}"));
    }

    #[test]
    fn test_validate_min_lt_max() {
        let cfg = PoolPressureMonitorConfig {
            min_pool_size: 10,
            max_pool_size: 5,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "min >= max should be invalid, got: {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn test_validate_min_equals_max_is_invalid() {
        let cfg = PoolPressureMonitorConfig {
            min_pool_size: 10,
            max_pool_size: 10,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "min == max should be invalid, got: {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn test_validate_idle_ratio_above_one() {
        let cfg = PoolPressureMonitorConfig {
            scale_down_idle_ratio: 1.5,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "idle ratio > 1.0 should be invalid, got: {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn test_validate_idle_ratio_negative() {
        let cfg = PoolPressureMonitorConfig {
            scale_down_idle_ratio: -0.1,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "idle ratio < 0.0 should be invalid, got: {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn test_validate_zero_scale_up_step() {
        let cfg = PoolPressureMonitorConfig {
            scale_up_step: 0,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "scale_up_step == 0 should be invalid, got: {:?}",
            cfg.validate()
        );
    }

    #[test]
    fn test_validate_zero_scale_down_step() {
        let cfg = PoolPressureMonitorConfig {
            scale_down_step: 0,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "scale_down_step == 0 should be invalid, got: {:?}",
            cfg.validate()
        );
    }

    #[test]
    #[allow(deprecated)] // Reason: re-exporting deprecated alias for backward compatibility
    fn test_pool_tuning_config_alias_works() {
        // PoolTuningConfig is a deprecated alias for PoolPressureMonitorConfig
        let _cfg: PoolTuningConfig = PoolTuningConfig::default();
    }

    #[test]
    fn test_validate_interval_too_short() {
        let cfg = PoolPressureMonitorConfig {
            tuning_interval_ms: 50,
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "tuning_interval_ms < 100 should be invalid, got: {:?}",
            cfg.validate()
        );
    }
}

// ── validation_tests ──────────────────────────────────────────────────────────

mod validation_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use fraiseql_error::ConfigError;

    use super::super::validation::*;
    use crate::config::RuntimeConfig;

    // ─── ValidationResult ───────────────────────────────────────────────────

    #[test]
    fn empty_result_is_ok() {
        let result = ValidationResult::new();
        assert!(result.is_ok());
        assert!(!result.is_err());
    }

    #[test]
    fn result_with_error_is_err() {
        let mut result = ValidationResult::new();
        result.add_error(ConfigError::ValidationError {
            field:   "test".into(),
            message: "bad".into(),
        });
        assert!(result.is_err());
        assert!(!result.is_ok());
    }

    #[test]
    fn result_with_only_warnings_is_ok() {
        let mut result = ValidationResult::new();
        result.add_warning("heads up");
        assert!(result.is_ok());
    }

    #[test]
    fn into_result_single_error() {
        let mut result = ValidationResult::new();
        result.add_error(ConfigError::ValidationError {
            field:   "port".into(),
            message: "invalid".into(),
        });
        let err = result.into_result().unwrap_err();
        assert!(
            matches!(err, ConfigError::ValidationError { ref field, .. } if field == "port"),
            "single error must be unwrapped, not wrapped in MultipleErrors"
        );
    }

    #[test]
    fn into_result_multiple_errors() {
        let mut result = ValidationResult::new();
        result.add_error(ConfigError::ValidationError {
            field:   "a".into(),
            message: "bad a".into(),
        });
        result.add_error(ConfigError::ValidationError {
            field:   "b".into(),
            message: "bad b".into(),
        });
        let err = result.into_result().unwrap_err();
        assert!(
            matches!(err, ConfigError::MultipleErrors { ref errors } if errors.len() == 2),
            "multiple errors must be wrapped in MultipleErrors"
        );
    }

    #[test]
    fn into_result_ok_returns_warnings() {
        let mut result = ValidationResult::new();
        result.add_warning("warn1");
        result.add_warning("warn2");
        let warnings = result.into_result().unwrap();
        assert_eq!(warnings.len(), 2);
    }

    // ─── ConfigValidator — server validation ────────────────────────────────

    /// Minimal valid TOML for constructing a `RuntimeConfig`.
    fn minimal_config(toml_override: &str) -> RuntimeConfig {
        let toml = format!(
            r#"
            [server]
            port = 4000

            [database]
            url_env = "DATABASE_URL"

            {toml_override}
            "#
        );
        toml::from_str(&toml).unwrap()
    }

    #[test]
    fn valid_minimal_config_passes_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let config = minimal_config("");
            let result = ConfigValidator::new(&config).validate();
            assert!(result.is_ok(), "valid minimal config must pass: {:?}", result.errors);
        });
    }

    #[test]
    fn port_zero_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 0

                [database]
                url_env = "DATABASE_URL"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(result.is_err(), "port=0 must fail validation");
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field.contains("port"))
                }),
                "error must reference port field"
            );
        });
    }

    #[test]
    fn pool_size_zero_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"
                pool_size = 0
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(result.is_err(), "pool_size=0 must fail validation");
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field.contains("pool_size"))
                }),
                "error must reference pool_size field"
            );
        });
    }

    #[test]
    fn empty_database_url_env_fails_validation() {
        let toml = r#"
            [server]
            port = 4000

            [database]
            url_env = ""
        "#;
        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();
        assert!(result.is_err(), "empty url_env must fail validation");
    }

    #[test]
    fn placeholder_section_notifications_fails() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"

                [notifications]
                enabled = true
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field == "notifications")
                }),
                "placeholder 'notifications' section must be rejected"
            );
        });
    }

    #[test]
    fn placeholder_section_logging_fails() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"

                [logging]
                level = "debug"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. } if field == "logging")
                }),
                "placeholder 'logging' section must be rejected"
            );
        });
    }

    #[test]
    fn invalid_max_request_size_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [server.limits]
                max_request_size = "not-a-size"

                [database]
                url_env = "DATABASE_URL"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. }
                        if field.contains("max_request_size"))
                }),
                "invalid max_request_size must fail validation"
            );
        });
    }

    #[test]
    fn zero_max_concurrent_requests_fails_validation() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [server.limits]
                max_concurrent_requests = 0

                [database]
                url_env = "DATABASE_URL"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            assert!(
                result.errors.iter().any(|e| {
                    matches!(e, ConfigError::ValidationError { ref field, .. }
                        if field.contains("max_concurrent_requests"))
                }),
                "max_concurrent_requests=0 must fail validation"
            );
        });
    }

    #[test]
    fn redis_rate_limiting_without_cache_error_references_fraiseql_toml() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let toml = r#"
                [server]
                port = 4000

                [database]
                url_env = "DATABASE_URL"

                [rate_limiting]
                default = "100/minute"
                backend = "redis"
            "#;
            let config: RuntimeConfig = toml::from_str(toml).unwrap();
            let result = ConfigValidator::new(&config).validate();
            let has_toml_ref = result.errors.iter().any(|e| {
                matches!(e, ConfigError::ValidationError { ref message, .. }
                    if message.contains("fraiseql.toml"))
            });
            assert!(
                has_toml_ref,
                "error message must reference fraiseql.toml; errors: {:?}",
                result.errors
            );
        });
    }

    #[test]
    fn multiple_errors_collected_in_one_pass() {
        let toml = r#"
            [server]
            port = 0

            [database]
            url_env = ""
            pool_size = 0
        "#;
        let config: RuntimeConfig = toml::from_str(toml).unwrap();
        let result = ConfigValidator::new(&config).validate();
        assert!(
            result.errors.len() >= 3,
            "validator must collect all errors in one pass, got {} errors: {:?}",
            result.errors.len(),
            result.errors
        );
    }
}
