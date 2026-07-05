//! Unit tests for the [`FieldAuthorizer`](super::FieldAuthorizer) trait surface and
//! the reference implementations used across the field-authz test suites.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use chrono::Utc;
use serde_json::json;

use super::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest};
use crate::{
    error::{FraiseQLError, Result},
    schema::FieldDenyPolicy,
    security::SecurityContext,
    types::UserId,
};

/// Allows every field. Reference impl for the passthrough/no-op case.
struct AllowAll;
impl FieldAuthorizer for AllowAll {
    fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        Ok(FieldAuthzDecision::Allow)
    }
}

/// Denies every field with `Reject`. Reference impl for the hard-deny case.
struct DenyAll;
impl FieldAuthorizer for DenyAll {
    fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        Ok(FieldAuthzDecision::Deny {
            code:    "denied".to_string(),
            on_deny: FieldDenyPolicy::Reject,
        })
    }
}

/// Always returns `Err`. Reference impl for the fail-closed honesty invariant.
struct RaisingFieldAuthorizer;
impl FieldAuthorizer for RaisingFieldAuthorizer {
    fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        Err(FraiseQLError::Validation {
            message: "policy backend unreachable".to_string(),
            path:    None,
        })
    }
}

/// Reveals the field only to the row's owner; masks it otherwise.
struct OwnerOnly;
impl FieldAuthorizer for OwnerOnly {
    fn authorize_field(&self, req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        let owner = req.parent.and_then(|p| p.get("owner_id")).and_then(|v| v.as_str());
        if owner == Some(req.principal.user_id.as_str()) {
            Ok(FieldAuthzDecision::Allow)
        } else {
            Ok(FieldAuthzDecision::Deny {
                code:    "not_owner".to_string(),
                on_deny: FieldDenyPolicy::Mask,
            })
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

fn request<'a>(
    principal: &'a SecurityContext,
    parent: &'a serde_json::Value,
) -> FieldAuthzRequest<'a> {
    FieldAuthzRequest {
        principal,
        type_name: "User",
        field_name: "email",
        parent: Some(parent),
        arguments: None,
    }
}

#[test]
fn allow_all_allows() {
    let principal = ctx("u1");
    let parent = json!({ "owner_id": "u2" });
    let decision = AllowAll.authorize_field(&request(&principal, &parent)).unwrap();
    assert!(matches!(decision, FieldAuthzDecision::Allow));
}

#[test]
fn deny_all_rejects() {
    let principal = ctx("u1");
    let parent = json!({});
    let decision = DenyAll.authorize_field(&request(&principal, &parent)).unwrap();
    match decision {
        FieldAuthzDecision::Deny { code, on_deny } => {
            assert_eq!(code, "denied");
            assert_eq!(on_deny, FieldDenyPolicy::Reject);
        },
        FieldAuthzDecision::Allow => panic!("expected deny"),
    }
}

#[test]
fn raising_authorizer_returns_err() {
    let principal = ctx("u1");
    let parent = json!({});
    let result = RaisingFieldAuthorizer.authorize_field(&request(&principal, &parent));
    assert!(result.is_err(), "raising authorizer must return Err for fail-closed handling");
}

#[test]
fn owner_only_allows_owner() {
    let principal = ctx("u1");
    let parent = json!({ "owner_id": "u1" });
    let decision = OwnerOnly.authorize_field(&request(&principal, &parent)).unwrap();
    assert!(matches!(decision, FieldAuthzDecision::Allow));
}

#[test]
fn owner_only_masks_non_owner() {
    let principal = ctx("u1");
    let parent = json!({ "owner_id": "u2" });
    let decision = OwnerOnly.authorize_field(&request(&principal, &parent)).unwrap();
    match decision {
        FieldAuthzDecision::Deny { code, on_deny } => {
            assert_eq!(code, "not_owner");
            assert_eq!(on_deny, FieldDenyPolicy::Mask);
        },
        FieldAuthzDecision::Allow => panic!("expected deny for non-owner"),
    }
}

// ── inline-fragment gated-field detection (released #423 bypass regression) ───

/// A `User` type whose `ssn` field is policy-gated.
fn schema_with_gated_ssn() -> crate::schema::CompiledSchema {
    use crate::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};
    let mut schema = CompiledSchema::new();
    let mut user = TypeDefinition::new("User", "v_user");
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("ssn", FieldType::String).with_authorize(true),
    ];
    schema.types.push(user);
    schema.build_indexes();
    schema
}

/// A bare field selection (no args / nesting / directives).
fn field(name: &str) -> crate::graphql::FieldSelection {
    crate::graphql::FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }
}

/// Regression for a released-version (#423, since v2.5.0) query-path authorization
/// bypass: a policy-gated field wrapped in a same-type inline fragment
/// (`{ users { ... on User { ssn } } }`) was invisible to gated-field detection —
/// the detector matched selection names literally, so the authorizer was skipped
/// while the projector (which resolves fragments) emitted the field. The detector
/// now resolves inline fragments before matching.
#[test]
fn gated_field_inside_inline_fragment_is_detected() {
    let schema = schema_with_gated_ssn();
    let fragment = crate::graphql::FieldSelection {
        name:          "...on User".to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![field("id"), field("ssn")],
        directives:    vec![],
    };
    let selections = vec![fragment];

    assert!(
        super::selection_set_selects_gated_field(&schema, "User", &selections),
        "a gated field inside a same-type inline fragment must be detected"
    );
    let gated = super::collect_top_level_gated_fields(&schema, "User", &selections);
    assert_eq!(gated.len(), 1, "the fragment-wrapped gated field must be collected");
    assert_eq!(gated[0].field_name, "ssn");
}
