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
mod tests;
