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

pub mod admin;
pub mod config;
pub mod correlation;
pub mod cursor;
pub mod imap;
pub mod probe;
pub mod smtp;
pub mod store;
pub mod tracking;
pub mod warming;
pub mod worker;

pub use admin::{SuppressionAdminState, suppression_admin_router};
pub use config::{
    ImapConfig, MailboxConfig, MailboxSmtpConfig, ReturnPathConfig, RoutingRuleConfig,
    SendSettings, SmtpTlsMode,
};
pub use correlation::correlate;
pub use cursor::Cursor;
pub use imap::{FetchBatch, FetchedMessage, ImapMailboxFetcher, MailboxFetcher};
pub use probe::{ProbeOutcome, probe_recipient, run_return_path_probe};
pub use smtp::{SmtpMailboxTransport, build_email_transport};
pub use store::PostgresEmailCursorStore;
pub use tracking::{
    CorrelatedSend, PgSendTracker, RecordedSend, SendCorrelator, SendTracker, SentRecord,
    SuppressionReason,
};
pub use warming::{SendCounter, WarmingState, warming_daily_limit};
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

/// Build a poll worker (and its interval) for each mailbox with an IMAP half.
///
/// A mailbox with no `[mailbox.<name>.imap]` half is skipped (send-only). A mailbox
/// whose `password_env` is unset, or whose TLS connector cannot be built, is
/// skipped with a warning rather than started without credentials. `get_env`
/// resolves the password env (in production, [`std::env::var`]); `sink` is the
/// storage backend attachments stream into (`None` drops attachments); `hooks`
/// fire `after:ingest:email` (`None` ingests without dispatch).
#[must_use]
#[allow(clippy::too_many_arguments)] // Reason: worker assembly wires several independent collaborators.
pub fn build_workers<S: std::hash::BuildHasher>(
    mailboxes: &HashMap<String, MailboxConfig, S>,
    pool: &sqlx::PgPool,
    hooks: Option<&Arc<BeforeMutationHooks>>,
    sink: Option<&Arc<dyn StorageBackend>>,
    correlator: Option<&Arc<dyn SendCorrelator>>,
    address_hash_key: Option<&Arc<[u8]>>,
    challenge_suppress_after: u32,
    get_env: impl Fn(&str) -> Option<String>,
) -> Vec<(EmailPollWorker, Duration)> {
    let mut workers = Vec::new();
    for (name, mailbox) in mailboxes {
        // Send-only mailboxes have no IMAP half — nothing to poll.
        let Some(imap) = mailbox.imap.as_ref() else {
            continue;
        };
        let Some(password) = get_env(&imap.password_env) else {
            warn!(
                mailbox = %name,
                password_env = %imap.password_env,
                "poll-IMAP mailbox not started: password env is unset"
            );
            continue;
        };
        let fetcher = match ImapMailboxFetcher::new(
            &imap.host,
            imap.port,
            &imap.username,
            password,
            &imap.mailbox,
        ) {
            Ok(fetcher) => Arc::new(fetcher),
            Err(error) => {
                warn!(mailbox = %name, %error, "poll-IMAP mailbox not started: TLS setup failed");
                continue;
            },
        };
        let routing_rules = imap.routing.iter().map(RoutingRuleConfig::to_rule).collect();
        let worker = EmailPollWorker::new(
            name.clone(),
            fetcher,
            pool.clone(),
            routing_rules,
            imap.batch_size,
            imap.attachment_bucket.clone(),
            sink.cloned(),
            hooks.cloned(),
            correlator.cloned(),
            address_hash_key.cloned(),
            challenge_suppress_after,
        );
        let interval = Duration::from_secs(imap.poll_interval_secs.max(1));
        info!(
            mailbox = %name,
            host = %imap.host,
            folder = %imap.mailbox,
            "poll-IMAP mailbox configured"
        );
        workers.push((worker, interval));
    }
    workers
}

/// How long to wait for a startup Return-Path probe to land before warning.
const PROBE_TIMEOUT: Duration = Duration::from_secs(120);
/// How often to poll the mailbox while waiting for the probe.
const PROBE_INTERVAL: Duration = Duration::from_secs(10);

/// Run a Return-Path probe at startup for each mailbox with both halves.
///
/// Opt-in (`[send] verp_probe_on_start`): for every mailbox that both sends (SMTP)
/// and receives (IMAP), send a self-addressed `bounces+probe-<nonce>@…` and confirm
/// it lands with the plus-tag intact — proving the provider preserves plus-
/// addressing, without which VERP delivery correlation silently fails. The outcome
/// is logged loudly; a probe never blocks a send. `get_env` resolves the account
/// passwords (in production, [`std::env::var`]).
pub async fn run_startup_probes<S: std::hash::BuildHasher + Sync>(
    mailboxes: &HashMap<String, MailboxConfig, S>,
    get_env: impl Fn(&str) -> Option<String> + Send,
) {
    for (name, mailbox) in mailboxes {
        // Only a mailbox that both sends and receives can be self-probed.
        let (Some(imap), Some(smtp)) = (mailbox.imap.as_ref(), mailbox.smtp.as_ref()) else {
            continue;
        };
        let Some(transport) =
            SmtpMailboxTransport::build(std::iter::once((name.as_str(), smtp)), &get_env)
        else {
            continue; // password unset / relay build failed — already warned by build
        };
        let Some(password) = get_env(&imap.password_env) else {
            continue;
        };
        let fetcher = match ImapMailboxFetcher::new(
            &imap.host,
            imap.port,
            &imap.username,
            password,
            &imap.mailbox,
        ) {
            Ok(fetcher) => fetcher,
            Err(error) => {
                warn!(mailbox = %name, %error, "Return-Path probe skipped: IMAP TLS setup failed");
                continue;
            },
        };

        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let sender = fraiseql_functions::SenderIdentity {
            address:      smtp.address.clone(),
            display_name: None,
        };
        let probe_to = probe::probe_recipient(
            smtp.return_path_local_part(),
            smtp.return_path_domain(),
            &nonce,
        );
        match probe::run_return_path_probe(
            &transport,
            &fetcher,
            &sender,
            &probe_to,
            &nonce,
            PROBE_TIMEOUT,
            PROBE_INTERVAL,
        )
        .await
        {
            Ok(ProbeOutcome::Confirmed) => info!(
                mailbox = %name,
                "VERP Return-Path probe confirmed — delivery correlation is active"
            ),
            Ok(ProbeOutcome::NotObserved) => warn!(
                mailbox = %name,
                "VERP Return-Path probe did NOT land within the window — plus-addressing may be \
                 stripped by the provider; delivery correlation may not work (sends still go out, \
                 but bounces/challenges/replies will not be tracked)"
            ),
            Err(error) => {
                warn!(mailbox = %name, %error, "VERP Return-Path probe failed to run");
            },
        }
    }
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
