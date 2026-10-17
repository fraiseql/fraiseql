//! A dedicated PostgreSQL connection for `LISTEN`/`NOTIFY` (#1577).
//!
//! A pooled connection cannot listen: the pool's driver task discards the notifications a
//! connection receives, and a connection handed back to the pool takes its `LISTEN` with it
//! to whichever request borrows it next. A [`NotifySession`] is one connection of its own,
//! opened with the same URL and the same `[database_tls]` settings as the pool, whose
//! notifications are forwarded to its owner.

use std::{future::poll_fn, str::FromStr};

use fraiseql_error::{FraiseQLError, Result};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};
use tokio_postgres::{AsyncMessage, Client, Connection, NoTls};

use super::{PostgresConnector, PostgresSslMode, PostgresTlsConfig, pg_detail};

/// The `application_name` a session reports when the URL names none, so an operator (and
/// `pg_stat_activity`) can tell the feed's connection from the pool's.
pub const NOTIFY_APPLICATION_NAME: &str = "fraiseql-notify";

/// The longest `application_name` PostgreSQL keeps (`NAMEDATALEN - 1`).
const MAX_APPLICATION_NAME_BYTES: usize = 63;

/// One notification received on a channel the session listens to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// The channel it was sent on.
    pub channel:    String,
    /// Its payload.
    pub payload:    String,
    /// The backend process that sent it.
    pub process_id: i32,
}

/// Opens [`NotifySession`]s: the connection settings of the server's pool, without the pool.
#[derive(Debug, Clone)]
pub struct NotifyConnector {
    config: tokio_postgres::Config,
    tls:    PostgresTlsConfig,
}

impl NotifyConnector {
    /// Settings for sessions to the database `connection_string` names, secured as `tls`
    /// says.
    ///
    /// An explicit `tls` mode overrides a `sslmode` in the URL, and no mode leaves the URL's
    /// in force: the rule the pool follows, so the two connections cannot disagree.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Configuration`] when the URL cannot be parsed or `tls` contradicts
    /// itself.
    pub fn new(connection_string: &str, tls: PostgresTlsConfig) -> Result<Self> {
        tls.validate()?;
        let mut config = tokio_postgres::Config::from_str(connection_string).map_err(|e| {
            FraiseQLError::Configuration {
                message: format!("invalid database URL for the notification connection: {e}"),
            }
        })?;
        if let Some(mode) = tls.mode {
            config.ssl_mode(match mode {
                PostgresSslMode::Disable => tokio_postgres::config::SslMode::Disable,
                PostgresSslMode::Prefer => tokio_postgres::config::SslMode::Prefer,
                PostgresSslMode::Require | PostgresSslMode::VerifyFull => {
                    tokio_postgres::config::SslMode::Require
                },
            });
        }
        if config.get_application_name().is_none() {
            config.application_name(NOTIFY_APPLICATION_NAME);
        }
        Ok(Self { config, tls })
    }

    /// The same settings, for sessions serving `purpose`: they report
    /// `<application_name>/<purpose>`, the URL's name or [`NOTIFY_APPLICATION_NAME`] first,
    /// so a server's sessions for different feeds can be told apart in `pg_stat_activity`.
    /// Cut to PostgreSQL's 63 bytes, at a character boundary.
    #[must_use]
    pub fn serving(mut self, purpose: &str) -> Self {
        let base = self.config.get_application_name().unwrap_or(NOTIFY_APPLICATION_NAME);
        let mut name = format!("{base}/{purpose}");
        while name.len() > MAX_APPLICATION_NAME_BYTES {
            name.pop();
        }
        self.config.application_name(&name);
        self
    }

    /// Open a session.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::ConnectionPool`] when the database cannot be reached, and
    /// [`FraiseQLError::Configuration`] when the TLS settings cannot be built.
    pub async fn connect(&self) -> Result<NotifySession> {
        let (sender, notifications) = mpsc::unbounded_channel();
        let client = match self.tls.connector()? {
            PostgresConnector::Plaintext => {
                let (client, connection) =
                    self.config.connect(NoTls).await.map_err(|e| connect_error(&e))?;
                tokio::spawn(forward_notifications(connection, sender));
                client
            },
            PostgresConnector::Tls(connector) => {
                let (client, connection) =
                    self.config.connect(*connector).await.map_err(|e| connect_error(&e))?;
                tokio::spawn(forward_notifications(connection, sender));
                client
            },
        };
        Ok(NotifySession {
            client,
            notifications,
        })
    }
}

fn connect_error(e: &tokio_postgres::Error) -> FraiseQLError {
    FraiseQLError::ConnectionPool {
        message: format!("failed to open the notification connection: {}", pg_detail(e)),
    }
}

/// Drive `connection`, handing every notification to `sender`, until the connection ends
/// or nobody receives any more.
async fn forward_notifications<S, T>(
    mut connection: Connection<S, T>,
    sender: mpsc::UnboundedSender<Notification>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
    T: AsyncRead + AsyncWrite + Unpin,
{
    while let Some(message) = poll_fn(|cx| connection.poll_message(cx)).await {
        match message {
            Ok(AsyncMessage::Notification(n)) => {
                let notification = Notification {
                    channel:    n.channel().to_string(),
                    payload:    n.payload().to_string(),
                    process_id: n.process_id(),
                };
                if sender.send(notification).is_err() {
                    return;
                }
            },
            Ok(_) => {},
            Err(e) => {
                tracing::warn!(error = %pg_detail(&e), "notification connection lost");
                return;
            },
        }
    }
}

/// One connection that listens, and sends notifications.
///
/// [`recv`](Self::recv) answers `None` once the connection is gone: from then on nothing
/// sent on a listened channel reaches this session, and its owner must open another.
pub struct NotifySession {
    client:        Client,
    notifications: mpsc::UnboundedReceiver<Notification>,
}

impl NotifySession {
    /// `LISTEN` on `channel`.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Database`] when the statement fails.
    pub async fn listen(&self, channel: &str) -> Result<()> {
        let statement = format!("LISTEN {}", quote_identifier(channel));
        self.client
            .batch_execute(&statement)
            .await
            .map_err(|e| super::database_error(format!("LISTEN failed: {}", pg_detail(&e)), &e))
    }

    /// Send `payload` on `channel`, outside any transaction: it is delivered at once.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Database`] when the statement fails (the connection is gone, or the
    /// payload is 8000 bytes or longer).
    pub async fn notify(&self, channel: &str, payload: &str) -> Result<()> {
        self.client
            .execute("SELECT pg_notify($1, $2)", &[&channel, &payload])
            .await
            .map(drop)
            .map_err(|e| super::database_error(format!("NOTIFY failed: {}", pg_detail(&e)), &e))
    }

    /// Run `statement`, which returns one `bigint`, on this session's connection, between
    /// the notifications it receives.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Database`], with the statement's SQLSTATE, when it fails.
    pub async fn query_bigint(&self, statement: &str) -> Result<i64> {
        self.client.query_one(statement, &[]).await.map(|row| row.get(0)).map_err(|e| {
            super::database_error(format!("{statement} failed: {}", pg_detail(&e)), &e)
        })
    }

    /// The next notification, or `None` once the connection is gone.
    pub async fn recv(&mut self) -> Option<Notification> {
        self.notifications.recv().await
    }
}

/// `name` as a quoted SQL identifier.
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod tests;
