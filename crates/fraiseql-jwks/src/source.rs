//! The bounded cache: what makes a miss cost one request instead of one each.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::RwLock;

use crate::{
    BoxFuture, Jwk, JwkSet, JwksError, MonotonicClock, REFETCH_COOLDOWN, fetch::Endpoint,
    system_clock,
};

/// A source of published verification keys, selected by `kid`.
///
/// The seam a caller substitutes in a test. [`JwksSource`] is the only
/// implementation that talks to a publisher; a test hands in one that answers
/// from a local key, which is how a scheme's own suite runs with **no network**
/// at all.
///
/// `Ok(None)` and `Err` are different answers and callers must treat them so:
/// `None` is "this publisher does not publish that key", which refuses one token,
/// while `Err` is "the key set is unreachable or unusable", which is the
/// operator's or the publisher's problem.
pub trait JwksKeys: Send + Sync + std::fmt::Debug {
    /// The key named by `kid`.
    ///
    /// # Errors
    ///
    /// [`JwksError`] when the key set cannot be fetched or is not a key set. A
    /// `kid` the publisher simply does not publish is `Ok(None)`.
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>>;
}

/// The key set held for one `jwks_uri`, and when it was fetched.
#[derive(Debug)]
struct Held {
    set:        JwkSet,
    fetched_at: Instant,
}

/// The last fetch this source attempted, and how it went.
///
/// The outcome is kept, not just the instant, because the cooldown that follows a
/// *failed* attempt must not answer later lookups "that key is not published".
/// See [`JwksError::Unavailable`].
#[derive(Debug)]
struct Attempt {
    at:      Instant,
    /// `None` on success; otherwise the rendered error, for the operator's log.
    failure: Option<String>,
}

/// One publisher's key set, fetched at most once per cooldown.
///
/// See the crate documentation for what is bounded and why. In short: a miss is
/// remembered, concurrent misses single-flight, an expired set is never served,
/// and an operator-forced [`refresh`](Self::refresh) ignores the cooldown.
pub struct JwksSource {
    endpoint: Endpoint,
    /// How long a fetched set may be served for. Past this it is not served at
    /// all — a key the publisher rotated out has to stop validating (#361), and
    /// the cooldown must not extend that window.
    ttl:      Duration,
    /// How long a fetch is deferred after the previous **attempt**, successful or
    /// not.
    ///
    /// [`REFETCH_COOLDOWN`], or the TTL when that is shorter. Without the
    /// clamping, an operator who shortened the TTL below the cooldown would get a
    /// cache that expires and then may not be refilled: every token refused for
    /// the difference, on a loop.
    cooldown: Duration,
    clock:    Arc<dyn MonotonicClock>,
    held:     RwLock<Option<Held>>,
    /// The last fetch attempted, successful or not. Separate from [`Held`]
    /// because a failed fetch leaves nothing held and must still cool down —
    /// otherwise an unreachable publisher turns every inbound request into an
    /// outbound one, which is the amplification with extra steps, through the door
    /// that opens exactly when the publisher is already struggling.
    attempt:  RwLock<Option<Attempt>>,
    /// Held across a fetch so that concurrent callers make one request between
    /// them. The losers re-read [`attempted_at`](Self::attempted_at) under it and
    /// use what the winner published.
    ///
    /// [`tokio::sync::Mutex`] rather than a synchronous one because it is held
    /// across the `await` of the fetch itself.
    flight:   tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for JwksSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwksSource")
            .field("endpoint", &self.endpoint)
            .field("ttl", &self.ttl)
            .field("cooldown", &self.cooldown)
            .field("held_keys", &self.held.read().as_ref().map_or(0, |held| held.set.keys.len()))
            .finish_non_exhaustive()
    }
}

impl JwksSource {
    /// A source for one publisher's `jwks_uri`, holding its key set for `ttl`.
    ///
    /// Nothing is fetched here: the first lookup does that, so a server whose `IdP`
    /// is briefly unreachable still boots.
    ///
    /// # Errors
    ///
    /// [`JwksError::InvalidUrl`] or [`JwksError::InvalidScheme`] — both boot-time
    /// configuration errors, raised here so a route or a validator that cannot
    /// ever fetch a key is refused while the operator is still watching.
    pub fn new(jwks_uri: &str, ttl: Duration) -> Result<Self, JwksError> {
        Ok(Self {
            endpoint: Endpoint::parse(jwks_uri)?,
            ttl,
            cooldown: REFETCH_COOLDOWN.min(ttl),
            clock: system_clock(),
            held: RwLock::new(None),
            attempt: RwLock::new(None),
            flight: tokio::sync::Mutex::new(()),
        })
    }

