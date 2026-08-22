//! Tests for the one action-result cache-key derivation (#1011).

use std::collections::{BTreeSet, HashMap};

use uuid::Uuid;

use super::action_result_key;
use crate::{
    config::ActionConfig,
    event::{EntityEvent, EventKind},
};

fn event() -> EntityEvent {
    EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), serde_json::json!({}))
}

fn webhook_with_headers() -> ActionConfig {
    webhook_with_trace("abc")
}

fn webhook_with_trace(trace: &str) -> ActionConfig {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer t".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("X-Trace".to_string(), trace.to_string());
    headers.insert("X-Tenant".to_string(), "acme".to_string());
    ActionConfig::Webhook {
        url: Some("https://example.com/hook".to_string()),
        url_env: None,
        method: None,
        headers,
        body_template: None,
        signing_secret: None,
        signing_secret_env: None,
    }
}

/// The key must depend on the action's content, not on how the process
/// happened to lay out its `HashMap`.
///
/// Twenty draws, because the four-entry permutation space is 24: a
/// two-map assertion passes by luck roughly one run in twenty-four, which
/// is how this survived the existing cache tests (they use header-less and
/// single-header actions, where there is nothing to permute).
#[test]
fn identical_actions_produce_one_cache_key() {
    let event = event();

    let keys: BTreeSet<String> = (0..20)
        .map(|_| action_result_key(&event, &webhook_with_headers()).expect("renders"))
        .collect();

    assert_eq!(
        keys.len(),
        1,
        "twenty identical webhook actions produced {} distinct cache keys, so the \
         action-result cache can never hit",
        keys.len()
    );
}

/// Distinct actions must not share a key — the property that makes the
/// "one canonical rendering" fix safe rather than merely stable.
#[test]
fn a_changed_header_changes_the_key() {
    let event = event();

    assert_ne!(
        action_result_key(&event, &webhook_with_headers()),
        action_result_key(&event, &webhook_with_trace("different")),
        "a differing header value must not reuse the other action's cached result"
    );
}

/// Two different action *types* must not share a key either.
#[test]
fn a_different_action_type_changes_the_key() {
    let event = event();
    let email = ActionConfig::Email {
        to:               Some("a@b.com".to_string()),
        to_template:      None,
        subject:          Some("s".to_string()),
        subject_template: None,
        body_template:    None,
        reply_to:         None,
    };

    assert_ne!(
        action_result_key(&event, &webhook_with_headers()),
        action_result_key(&event, &email),
        "distinct action types must key distinctly"
    );
}

/// These keys are persisted, so the digest is a wire format: pin it.
///
/// `DefaultHasher` — what both call sites used — documents its output as
/// unstable across Rust releases, so a toolchain bump would have orphaned
/// every live entry with no error anywhere. A golden value is what makes
/// that class of change loud instead of silent.
#[test]
fn the_key_is_pinned_across_builds() {
    let mut event = event();
    event.id = Uuid::nil();
    event.entity_id = Uuid::nil();

    let key = action_result_key(&event, &webhook_with_headers()).expect("renders");

    assert_eq!(
        key,
        "action_result:00000000-0000-0000-0000-000000000000:\
         9a7087a6310a2c069f540dd4314d36d6:Order:\
         00000000-0000-0000-0000-000000000000",
        "the persisted key format changed; live cache entries are orphaned by this"
    );
}
