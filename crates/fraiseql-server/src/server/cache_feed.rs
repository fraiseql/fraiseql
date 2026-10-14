//! The server's end of the result cache's feed to every other server (#1577).
//!
//! One task per server, on one PostgreSQL connection of its own: it `LISTEN`s on
//! [`CHANNEL`], sends what this server's writes evicted (after their commit), and applies
//! what every other server's evicted. What travels, and why after commit, is in
//! [`fraiseql_core::cache::feed`].
//!
//! The cache stores nothing unless this server is known to hear the feed: it starts
//! suspended, resumes once the connection has received the server's own message back (a
//! `LISTEN` through a pooler in transaction mode receives nothing, and would otherwise pass
//! for a working feed), and is suspended again the moment the connection is lost. Every
//! resume clears it, so whatever another server wrote while this one was not listening
//! cannot be served from it.

use std::time::Duration;

use fraiseql_core::{
    cache::feed::{
        CHANNEL, CacheEvictor, Eviction, OutboundQueue, Received, decode, encode, encode_hello,
    },
    db::postgres::{NotifyConnector, NotifySession},
};
use tracing::{info, warn};

/// The feed's halves a server holds until it serves.
pub struct CacheFeedParts {
    /// What this server's writes evicted, waiting to be sent.
    pub queue:   OutboundQueue,
    /// Applies what other servers evicted to this server's cache.
    pub evictor: CacheEvictor,
}

/// How long a new connection may take to receive its own message back.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// The first wait before reconnecting; it doubles per failure, up to [`MAX_BACKOFF`].
const MIN_BACKOFF: Duration = Duration::from_millis(200);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Why a connection stopped relaying.
enum Ended {
    /// The connection was lost; connect again.
    Disconnected,
    /// Every outbox is gone: the server's executor was dropped.
    Closed,
}

/// Relay `parts` over sessions `connector` opens, as server `instance`, until the server's
/// executor is gone. Reconnects with backoff; never returns while the server runs.
pub async fn run(connector: NotifyConnector, mut parts: CacheFeedParts, instance: String) {
    let mut backoff = MIN_BACKOFF;
    let mut connected_before = false;
    loop {
        match listening_session(&connector, &instance).await {
            Ok(mut session) => {
                backoff = MIN_BACKOFF;
                if connected_before {
                    metrics::counter!("fraiseql_cache_feed_reconnects_total").increment(1);
                }
                connected_before = true;
                parts.evictor.resume();
                metrics::gauge!("fraiseql_cache_feed_connected").set(1.0);
                info!(channel = CHANNEL, "Query result cache: listening to other servers' writes");

                let ended = relay(&mut session, &mut parts, &instance).await;

                parts.evictor.suspend();
                metrics::gauge!("fraiseql_cache_feed_connected").set(0.0);
                if matches!(ended, Ended::Closed) {
                    return;
                }
                warn!(
                    "Query result cache: the connection to other servers' writes was lost; \
                     nothing is cached until it is back"
                );
            },
            Err(error) => {
                warn!(
                    %error,
                    retry_in_ms = backoff.as_millis(),
                    "Query result cache: cannot listen to other servers' writes; nothing is \
                     cached until it can"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            },
        }
    }
}

/// Open a session, `LISTEN`, and check it receives what it sends.
async fn listening_session(
    connector: &NotifyConnector,
    instance: &str,
) -> fraiseql_core::error::Result<NotifySession> {
    let mut session = connector.connect().await?;
    session.listen(CHANNEL).await?;
    session.notify(CHANNEL, &encode_hello(instance)).await?;

    // The session hears only what was sent after its `LISTEN`, so a hello from this server
    // is the one just sent. Other servers' messages arriving first are dropped: the resume
    // that follows clears the cache, which covers every write they announce.
    let heard = tokio::time::timeout(HELLO_TIMEOUT, async {
        while let Some(notification) = session.recv().await {
            if matches!(decode(&notification.payload), Received::Hello { from } if from == instance)
            {
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
                "the notification connection did not receive its own message on `{CHANNEL}` \
                 within {}s; is `database_url` a pooler in transaction mode? `LISTEN` needs a \
                 session of its own",
                HELLO_TIMEOUT.as_secs()
            ),
        }),
    }
}

/// Send this server's evictions and apply other servers' until the session or the queue
/// ends.
async fn relay(session: &mut NotifySession, parts: &mut CacheFeedParts, instance: &str) -> Ended {
    // Announcements lost while no session could carry them.
    let lost = outgoing(&parts.queue, Vec::new());
    if !lost.is_empty() && !send(session, parts, instance, lost).await {
        return Ended::Disconnected;
    }
    loop {
        tokio::select! {
            notification = session.recv() => match notification {
                Some(n) => apply(&parts.evictor, &n.payload, instance),
                None => return Ended::Disconnected,
            },
            batch = parts.queue.next_batch() => match batch {
                Some(batch) => {
                    let batch = outgoing(&parts.queue, batch);
                    if !send(session, parts, instance, batch).await {
                        return Ended::Disconnected;
                    }
                },
                None => return Ended::Closed,
            },
        }
    }
}

/// What to send for `batch`: itself, or, when announcements were lost since the last send
/// (the queue overflowed, or a send failed), [`Eviction::Everything`]: no other server can
/// know what the lost ones evicted.
pub(super) fn outgoing(queue: &OutboundQueue, batch: Vec<Eviction>) -> Vec<Eviction> {
    if queue.take_overflow() {
        metrics::counter!("fraiseql_cache_feed_lost_total").increment(1);
        vec![Eviction::Everything]
    } else {
        batch
    }
}

/// Send `batch`; on failure, record it lost (the next session evicts everything everywhere)
/// and answer `false`.
async fn send(
    session: &NotifySession,
    parts: &CacheFeedParts,
    instance: &str,
    batch: Vec<Eviction>,
) -> bool {
    for payload in encode(instance, batch) {
        if let Err(error) = session.notify(CHANNEL, &payload).await {
            warn!(%error, "Query result cache: an eviction could not be sent to other servers");
            parts.queue.mark_lost();
            return false;
        }
        metrics::counter!("fraiseql_cache_feed_sent_total").increment(1);
    }
    true
}

/// Apply a message another server sent; this server's own come back too, and are skipped.
fn apply(evictor: &CacheEvictor, payload: &str, instance: &str) {
    let Received::Evictions { from, evictions } = decode(payload) else {
        return;
    };
    if from == instance {
        return;
    }
    metrics::counter!("fraiseql_cache_feed_received_total").increment(1);
    for eviction in &evictions {
        if let Err(error) = evictor.apply(eviction) {
            warn!(%error, "Query result cache: another server's eviction could not be applied");
            // Evicting less than another server's write requires is a stale answer.
            let _ = evictor.apply(&Eviction::Everything);
        }
    }
}

#[cfg(test)]
#[path = "cache_feed_tests.rs"]
mod cache_feed_tests;
