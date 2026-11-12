//! The ring as one process holds it: signing with the active key, and answering for
//! every published key, while other replicas rotate the table underneath.

use std::{
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::Value;

use super::{KeyRing, PgSigningKeyStore};
use crate::error::{AuthError, Result};

/// The signing-key ring as this process holds it.
///
/// It signs with the active key of the snapshot it last read. A `kid` the snapshot does
/// not publish makes it re-read the table, so a key another replica rotated in is found
/// on first sight; such re-reads happen at most once per `reload_cooldown`, however many
/// unknown `kid`s arrive. [`Self::tick`] rotates, sweeps and re-reads on a schedule, so
/// a replica stops signing with a retired key within one tick.
#[derive(Debug)]
pub struct LiveKeyRing {
    store:               PgSigningKeyStore,
    publish_window_secs: u64,
    reload_cooldown:     Duration,
    ring:                RwLock<Option<Arc<KeyRing>>>,
    last_miss_reload:    Mutex<Option<Instant>>,
    stale:               AtomicBool,
}

impl LiveKeyRing {
    /// A ring not read yet: it signs nothing until a [`Self::tick`] or [`Self::reload`]
    /// has read the table, and its first lookup reads it.
    #[must_use]
    pub const fn new(
        store: PgSigningKeyStore,
        publish_window_secs: u64,
        reload_cooldown: Duration,
    ) -> Self {
        Self {
            store,
            publish_window_secs,
            reload_cooldown,
            ring: RwLock::new(None),
            last_miss_reload: Mutex::new(None),
            stale: AtomicBool::new(false),
        }
    }

    /// [`Self::new`], then the first [`Self::tick`]: read the ring at `now`, creating its first key, or replacing an active key past
    /// its rotation period, first.
    ///
    /// # Errors
    ///
    /// The store's error when the ring cannot be read or the active key does not open.
    pub async fn open(
        store: PgSigningKeyStore,
        now: u64,
        publish_window_secs: u64,
        reload_cooldown: Duration,
    ) -> Result<Self> {
        let ring = Self::new(store, publish_window_secs, reload_cooldown);
        ring.tick(now).await?;
        Ok(ring)
    }

    /// The snapshot this process signs with.
    ///
    /// # Errors
    ///
    /// [`AuthError::ConfigError`] before the table was first read.
    pub fn current(&self) -> Result<Arc<KeyRing>> {
        unpoisoned(self.ring.read()).clone().ok_or_else(|| AuthError::ConfigError {
            message: "the first-party issuer's signing keys have not been read yet".to_string(),
        })
    }

    /// Re-read the ring at `now`. Returns how many keys it publishes.
    ///
    /// # Errors
    ///
    /// The store's error; the snapshot is kept.
    pub async fn reload(&self, now: u64) -> Result<usize> {
        let ring = self.store.load(now, self.publish_window_secs).await?;
        let published = ring.published_count();
        *unpoisoned(self.ring.write()) = Some(Arc::new(ring));
        self.stale.store(false, Ordering::Release);
        Ok(published)
    }

    /// The published JWK for `kid`, re-reading the table at `now` when the snapshot does
    /// not publish it and no re-read for an unknown `kid` happened within the cooldown,
    /// or when [`Self::invalidate`] asked for one. `None` for a key the ring does not
    /// publish.
    ///
    /// # Errors
    ///
    /// The store's error when a re-read fails.
    pub async fn published_key(&self, kid: &str, now: u64) -> Result<Option<Value>> {
        let snapshot = unpoisoned(self.ring.read()).clone();
        if let (false, Some(ring)) = (self.stale.load(Ordering::Acquire), snapshot) {
            if let Some(jwk) = ring.published_key(kid) {
                return Ok(Some(jwk.clone()));
            }
            if !self.claim_miss_reload() {
                return Ok(None);
            }
        }
        self.reload(now).await?;
        Ok(self.current()?.published_key(kid).cloned())
    }

    /// Distrust the snapshot: the next lookup re-reads the table whatever the cooldown.
    pub fn invalidate(&self) {
        self.stale.store(true, Ordering::Release);
    }

    /// The scheduled pass at `now`: rotate an active key past its period, delete keys past
    /// their publish window, and re-read the ring.
    ///
    /// # Errors
    ///
    /// The store's error. The snapshot is kept, so the next tick retries.
    pub async fn tick(&self, now: u64) -> Result<()> {
        self.store.ensure_active(now).await?;
        self.store.sweep(now, self.publish_window_secs).await?;
        self.reload(now).await?;
        Ok(())
    }

    /// Whether a lookup that missed may re-read the table now: once per cooldown,
    /// claimed before the re-read so concurrent misses do not all re-read.
    fn claim_miss_reload(&self) -> bool {
        let mut last = unpoisoned(self.last_miss_reload.lock());
        if last.is_some_and(|at| at.elapsed() < self.reload_cooldown) {
            return false;
        }
        *last = Some(Instant::now());
        true
    }
}

/// Keep using a lock a panicking thread held: the data it guards is replaced whole, so
/// it is never left half-written.
fn unpoisoned<T>(result: std::result::Result<T, PoisonError<T>>) -> T {
    result.unwrap_or_else(PoisonError::into_inner)
}
