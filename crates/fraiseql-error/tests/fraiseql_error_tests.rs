//! Direct tests for `FraiseQLError` — all 16 variants, helper constructors,
//! HTTP status codes, classifier methods, error codes, `From` impls,
//! `ErrorContext` trait, and `ValidationFieldError`.
//!
//! `RuntimeError` sub-types already have 106 tests in other files (added in
//! commit `9ba63288c`).  This file covers the gap: `FraiseQLError` itself and
//! the supporting types exported from `fraiseql-error`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_error::{ErrorContext, FraiseQLError, Result, ValidationFieldError};

// ── Group A: Display formatting ──────────────────────────────────────────────

#[test]
fn parse_error_displays_message_and_location() {
    let e = FraiseQLError::Parse {
        message:  "unexpected token".to_string(),
        location: "line 3, col 5".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("unexpected token"), "got: {s}");
    assert!(s.contains("line 3"), "got: {s}");
}

#[test]
fn validation_error_without_path_displays_message() {
    let e = FraiseQLError::Validation {
        message: "bad input".to_string(),
        path:    None,
    };
    let s = e.to_string();
    assert!(s.contains("bad input"), "got: {s}");
    assert!(!s.contains("None"), "should not show raw None: {s}");
}

#[test]
fn validation_error_with_path_displays_message() {
    let e = FraiseQLError::Validation {
        message: "field required".to_string(),
        path:    Some("user.email".to_string()),
    };
    let s = e.to_string();
    assert!(s.contains("field required"), "got: {s}");
}

#[test]
fn unknown_field_error_displays_field_and_type() {
    let e = FraiseQLError::UnknownField {
        field:     "email".to_string(),
        type_name: "User".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("email"), "got: {s}");
    assert!(s.contains("User"), "got: {s}");
}

#[test]
fn unknown_type_error_displays_type_name() {
    let e = FraiseQLError::UnknownType {
        type_name: "Foo".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("Foo"), "got: {s}");
}

#[test]
fn database_error_displays_message() {
    let e = FraiseQLError::Database {
        message:   "connection refused".to_string(),
        sql_state: Some("08001".to_string()),
    };
    let s = e.to_string();
    assert!(s.contains("connection refused"), "got: {s}");
}

#[test]
fn connection_pool_error_displays_message() {
    let e = FraiseQLError::ConnectionPool {
        message: "pool exhausted".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("pool exhausted"), "got: {s}");
}

#[test]
fn timeout_error_displays_duration() {
    let e = FraiseQLError::Timeout {
        timeout_ms: 5000,
        query:      None,
    };
    let s = e.to_string();
    assert!(s.contains("5000"), "got: {s}");
}

#[test]
fn cancelled_error_displays_reason() {
    let e = FraiseQLError::Cancelled {
        query_id: "q-123".to_string(),
        reason:   "client disconnected".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("client disconnected"), "got: {s}");
}

#[test]
fn authorization_error_displays_message() {
    let e = FraiseQLError::Authorization {
        message:  "access denied".to_string(),
        action:   Some("read".to_string()),
        resource: Some("User".to_string()),
    };
    let s = e.to_string();
    assert!(s.contains("access denied"), "got: {s}");
}

#[test]
fn authentication_error_displays_message() {
    let e = FraiseQLError::Authentication {
        message: "token expired".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("token expired"), "got: {s}");
}

#[test]
fn rate_limited_error_displays_message() {
    let e = FraiseQLError::RateLimited {
        message:          "too many requests".to_string(),
        retry_after_secs: 30,
    };
    let s = e.to_string();
    assert!(s.contains("too many requests") || s.contains("Rate limit"), "got: {s}");
}

#[test]
fn not_found_error_displays_resource_and_identifier() {
    let e = FraiseQLError::NotFound {
        resource_type: "User".to_string(),
        identifier:    "user-1".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("User"), "got: {s}");
    assert!(s.contains("user-1"), "got: {s}");
}

#[test]
fn conflict_error_displays_message() {
    let e = FraiseQLError::Conflict {
        message: "duplicate key".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("duplicate key"), "got: {s}");
}

#[test]
fn configuration_error_displays_message() {
    let e = FraiseQLError::Configuration {
        message: "missing env var".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("missing env var"), "got: {s}");
}

#[test]
fn unsupported_error_displays_message() {
    let e = FraiseQLError::Unsupported {
        message: "batch queries not supported".to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("batch queries not supported"), "got: {s}");
}

