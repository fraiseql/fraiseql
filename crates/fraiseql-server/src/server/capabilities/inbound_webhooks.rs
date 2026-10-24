//! The inbound webhook receiver.

#[cfg(feature = "inbound")]
use super::ServeCx;
use crate::capability::Capability;
#[cfg(feature = "inbound")]
use crate::server::provision::{BootCx, ProvisionStep};

/// `[webhooks.*]`, which needs a database pool to persist what it receives.
#[derive(Debug)]
pub struct InboundWebhooks;

#[async_trait::async_trait]
impl Capability for InboundWebhooks {
    #[cfg(feature = "inbound")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.db_pool.is_some() && !cx.server.webhook_routes.is_empty()
    }

    /// The spine that holds normalized messages, and the idempotency ledger behind the
    /// pipeline's atomic claim.
    #[cfg(feature = "inbound")]
    fn provision(&self, cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        let server = cx.server;
        match server.db_pool.as_ref() {
            Some(pool) if !server.config.webhooks.is_empty() => vec![ProvisionStep::new(
                "inbound webhook",
                pool,
                &[
                    fraiseql_functions::migrations::inbound_migration_sql(),
                    fraiseql_webhooks::idempotency::SCHEMA_SQL,
                ],
            )],
            _ => Vec::new(),
        }
    }

    /// `POST /webhooks/{provider}`.
    #[cfg(feature = "inbound")]
    fn routes(&self, cx: &ServeCx<'_>) -> Option<axum::Router> {
        Some(cx.server.add_inbound_routes(axum::Router::new(), cx.app))
            .filter(axum::Router::has_routes)
    }
}
