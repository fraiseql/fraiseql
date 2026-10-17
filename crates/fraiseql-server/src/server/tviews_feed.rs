//! The server's listener on `pg_tviews`' change feed (#1589).
//!
//! When the database publishes rows (`pg_tviews.publish_changes = 'rows'`), every committed
//! transaction that changed TVIEW rows says which on [`CHANNEL`], whoever ran it: this
//! server, another one, a migration, another application. One task per server listens on a
//! PostgreSQL connection of its own and evicts what each message names
//! ([`fraiseql_core::cache::tviews_feed`]).
//!
//! Messages sent while no connection listens are lost. So every connection, the first and
//! each one after a loss, follows `pg_tviews`' reconnect rule: `LISTEN`, check the session
//! receives notifications (a `LISTEN` through a pooler in transaction mode receives
//! nothing), clear the cache, then apply messages as they arrive. While no connection
//! listens, composed reads are indexed by their views only, and nothing cached relies on
//! the feed.
//!
//! A connection that stays up can miss transactions too: while the cluster's NOTIFY queue is
//! over 90 % full, `pg_tviews` commits them without publishing, and counts them
//! ([`FEED_GAPS`], #1604). The listener reads the count right after `LISTEN` and on every
//! heartbeat, and clears the cache when it moved. A `pg_tviews` that cannot count (not
//! preloaded, or a release without the counter) leaves a reconnect as the only clear.

use std::time::Duration;

use fraiseql_core::{
    cache::tviews_feed::{CHANNEL, TviewsListener},
    db::postgres::{NotifyConnector, NotifySession},
    error::{FraiseQLError, Result},
};
use tracing::{info, warn};

/// The first wait before reconnecting; it doubles per failure, up to [`MAX_BACKOFF`].
/// How often a listening session checks it still receives what it sends: a probe not back
/// by the next beat ends the session.
const HEARTBEAT: Duration = Duration::from_secs(5);
use super::notify_session::PROBE_TIMEOUT;
const MIN_BACKOFF: Duration = Duration::from_millis(200);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// The transactions the cluster committed without publishing their changes (the NOTIFY
/// queue was over 90 % full) since it started (#1604). Any session can read it; the count
/// moves only once such a transaction is committed and visible, so a read after a clear
/// sees what it wrote.
const FEED_GAPS: &str = "SELECT tviews.pg_tviews_feed_gaps()";

/// The SQLSTATEs of a [`FEED_GAPS`] read that cannot tell: `55000`, `pg_tviews` is not in
/// `shared_preload_libraries`; `42883`, an older `pg_tviews` release has no such function.
const CANNOT_TELL: [&str; 2] = ["55000", "42883"];

/// One read of [`FEED_GAPS`].
#[derive(Debug)]
enum GapRead {
    /// The count.
    Count(i64),
    /// `pg_tviews` cannot count here (see [`CANNOT_TELL`]): why.
    CannotTell(String),
}

/// What [`FEED_GAPS`]' result means; `None` when the session is unusable (it ends, as a lost
/// connection does).
fn gap_read(result: Result<i64>) -> Option<GapRead> {
    match result {
        Ok(count) => Some(GapRead::Count(count)),
        Err(FraiseQLError::Database {
            message,
            sql_state: Some(state),
            ..
        }) if CANNOT_TELL.contains(&state.as_str()) => Some(GapRead::CannotTell(message)),
        Err(_) => None,
    }
}

/// What a read of [`FEED_GAPS`] asks of a listening session.
#[derive(Debug, PartialEq, Eq)]
enum Beat {
    /// Go on: nothing was missed.
    Keep,
    /// Clear the cache: transactions were committed unpublished since the last read.
    Clear,
    /// End the session: it cannot read.
    End,
}

/// What the listener knows of [`FEED_GAPS`], across its sessions.
#[derive(Debug, Default)]
struct Gaps {
    /// The count this session read last; `None` before its first.
    seen: Option<i64>,
    /// Whether "cannot tell" was logged: once per server.
    told: bool,
}

impl Gaps {
    /// A new session, about to clear the cache: what came before needs no count.
    const fn session(&mut self) {
        self.seen = None;
    }

    /// Note `read`. Whether the cache must be cleared: the count moved since this session's
    /// last read.
    fn moved(&mut self, read: GapRead) -> bool {
        match read {
            GapRead::Count(count) => {
                let moved = self.seen.is_some_and(|seen| seen != count);
                self.seen = Some(count);
                moved
            },
            GapRead::CannotTell(reason) => {
                if self.first_cannot_tell() {
                    warn!(
                        %reason,
                        "Query result cache: pg_tviews cannot count the transactions it commits \
                         unpublished (preload a release with tviews.pg_tviews_feed_gaps()); a \
                         write this server does not run, committed while the NOTIFY queue is \
                         full, stays cached until the feed reconnects or the TTL expires"
                    );
                }
                false
            },
        }
    }

