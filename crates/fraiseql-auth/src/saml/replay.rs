//! Single-use replay protection for SAML assertions.
//!
//! `samael` verifies an assertion's signature and time-window but is stateless: it cannot
//! tell that a *valid* assertion has already been consumed. Without a replay guard, an
//! attacker who captures one valid `SAMLResponse` (e.g. from a proxy log or the browser
//! POST) can replay it until the `NotOnOrAfter` window closes and obtain a session each
//! time. A [`SamlReplayStore`] records each assertion `ID` for the remainder of its
//! validity window so a second presentation is rejected.
//!
//! # Why this is a store and not a map
//!
//! It was a map — an in-process [`DashMap`] whose own module doc admitted "replay
//! protection therefore holds within a single process". Behind more than one replica that
//! is a real weakening rather than a theoretical one: an attacker who captures a signed
//! `Response` replays it against a **different** replica, which has never seen that
//! assertion ID and accepts it. The signature is valid; only the replay cache would have
//! stopped it. A restart has the same shape — the map is empty afterwards, so an assertion
//! captured beforehand is replayable until its `NotOnOrAfter` passes (#949).
//!
//! [`PgSamlReplayStore`] closes that. `[saml]` already requires a pool for sessions and
//! account linking, so Postgres adds no infrastructure, and `INSERT … ON CONFLICT DO
//! NOTHING` makes "have I seen this?" a single atomic statement rather than a
//! read-then-write that two replicas could interleave.
//!
//! # Failure posture
//!
//! **Fail-closed.** A backend error refuses the assertion. This is deliberately unlike
//! `fraiseql_core::security::oidc::ReplayCache`, whose default `FailurePolicy::FailOpen`
//! trades replay protection for auth availability during a Redis outage: there the token
//! was already signature-verified *and* short-lived, and the blast radius of a refusal is
//! every request. Here the store lives in the same Postgres the session mint needs one
//! statement later, so an unreachable backend means the login could not have completed
//! anyway — and accepting an assertion we cannot prove is fresh is the whole defect.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::{DashMap, mapref::entry::Entry};
use sqlx::PgPool;

use super::SamlError;

/// A backend that records consumed SAML assertion IDs.
#[async_trait]
pub trait SamlReplayStore: Send + Sync + std::fmt::Debug {
    /// Atomically check-and-record an assertion ID.
    ///
    /// Returns `Ok(true)` if the ID is **fresh** (and is now recorded), `Ok(false)` if it
    /// was already present — a replay.
    ///
    /// Implementations must make the check and the record one atomic step. A
    /// read-then-write leaves a window in which two concurrent presentations of the same
    /// assertion both read "fresh".
    ///
    /// # Errors
    ///
    /// Returns [`SamlError::Verification`] if the backend fails. Callers treat that as a
    /// refusal — see the module's failure-posture note.
    async fn check_and_record(
        &self,
        assertion_id: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, SamlError>;

    /// Whether this store is shared across processes.
    ///
    /// The server refuses to boot a multi-replica posture on a single-process store, the
    /// way it already does for rate limiting and token revocation: an operator who has
    /// asserted distributed state must not silently get per-process replay protection.
    fn is_distributed(&self) -> bool;
}

/// In-process replay store. Correct for exactly one server instance.
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

#[async_trait]
impl SamlReplayStore for SamlReplayCache {
    async fn check_and_record(
        &self,
        assertion_id: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, SamlError> {
        // Prune expired entries. An assertion whose window has closed is rejected by the
        // signature time-check anyway, so forgetting it cannot enable a replay.
        self.seen.retain(|_, expiry| *expiry > now);

        Ok(match self.seen.entry(assertion_id.to_owned()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(slot) => {
                slot.insert(expires_at);
                true
            },
        })
    }

    fn is_distributed(&self) -> bool {
        false
    }
}

/// DDL for the shared replay table. Executed at boot, which is what keeps it valid
/// PostgreSQL (the `#748` precedent).
pub const PG_SAML_REPLAY_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;
CREATE TABLE IF NOT EXISTS core.tb_saml_replay (
    assertion_id TEXT PRIMARY KEY,
    expires_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_saml_replay_expires ON core.tb_saml_replay(expires_at);
REVOKE ALL ON core.tb_saml_replay FROM PUBLIC;
";

/// Postgres-backed replay store, shared by every replica over one database.
#[derive(Debug, Clone)]
pub struct PgSamlReplayStore {
    db: PgPool,
}

impl PgSamlReplayStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Execute the table DDL. Idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`SamlError::Config`] if a statement fails.
    pub async fn init(&self) -> Result<(), SamlError> {
        for stmt in PG_SAML_REPLAY_SCHEMA_SQL.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt)
                .execute(&self.db)
                .await
                .map_err(|e| SamlError::Config(format!("saml replay table: {e}")))?;
        }
        Ok(())
    }

    /// Delete rows whose validity window has closed. Returns the number removed.
    ///
    /// An assertion past its `NotOnOrAfter` is refused by the signature time-check, so
    /// forgetting it cannot enable a replay — this only bounds the table.
    ///
    /// # Errors
    ///
    /// Returns [`SamlError::Verification`] if the delete fails.
    pub async fn sweep_expired(&self, now: DateTime<Utc>) -> Result<u64, SamlError> {
        sqlx::query("DELETE FROM core.tb_saml_replay WHERE expires_at <= $1")
            .bind(now)
            .execute(&self.db)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| SamlError::Verification(format!("saml replay sweep: {e}")))
    }
}

#[async_trait]
impl SamlReplayStore for PgSamlReplayStore {
    async fn check_and_record(
        &self,
        assertion_id: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, SamlError> {
        // One statement does the whole check. `ON CONFLICT DO NOTHING` reports zero rows
        // affected when the ID is already present, so "have I seen this?" is answered by
        // Postgres' own uniqueness rather than by a read this process then acts on — two
        // replicas presenting the same assertion at once cannot both see "fresh".
        //
        // The `WHERE expires_at <= $3` on the conflict target lets a *stale* row be
        // reclaimed: past its window the assertion is refused by the signature check
        // anyway, so treating the ID as fresh again is safe and keeps the table from
        // rejecting a legitimately new assertion that reuses an old ID.
        let inserted = sqlx::query(
            "INSERT INTO core.tb_saml_replay (assertion_id, expires_at)
             VALUES ($1, $2)
             ON CONFLICT (assertion_id) DO UPDATE
                SET expires_at = EXCLUDED.expires_at
              WHERE core.tb_saml_replay.expires_at <= $3",
        )
        .bind(assertion_id)
        .bind(expires_at)
        .bind(now)
        .execute(&self.db)
        .await
        .map_err(|e| SamlError::Verification(format!("saml replay store: {e}")))?
        .rows_affected();

        Ok(inserted > 0)
    }

    fn is_distributed(&self) -> bool {
        true
    }
}
