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

/// The `requires_actor` predicate itself (#966).
///
/// These pin the *decision*; that the decision is reached on every transport is
/// what `actor_predicate_e2e_pg` proves against a real server.
mod requires_actor {
    use super::{ActorType, HashMap, Uuid, json};
    use crate::security::{SecurityContext, actor_type::enforce_requires_actor};

    fn ctx(actor: ActorType) -> SecurityContext {
        let mut c = SecurityContext::system_job("t", "r", vec![], vec![], None);
        // `system_job` stamps SystemJob; re-stamp to the class under test so each
        // case goes through the same public setter the auth paths use.
        c = c.with_actor_type(actor);
        c
    }

    /// The common case costs a slice check and admits everything.
    #[test]
    fn an_empty_list_admits_every_actor_and_the_anonymous() {
        for actor in ActorType::ALL {
            assert!(enforce_requires_actor("Query", "q", &[], Some(&ctx(actor))).is_ok());
        }
        assert!(enforce_requires_actor("Query", "q", &[], None).is_ok());
    }

    /// The issue's own example: agents may not run this, humans may.
    #[test]
    fn a_list_admits_only_the_classes_it_names() {
        let allow = [ActorType::HumanUser];
        assert!(
            enforce_requires_actor(
                "Mutation",
                "deleteTenant",
                &allow,
                Some(&ctx(ActorType::HumanUser))
            )
            .is_ok()
        );
        for denied in [
            ActorType::AiAgent,
            ActorType::ServiceAccount,
            ActorType::SystemJob,
        ] {
            let err =
                enforce_requires_actor("Mutation", "deleteTenant", &allow, Some(&ctx(denied)))
                    .expect_err("must refuse");
            assert!(
                matches!(err, crate::error::FraiseQLError::Authorization { .. }),
                "an actor refusal is an authorization error, not a 'not found': {err}"
            );
            assert!(
                err.to_string().contains(denied.as_str()),
                "the refusal names the class that was refused: {err}"
            );
        }
    }

    /// An unclassifiable request belongs to no class. Asserted separately from
    /// the allow-list cases because the tempting implementation — read the
    /// context's actor type, defaulting to `ActorType::default()` — admits every
    /// anonymous request to a `["human_user"]` operation, since the default *is*
    /// `HumanUser`.
    #[test]
    fn anonymous_is_refused_by_any_non_empty_list() {
        for actor in ActorType::ALL {
            assert!(
                enforce_requires_actor("Query", "q", &[actor], None).is_err(),
                "anonymous must not satisfy [{}]",
                actor.as_str()
            );
        }
    }

    /// Delegation is deliberately not consulted: an agent acting for a human is
    /// an agent, however privileged that human is.
    ///
    /// This is the whole reason the predicate exists — "agents cannot delete a
    /// tenant *regardless of* the underlying user's permissions". An
    /// implementation that fell back to `acting_for` would pass every other test
    /// here and silently do the opposite of what was asked.
    #[test]
    fn a_delegated_agent_does_not_inherit_the_humans_admission() {
        let sub = "550e8400-e29b-41d4-a716-446655440000";
        let mut extra = HashMap::new();
        extra.insert("act".to_string(), json!({ "sub": "agent-robot-7" }));
        let (actor, acting_for) = super::super::derive_actor(sub, &[], &extra);
        assert_eq!(actor, ActorType::AiAgent);
        assert_eq!(acting_for, Some(Uuid::parse_str(sub).unwrap()));

        let mut c = ctx(actor);
        c.roles = vec!["admin".to_string(), "owner".to_string()];
        let err =
            enforce_requires_actor("Mutation", "deleteTenant", &[ActorType::HumanUser], Some(&c))
                .expect_err("an agent is refused however privileged the human it acts for");
        assert!(err.to_string().contains("ai_agent"), "{err}");
    }
}