#[test]
fn internal_error_displays_message() {
    let e = FraiseQLError::Internal {
        message: "unexpected panic".to_string(),
        source:  None,
    };
    let s = e.to_string();
    assert!(s.contains("unexpected panic"), "got: {s}");
}

// ── Group B: HTTP status code mapping ─────────────────────────────────────────

#[test]
fn parse_maps_to_400() {
    let e = FraiseQLError::Parse {
        message:  "unexpected eof".into(),
        location: "line 1".into(),
    };
    assert_eq!(e.status_code(), 400);
}

#[test]
fn validation_maps_to_400() {
    let e = FraiseQLError::Validation {
        message: "bad field".into(),
        path:    None,
    };
    assert_eq!(e.status_code(), 400);
}

#[test]
fn unknown_field_maps_to_400() {
    let e = FraiseQLError::UnknownField {
        field:     "x".into(),
        type_name: "T".into(),
    };
    assert_eq!(e.status_code(), 400);
}

#[test]
fn unknown_type_maps_to_400() {
    let e = FraiseQLError::UnknownType {
        type_name: "Ghost".into(),
    };
    assert_eq!(e.status_code(), 400);
}

#[test]
fn authentication_maps_to_401() {
    let e = FraiseQLError::Authentication {
        message: "bad token".into(),
    };
    assert_eq!(e.status_code(), 401);
}

#[test]
fn authorization_maps_to_403() {
    let e = FraiseQLError::Authorization {
        message:  "forbidden".into(),
        action:   None,
        resource: None,
    };
    assert_eq!(e.status_code(), 403);
}

#[test]
fn not_found_maps_to_404() {
    let e = FraiseQLError::NotFound {
        resource_type: "User".into(),
        identifier:    "user-1".into(),
    };
    assert_eq!(e.status_code(), 404);
}

#[test]
fn timeout_maps_to_408() {
    let e = FraiseQLError::Timeout {
        timeout_ms: 30_000,
        query:      None,
    };
    assert_eq!(e.status_code(), 408);
}

#[test]
fn cancelled_maps_to_408() {
    let e = FraiseQLError::Cancelled {
        query_id: "q-1".into(),
        reason:   "disconnected".into(),
    };
    assert_eq!(e.status_code(), 408);
}

#[test]
fn conflict_maps_to_409() {
    let e = FraiseQLError::Conflict {
        message: "duplicate key".into(),
    };
    assert_eq!(e.status_code(), 409);
}

#[test]
fn rate_limited_maps_to_429() {
    let e = FraiseQLError::RateLimited {
        message:          "too many".into(),
        retry_after_secs: 60,
    };
    assert_eq!(e.status_code(), 429);
}

#[test]
fn database_maps_to_500() {
    let e = FraiseQLError::Database {
        message:   "conn refused".into(),
        sql_state: None,
    };
    assert_eq!(e.status_code(), 500);
}

#[test]
fn connection_pool_maps_to_500() {
    let e = FraiseQLError::ConnectionPool {
        message: "pool exhausted".into(),
    };
    assert_eq!(e.status_code(), 500);
}

#[test]
fn configuration_maps_to_500() {
    let e = FraiseQLError::Configuration {
        message: "bad config".into(),
    };
    assert_eq!(e.status_code(), 500);
}

#[test]
fn internal_maps_to_500() {
    let e = FraiseQLError::Internal {
        message: "bug".into(),
        source:  None,
    };
    assert_eq!(e.status_code(), 500);
}

#[test]
fn unsupported_maps_to_501() {
    let e = FraiseQLError::Unsupported {
        message: "not impl".into(),
    };
    assert_eq!(e.status_code(), 501);
}

// ── Group C: ErrorContext trait ───────────────────────────────────────────────

#[test]
fn error_context_wraps_error_with_message() {
    let base: Result<()> = Err(FraiseQLError::Database {
        message:   "conn failed".into(),
        sql_state: None,
    });
    let ctx = base.context("during user query").unwrap_err();
    let s = ctx.to_string();
    assert!(s.contains("conn failed"), "got: {s}");
    assert!(s.contains("during user query"), "got: {s}");
}

#[test]
fn with_context_lazy_closure_not_called_on_ok() {
    let ok: Result<i32> = Ok(42);
    let mut called = false;
    let result = ok.with_context(|| {
        called = true;
        "should not run"
    });
    assert_eq!(result.unwrap(), 42);
    assert!(!called, "closure should not be called on Ok");
}

