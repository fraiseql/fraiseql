use super::*;

#[test]
fn test_parse_error() {
    let err = FraiseQLError::parse("unexpected token");
    assert!(err.is_client_error());
    assert!(!err.is_server_error());
    assert_eq!(err.status_code(), 400);
    assert_eq!(err.error_code(), "GRAPHQL_PARSE_FAILED");
}

#[test]
fn test_database_error() {
    let err = FraiseQLError::database("connection refused");
    assert!(!err.is_client_error());
    assert!(err.is_server_error());
    assert_eq!(err.status_code(), 500);
}

#[test]
fn test_not_found_error() {
    let err = FraiseQLError::not_found("User", "123");
    assert!(err.is_client_error());
    assert_eq!(err.status_code(), 404);
    assert_eq!(err.to_string(), "User not found: 123");
}

#[test]
fn test_retryable_errors() {
    assert!(
        FraiseQLError::ConnectionPool {
            message: "timeout".to_string(),
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
    assert!(!FraiseQLError::parse("bad query").is_retryable());
}

#[test]
fn test_unsupported_is_501() {
    let err = FraiseQLError::Unsupported {
        message: "execute_function_call not supported on SQLite".to_string(),
    };
    assert_eq!(err.status_code(), 501);
    assert!(err.is_server_error());
    assert_eq!(err.error_code(), "UNSUPPORTED_OPERATION");
}

#[test]
fn test_from_serde_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("not json")
        .expect_err("'not json' must fail to parse");
    let err: FraiseQLError = json_err.into();
    assert!(matches!(err, FraiseQLError::Parse { .. }));
}

#[test]
fn test_validation_field_error_creation() {
    let field_err = ValidationFieldError::new("user.email", "pattern", "Invalid email format");
    assert_eq!(field_err.field, "user.email");
    assert_eq!(field_err.rule_type, "pattern");
    assert_eq!(field_err.message, "Invalid email format");
}

#[test]
fn test_levenshtein_ascii() {
    // Basic sanity
    assert_eq!(FraiseQLError::levenshtein_distance("kitten", "sitting"), 3);
    assert_eq!(FraiseQLError::levenshtein_distance("", "abc"), 3);
    assert_eq!(FraiseQLError::levenshtein_distance("abc", ""), 3);
    assert_eq!(FraiseQLError::levenshtein_distance("same", "same"), 0);
}

#[test]
fn test_levenshtein_multibyte_utf8() {
    // "café" is 4 chars but 5 bytes — previously the byte-length bug returned
    // matrix[5][5] instead of matrix[4][4], which was an unmodified zero cell.
    assert_eq!(FraiseQLError::levenshtein_distance("café", "cafe"), 1);
    assert_eq!(FraiseQLError::levenshtein_distance("naïve", "naive"), 1);
    // Two multi-byte strings: distance should equal number of differing chars
    assert_eq!(FraiseQLError::levenshtein_distance("café", "café"), 0);
}

// ---------------------------------------------------------------------------
// F049 round-3 audit: source-chain downcast for boxed-payload variants.
//
// Round-2 closed F049 by adding `#[source]` to the three tuple variants and
// regression tests that asserted `.source().is_some()`. Round-3 strengthens
// the contract by asserting the boxed payload is also recoverable via
// `Error::source().and_then(downcast_ref::<T>())` — which is exactly the
// pattern the variant rustdoc promises to downstream consumers.
//
// `fraiseql-error` is a leaf crate (it cannot depend on `fraiseql-auth`,
// `fraiseql-webhooks`, or `fraiseql-observers`). The test therefore stands
// in a local sentinel error type that exercises the same dyn-erasure +
// downcast_ref path the subsystem `From` impls produce. If the variant
// rustdoc's recovery pattern is correct, this downcast must succeed.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SentinelSubsystemError {
    kind: &'static str,
}

impl std::fmt::Display for SentinelSubsystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sentinel-subsystem-error: {}", self.kind)
    }
}

impl std::error::Error for SentinelSubsystemError {}

fn assert_source_downcasts_to_sentinel(err: &FraiseQLError, expected_kind: &str) {
    use std::error::Error as _;
    let source = err
        .source()
        .expect("boxed-payload variants must expose Error::source()");
    let downcast = source
        .downcast_ref::<SentinelSubsystemError>()
        .expect("Error::source() must downcast to the original concrete subsystem type");
    assert_eq!(downcast.kind, expected_kind);
}

#[test]
fn auth_variant_source_downcasts_to_subsystem_type() {
    let inner = SentinelSubsystemError {
        kind: "auth-token-expired",
    };
    let err = FraiseQLError::Auth(Box::new(inner));
    assert_source_downcasts_to_sentinel(&err, "auth-token-expired");
}

#[test]
fn webhook_variant_source_downcasts_to_subsystem_type() {
    let inner = SentinelSubsystemError {
        kind: "webhook-delivery-refused",
    };
    let err = FraiseQLError::Webhook(Box::new(inner));
    assert_source_downcasts_to_sentinel(&err, "webhook-delivery-refused");
}

#[test]
fn observer_variant_source_downcasts_to_subsystem_type() {
    let inner = SentinelSubsystemError {
        kind: "observer-dispatch-failed",
    };
    let err = FraiseQLError::Observer(Box::new(inner));
    assert_source_downcasts_to_sentinel(&err, "observer-dispatch-failed");
}
