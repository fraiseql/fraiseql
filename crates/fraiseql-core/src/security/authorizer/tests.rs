//! Unit tests for the [`Authorizer`](super::Authorizer) trait surface and the
//! shared [`enforce_authz`](super::enforce_authz) fail-closed enforcement helper.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use chrono::Utc;
use serde_json::json;

use super::{
    Authorizer, AuthzDecision, AuthzNesting, AuthzOperation, AuthzRequest, OperationKind,
    enforce_authz,
};
use crate::{
    error::{FraiseQLError, Result},
    security::SecurityContext,
    types::UserId,
};

/// Allows every operation. Reference impl for the passthrough/no-op case.
struct AllowAll;
impl Authorizer for AllowAll {
    fn authorize(&self, _req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        Ok(AuthzDecision::Allow)
    }
}

/// Denies every operation. Reference impl for the hard-deny case.
struct DenyAll;
impl Authorizer for DenyAll {
    fn authorize(&self, _req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        Ok(AuthzDecision::Deny {
            reason: "denied".to_string(),
        })
    }
}

/// Always returns `Err`. Reference impl for the fail-closed honesty invariant.
struct RaisingAuthorizer;
impl Authorizer for RaisingAuthorizer {
    fn authorize(&self, _req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        Err(FraiseQLError::Validation {
            message: "policy backend unreachable".to_string(),
            path:    None,
        })
    }
}

/// Denies only operations named `"secret"`; allows everything else.
struct DenySecret;
impl Authorizer for DenySecret {
    fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        if req.name == "secret" {
            Ok(AuthzDecision::Deny {
                reason: "no access to secret".to_string(),
            })
        } else {
            Ok(AuthzDecision::Allow)
        }
    }
}

fn ctx(user_id: &str) -> SecurityContext {
    SecurityContext {
        user_id:          UserId::new(user_id),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-test".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

#[test]
fn allow_all_allows() {
    let req = AuthzRequest {
        principal:   None,
        operation:   OperationKind::Query,
        name:        "users",
        input:       None,
        target_type: None,
        nesting:     None,
    };
    assert!(matches!(AllowAll.authorize(&req).unwrap(), AuthzDecision::Allow));
}

#[test]
fn deny_all_denies_with_reason() {
    let req = AuthzRequest {
        principal:   None,
        operation:   OperationKind::Mutation,
        name:        "createUser",
        input:       None,
        target_type: None,
        nesting:     None,
    };
    match DenyAll.authorize(&req).unwrap() {
        AuthzDecision::Deny { reason } => assert_eq!(reason, "denied"),
        AuthzDecision::Allow => panic!("expected deny"),
    }
}

#[test]
fn operation_kind_labels() {
    assert_eq!(OperationKind::Query.as_str(), "query");
    assert_eq!(OperationKind::Mutation.as_str(), "mutation");
    assert_eq!(OperationKind::Subscription.as_str(), "subscription");
}

// ── enforce_authz: the shared fail-closed gate ──────────────────────────────────

#[test]
fn enforce_allow_is_ok() {
    let ops = [AuthzOperation::root(OperationKind::Query, "users", None)];
    assert!(enforce_authz(&AllowAll, None, &ops, None).is_ok());
}

#[test]
fn enforce_deny_is_authorization_403() {
    let principal = ctx("u1");
    let ops = [AuthzOperation::root(
        OperationKind::Mutation,
        "createUser",
        None,
    )];
    let err = enforce_authz(&DenyAll, Some(&principal), &ops, None).unwrap_err();
    match err {
        FraiseQLError::Authorization {
            message,
            action,
            resource,
        } => {
            // The app-supplied reason is folded into the message.
            assert!(message.contains("denied"), "reason folded into message: {message}");
            assert_eq!(action.as_deref(), Some("mutation"));
            assert_eq!(resource.as_deref(), Some("createUser"));
        },
        other => panic!("expected Authorization, got {other:?}"),
    }
}

#[test]
fn enforce_raising_fails_closed_as_unavailable_not_forbidden() {
    // A raising policy must never silently allow — load-bearing honesty test — and must
    // not claim the caller was adjudicated and refused (#1374): the policy backend could
    // not be reached, which is a 503 the client may retry, not a 403.
    let ops = [AuthzOperation::root(OperationKind::Query, "users", None)];
    let err = enforce_authz(&RaisingAuthorizer, None, &ops, None).unwrap_err();
    assert!(
        matches!(err, FraiseQLError::ServiceUnavailable { .. }),
        "raising authorizer must fail closed as ServiceUnavailable/503, got {err:?}"
    );
    // The underlying error text must NOT leak through.
    assert!(
        !err.to_string().contains("backend unreachable"),
        "policy error must not leak: {err}"
    );
}

#[test]
fn enforce_multi_root_denies_on_any() {
    // Multi-root: deny on the SECOND root denies the whole request (no partial pass).
    let ops = [
        AuthzOperation::root(OperationKind::Query, "public", None),
        AuthzOperation::root(OperationKind::Query, "secret", None),
    ];
    let err = enforce_authz(&DenySecret, None, &ops, None).unwrap_err();
    match err {
        FraiseQLError::Authorization { resource, .. } => {
            assert_eq!(resource.as_deref(), Some("secret"), "denied root is the secret one");
        },
        other => panic!("expected Authorization, got {other:?}"),
    }
}

#[test]
fn enforce_passes_input_and_principal() {
    // A policy keying on input + principal sees both.
    struct NeedsInput;
    impl Authorizer for NeedsInput {
        fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
            let ok = req.principal.is_some()
                && req.input.and_then(|v| v.get("ok")).and_then(serde_json::Value::as_bool)
                    == Some(true);
            if ok {
                Ok(AuthzDecision::Allow)
            } else {
                Ok(AuthzDecision::Deny {
                    reason: "missing input".to_string(),
                })
            }
        }
    }
    let principal = ctx("u1");
    let ops = [AuthzOperation::root(OperationKind::Query, "users", None)];
    let input = json!({ "ok": true });
    assert!(enforce_authz(&NeedsInput, Some(&principal), &ops, Some(&input)).is_ok());
    assert!(enforce_authz(&NeedsInput, None, &ops, Some(&input)).is_err());
}

/// A nested level reaches the authorizer with its type and where it sits, and a denial
/// names the path rather than an operation the client never wrote.
#[test]
fn a_nested_level_carries_its_type_and_path() {
    struct DenyNestedOrders;
    impl Authorizer for DenyNestedOrders {
        fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
            if req.target_type == Some("Order") && req.nesting.is_some() {
                Ok(AuthzDecision::Deny {
                    reason: "orders only through the account".to_string(),
                })
            } else {
                Ok(AuthzDecision::Allow)
            }
        }
    }
    let root = [AuthzOperation::root(
        OperationKind::Query,
        "orders",
        Some("Order"),
    )];
    assert!(enforce_authz(&DenyNestedOrders, None, &root, None).is_ok());

    let nested = [AuthzOperation::nested(
        "orders",
        "Order",
        AuthzNesting::new("User", "orders"),
    )];
    match enforce_authz(&DenyNestedOrders, None, &nested, None).unwrap_err() {
        FraiseQLError::Authorization {
            message, resource, ..
        } => {
            assert!(message.contains("'Order' at 'User.orders'"), "{message}");
            assert_eq!(resource.as_deref(), Some("orders"));
        },
        other => panic!("expected Authorization, got {other:?}"),
    }
}
