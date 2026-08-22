//! The one definition of an action-result cache key.
//!
//! There were two hand-written ones (#1011), and both derived the action's
//! identity from `format!("{action:?}")`. `ActionConfig::Webhook` holds a
//! `HashMap<String, String>` of headers, and `RandomState` reseeds per map
//! instance, so two `ActionConfig` values carrying *identical* data render
//! differently and hash differently — within a single process. For any webhook
//! action with two or more headers the cache therefore never hit: every lookup
//! computed a fresh key, missed, executed the action, and wrote another
//! distinct key, growing the store by one entry per dispatch with no eviction
//! pressure beyond TTL.
//!
//! A cache key is a rule, so it gets one definition rather than one per caller.

use sha2::{Digest, Sha256};

use crate::{config::ActionConfig, dispatch::canonical_json, event::EntityEvent};

/// Derive the cache key identifying one action's result for one event.
///
/// The action's contribution is a `SHA-256` of its canonical JSON — key-sorted,
/// so the key depends on the action's content and not on the iteration order of
/// whatever map happens to hold its headers. `SHA-256` rather than
/// `DefaultHasher` because these keys are *persisted* (Redis) and
/// `DefaultHasher`'s output is explicitly not stable across Rust releases: a
/// toolchain bump would silently orphan every live entry.
///
/// Returns `None` when the action does not render to JSON, which callers treat
/// as "not cacheable" — no lookup, no store. Falling back to another rendering
/// would let two distinct actions collide onto one key, which serves one
/// action's result in place of another's; a miss is the safe failure.
#[must_use]
pub fn action_result_key(event: &EntityEvent, action: &ActionConfig) -> Option<String> {
    let canonical = canonical_json(&serde_json::to_value(action).ok()?).to_string();
    let digest = Sha256::digest(canonical.as_bytes());

    // `event.id` and `event.entity_id` are UUIDs and the digest is fixed-width
    // hex, so `entity_type` is the only free field and sits between two
    // fixed-width ones — no two distinct tuples render to one string.
    Some(format!(
        "action_result:{}:{}:{}:{}",
        event.id,
        hex::encode(&digest[..16]),
        event.entity_type,
        event.entity_id
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use uuid::Uuid;

    use super::action_result_key;
    use crate::{
        config::ActionConfig,
        event::{EntityEvent, EventKind},
    };

    fn event() -> EntityEvent {
        EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        )
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
}
