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
}
