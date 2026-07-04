//! The poll-IMAP email adapter — the first PULL inbound [`Source`].
//!
//! [`Source`]: fraiseql_functions::Source
//!
//! Email is the inbound mirror's first *pull* transport (webhooks are *push*).
//! The adapter is stateless-with-a-cursor: it does not hold a long-lived IMAP
//! connection (no IMAP-IDLE), it polls. Each poll fetches the messages above a
//! per-mailbox UID watermark, normalizes their MIME through the shared
//! [`fraiseql_functions`] layer (the same
//! [`InboundMessage`](fraiseql_functions::InboundMessage) the webhook adapter
//! produces), emits them onto the durable spine, and fires `after:ingest:email`
//! functions — exactly the path the push adapter uses, with a different edge.
//!
//! ```text
//!   IMAP mailbox ─poll─► fetch(uid > cursor) ─► normalize ─► spine ─► after:ingest:email
//!   (imap.rs)            (cursor.rs watermark)   (functions)  (emit)   (worker.rs dispatch)
//! ```
//!
//! ## Modules
//!
//! - [`config`] — the `[imap.<name>]` mailbox configuration.
//! - [`cursor`] — the pure UID-watermark arithmetic.
//! - [`imap`] — the TLS IMAP transport ([`MailboxFetcher`]).
//! - [`store`] — the durable per-mailbox cursor store.
//! - [`worker`] — the poll loop that drives it all.

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use fraiseql_functions::host::live::storage::StorageBackend;
use tracing::{info, warn};

pub mod config;
pub mod cursor;
pub mod imap;
pub mod store;
pub mod worker;

pub use config::{ImapMailboxConfig, RoutingRuleConfig};
pub use cursor::Cursor;
pub use imap::{FetchBatch, FetchedMessage, ImapMailboxFetcher, MailboxFetcher};
pub use store::PostgresEmailCursorStore;
pub use worker::EmailPollWorker;

use crate::subsystems::BeforeMutationHooks;

/// Create the email cursor table (idempotent). Call once on startup.
///
/// # Errors
///
/// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database) if
/// the DDL fails.
pub async fn init_cursor_store(pool: &sqlx::PgPool) -> fraiseql_error::Result<()> {
    PostgresEmailCursorStore::new(pool.clone()).init().await
}

/// Build a poll worker (and its interval) for each configured mailbox.
///
/// A mailbox whose `password_env` is unset, or whose TLS connector cannot be
/// built, is skipped with a warning rather than started without credentials.
/// `get_env` resolves the password env (in production, [`std::env::var`]); `sink`
/// is the storage backend attachments stream into (`None` drops attachments);
/// `hooks` fire `after:ingest:email` (`None` ingests without dispatch).
#[must_use]
pub fn build_workers<S: std::hash::BuildHasher>(
    mailboxes: &HashMap<String, ImapMailboxConfig, S>,
    pool: &sqlx::PgPool,
    hooks: Option<&Arc<BeforeMutationHooks>>,
    sink: Option<&Arc<dyn StorageBackend>>,
    get_env: impl Fn(&str) -> Option<String>,
) -> Vec<(EmailPollWorker, Duration)> {
    let mut workers = Vec::new();
    for (name, config) in mailboxes {
        let Some(password) = get_env(&config.password_env) else {
            warn!(
                mailbox = %name,
                password_env = %config.password_env,
                "poll-IMAP mailbox not started: password env is unset"
            );
            continue;
        };
        let fetcher = match ImapMailboxFetcher::new(
            &config.host,
            config.port,
            &config.username,
            password,
            &config.mailbox,
        ) {
            Ok(fetcher) => Arc::new(fetcher),
            Err(error) => {
                warn!(mailbox = %name, %error, "poll-IMAP mailbox not started: TLS setup failed");
                continue;
            },
        };
        let routing_rules = config.routing.iter().map(RoutingRuleConfig::to_rule).collect();
        let worker = EmailPollWorker::new(
            name.clone(),
            fetcher,
            pool.clone(),
            routing_rules,
            config.batch_size,
            config.attachment_bucket.clone(),
            sink.cloned(),
            hooks.cloned(),
        );
        let interval = Duration::from_secs(config.poll_interval_secs.max(1));
        info!(
            mailbox = %name,
            host = %config.host,
            folder = %config.mailbox,
            "poll-IMAP mailbox configured"
        );
        workers.push((worker, interval));
    }
    workers
}

/// An attachment sink backed by the server's legacy storage.
///
/// Bridges the object-storage [`StorageBackend`] writes to the server's
/// [`StorageBackend`](crate::storage::StorageBackend), folding the logical bucket
/// into the object key (`<bucket>/<key>`).
pub struct LegacyStorageSink {
    backend: Arc<dyn crate::storage::StorageBackend>,
}

impl LegacyStorageSink {
    /// Wrap a legacy storage backend as an attachment sink.
    #[must_use]
    pub fn new(backend: Arc<dyn crate::storage::StorageBackend>) -> Self {
        Self { backend }
    }

    /// The flat key a `(bucket, key)` pair maps to.
    fn flat_key(bucket: &str, key: &str) -> String {
        format!("{bucket}/{key}")
    }
}

impl StorageBackend for LegacyStorageSink {
    fn get(
        &self,
        bucket: &str,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<Vec<u8>>> + Send + '_>> {
        let full = Self::flat_key(bucket, key);
        let backend = Arc::clone(&self.backend);
        Box::pin(async move { backend.download(&full).await.map_err(Into::into) })
    }

    fn put(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        content_type: &str,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<()>> + Send + '_>> {
        let full = Self::flat_key(bucket, key);
        let body = body.to_vec();
        let content_type = content_type.to_string();
        let backend = Arc::clone(&self.backend);
        Box::pin(async move {
            backend
                .upload(&full, &body, &content_type)
                .await
                .map(|_| ())
                .map_err(Into::into)
        })
    }
}
