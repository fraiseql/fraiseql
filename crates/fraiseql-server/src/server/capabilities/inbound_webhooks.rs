//! The inbound webhook receiver.

#[cfg(feature = "inbound")]
use super::ServeCx;
use crate::capability::Capability;

/// `[webhooks.*]`, which needs a database pool to persist what it receives.
#[derive(Debug)]
pub struct InboundWebhooks;

#[async_trait::async_trait]
impl Capability for InboundWebhooks {
    #[cfg(feature = "inbound")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.db_pool.is_some() && !cx.server.webhook_routes.is_empty()
    }

    /// `POST /webhooks/{provider}`.
    #[cfg(feature = "inbound")]
    fn routes(&self, cx: &ServeCx<'_>) -> Option<axum::Router> {
        Some(cx.server.add_inbound_routes(axum::Router::new(), cx.app))
            .filter(axum::Router::has_routes)
    }
}
