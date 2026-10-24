//! Poll-IMAP inbound email.

#[cfg(feature = "inbound-email")]
use tracing::info;

#[cfg(feature = "inbound-email")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "inbound-email")]
use crate::{
    capability::Started,
    server::provision::{BootCx, ProvisionStep},
};

/// A `[mailbox.<name>.imap]` half, on a server with a database pool.
#[derive(Debug)]
pub struct InboundEmail;

#[async_trait::async_trait]
impl Capability for InboundEmail {
    #[cfg(feature = "inbound-email")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.db_pool.is_some()
            && cx.server.config.mailbox.values().any(|mailbox| mailbox.imap.is_some())
    }

    /// The inbound spine (shared with webhooks, so a mailbox-only deployment has it too),
    /// the mailbox cursors, and the send-tracking tables the correlator writes.
    #[cfg(feature = "inbound-email")]
    fn provision(&self, cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        let server = cx.server;
        let polled = server.config.mailbox.values().any(|mailbox| mailbox.imap.is_some());
        match server.db_pool.as_ref() {
            Some(pool) if polled => vec![ProvisionStep::new(
                "inbound email",
                pool,
                &[
                    fraiseql_functions::migrations::inbound_migration_sql(),
                    fraiseql_observers::migrations::source_cursor_sql(),
                    fraiseql_functions::migrations::send_tracking_migration_sql(),
                ],
            )],
            _ => Vec::new(),
        }
    }

    /// One background poll loop per IMAP mailbox, on the server's task set so graceful
    /// shutdown aborts them. The workers reuse the durable inbound spine and the
    /// `after:ingest` dispatch path; attachments stream into the configured storage
    /// backend.
    #[cfg(feature = "inbound-email")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        use crate::inbound::email;

        let app_state = cx.app;
        let server = &mut *cx.server;
        let Some(db_pool) = server.db_pool.as_ref() else {
            return Ok(Started::Running);
        };

        // The correlator transitions send-status and suppression from inbound signals.
        let tracker = std::sync::Arc::new(email::PgSendTracker::new(db_pool.clone()));
        let correlator =
            std::sync::Arc::clone(&tracker) as std::sync::Arc<dyn email::SendCorrelator>;
        let address_hash_key = server.build_address_hash_key();

        // The attachment sink is the server's one configured storage
        // backend. It used to hang off a separate `with_storage` builder
        // method that no binary ever called, so `[mailbox]
        // attachment_bucket` was unreachable from the shipped server and
        // attachments were dropped with a warning.
        let sink = server.storage_state.as_ref().map(|state| {
            std::sync::Arc::new(email::StorageAttachmentSink::new(state.backend.clone()))
                as std::sync::Arc<dyn fraiseql_functions::host::live::storage::StorageBackend>
        });
        // Opt-in Return-Path probe: verify the provider preserves
        // plus-addressing before trusting VERP correlation. Blocks startup
        // up to the probe window per eligible mailbox; off by default.
        if server.config.send.verp_probe_on_start {
            email::run_startup_probes(&server.config.mailbox, |name| std::env::var(name).ok())
                .await;
        }

        let hooks = app_state.before_mutation_hooks.clone();
        // #594: the after:ingest `fraiseql_query` bridge factory, built over the
        // app's hot-reloadable executor (same one the route handlers use); only
        // meaningful when function-dispatch hooks are present.
        let query_executor_factory = hooks.as_ref().map(|_| {
            crate::routes::after_mutation::make_query_executor_factory(
                crate::query_bridge::ExecutorSource::of(app_state),
            )
        });
        let pollers = email::build_pollers(
            &server.config.mailbox,
            db_pool,
            hooks.as_ref(),
            query_executor_factory.as_ref(),
            sink.as_ref(),
            Some(&correlator),
            address_hash_key.as_ref(),
            server.config.send.challenge_suppress_after,
            server.config.inbound_dispatch.settings().map_or(
                crate::inbound::dispatch_ledger::LedgerSettings::default().lease,
                |settings| settings.lease,
            ),
            |name| std::env::var(name).ok(),
        );
        let started = pollers.len();
        for (poller, interval) in pollers {
            server.tasks.spawn(async move { poller.poll_forever(interval).await });
        }
        info!(mailboxes = started, "poll-IMAP email sources started");
        Ok(Started::Running)
    }

    /// The suppression admin API (append + query): the operator surface for manual
    /// do-not-contact entries (support removals, GDPR requests), behind the admin bearer
    /// gate. Mounted only with a database pool, the admin token and the address-hash key
    /// (the server HMAC secret), since an address is hashed server-side before it touches
    /// the store. A send-only deployment has it too: it does not need a polled mailbox.
    #[cfg(feature = "inbound-email")]
    fn routes(&self, cx: &ServeCx<'_>) -> Option<axum::Router> {
        let server = cx.server;
        let (Some(pool), Some(token), Some(key)) = (
            server.db_pool.as_ref(),
            server.config.admin_token.as_ref(),
            server.build_address_hash_key(),
        ) else {
            return None;
        };
        let tracker = std::sync::Arc::new(crate::inbound::email::PgSendTracker::new(pool.clone()));
        let suppression_state =
            std::sync::Arc::new(crate::inbound::email::SuppressionAdminState::new(tracker, key));
        let auth_state = crate::server::BearerAuthState::with_max_failures(
            token.clone(),
            server.config.admin_auth_max_failures,
        );
        info!(
            "Suppression admin API enabled (POST /api/email/suppress, POST \
             /api/email/suppression; admin bearer token required)"
        );
        Some(crate::inbound::email::suppression_admin_router(suppression_state).route_layer(
            axum::middleware::from_fn_with_state(auth_state, crate::server::bearer_auth_middleware),
        ))
    }
}