    /// Measure the TTL and the cooldown against `clock` instead of the system's.
    ///
    /// The seam both sides of the cooldown are tested across; see
    /// [`MonotonicClock`].
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }

    /// The cooldown in force, which is the shorter of [`REFETCH_COOLDOWN`] and
    /// the TTL.
    #[must_use]
    pub const fn cooldown(&self) -> Duration {
        self.cooldown
    }

    /// The held key set, if it is still within its TTL.
    ///
    /// An expired set is not returned, here or anywhere: that is what keeps the
    /// stolen-key window at the TTL rather than the TTL plus a cooldown.
    #[must_use]
    pub fn fresh_set(&self) -> Option<JwkSet> {
        let held = self.held.read();
        held.as_ref()
            .filter(|held| self.clock.now().duration_since(held.fetched_at) <= self.ttl)
            .map(|held| held.set.clone())
    }

    /// The key named by `kid`, if a fresh set is held and names it.
    ///
    /// Takes the read lock once and clones only the key, not the set — this is the
    /// path every legitimate request in steady state takes.
    fn find_fresh(&self, kid: &str) -> Option<Jwk> {
        let held = self.held.read();
        let held = held.as_ref()?;
        if self.clock.now().duration_since(held.fetched_at) > self.ttl {
            return None;
        }
        held.set.find(kid).cloned()
    }

    /// Whether enough time has passed since the last attempt to look again.
    fn may_fetch(&self) -> bool {
        self.attempt
            .read()
            .as_ref()
            .is_none_or(|attempt| self.clock.now().duration_since(attempt.at) >= self.cooldown)
    }

    /// "I cannot say", carrying why the last attempt failed.
    ///
    /// Raised only when **no** key set is held at all. A held set that does not
    /// name a `kid` is `Ok(None)` — see [`JwksError::Unavailable`] for why the two
    /// must not be the same answer.
    fn unavailable(&self) -> JwksError {
        JwksError::Unavailable {
            uri:    self.endpoint.uri().to_string(),
            reason: self
                .attempt
                .read()
                .as_ref()
                .and_then(|attempt| attempt.failure.clone())
                .unwrap_or_else(|| "no key set has been fetched yet".to_string()),
        }
    }

    /// The key named by `kid`, fetching the publisher's set at most once per
    /// cooldown.
    ///
    /// # Errors
    ///
    /// [`JwksError::Unreachable`] and friends when a fetch was due and failed, and
    /// [`JwksError::Unavailable`] when a fetch was **not** due and no key set is
    /// held at all — "I cannot say", which is not the same claim as "that key is
    /// not published" and must not be collapsed into it.
    ///
    /// `Ok(None)` is the answer when a key set *is* held and does not name the
    /// `kid`. That is a real answer about the publisher's keys as of the last
    /// successful fetch.
    pub async fn key(&self, kid: &str) -> Result<Option<Jwk>, JwksError> {
        // A fresh set that names the key: one read lock, no request. This is every
        // legitimate request in steady state.
        if let Some(key) = self.find_fresh(kid) {
            return Ok(Some(key));
        }
        // Otherwise either nothing fresh is held or what is held does not name the
        // key. Both want a look at the publisher, and one cooldown bounds both:
        // separating them would let a caller alternate between the two and fetch
        // twice as often.
        //
        // The cooldown is NOT consulted here. It is consulted once, inside the
        // flight lock, and that is deliberate: a copy of the rule outside the lock
        // reads like a fast path and behaves like a second gate, so removing
        // either one left the other enforcing it and neither mutation could redden
        // its own case. A miss therefore pays for an uncontended lock — while a
        // legitimate request in steady state never reaches this line at all.
        // A set fetched *by this call* answers it. Re-deriving freshness from the
        // clock afterwards would make `ttl` = 0 — a legitimate "never cache" —
        // refuse every token instead, because the set it just fetched would read as
        // already expired; and for any ttl it would let a slow fetch expire its own
        // result.
        if let Some(set) = self.fetch_and_hold().await? {
            return Ok(set.find(kid).cloned());
        }
        // No fetch: the cooldown holds. Answer from what is held, and only if
        // fresh. Holding nothing is not the same answer as holding a set without
        // this key — the first cannot distinguish an unpublished key from an
        // unreachable publisher, and a caller that maps the two to HTTP gives them
        // opposite statuses.
        match self.fresh_set() {
            Some(set) => Ok(set.find(kid).cloned()),
            None => Err(self.unavailable()),
        }
    }

    /// Force a fetch now, ignoring the cooldown, and report how many keys the
    /// publisher serves.
    ///
    /// The operator's response to a known key compromise — it backs
    /// `POST /admin/v1/auth/refresh-jwks`. The cooldown deliberately does not
    /// apply: it bounds refetches that an *unauthenticated sender* can trigger by
    /// naming an unknown `kid`, and this is neither unauthenticated nor
    /// sender-triggered. Refusing it would make the one control an operator has
    /// during an incident answer "try again in thirty seconds".
    ///
    /// # Errors
    ///
    /// [`JwksError`] when the fetch fails. The previously held set is left in
    /// place in that case, since replacing it with nothing would refuse every
    /// token until the publisher came back.
    pub async fn refresh(&self) -> Result<usize, JwksError> {
        let _flight = self.flight.lock().await;
        self.fetch_and_hold_locked().await.map(|set| set.keys.len())
    }

    /// Discard the held set so the next lookup fetches.
    ///
    /// The cooldown is cleared with it. An operator flushing the cache means "do
    /// not serve these keys again"; leaving the cooldown armed would answer that
    /// by refusing every token until it lapsed, rather than by fetching.
    pub fn invalidate(&self) {
        *self.held.write() = None;
        *self.attempt.write() = None;
        tracing::info!("JWKS cache invalidated; the next lookup fetches from the publisher");
    }

    /// One fetch, however many callers arrive together.
    ///
    /// `Ok(None)` means no fetch happened because the cooldown had not lapsed —
    /// not that there is nothing to report.
    async fn fetch_and_hold(&self) -> Result<Option<JwkSet>, JwksError> {
        let _flight = self.flight.lock().await;
        // Re-read under the lock. A caller that queued behind the winner must use
        // what the winner published rather than open a second request, and this is
        // the only place that can know: the cooldown is armed when a fetch
        // *finishes*, and every one of these callers checked before that.
        if !self.may_fetch() {
            return Ok(None);
        }
        self.fetch_and_hold_locked().await.map(Some)
    }

    /// Fetch and replace the held set, returning it. Caller holds
    /// [`flight`](Self::flight).
    async fn fetch_and_hold_locked(&self) -> Result<JwkSet, JwksError> {
        let fetched = self.endpoint.fetch().await;
        // Recorded whatever the outcome — a *failed* fetch cools down too, or an
        // unreachable publisher turns every inbound request into an outbound one.
        // Written after the await rather than before because every caller that
        // reads it does so under `flight`, so there is no window to protect and
        // the outcome is worth keeping with the instant.
        *self.attempt.write() = Some(Attempt {
            at:      self.clock.now(),
            failure: fetched.as_ref().err().map(ToString::to_string),
        });
        let set = fetched?;
        self.warn_on_rotation(&set);
        *self.held.write() = Some(Held {
            set:        set.clone(),
            fetched_at: self.clock.now(),
        });
        Ok(set)
    }

    /// Report keys that were held and are no longer published (#361).
    ///
    /// Replacing the set wholesale is what makes a rotated-out key stop
    /// validating; this only tells the operator it happened, because a rotation
    /// they did not initiate is worth seeing in a log.
    fn warn_on_rotation(&self, fetched: &JwkSet) {
        let held = self.held.read();
        let Some(previous) = held.as_ref() else {
            return;
        };
        let dropped = previous.set.kids_missing_from(fetched);
        if !dropped.is_empty() {
            tracing::warn!(
                dropped = ?dropped,
                "the publisher no longer serves keys this server held; they are evicted, so \
                 tokens signed by them stop validating"
            );
        }
    }
}

impl JwksKeys for JwksSource {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        Box::pin(Self::key(self, kid))
    }
}
