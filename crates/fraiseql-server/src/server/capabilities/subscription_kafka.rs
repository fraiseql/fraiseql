//! The subscription Kafka mirror (#1102).

#[cfg(feature = "subscription-kafka")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "subscription-kafka")]
use crate::capability::Started;

/// `[subscription_kafka]`.
#[derive(Debug)]
pub struct SubscriptionKafka;

#[async_trait::async_trait]
impl Capability for SubscriptionKafka {
    #[cfg(feature = "subscription-kafka")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.subscription_kafka.is_some()
    }

    /// On the server's task set, so a graceful shutdown stops it with everything else.
    /// Building it was fail-loud at boot; there is nothing left to refuse here.
    #[cfg(feature = "subscription-kafka")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        let server = &mut *cx.server;
        if let Some(mirror) = server.subscription_kafka.take() {
            crate::subscription_kafka::spawn(
                mirror,
                &server.subscription_manager,
                &mut server.tasks,
            );
        }
        Ok(Started::Running)
    }
}