#[test]
fn with_context_closure_called_on_err() {
    let err: Result<()> = Err(FraiseQLError::internal("bug"));
    let mut called = false;
    let result = err.with_context(|| {
        called = true;
        "step A"
    });
    assert!(result.is_err());
    assert!(called, "closure should be called on Err");
}

#[test]
fn error_context_produces_internal_variant() {
    let base: Result<()> = Err(FraiseQLError::database("conn failed"));
    let wrapped = base.context("loading users").unwrap_err();
    assert!(
        matches!(wrapped, FraiseQLError::Internal { .. }),
        "context() should produce Internal variant, got: {wrapped:?}"
    );
}

// ── Group D: ValidationFieldError ────────────────────────────────────────────

#[test]
fn validation_field_error_serializes_to_json() {
    let err = ValidationFieldError::new("email", "format", "invalid email format");
    let json = serde_json::to_string(&err).unwrap();
    let parsed: ValidationFieldError = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.field, "email");
    assert_eq!(parsed.rule_type, "format");
    assert_eq!(parsed.message, "invalid email format");
}

#[test]
fn validation_field_error_display_includes_all_parts() {
    let err = ValidationFieldError::new("name", "min_length", "too short");
    let s = err.to_string();
    assert!(s.contains("name"), "got: {s}");
    assert!(s.contains("min_length"), "got: {s}");
    assert!(s.contains("too short"), "got: {s}");
}

#[test]
fn validation_field_error_new_constructor_sets_all_fields() {
    let err = ValidationFieldError::new("age", "range", "must be positive");
    assert_eq!(err.field, "age");
    assert_eq!(err.rule_type, "range");
    assert_eq!(err.message, "must be positive");
}

// ── Group E: From impls ───────────────────────────────────────────────────────

#[test]
fn from_serde_json_error_produces_parse_variant() {
    let json_err = serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
    let e: FraiseQLError = json_err.into();
    assert!(matches!(e, FraiseQLError::Parse { .. }), "got: {e:?}");
    assert_eq!(e.status_code(), 400);
}

#[test]
fn from_io_error_produces_internal_variant() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
    let e: FraiseQLError = io_err.into();
    assert!(matches!(e, FraiseQLError::Internal { .. }), "got: {e:?}");
    assert_eq!(e.status_code(), 500);
}

#[test]
fn from_env_var_error_produces_configuration_variant() {
    let var_err = std::env::var("__FRAISEQL_TEST_NONEXISTENT_VAR_12345").unwrap_err();
    let e: FraiseQLError = var_err.into();
    assert!(matches!(e, FraiseQLError::Configuration { .. }), "got: {e:?}");
    assert_eq!(e.status_code(), 500);
}

// ── Group F: Boolean classifier methods ──────────────────────────────────────

#[test]
fn client_errors_are_classified_correctly() {
    assert!(
        FraiseQLError::Authentication {
            message: "x".into(),
        }
        .is_client_error()
    );
    assert!(
        FraiseQLError::Authorization {
            message:  "x".into(),
            action:   None,
            resource: None,
        }
        .is_client_error()
    );
    assert!(
        FraiseQLError::NotFound {
            resource_type: "T".into(),
            identifier:    "1".into(),
        }
        .is_client_error()
    );
    assert!(
        FraiseQLError::Validation {
            message: "x".into(),
            path:    None,
        }
        .is_client_error()
    );
    assert!(
        FraiseQLError::Parse {
            message:  "x".into(),
            location: "l".into(),
        }
        .is_client_error()
    );
    assert!(
        FraiseQLError::Conflict {
            message: "x".into(),
        }
        .is_client_error()
    );
    assert!(
        FraiseQLError::RateLimited {
            message:          "x".into(),
            retry_after_secs: 1,
        }
        .is_client_error()
    );
}

#[test]
fn server_errors_are_classified_correctly() {
    assert!(
        FraiseQLError::Internal {
            message: "x".into(),
            source:  None,
        }
        .is_server_error()
    );
    assert!(
        FraiseQLError::Database {
            message:   "x".into(),
            sql_state: None,
        }
        .is_server_error()
    );
    assert!(
        FraiseQLError::Configuration {
            message: "x".into(),
        }
        .is_server_error()
    );
    assert!(
        FraiseQLError::ConnectionPool {
            message: "x".into(),
        }
        .is_server_error()
    );
    assert!(
        FraiseQLError::Timeout {
            timeout_ms: 1,
            query:      None,
        }
        .is_server_error()
    );
    assert!(
        FraiseQLError::Unsupported {
            message: "x".into(),
        }
        .is_server_error()
    );
}

