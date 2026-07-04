//! In-memory cache for resolved identities (DESIGN §6) — one policy, reused by
//! every profile instance.
//!
//! Evolved from #242's per-`sub` TTL map in two ways the design requires:
//!
//! - **Key = the ordered bound-`$param` tuple**, not bare `sub` (amendment A). `sub` is unique only
//!   *per issuer*, and FraiseQL speaks multi-IdP; keying on the bound parameters makes cache
//!   correctness exactly track the query's `WHERE` clause (a multi-issuer app binds `$iss`), and
//!   closes the same-`sub`-different-`$email` staleness case for free. The key is computed by the
//!   resolver from `BoundQuery::binds`.
//! - **Positive *and* negative TTL.** A `Resolved` outcome is cached for `cache_ttl_secs`; a
//!   `Denied` outcome for a short `negative_ttl_secs` (so a freshly provisioned actor goes live
//!   quickly). An `Unavailable` outcome is **never** cached — a transient blip must not pin a
//!   denial.
//!
//! Each entry also records its `sub` so `flush(sub)` can evict every entry for a
//! subject on revoke/provision, independent of which parameters the query binds.

use std::time::{Duration, Instant};

use dashmap::DashMap;

use super::failure::DenyReason;

/// A cacheable resolution outcome. `Unavailable` is deliberately absent: transient
/// failures are never cached (DESIGN §5.3).
#[derive(Clone)]
pub(super) enum CachedOutcome {
    /// A resolved identity map (renamed enriched fields), cached positively.
    Resolved(serde_json::Map<String, serde_json::Value>),
    /// A permanent denial, cached negatively for a short TTL.
    Denied(DenyReason),
}

/// One cache entry: an outcome, its owning subject (for `flush(sub)`), and an
/// absolute expiry instant.
struct CacheEntry {
    outcome:    CachedOutcome,
    // Reason: read only by `flush(sub)`, whose admin/after-mutation caller is the
    // immediate follow-up (see the flush methods below).
    #[allow(dead_code)]
    sub:        String,
    expires_at: Instant,
}

/// Identity-resolution cache, keyed by the serialized bound-`$param` tuple.
#[derive(Default)]
pub(super) struct IdentityCache {
    entries: DashMap<String, CacheEntry>,
}

impl IdentityCache {
    /// Create an empty cache.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Return a cached outcome if one exists for `key` and has not expired.
    ///
    /// An expired entry is evicted as a side effect before returning `None`.
    pub(super) fn get(&self, key: &str) -> Option<CachedOutcome> {
        let entry = self.entries.get(key)?;
        if Instant::now() < entry.expires_at {
            Some(entry.outcome.clone())
        } else {
            // Release the read guard before removing to avoid a self-deadlock.
            drop(entry);
            self.entries.remove(key);
            None
        }
    }

    /// Insert an outcome for `key` (owned by `sub`) with the given TTL.
    pub(super) fn insert(&self, key: String, sub: String, outcome: CachedOutcome, ttl: Duration) {
        self.entries.insert(
            key,
            CacheEntry {
                outcome,
                sub,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    /// Evict every entry belonging to `sub` (DESIGN §6). Rare/admin — the sweep
    /// is fine. Propagates a grant or revoke for a subject instantly.
    // Reason: exposed via `IdentityResolver::flush`; its admin endpoint /
    // after-mutation hook caller is the immediate follow-up.
    #[allow(dead_code)]
    pub(super) fn flush(&self, sub: &str) {
        self.entries.retain(|_key, entry| entry.sub != sub);
    }

    /// Evict all entries.
    #[allow(dead_code)] // Reason: exposed via `IdentityResolver::flush_all` (same follow-up).
    pub(super) fn flush_all(&self) {
        self.entries.clear();
    }

    /// Number of live (not-yet-swept) entries. Test-only visibility into the map.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}
