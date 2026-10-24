//! The outbound CDC drains (#382).

#[cfg(feature = "cdc-outbound")]
use tracing::info;

#[cfg(feature = "cdc-outbound")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "cdc-outbound")]
use crate::{
    capability::Started,
    server::provision::{BootCx, ProvisionStep},
};

/// `[[cdc_outbound.sinks]]`.
#[derive(Debug)]
pub struct CdcOutbound;

#[async_trait::async_trait]
impl Capability for CdcOutbound {
    #[cfg(feature = "cdc-outbound")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        !cx.server.cdc_drains.is_empty()
    }

    /// The per-sink delivery state (`core.tb_cdc_sink_state`). A server that cannot
    /// create it refuses to serve rather than drain without durable delivery state,
    /// which would re-publish every event on restart.
    #[cfg(feature = "cdc-outbound")]
    fn provision(&self, cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        let server = cx.server;
        match server.db_pool.as_ref() {
            Some(pool) if server.config.cdc_outbound.is_some() => vec![ProvisionStep::new(
                "cdc outbound",
                pool,
                &[fraiseql_cdc_sinks::outbox_sink_state_migration_sql()],
            )],
            _ => Vec::new(),
        }
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
