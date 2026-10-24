//! The observer runtime, and the event bridge that feeds subscriptions from it.

#[cfg(feature = "observers")]
use tracing::{error, info, warn};

#[cfg(feature = "observers")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "observers")]
use crate::{
    capability::Started,
    subscriptions::event_bridge::{EventBridge, EventBridgeConfig},
};

/// `[observers]`.
#[derive(Debug)]
pub struct Observers;

#[async_trait::async_trait]
impl Capability for Observers {
    #[cfg(feature = "observers")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.observer_runtime.is_some()
    }

    /// Start the runtime and the event bridge that forwards its events to subscriptions.
    ///
    /// A broker-backed transport (NATS) that fails to start refuses the boot in
    /// production (#350); the default PostgreSQL transport, and development, serve on
    /// without observers.
    #[cfg(feature = "observers")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        let server = &mut *cx.server;
        let app_state = cx.app;
        let Some(ref runtime) = server.observer_runtime else {
            return Ok(Started::Running);
        };
        info!("Starting observer runtime...");

        // Create EventBridge to forward CDC events to GraphQL subscriptions,
        // and — since #1309 — to the entity-event fan-out the REST
        // `/{resource}/stream` mount reads. The runtime feeds it from this
        // replica's own read of the changes (#1503), never from a per-request
        // subscription: `EventTransport::subscribe` is a competing consumer, so a
        // browser tab subscribing would have taken events away from dispatch.
        let mut bridge =
            EventBridge::new(server.subscription_manager.clone(), EventBridgeConfig::new());
        if let Some(ref fanout) = server.entity_event_fanout {
            bridge = bridge.with_entity_fanout(fanout.clone());
        }
        let sender = bridge.sender();

        let mut guard = runtime.write().await;
        guard.set_event_bridge_sender(sender);

        // #366: wire after:capture dispatch — externally-captured writes
        // (from the change-log reader) drive `after:capture` functions on
        // the phase-02 `run_as` host. The hook is a cheap no-op for
        // FraiseQL's own (non-captured) rows, so mediated writes never loop.
        #[cfg(feature = "functions-runtime")]
        if let Some(hooks) = app_state.before_mutation_hooks.as_ref() {
            let hooks = std::sync::Arc::clone(hooks);
            let factory = crate::routes::after_mutation::make_query_executor_factory(
                crate::query_bridge::ExecutorSource::of(app_state),
            );
            guard.set_capture_dispatch(std::sync::Arc::new(move |event| {
                if let Some((fn_event, cdc)) =
                    crate::routes::after_mutation::observer_event_to_capture(event)
                {
                    let plans = crate::routes::after_mutation::plan_after_capture_dispatch(
                        &hooks,
                        &fn_event,
                        cdc.as_deref(),
                    );
                    if !plans.is_empty() {
                        crate::routes::after_mutation::spawn_after_capture(
                            &hooks,
                            plans,
                            Some(factory.clone()),
                        );
                    }
                }
            }));
        }
        #[cfg(not(feature = "functions-runtime"))]
        let _ = app_state;

        match guard.start().await {
            Ok(()) => {
                info!("Observer runtime started");
                // The bridge runs on the server's JoinSet, so shutdown stops it
                // with every other task instead of leaving it detached.
                server.tasks.spawn(bridge.run());
                info!("EventBridge started — CDC events will be forwarded to subscriptions");
                Ok(Started::Running)
            },
            Err(e) => {
                // A broker-backed transport (NATS) was an explicit operator
                // choice; refusing to boot in production rather than silently
                // coming up without it is the #350 dead-broker contract. The
                // default PostgreSQL transport keeps the resilient
                // log-and-continue behaviour (and development downgrades the
                // NATS failure to the same warning).
                if guard.transport_requires_broker() && crate::ServerConfig::is_production_mode() {
                    error!(
                        error = %e,
                        "Observer runtime failed to start on its configured \
                         transport; refusing to boot (set FRAISEQL_ENV=development \
                         to downgrade to a warning)"
                    );
                    return Err(e);
                }
                error!("Failed to start observer runtime: {}", e);
                warn!("Server will continue without observers");
                Ok(Started::Degraded)
            },
        }
    }

    /// The observer admin API under `/api/observers`.
    #[cfg(feature = "observers")]
    fn routes(&self, cx: &ServeCx<'_>) -> Option<axum::Router> {
        Some(cx.server.add_observer_routes(axum::Router::new(), cx.app))
            .filter(axum::Router::has_routes)
    }

    /// Whether the runtime is running.
    #[cfg(feature = "observers")]
    async fn health(&self, app: &crate::routes::graphql::AppState) -> Option<bool> {
        let runtime = app.observer_runtime.as_ref()?;
        Some(runtime.read().await.health().running)
    }
}
