//! The per-mailbox poll loop, rebuilt on the generic source primitives (#573).
//!
//! Replaces the bespoke `EmailPollWorker` loop: it drives the generic source
//! envelope ([`fraiseql_functions::run_source_once`]) with the email
//! [`ImapSource`] and [`EmailIngestSink`] under a single-firing
//! [`LeaseGuardedRunner`], so a mailbox polled by several replicas is polled by
//! exactly one — fixing the multi-replica double-poll that the old per-replica
//! worker had.

use std::time::Duration;

use fraiseql_functions::{SourceOutcome, run_source_once};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore};
use tracing::{debug, info, warn};

use super::{sink::EmailIngestSink, source::ImapSource};

/// A poll loop for one mailbox: the source, its durable sink, the cursor store the
/// envelope reads, and the advisory lease that makes polling single-firing.
pub struct MailboxPoller {
    source: ImapSource,
    sink:   EmailIngestSink,
    store:  PostgresSourceCursorStore,
    runner: LeaseGuardedRunner,
}

impl MailboxPoller {
    /// Assemble a poller. The `runner` must be keyed on the mailbox name (so each
    /// mailbox coordinates on its own lease and cursor); `store` and the sink's own
    /// handles must share the mailbox's pool.
    #[must_use]
    pub const fn new(
        source: ImapSource,
        sink: EmailIngestSink,
        store: PostgresSourceCursorStore,
        runner: LeaseGuardedRunner,
    ) -> Self {
        Self {
            source,
            sink,
            store,
            runner,
        }
    }

    /// Poll forever on `interval`, logging and continuing past any poll error.
    ///
    /// Shutdown is by task abort (the server drives the poller on its `JoinSet`).
    pub async fn poll_forever(&self, interval: Duration) {
        let mailbox = self.runner.source_name().to_string();
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        info!(
            mailbox = %mailbox,
            interval_secs = interval.as_secs(),
            "poll-IMAP email source started"
        );
        loop {
            ticker.tick().await;
            match run_source_once(&self.runner, &self.store, &self.source, &self.sink).await {
                Ok(SourceOutcome::Ingested { messages }) => {
                    info!(mailbox = %mailbox, count = messages, "poll: ingested new mail");
                },
                Ok(SourceOutcome::NoData) => {
                    debug!(mailbox = %mailbox, "poll: no new mail");
                },
                Ok(SourceOutcome::SkippedNotLeader) => {
                    debug!(mailbox = %mailbox, "poll: skipped — another replica holds the lease");
                },
                Ok(SourceOutcome::CursorRaceLost) => {
                    warn!(mailbox = %mailbox, "poll: cursor race lost — rolled back, will retry");
                },
                Err(error) => {
                    warn!(mailbox = %mailbox, %error, "poll failed; cursor held, will retry");
                },
            }
        }
    }
}
