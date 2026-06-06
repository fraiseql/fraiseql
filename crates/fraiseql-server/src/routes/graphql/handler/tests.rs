//! Tests for tenant-dispatch error mapping (#332).
//!
//! `executor_for_tenant` errors must map to the correct HTTP semantics: a
//! suspended tenant (`ServiceUnavailable`) → 503 + `Retry-After`, and an unknown
//! key (`Authorization`) → 403. Previously both collapsed to 403.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use axum::http::StatusCode;
use fraiseql_error::FraiseQLError;

use super::tenant_dispatch_error;
use crate::error::ErrorCode;

#[test]
fn suspended_tenant_maps_to_503_with_retry_after() {
    let err = FraiseQLError::ServiceUnavailable {
        message:     "Tenant 'acme' is suspended".to_string(),
        retry_after: Some(60),
    };
    let gql = tenant_dispatch_error(&err);
    assert_eq!(gql.code, ErrorCode::ServiceUnavailable, "suspended → ServiceUnavailable");
    assert_eq!(gql.code.status_code(), StatusCode::SERVICE_UNAVAILABLE);
    let ext = gql.extensions.expect("a 503 error carries extensions");
    assert_eq!(ext.retry_after_secs, Some(60), "the Retry-After hint must be preserved");
}

#[test]
fn suspended_tenant_without_hint_still_maps_to_503() {
    let err = FraiseQLError::ServiceUnavailable {
        message:     "suspended".to_string(),
        retry_after: None,
    };
    let gql = tenant_dispatch_error(&err);
    assert_eq!(gql.code.status_code(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn unknown_tenant_maps_to_403_forbidden() {
    let err = FraiseQLError::Authorization {
        message:  "Unknown tenant key 'ghost'".to_string(),
        action:   None,
        resource: None,
    };
    let gql = tenant_dispatch_error(&err);
    assert_eq!(gql.code, ErrorCode::Forbidden, "unknown key → Forbidden");
    assert_eq!(gql.code.status_code(), StatusCode::FORBIDDEN);
}
