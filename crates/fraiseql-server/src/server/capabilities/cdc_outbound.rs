//! The outbound CDC drains (#382).

#[cfg(feature = "cdc-outbound")]
use tracing::info;

#[cfg(feature = "cdc-outbound")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "cdc-outbound")]
use crate::capability::Started;

/// `[[cdc_outbound.sinks]]`.
#[derive(Debug)]
pub struct CdcOutbound;

#[async_trait::async_trait]
impl Capability for CdcOutbound {
    #[cfg(feature = "cdc-outbound")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        !cx.server.cdc_drains.is_empty()
    }

    /// One worker per configured sink, draining the change-log outbox to its broker on the
    /// server's task set so graceful shutdown stops them. Building them was fail-loud (no
    /// pool, unreachable broker, DDL failure all refuse to boot): a server that starts
    /// without its drain is silent data loss for every downstream consumer.
    #[cfg(feature = "cdc-outbound")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        let drains = std::mem::take(&mut cx.server.cdc_drains);
        let started = drains.len();
        crate::cdc_outbound::spawn_all(drains, &mut cx.server.tasks);
        info!(sinks = started, "cdc outbound drains started");
        Ok(Started::Running)
    }
}
