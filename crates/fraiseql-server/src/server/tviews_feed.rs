//! The server's listener on pg_tviews' change feed (#1589).
//!
//! When the database publishes rows (`pg_tviews.publish_changes = 'rows'`), every committed
//! transaction that changed TVIEW rows says which on [`CHANNEL`], whoever ran it: this
//! server, another one, a migration, another application. One task per server listens on a
//! PostgreSQL connection of its own and evicts what each message names
//! ([`fraiseql_core::cache::tviews_feed`]).
//!
//! Messages sent while no connection listens are lost. So every connection, the first and
//! each one after a loss, follows pg_tviews' reconnect rule: `LISTEN`, check the session
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

/// How long a new connection may take to receive its own probe back.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// The first wait before reconnecting; it doubles per failure, up to [`MAX_BACKOFF`].
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

                while let Some(notification) = session.recv().await {
                    if notification.channel == CHANNEL {
                        metrics::counter!("fraiseql_tviews_feed_received_total").increment(1);
                        listener.receive(&notification.payload);
                    }
                }

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

/// Open a session, `LISTEN` on [`CHANNEL`], and check it receives notifications, on a
/// channel of this server's own (a probe on [`CHANNEL`] would reach every other listener
/// as an unreadable message).
async fn listening_session(
    connector: &NotifyConnector,
    probe: &str,
) -> fraiseql_core::error::Result<NotifySession> {
    let mut session = connector.connect().await?;
    session.listen(CHANNEL).await?;
    session.listen(probe).await?;
    session.notify(probe, "").await?;

    // Messages on CHANNEL arriving before the probe are dropped: `listening()`, which
    // follows, clears the cache, which covers every write they name.
    let heard = tokio::time::timeout(PROBE_TIMEOUT, async {
        while let Some(notification) = session.recv().await {
            if notification.channel == probe {
                return true;
            }
        }
        false
    })
    .await;
    match heard {
        Ok(true) => Ok(session),
        Ok(false) => Err(fraiseql_core::error::FraiseQLError::ConnectionPool {
            message: "the notification connection closed before it heard itself".to_string(),
        }),
        Err(_) => Err(fraiseql_core::error::FraiseQLError::ConnectionPool {
            message: format!(
                "the notification connection did not receive its own message within {}s; is \
                 `database_url` a pooler in transaction mode? `LISTEN` needs a session of its \
                 own",
                PROBE_TIMEOUT.as_secs()
            ),
        }),
    }
}