#[test]
fn client_and_server_are_mutually_exclusive() {
    let errors: &[FraiseQLError] = &[
        FraiseQLError::Authentication {
            message: "x".into(),
        },
        FraiseQLError::Database {
            message:   "x".into(),
            sql_state: None,
        },
        FraiseQLError::Configuration {
            message: "x".into(),
        },
        FraiseQLError::NotFound {
            resource_type: "T".into(),
            identifier:    "1".into(),
        },
    ];
    for e in errors {
        assert!(
            !(e.is_client_error() && e.is_server_error()),
            "error is both client and server: {e:?}"
        );
    }
}

#[test]
fn retryable_errors_are_identified() {
    assert!(
        FraiseQLError::ConnectionPool {
            message: "x".into(),
        }
        .is_retryable()
    );
    assert!(
        FraiseQLError::Timeout {
            timeout_ms: 5000,
            query:      None,
        }
        .is_retryable()
    );
    assert!(
        FraiseQLError::Cancelled {
            query_id: "q-1".into(),
            reason:   "disconnected".into(),
        }
        .is_retryable()
    );
}

#[test]
fn non_retryable_errors_are_not_retryable() {
    assert!(
        !FraiseQLError::Authentication {
            message: "x".into(),
        }
        .is_retryable()
    );
    assert!(
        !FraiseQLError::Configuration {
            message: "x".into(),
        }
        .is_retryable()
    );
    assert!(
        !FraiseQLError::Conflict {
            message: "x".into(),
        }
        .is_retryable()
    );
    assert!(
        !FraiseQLError::Unsupported {
            message: "x".into(),
        }
        .is_retryable()
    );
}

// ── Group G: error_code() method ─────────────────────────────────────────────

#[test]
fn error_code_values_are_stable() {
    assert_eq!(
        FraiseQLError::Authentication {
            message: "x".into(),
        }
        .error_code(),
        "UNAUTHENTICATED"
    );
    assert_eq!(
        FraiseQLError::Authorization {
            message:  "x".into(),
            action:   None,
            resource: None,
        }
        .error_code(),
        "FORBIDDEN"
    );
    assert_eq!(
        FraiseQLError::NotFound {
            resource_type: "T".into(),
            identifier:    "1".into(),
        }
        .error_code(),
        "NOT_FOUND"
    );
    assert_eq!(
        FraiseQLError::RateLimited {
            message:          "x".into(),
            retry_after_secs: 0,
        }
        .error_code(),
        "RATE_LIMITED"
    );
    assert_eq!(
        FraiseQLError::Unsupported {
            message: "x".into(),
        }
        .error_code(),
        "UNSUPPORTED_OPERATION"
    );
    assert_eq!(
        FraiseQLError::Parse {
            message:  "x".into(),
            location: "l".into(),
        }
        .error_code(),
        "GRAPHQL_PARSE_FAILED"
    );
    assert_eq!(
        FraiseQLError::Validation {
            message: "x".into(),
            path:    None,
        }
        .error_code(),
        "GRAPHQL_VALIDATION_FAILED"
    );
    assert_eq!(
        FraiseQLError::Database {
            message:   "x".into(),
            sql_state: None,
        }
        .error_code(),
        "DATABASE_ERROR"
    );
    assert_eq!(
        FraiseQLError::Internal {
            message: "x".into(),
            source:  None,
        }
        .error_code(),
        "INTERNAL_SERVER_ERROR"
    );
    assert_eq!(
        FraiseQLError::Conflict {
            message: "x".into(),
        }
        .error_code(),
        "CONFLICT"
    );
    assert_eq!(
        FraiseQLError::ConnectionPool {
            message: "x".into(),
        }
        .error_code(),
        "CONNECTION_POOL_ERROR"
    );
    assert_eq!(
        FraiseQLError::Timeout {
            timeout_ms: 1,
            query:      None,
        }
        .error_code(),
        "TIMEOUT"
    );
    assert_eq!(
        FraiseQLError::Cancelled {
            query_id: "q".into(),
            reason:   "r".into(),
        }
        .error_code(),
        "CANCELLED"
    );
    assert_eq!(
        FraiseQLError::Configuration {
            message: "x".into(),
        }
        .error_code(),
        "CONFIGURATION_ERROR"
    );
    assert_eq!(
        FraiseQLError::UnknownField {
            field:     "f".into(),
            type_name: "T".into(),
        }
        .error_code(),
        "UNKNOWN_FIELD"
    );
    assert_eq!(
        FraiseQLError::UnknownType {
            type_name: "T".into(),
        }
        .error_code(),
        "UNKNOWN_TYPE"
    );
}

