//! Unit tests for [`ActorType`] and [`derive_actor`].
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::doc_markdown)] // Reason: informal test doc comments

use std::collections::HashMap;

use serde_json::json;
use uuid::Uuid;

use super::{ActorType, derive_actor};

/// A delegated user JWT carries an `act` claim → classified as an agent, with
/// `acting_for` = the underlying human (the top-level `sub`), per RFC 8693.
#[test]
fn act_claim_classifies_ai_agent_acting_for_the_subject() {
    let sub = "550e8400-e29b-41d4-a716-446655440000";
    let mut extra = HashMap::new();
    extra.insert("act".to_string(), json!({ "sub": "agent-robot-7" }));

    let (actor, acting_for) = derive_actor(sub, &[], &extra);

    assert_eq!(actor, ActorType::AiAgent);
    assert_eq!(acting_for, Some(Uuid::parse_str(sub).unwrap()));
}

/// A delegated request whose subject is not UUID-shaped is still an agent, but
/// `acting_for` is NULL rather than aborting — the change-log never fails over a
/// stamp.
#[test]
fn ai_agent_with_non_uuid_subject_leaves_acting_for_none() {
    let mut extra = HashMap::new();
    extra.insert("act".to_string(), json!({ "sub": "agent-robot-7" }));

    let (actor, acting_for) = derive_actor("opaque-idp-subject", &[], &extra);

    assert_eq!(actor, ActorType::AiAgent);
    assert_eq!(acting_for, None);
}

/// A null `act` claim is not a delegation marker.
#[test]
fn null_act_claim_is_not_a_delegation() {
    let mut extra = HashMap::new();
    extra.insert("act".to_string(), serde_json::Value::Null);

    let (actor, acting_for) = derive_actor("550e8400-e29b-41d4-a716-446655440000", &[], &extra);

    assert_eq!(actor, ActorType::HumanUser);
    assert_eq!(acting_for, None);
}

/// A `service_account` scope (without delegation) → service account.
#[test]
fn service_account_scope_classifies_service_account() {
    let scopes = vec!["read:user".to_string(), "service_account".to_string()];

    let (actor, acting_for) = derive_actor("svc-123", &scopes, &HashMap::new());

    assert_eq!(actor, ActorType::ServiceAccount);
    assert_eq!(acting_for, None);
}

/// Delegation wins over the `service_account` scope (first match wins).
#[test]
fn delegation_takes_precedence_over_service_account_scope() {
    let sub = "550e8400-e29b-41d4-a716-446655440000";
    let scopes = vec!["service_account".to_string()];
    let mut extra = HashMap::new();
    extra.insert("act".to_string(), json!({ "sub": "agent" }));

    let (actor, acting_for) = derive_actor(sub, &scopes, &extra);

    assert_eq!(actor, ActorType::AiAgent);
    assert_eq!(acting_for, Some(Uuid::parse_str(sub).unwrap()));
}

/// An ordinary user JWT (no delegation, no service scope) → human user.
#[test]
fn plain_user_classifies_human_user() {
    let (actor, acting_for) = derive_actor("user-1", &["read:user".to_string()], &HashMap::new());

    assert_eq!(actor, ActorType::HumanUser);
    assert_eq!(acting_for, None);
}

/// `as_str` matches the `snake_case` serde representation byte-for-byte, so the
/// borrowed change-log stamp and a JSON round-trip never diverge.
#[test]
fn as_str_matches_serde_snake_case() {
    for actor in [
        ActorType::HumanUser,
        ActorType::ServiceAccount,
        ActorType::AiAgent,
        ActorType::SystemJob,
    ] {
        let json = serde_json::to_value(actor).unwrap();
        assert_eq!(json, serde_json::Value::String(actor.as_str().to_string()));
        let back: ActorType = serde_json::from_value(json).unwrap();
        assert_eq!(back, actor);
    }
}

/// `from_token` is the exact inverse of `as_str`, and rejects unknown tokens.
#[test]
fn from_token_round_trips_as_str() {
    for actor in [
        ActorType::HumanUser,
        ActorType::ServiceAccount,
        ActorType::AiAgent,
        ActorType::SystemJob,
    ] {
        assert_eq!(ActorType::from_token(actor.as_str()), Some(actor));
    }
    assert_eq!(ActorType::from_token("nonsense"), None);
}

/// The default is the safe, most-common classification.
#[test]
fn default_is_human_user() {
    assert_eq!(ActorType::default(), ActorType::HumanUser);
}
