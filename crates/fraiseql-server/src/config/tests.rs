#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code

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
    #[allow(clippy::wildcard_imports)]
    // Reason: test module — wildcard import keeps test boilerplate minimal
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
