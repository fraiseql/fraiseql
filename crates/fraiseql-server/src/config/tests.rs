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

    /// #1153: the database errors a caller can provoke on demand are the ones the
    /// sanitizer could not reach.
    ///
    /// #413 maps SQLSTATE class 22 to `BAD_USER_INPUT` and class 23 to
    /// `CONSTRAINT_VIOLATION` so they answer 400 rather than 500 — correct for the
    /// status, and it moved them outside a gate that matched only
    /// `InternalServerError`/`DatabaseError`. The set the sanitizer rewrote became the
    /// complement of the set an attacker could trigger, while the compiled artefact
    /// advertised `sanitize_database_errors: true`.
    ///
    /// The messages here are real ones, reduced from a live report: they name an
    /// internal SQL function, the write-path mechanism, two table names the client never
    /// referenced, and a constraint identifier.
    #[test]
    fn a_client_triggerable_database_message_is_replaced() {
        let s = enabled_sanitizer();

        let fk = GraphQLError::new(
            "Database error: Function call app.delete_order (with change-log outbox) failed: \
             update or delete on table \"tb_order\" violates foreign key constraint \
             \"tb_order_line_fk_order_fkey\" on table \"tb_order_line\"",
            ErrorCode::ConstraintViolation,
        );
        let out = s.sanitize(fk);
        assert_eq!(out.message, "The request conflicts with the current state of the data");
        for leaked in ["tb_order", "app.delete_order", "fkey", "outbox"] {
            assert!(!out.message.contains(leaked), "still leaks {leaked}: {}", out.message);
        }

        let cast = GraphQLError::new(
            "Database error: Function call app.create_thing (with change-log outbox) failed: \
             invalid input syntax for type inet: \"not-an-ip\"",
            ErrorCode::BadUserInput,
        );
        let out = s.sanitize(cast);
        assert_eq!(out.message, "The request contains an invalid value");
        assert!(!out.message.contains("app.create_thing"), "{}", out.message);
    }

    /// A 400 must not be answered with a sentence claiming the server broke: the
    /// request was the fault, and a client told otherwise retries or escalates for
    /// nothing. The code still carries the actionable part.
    #[test]
    fn a_client_fault_is_not_described_as_an_internal_error() {
        let s = enabled_sanitizer();
        for code in [ErrorCode::BadUserInput, ErrorCode::ConstraintViolation] {
            let out = s.sanitize(GraphQLError::new("raw db text", code));
            // Both halves are load-bearing. Without the first, an *unsanitized* raw
            // message satisfies "does not contain 'internal'" and the test passes with
            // the defect present — which is exactly what it did before this line.
            assert_ne!(out.message, "raw db text", "{code:?} must be replaced at all");
            assert!(
                !out.message.contains("internal"),
                "{code:?} replacement must not claim an internal error: {}",
                out.message
            );
        }
        // …while a genuine server fault still says so.
        let out = s.sanitize(GraphQLError::new("raw db text", ErrorCode::DatabaseError));
        assert_eq!(out.message, "An internal error occurred");
    }

    /// An operator who set a custom message still gets it, on every sanitized class.
    #[test]
    fn a_custom_message_covers_the_client_fault_classes_too() {
        let s = ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            sanitize_database_errors: true,
            custom_error_message: Some("Contact support".to_string()),
            ..Default::default()
        });
        for code in [
            ErrorCode::BadUserInput,
            ErrorCode::ConstraintViolation,
            ErrorCode::DatabaseError,
            ErrorCode::InternalServerError,
        ] {
            assert_eq!(
                s.sanitize(GraphQLError::new("raw db text", code)).message,
                "Contact support",
                "{code:?}"
            );
        }
    }

    /// The opt-out still opts out, on the newly covered classes as well — otherwise
    /// this fix would silently override a deployment that asked for raw messages.
    #[test]
    fn sanitize_database_errors_false_still_passes_client_faults_through() {
        let s = ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            sanitize_database_errors: false,
            ..Default::default()
        });
        let raw = "Database error: violates foreign key constraint \"x_fkey\"";
        for code in [ErrorCode::BadUserInput, ErrorCode::ConstraintViolation] {
            assert_eq!(s.sanitize(GraphQLError::new(raw, code)).message, raw, "{code:?}");
        }
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
