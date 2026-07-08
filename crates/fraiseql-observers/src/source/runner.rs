//! Single-firing runner: run a source's work on exactly one replica per tick.
//!
//! Wraps the (previously unwired) [`CheckpointLease`] advisory lease so a source
//! scheduled on N replicas fires on one. The runner acquires the lease keyed on
//! the source name, runs the closure while holding it, then **releases
//! explicitly** — Phase 00 characterization proved that dropping the lease does
//! *not* free a PostgreSQL session advisory lock (its pooled connection stays
//! alive), so drop-based RAII is not enough on the happy path; only a dead
//! connection (a real crash) releases via the session ending, which is the
//! crash-safety backstop.

use std::{
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
};

use sha2::{Digest, Sha256};

use crate::{error::Result, listener::CheckpointLease};

/// Derive the advisory-lock key for a source from its name.
///
/// The first 8 bytes of `SHA-256(source_name)` as an `i64`. A cryptographic
/// digest makes an accidental collision between two distinct source names
/// negligible (a source's name is also validated unique at compile time), and the
/// mapping is stable across processes and releases — a plain `Hash` (`SipHash`) is
/// not, so two replicas would derive different lock keys and never coordinate.
#[must_use]
pub fn lock_id(source_name: &str) -> i64 {
    let digest = Sha256::digest(source_name.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    i64::from_be_bytes(bytes)
}

/// The outcome of a [`LeaseGuardedRunner::run`] attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum RunOutcome<T> {
    /// This replica won the lease and ran the closure, which returned `T`.
    Ran(T),
    /// Another replica holds the lease; the closure did not run.
    SkippedNotLeader,
}

/// Runs a closure only when this replica holds the source's advisory lease.
///
/// Construct one per source with [`postgres`](Self::postgres) (cross-replica
/// coordination) or [`in_process`](Self::in_process) (single-node / test — guards
/// a source against overlapping *itself*, but does not coordinate across separately
/// constructed runners). Each [`run`](Self::run) acquires, executes, and releases.
pub struct LeaseGuardedRunner {
    lease:            CheckpointLease,
    source_name:      String,
    skips_not_leader: AtomicU64,
}

impl LeaseGuardedRunner {
    /// A runner backed by a PostgreSQL session advisory lock — the lock key is the
    /// stable [`lock_id`] of the source name, so every replica contends on the
    /// same key.
    #[cfg(feature = "postgres")]
    #[must_use]
    pub fn postgres(pool: sqlx::PgPool, source_name: impl Into<String>) -> Self {
        let source_name = source_name.into();
        let key = lock_id(&source_name);
        Self {
            lease: CheckpointLease::postgres(pool, format!("source:{source_name}"), key),
            source_name,
            skips_not_leader: AtomicU64::new(0),
        }
    }

    /// A runner backed by an in-process lease (single-node / tests). Prevents a
    /// source from overlapping itself within one process; does not coordinate
    /// across replicas — use [`postgres`](Self::postgres) for that.
    #[must_use]
    pub fn in_process(source_name: impl Into<String>) -> Self {
        let source_name = source_name.into();
        let key = lock_id(&source_name);
        Self {
            // No TTL semantics needed: the runner acquires and releases within one
            // `run`, so an effectively unbounded lease duration is correct.
            lease: CheckpointLease::in_process(format!("source:{source_name}"), key, u64::MAX),
            source_name,
            skips_not_leader: AtomicU64::new(0),
        }
    }

    /// The source name this runner coordinates.
    #[must_use]
    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    /// How many times this runner skipped because another replica held the lease.
    #[must_use]
    pub fn skips_not_leader(&self) -> u64 {
        self.skips_not_leader.load(Ordering::Relaxed)
    }

    /// Acquire the lease; if won, run `f` to completion and then release; if lost,
    /// count a skip and return [`RunOutcome::SkippedNotLeader`] without running.
    ///
    /// The lease is held for the whole closure (so a second replica cannot start
    /// the same long fetch) and released explicitly afterwards — see the module
    /// doc on why drop is insufficient. A release failure is logged, not
    /// propagated: the closure's work already committed, and a stuck lock
    /// self-heals when the connection dies.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError`](crate::error::ObserverError) only if *acquiring*
    /// the lease fails (the closure never ran). The closure's own `T` — commonly a
    /// `Result` — is returned verbatim inside [`RunOutcome::Ran`].
    pub async fn run<F, Fut, T>(&self, f: F) -> Result<RunOutcome<T>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        if !self.lease.acquire().await? {
            self.skips_not_leader.fetch_add(1, Ordering::Relaxed);
            return Ok(RunOutcome::SkippedNotLeader);
        }

        let output = f().await;

        if let Err(error) = self.lease.release().await {
            tracing::warn!(
                source = %self.source_name,
                error = %error,
                "source lease release failed — lock self-heals on connection death"
            );
        }

        Ok(RunOutcome::Ran(output))
    }
}
