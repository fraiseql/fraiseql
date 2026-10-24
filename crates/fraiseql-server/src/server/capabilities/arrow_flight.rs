//! Arrow Flight, on a port of its own.

#[cfg(feature = "arrow")]
use tracing::{error, info};

#[cfg(feature = "arrow")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "arrow")]
use crate::capability::Started;

/// Every `arrow` build serves Flight on `flight_bind_addr`.
#[derive(Debug)]
pub struct ArrowFlight;

#[async_trait::async_trait]
impl Capability for ArrowFlight {
    #[cfg(feature = "arrow")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.flight_service.is_some()
    }

    #[cfg(feature = "arrow")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        let server = &mut *cx.server;
        let Some(mut flight_service) = server.flight_service.take() else {
            return Ok(Started::Running);
        };
        let flight_addr = server.config.flight_bind_addr;

        // #954: attach the policy seam, never a bare executor. This is the
        // first point where both halves exist — `create_flight_service` builds
        // the service before there is any `AppState` to enforce policy from —
        // so the wiring lives here rather than in the constructor. Without it
        // Flight is the one transport that skips tenant resolution, the
        // suspended-tenant gate, per-tenant quotas and trusted documents.
        let app_state = cx.app.clone();

        // #1349: the same wiring point, for the same reason. Flight builds its own
        // principal from a session token and, until now, dispatched it unresolved —
        // so `[identity.enrichment]`'s "every authenticated request resolves and
        // fail-closes" was true of six transports out of seven, and this one was
        // refused outright by the engine's backstop on any enrichment-declaring
        // deployment.
        #[cfg(feature = "auth")]
        if !app_state.identity.is_empty() {
            flight_service.set_identity_enricher(std::sync::Arc::new(app_state.identity.clone()));
            info!("Arrow Flight resolves identity before dispatch (#1349)");
        }

        flight_service.set_executor(crate::arrow::policy_seam::policy_gated_executor(app_state));
        info!(
            "Arrow Flight GraphQL execution enabled through the policy seam (tenant \
             dispatch, trusted documents, per-tenant quotas)"
        );

        info!("Arrow Flight server listening on grpc://{}", flight_addr);

        // Spawn Flight server in background, registered on the server's
        // JoinSet. The set's `shutdown` step abort-then-awaits the gRPC
        // server when the HTTP server exits.
        server.tasks.spawn(async move {
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(flight_service.into_server())
                .serve(flight_addr)
                .await
            {
                error!(error = %e, "Arrow Flight server terminated with error");
            }
        });
        Ok(Started::Running)
    }
}
