//! In-memory per-`sub` TTL cache for resolved identities.
//!
//! Ported verbatim from #242 (`routes/enrichment.rs`, `v2.2.1`): a `DashMap`
//! keyed on the subject, with absolute-instant expiry checked on read.
//!
//! P01 refines this in two ways specified by the design
//! (`.phases/539-enriched-identity/DESIGN.md` §6): the key becomes the ordered
//! bound-`$param` tuple rather than bare `sub` (amendment A — cross-issuer
//! correctness), and a short negative TTL is added for `Denied` results. This
//! phase lands the proven get/insert/expiry behaviour and its tests unchanged.

use std::time::{Duration, Instant};

use dashmap::DashMap;

/// A cached identity map with an absolute expiry instant.
pub(super) struct CacheEntry {
    /// The resolved (and possibly column-renamed) identity fields.
    pub(super) value:      serde_json::Map<String, serde_json::Value>,
    /// The instant after which this entry is stale.
    pub(super) expires_at: Instant,
}

/// Per-`sub` cache for identity-resolution results.
#[derive(Default)]
pub(super) struct EnrichmentCache {
    /// Live entries, keyed by subject.
    pub(super) entries: DashMap<String, CacheEntry>,
}

impl EnrichmentCache {
    /// Create an empty cache.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Return a cached result if one exists and has not expired.
    ///
    /// An expired entry is evicted as a side effect before returning `None`.
    pub(super) fn get(&self, sub: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
        let entry = self.entries.get(sub)?;
        if Instant::now() < entry.expires_at {
            Some(entry.value.clone())
        } else {
            // Release the read guard before removing to avoid a self-deadlock.
            drop(entry);
            self.entries.remove(sub);
            None
        }
    }

    /// Insert a result with the given TTL.
    pub(super) fn insert(
        &self,
        sub: String,
        value: serde_json::Map<String, serde_json::Value>,
        ttl: Duration,
    ) {
        self.entries.insert(
            sub,
            CacheEntry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
    }
}