    /// What `read`, a heartbeat's read of [`FEED_GAPS`] (`None`: it failed), asks of the
    /// session.
    fn beat(&mut self, read: Option<GapRead>) -> Beat {
        match read.map(|read| self.moved(read)) {
            None => Beat::End,
            Some(true) => Beat::Clear,
            Some(false) => Beat::Keep,
        }
    }

    /// Whether this is the first time the count cannot be read.
    const fn first_cannot_tell(&mut self) -> bool {
        let first = !self.told;
        self.told = true;
        first
    }
}

/// Read [`FEED_GAPS`] on `session`, bounded as a probe is; `None` when the session is
/// unusable.
async fn read_gaps(session: &NotifySession) -> Option<GapRead> {
    tokio::time::timeout(PROBE_TIMEOUT, session.query_bigint(FEED_GAPS))
        .await
        .ok()
        .and_then(gap_read)
}

/// Listen on [`CHANNEL`] over sessions `connector` opens, as server `instance`, applying
/// every message to `listener`'s cache. Reconnects with backoff; never returns.
pub async fn run(connector: NotifyConnector, listener: TviewsListener, instance: String) {
    let probe = format!("fraiseql_tviews_probe_{instance}");
    let mut backoff = MIN_BACKOFF;
    let mut gaps = Gaps::default();
    loop {
        match listening_session(&connector, &probe).await {
            Ok(mut session) => {
                // The count before the clear: a transaction it misses from here on moves it.
                gaps.session();
                if gaps.beat(read_gaps(&session).await) == Beat::End {
                    warn!(
                        retry_in_ms = backoff.as_millis(),
                        "Query result cache: cannot read tviews.pg_tviews_feed_gaps(); writes \
                         this server does not run are not heard until it can"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }
                backoff = MIN_BACKOFF;
                listener.listening();
                metrics::gauge!("fraiseql_tviews_feed_connected").set(1.0);
                info!(channel = CHANNEL, "Query result cache: listening to pg_tviews' changes");

                relay(&mut session, &listener, &probe, &mut gaps).await;

                listener.lost();
                metrics::gauge!("fraiseql_tviews_feed_connected").set(0.0);
                warn!(
                    "Query result cache: the connection to pg_tviews' changes was lost; writes \
                     this server does not run are not heard until it is back (the cache was \
                     cleared)"
                );
            },
            Err(error) => {
                warn!(
                    %error,
                    retry_in_ms = backoff.as_millis(),
                    "Query result cache: cannot listen to pg_tviews' changes; writes this \
                     server does not run are not heard until it can"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            },
        }
    }
}

/// Apply what `session` receives to `listener` until the session is gone, or stops
/// delivering: every [`HEARTBEAT`] it sends `probe` to itself, and a probe not back by the
/// next one ends it, as a lost connection does (a half-open TCP connection, or one that has
/// stalled, would otherwise pass for a live feed). Every beat also reads [`FEED_GAPS`], and
/// clears the cache when it moved.
async fn relay(
    session: &mut NotifySession,
    listener: &TviewsListener,
    probe: &str,
    gaps: &mut Gaps,
) {
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let mut probe_outstanding = false;
    loop {
        tokio::select! {
            notification = session.recv() => match notification {
                Some(n) if n.channel == CHANNEL => {
                    metrics::counter!("fraiseql_tviews_feed_received_total").increment(1);
                    listener.receive(&n.payload);
                },
                Some(n) if n.channel == probe => probe_outstanding = false,
                Some(_) => {},
                None => return,
            },
            _ = heartbeat.tick() => {
                if probe_outstanding {
                    warn!("Query result cache: pg_tviews' change feed stopped delivering");
                    return;
                }
                // Bounded: on a half-open connection the reply never comes.
                match tokio::time::timeout(PROBE_TIMEOUT, session.notify(probe, "")).await {
                    Ok(Ok(())) => probe_outstanding = true,
                    _ => return,
                }
                match gaps.beat(read_gaps(session).await) {
                    Beat::Keep => {},
                    Beat::Clear => {
                        metrics::counter!("fraiseql_tviews_feed_gaps_total").increment(1);
                        warn!(
                            "Query result cache: pg_tviews committed transactions without \
                             publishing them (its NOTIFY queue was over 90 % full); the cache \
                             was cleared"
                        );
                        listener.missed();
                    },
                    Beat::End => return,
                }
            },
        }
    }
}

/// Open a session, `LISTEN` on [`CHANNEL`], and check it receives notifications, on a
/// channel of this server's own (a probe on [`CHANNEL`] would reach every other listener as
/// an unreadable message).
async fn listening_session(
    connector: &NotifyConnector,
    probe: &str,
) -> fraiseql_core::error::Result<NotifySession> {
    super::notify_session::verified_session(connector, &[CHANNEL, probe], probe, "", |n| {
        n.channel == probe
    })
    .await
}

#[cfg(test)]
#[path = "tviews_feed_tests.rs"]
mod tviews_feed_tests;
