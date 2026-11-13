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
///
/// A replica signs with the key it last read, so it may sign with a key another one
/// retired until its own next successful read. It signs from a read at most
/// `max_staleness_secs` old ([`Self::signing_ring`]), and a retired key stays published
/// for `token_validity_secs` plus that bound ([`Self::publish_window_secs`]): a token a
/// late replica signed validates until it expires.
#[derive(Debug)]
pub struct LiveKeyRing {
    store:               PgSigningKeyStore,
    token_validity_secs: u64,
    max_staleness_secs:  u64,
    reload_cooldown:     Duration,
    ring:                RwLock<Option<Read>>,
    last_miss_reload:    Mutex<Option<Instant>>,
    stale:               AtomicBool,
    stale_logged:        Mutex<Option<u64>>,
}

/// The ring as last read, and when.
#[derive(Debug, Clone)]
struct Read {
    ring: Arc<KeyRing>,
    at:   u64,
}

impl LiveKeyRing {
    /// A ring not read yet: it signs nothing until a [`Self::tick`] or [`Self::reload`]
    /// has read the table, and its first lookup reads it.
    ///
    /// `token_validity_secs` is the longest a token it signs can still validate;
    /// `max_staleness_secs` the oldest read it signs from.
    #[must_use]
    pub const fn new(
        store: PgSigningKeyStore,
        token_validity_secs: u64,
        max_staleness_secs: u64,
        reload_cooldown: Duration,
    ) -> Self {
        Self {
            store,
            token_validity_secs,
            max_staleness_secs,
            reload_cooldown,
            ring: RwLock::new(None),
            last_miss_reload: Mutex::new(None),
            stale: AtomicBool::new(false),
            stale_logged: Mutex::new(None),
        }
    }

    /// How long a retired key stays published: the longest a token it signed validates,
    /// plus the oldest read a replica signs from.
    #[must_use]
    pub const fn publish_window_secs(&self) -> u64 {
        self.token_validity_secs.saturating_add(self.max_staleness_secs)
    }

    /// The oldest read this ring signs from.
    #[must_use]
    pub const fn max_staleness_secs(&self) -> u64 {
        self.max_staleness_secs
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
        token_validity_secs: u64,
        max_staleness_secs: u64,
        reload_cooldown: Duration,
    ) -> Result<Self> {
        let ring = Self::new(store, token_validity_secs, max_staleness_secs, reload_cooldown);
        ring.tick(now).await?;
        Ok(ring)
    }

    /// The snapshot this process signs with.
    ///
    /// # Errors
    ///
    /// [`AuthError::ConfigError`] before the table was first read.
    pub fn current(&self) -> Result<Arc<KeyRing>> {
        Ok(self.read()?.ring)
    }

    /// The snapshot to sign with at `now`: refused when its last successful read is more
    /// than `max_staleness_secs` before `now`, since a key retired since then may already
    /// be past the window its tokens are published for. The first refusal for a read is
    /// logged; later ones for the same read are not.
    ///
    /// # Errors
    ///
    /// [`AuthError::ConfigError`] before the table was first read, and when the read is
    /// too old.
    pub fn signing_ring(&self, now: u64) -> Result<Arc<KeyRing>> {
        let read = self.read()?;
        let age = now.saturating_sub(read.at);
        if age <= self.max_staleness_secs {
            return Ok(read.ring);
        }
        let mut logged = unpoisoned(self.stale_logged.lock());
        if *logged != Some(read.at) {
            *logged = Some(read.at);
            tracing::error!(
                read_at = read.at,
                age_secs = age,
                max_staleness_secs = self.max_staleness_secs,
                "the first-party issuer refuses to sign: its signing keys were last read too \
                 long ago (is the auth database reachable?)"
            );
        }
        Err(AuthError::ConfigError {
            message: format!(
                "the signing keys were last read {age}s ago, more than the {}s this issuer \
                 signs from",
                self.max_staleness_secs
            ),
        })
    }

    fn read(&self) -> Result<Read> {
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
        let ring = self.store.load(now, self.publish_window_secs()).await?;
        let published = ring.published_count();
        *unpoisoned(self.ring.write()) = Some(Read {
            ring: Arc::new(ring),
            at:   now,
        });
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
        let snapshot = unpoisoned(self.ring.read()).clone().map(|read| read.ring);
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
        self.store.sweep(now, self.publish_window_secs()).await?;
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
