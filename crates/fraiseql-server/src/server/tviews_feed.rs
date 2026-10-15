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

use std::time::Duration;

use fraiseql_core::{
    cache::tviews_feed::{CHANNEL, TviewsListener},
    db::postgres::{NotifyConnector, NotifySession},
};
use tracing::{info, warn};

/// The first wait before reconnecting; it doubles per failure, up to [`MAX_BACKOFF`].
/// How often a listening session checks it still receives what it sends: a probe not back
/// by the next beat ends the session.
const HEARTBEAT: Duration = Duration::from_secs(5);
use super::notify_session::PROBE_TIMEOUT;
const MIN_BACKOFF: Duration = Duration::from_millis(200);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Listen on [`CHANNEL`] over sessions `connector` opens, as server `instance`, applying
/// every message to `listener`'s cache. Reconnects with backoff; never returns.
pub async fn run(connector: NotifyConnector, listener: TviewsListener, instance: String) {
    let probe = format!("fraiseql_tviews_probe_{instance}");
    let mut backoff = MIN_BACKOFF;
    loop {
        match listening_session(&connector, &probe).await {
            Ok(mut session) => {
                backoff = MIN_BACKOFF;
                listener.listening();
                metrics::gauge!("fraiseql_tviews_feed_connected").set(1.0);
                info!(channel = CHANNEL, "Query result cache: listening to pg_tviews' changes");

                relay(&mut session, &listener, &probe).await;

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
/// stalled, would otherwise pass for a live feed).
async fn relay(session: &mut NotifySession, listener: &TviewsListener, probe: &str) {
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
