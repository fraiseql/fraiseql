//! Single-use replay protection for SAML assertions.
//!
//! `samael` verifies an assertion's signature and time-window but is stateless: it cannot
//! tell that a *valid* assertion has already been consumed. Without a replay guard, an
//! attacker who captures one valid `SAMLResponse` (e.g. from a proxy log or the browser
//! POST) can replay it until the `NotOnOrAfter` window closes and obtain a session each
//! time. [`SamlReplayCache`] records each assertion `ID` for the remainder of its validity
//! window so a second presentation is rejected.
//!
//! # Scope
//!
//! v1 is an in-process [`DashMap`]; replay protection therefore holds within a single
//! server instance. A multi-instance deployment needs a shared backend (Redis/Postgres) —
//! a forward-compatible extension, mirroring the single-node posture of the rest of the
//! v2.9.0 auth foundation. Documented rather than silently assumed.

use chrono::{DateTime, Utc};
use dashmap::{DashMap, mapref::entry::Entry};

/// Records consumed SAML assertion IDs until their validity window expires.
#[derive(Debug, Default)]
pub struct SamlReplayCache {
    /// assertion `ID` → instant after which the entry may be pruned (the assertion's
    /// `NotOnOrAfter`). Once pruned the ID can no longer be replayed anyway because the
    /// signature's own time-window has closed.
    seen: DashMap<String, DateTime<Utc>>,
}

impl SamlReplayCache {
    /// Create an empty replay cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomically check-and-record an assertion ID.
    ///
    /// Returns `true` if the ID is **fresh** (now recorded) and `false` if it was already
    /// present — i.e. a replay. Entries whose window has closed (`expires_at <= now`) are
    /// pruned first, so memory is bounded by the number of assertions seen within one
    /// validity window.
    #[must_use]
    pub fn check_and_record(
        &self,
        assertion_id: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> bool {
        // Prune expired entries. An assertion whose window has closed is rejected by the
        // signature time-check anyway, so forgetting it cannot enable a replay.
        self.seen.retain(|_, expiry| *expiry > now);

        match self.seen.entry(assertion_id.to_owned()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(slot) => {
                slot.insert(expires_at);
                true
            },
        }
    }

    /// Number of assertion IDs currently tracked (primarily for tests/metrics).
    #[must_use]
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Whether the cache currently tracks no assertions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}
