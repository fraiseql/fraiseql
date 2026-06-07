//! Unit tests for the [`Authorizer`](super::Authorizer) trait surface and the
//! shared [`enforce_authz`](super::enforce_authz) fail-closed enforcement helper.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use chrono::Utc;
use serde_json::json;

use super::{Authorizer, AuthzDecision, AuthzRequest, OperationKind, enforce_authz};
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
        principal: None,
        operation: OperationKind::Query,
        name:      "users",
        input:     None,
    };
    assert!(matches!(AllowAll.authorize(&req).unwrap(), AuthzDecision::Allow));
}

#[test]
fn deny_all_denies_with_reason() {
    let req = AuthzRequest {
        principal: None,
        operation: OperationKind::Mutation,
        name:      "createUser",
        input:     None,
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
    let ops = [(OperationKind::Query, "users".to_string())];
    assert!(enforce_authz(&AllowAll, None, &ops, None).is_ok());
}

#[test]
fn enforce_deny_is_authorization_403() {
    let principal = ctx("u1");
    let ops = [(OperationKind::Mutation, "createUser".to_string())];
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
fn enforce_raising_fails_closed_to_403() {
    // A raising policy must DENY (403), never silently allow. Load-bearing honesty test.
    let ops = [(OperationKind::Query, "users".to_string())];
    let err = enforce_authz(&RaisingAuthorizer, None, &ops, None).unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Authorization { .. }),
        "raising authorizer must fail closed to Authorization/403, got {err:?}"
    );
    // The underlying error text must NOT leak through.
    if let FraiseQLError::Authorization { message, .. } = err {
        assert!(
            !message.contains("backend unreachable"),
            "policy error must not leak: {message}"
        );
    }
}

#[test]
fn enforce_multi_root_denies_on_any() {
    // Multi-root: deny on the SECOND root denies the whole request (no partial pass).
    let ops = [
        (OperationKind::Query, "public".to_string()),
        (OperationKind::Query, "secret".to_string()),
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
    let ops = [(OperationKind::Query, "users".to_string())];
    let input = json!({ "ok": true });
    assert!(enforce_authz(&NeedsInput, Some(&principal), &ops, Some(&input)).is_ok());
    assert!(enforce_authz(&NeedsInput, None, &ops, Some(&input)).is_err());
}
