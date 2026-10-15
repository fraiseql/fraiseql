//! A notification session a server listens on, checked to hear what it sends (#1577, #1589).
//!
//! A `LISTEN` through a pooler in transaction mode succeeds and receives nothing, which would
//! pass for a working feed. So a session is only handed out once it has received a probe it
//! sent itself.

use std::time::Duration;

use fraiseql_core::{
    db::postgres::{Notification, NotifyConnector, NotifySession},
    error::{FraiseQLError, Result},
};

/// How long a new session may take to receive its own probe back.
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Open a session, `LISTEN` on each of `channels`, send `probe` on `probe_channel`, and hand
/// the session out once `heard` recognises a notification as the probe. Notifications that
/// arrive first are dropped: the caller clears its cache once it has the session, which
/// covers every write they name.
pub(super) async fn verified_session(
    connector: &NotifyConnector,
    channels: &[&str],
    probe_channel: &str,
    probe: &str,
    heard: impl Fn(&Notification) -> bool + Send + Sync,
) -> Result<NotifySession> {
    let mut session = connector.connect().await?;
    for channel in channels {
        session.listen(channel).await?;
    }
    session.notify(probe_channel, probe).await?;
    let received = tokio::time::timeout(PROBE_TIMEOUT, async {
        while let Some(notification) = session.recv().await {
            if heard(&notification) {
                return true;
            }
        }
        false
    })
    .await;
    match received {
        Ok(true) => Ok(session),
        Ok(false) => Err(FraiseQLError::ConnectionPool {
            message: "the notification connection closed before it heard itself".to_string(),
        }),
        Err(_) => Err(FraiseQLError::ConnectionPool {
            message: format!(
                "the notification connection did not receive its own message on \
                 `{probe_channel}` within {}s; is `database_url` a pooler in transaction mode? \
                 `LISTEN` needs a session of its own",
                PROBE_TIMEOUT.as_secs()
            ),
        }),
    }
}