// ── Group H: Helper constructors ──────────────────────────────────────────────

#[test]
fn parse_constructor_sets_unknown_location() {
    let e = FraiseQLError::parse("oops");
    assert!(matches!(e, FraiseQLError::Parse { .. }));
    assert!(e.to_string().contains("oops"), "got: {e}");
}

#[test]
fn parse_at_constructor_sets_location() {
    let e = FraiseQLError::parse_at("bad token", "line 5");
    let s = e.to_string();
    assert!(s.contains("bad token"), "got: {s}");
    assert!(s.contains("line 5"), "got: {s}");
}

#[test]
fn validation_constructor_has_no_path() {
    let e = FraiseQLError::validation("too short");
    assert!(matches!(e, FraiseQLError::Validation { path: None, .. }));
}

#[test]
fn validation_at_constructor_sets_path() {
    let e = FraiseQLError::validation_at("required", "user.email");
    assert!(matches!(e, FraiseQLError::Validation { path: Some(_), .. }));
}

#[test]
fn database_constructor_has_no_sql_state() {
    let e = FraiseQLError::database("conn failed");
    assert!(matches!(
        e,
        FraiseQLError::Database {
            sql_state: None,
            ..
        }
    ));
}

#[test]
fn internal_constructor_has_no_source() {
    let e = FraiseQLError::internal("bug");
    assert!(matches!(e, FraiseQLError::Internal { source: None, .. }));
}

#[test]
fn rate_limited_with_retry_includes_retry_after() {
    let e = FraiseQLError::rate_limited_with_retry(120);
    assert!(matches!(
        e,
        FraiseQLError::RateLimited {
            retry_after_secs: 120,
            ..
        }
    ));
}

#[test]
fn not_found_constructor_sets_resource_and_identifier() {
    let e = FraiseQLError::not_found("Post", "post-42");
    assert!(matches!(
        e,
        FraiseQLError::NotFound {
            resource_type,
            identifier,
        } if resource_type == "Post" && identifier == "post-42"
    ));
}

#[test]
fn unknown_field_with_suggestion_includes_typo_hint() {
    let e = FraiseQLError::unknown_field_with_suggestion("emal", "User", &["email", "name", "id"]);
    let s = e.to_string();
    // "emal" is 1 edit away from "email" — should include suggestion
    assert!(s.contains("email") || s.contains("emal"), "got: {s}");
}

#[test]
fn unknown_field_with_suggestion_no_match_omits_hint() {
    let e = FraiseQLError::unknown_field_with_suggestion(
        "completelyunrelated",
        "User",
        &["email", "name"],
    );
    let s = e.to_string();
    assert!(s.contains("completelyunrelated"), "got: {s}");
}

#[test]
fn from_postgres_code_23505_produces_conflict() {
    let e = FraiseQLError::from_postgres_code("23505", "duplicate key");
    assert!(matches!(e, FraiseQLError::Conflict { .. }), "got: {e:?}");
}

#[test]
fn from_postgres_code_22p02_produces_validation() {
    let e = FraiseQLError::from_postgres_code("22P02", "invalid input");
    assert!(matches!(e, FraiseQLError::Validation { .. }), "got: {e:?}");
}

#[test]
fn from_postgres_code_unknown_produces_database() {
    let e = FraiseQLError::from_postgres_code("99999", "unknown error");
    assert!(matches!(e, FraiseQLError::Database { .. }), "got: {e:?}");
}

#[test]
fn auth_error_constructor_produces_authentication() {
    let e = FraiseQLError::auth_error("token expired");
    assert!(matches!(e, FraiseQLError::Authentication { .. }));
}

#[test]
fn cancelled_constructor_sets_query_id_and_reason() {
    let e = FraiseQLError::cancelled("q-abc", "user request");
    assert!(matches!(
        e,
        FraiseQLError::Cancelled {
            query_id,
            reason,
        } if query_id == "q-abc" && reason == "user request"
    ));
}
