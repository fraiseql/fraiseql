//! Starting the background capabilities: one function every serve entry point calls.
//!
//! `serve_with_shutdown` used to start them inline, so `serve_on_listener` — the entry
//! point every in-process suite serves through — mounted their routes and started none
//! of them, and `serve_mcp_stdio` skipped even the request-serving workers.

use tracing::{error, info, warn};

#[cfg(any(
    feature = "inbound-email",
    feature = "sources",
    feature = "functions-runtime"
))]
use super::ServerError;
use super::{Result, Server};
use crate::routes::graphql::AppState;
#[cfg(feature = "observers")]
use crate::subscriptions::event_bridge::{EventBridge, EventBridgeConfig};

/// The ways a server is served, which decide what it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryPoint {
    /// [`Server::serve_with_shutdown`]: the binary and `fraiseql run`.
    Http,
    /// [`Server::serve_on_listener`]: a caller-bound listener. Everything `Http` starts
    /// except Arrow Flight, which binds a port of its own the caller did not choose.
    Listener,
    /// `Server::serve_mcp_stdio`: one MCP client over stdin/stdout, a sidecar to the
    /// deployment's HTTP server. It runs the workers its own requests need (async
    /// operations, the inbound dispatch sweep, the storage quota sweep) and none of
    /// the deployment-wide consumers — observers, pollers, schedulers, CDC drains —
    /// which the HTTP server already runs and which a second copy would double-fire.
    McpStdio,
}

impl Server {
    /// Start every background capability this configuration enables, for `entry`.
    ///
    /// Called once the router is built, because the pollers and schedulers dispatch
    /// through the function hooks `build_router` supplies.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ServerError`] if a configured capability cannot start: its state
    /// tables cannot be created or read back, or its broker refuses in production.
    /// Callers box the future: it holds every capability's start state, which would
    /// push each serve future past clippy's 16-KiB `large_futures` threshold.
    #[allow(clippy::cognitive_complexity)] // Reason: one guarded block per capability, in start order
    pub(super) async fn start_capabilities(
        &mut self,
        app_state: &AppState,
        entry: EntryPoint,
    ) -> Result<()> {
        // The async-operation workers (#391), the inbound dispatch sweep and the storage
        // quota sweep, on the server's JoinSet so graceful shutdown aborts them (an
        // aborted execution's row goes stale and is reclaimed after the staleness
        // threshold — the P19 recovery path, exercised on every deploy).
        self.spawn_lifecycle_tasks(app_state);
        if entry == EntryPoint::McpStdio {
            return Ok(());
        }

        // Start the poll-IMAP email workers.
        // Each configured `[mailbox.<name>.imap]` half runs a background poll loop
        // on the server's JoinSet, so graceful shutdown aborts them. The workers
        // reuse the durable inbound spine and the `after:ingest` dispatch path;
        // attachments stream into the legacy storage backend when one is
        // configured. Must run here (async, after `build_router` supplies the
        // function-dispatch hooks) rather than in the sync `build_router`.
        #[cfg(feature = "inbound-email")]
        if let Some(ref db_pool) = self.db_pool {
            if self.config.mailbox.values().any(|mailbox| mailbox.imap.is_some()) {
                use crate::inbound::{email, spine::PostgresInboundSpine};

                // The email path shares the inbound spine; create it here too in
                // case only `[mailbox.*.imap]` (no `[webhooks.*]`) is configured.
                // Both DDLs are idempotent.
                PostgresInboundSpine::new(db_pool.clone()).init().await.map_err(|e| {
                    ServerError::ConfigError(format!(
                        "Failed to initialize inbound spine schema: {e}"
                    ))
                })?;
                email::init_cursor_store(db_pool).await.map_err(|e| {
                    ServerError::ConfigError(format!(
                        "Failed to initialize inbound email cursor schema: {e}"
                    ))
                })?;

                // The correlator transitions send-status and suppression from
                // inbound signals; its tables are also created by the send path,
                // but a receive-only deployment needs them here too (idempotent).
                let tracker = std::sync::Arc::new(email::PgSendTracker::new(db_pool.clone()));
                tracker.init().await.map_err(|e| {
                    ServerError::ConfigError(format!(
                        "Failed to initialize send-tracking schema: {e}"
                    ))
                })?;
                let correlator =
                    std::sync::Arc::clone(&tracker) as std::sync::Arc<dyn email::SendCorrelator>;
                let address_hash_key = self.build_address_hash_key();

                // The attachment sink is the server's one configured storage
                // backend. It used to hang off a separate `with_storage` builder
                // method that no binary ever called, so `[mailbox]
                // attachment_bucket` was unreachable from the shipped server and
                // attachments were dropped with a warning.
                let sink = self.storage_state.as_ref().map(|state| {
                    std::sync::Arc::new(email::StorageAttachmentSink::new(state.backend.clone()))
                        as std::sync::Arc<
                            dyn fraiseql_functions::host::live::storage::StorageBackend,
                        >
                });
                // Opt-in Return-Path probe: verify the provider preserves
                // plus-addressing before trusting VERP correlation. Blocks startup
                // up to the probe window per eligible mailbox; off by default.
                if self.config.send.verp_probe_on_start {
                    email::run_startup_probes(&self.config.mailbox, |name| {
                        std::env::var(name).ok()
                    })
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
                    &self.config.mailbox,
                    db_pool,
                    hooks.as_ref(),
                    query_executor_factory.as_ref(),
                    sink.as_ref(),
                    Some(&correlator),
                    address_hash_key.as_ref(),
                    self.config.send.challenge_suppress_after,
                    self.config.inbound_dispatch.settings().map_or(
                        crate::inbound::dispatch_ledger::LedgerSettings::default().lease,
                        |settings| settings.lease,
                    ),
                    |name| std::env::var(name).ok(),
                );
                let started = pollers.len();
                for (poller, interval) in pollers {
                    self.tasks.spawn(async move { poller.poll_forever(interval).await });
                }
                info!(mailboxes = started, "poll-IMAP email sources started");
            }
        }

        // Start the scheduled-ingress source scheduler (#573): one poller per
        // enabled Model B (Deno) source in the compiled schema, each firing its
        // connector on its cron schedule under a single-firing lease with a host
        // bound to the source's durable cursor and its `run_as` executor. Runs on
        // the server's JoinSet so graceful shutdown drains it. Native pull sources
        // (poll-IMAP email) run via their own pollers above. Must run here (async,
        // after `build_router` supplies the function-dispatch hooks).
        #[cfg(feature = "sources")]
        if let Some(ref db_pool) = self.db_pool {
            let sources = app_state.executor().schema().sources.clone();
            if !sources.is_empty() {
                let sources_config = self.config.sources.clone().unwrap_or_default();
                if !crate::sources::sources_enabled(&sources_config) {
                    info!(
                        count = sources.len(),
                        "source scheduler disabled by config — sources not started"
                    );
                } else if let Some(hooks) = app_state.before_mutation_hooks.as_ref() {
                    // The shared durable source-cursor table (idempotent DDL).
                    crate::migration_lock::run_migration(
                        db_pool,
                        fraiseql_observers::migrations::source_cursor_sql(),
                    )
                    .await
                    .map_err(|e| {
                        ServerError::ConfigError(format!(
                            "Failed to initialize source cursor schema: {e}"
                        ))
                    })?;
                    let host_config = crate::sources::source_host_config(&sources_config);
                    let pollers = crate::sources::build_source_pollers(
                        &sources,
                        db_pool,
                        &crate::query_bridge::ExecutorSource::of(app_state),
                        hooks.as_ref(),
                        &host_config,
                        sources_config.log_payloads,
                    )?;
                    let started = pollers.len();
                    for poller in pollers {
                        self.tasks.spawn(async move { poller.run_forever().await });
                    }
                    info!(sources = started, "source scheduler started");
                } else if let Some(source) = sources.iter().find(|source| source.enabled) {
                    // Provisioning refuses this case first (#1399); a backstop, so an
                    // enabled source can never be left silently unscheduled.
                    return Err(ServerError::ConfigError(format!(
                        "source {:?} is enabled but no functions subsystem was built to run \
                         its connector {:?}",
                        source.name, source.function
                    )));
                }
            }
        }

        // Start the outbound CDC drains (#382): one worker per configured
        // `[[cdc_outbound.sinks]]`, draining the change-log outbox to its
        // broker on the server's JoinSet so graceful shutdown stops them.
        // Building them is fail-loud (no pool / unreachable broker / DDL
        // failure all refuse to boot) — a server that starts without its drain
        // is silent data loss for every downstream consumer.
        #[cfg(feature = "cdc-outbound")]
        {
            let drains = std::mem::take(&mut self.cdc_drains);
            if !drains.is_empty() {
                let started = drains.len();
                crate::cdc_outbound::spawn_all(drains, &mut self.tasks);
                info!(sinks = started, "cdc outbound drains started");
            }
        }

        // Start the subscription Kafka mirror (#1102), on the server's JoinSet so a
        // graceful shutdown stops it with everything else. Building it was fail-loud at
        // boot; there is nothing left to refuse here.
        #[cfg(feature = "subscription-kafka")]
        if let Some(mirror) = self.subscription_kafka.take() {
            crate::subscription_kafka::spawn(mirror, &self.subscription_manager, &mut self.tasks);
        }

        // Start the `cron:` function scheduler (#595): one leased poller per cron
        // function in the compiled schema, each firing on its schedule under a
        // single-firing advisory lease with the phase-02 `run_as` host. A cron
        // function is a scheduled source without a cursor. Runs on the server's
        // JoinSet so graceful shutdown drains it; requires a DB pool (the advisory
        // lease + `_fraiseql_cron_state`) and the function-dispatch hooks.
        #[cfg(feature = "functions-runtime")]
        if let Some(ref db_pool) = self.db_pool {
            if let Some(hooks) = app_state.before_mutation_hooks.as_ref() {
                if !hooks.trigger_registry.cron_triggers.is_empty() {
                    let cron_state = crate::cron::PgCronState::new(db_pool.clone());
                    cron_state.init().await.map_err(|e| {
                        ServerError::ConfigError(format!(
                            "Failed to initialize cron state schema: {e}"
                        ))
                    })?;
                    let host_config = crate::routes::after_mutation::host_context_config();
                    let pollers = crate::cron::build_cron_pollers(
                        db_pool,
                        &crate::query_bridge::ExecutorSource::of(app_state),
                        hooks.as_ref(),
                        &host_config,
                    )
                    .await
                    .map_err(|e| {
                        // Fail loud: a cross-restart fire guard whose state cannot
                        // be read back is not a guard (#796).
                        ServerError::ConfigError(format!(
                            "Failed to read back cron state (_fraiseql_cron_state): {e}"
                        ))
                    })?;
                    let started = pollers.len();
                    for poller in pollers {
                        self.tasks.spawn(async move { poller.run_forever().await });
                    }
                    info!(cron_functions = started, "cron scheduler started");
                }
            }
        }

        // Spawn SIGUSR1 schema reload handler when running on Unix.
        // The handler loops forever, reloading on each signal, until the
        // server process exits — tracked on the server's JoinSet so graceful
        // shutdown awaits its termination.
        #[cfg(unix)]
        if let Some(ref schema_path) = app_state.schema_path {
            let reload_state = app_state.clone();
            let reload_path = schema_path.clone();
            self.tasks.spawn(async move {
                let mut sigusr1 = match tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::user_defined1(),
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(error = %e, "Failed to install SIGUSR1 handler — schema hot-reload disabled");
                        return;
                    },
                };
                loop {
                    sigusr1.recv().await;
                    info!(
                        path = %reload_path.display(),
                        "Received SIGUSR1 — reloading schema"
                    );
                    match reload_state.reload_schema(&reload_path).await {
                        Ok(()) => {
                            let hash = reload_state.executor().schema().content_hash();
                            reload_state
                                .metrics
                                .schema_reloads_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            info!(schema_hash = %hash, "Schema reloaded successfully via SIGUSR1");
                        },
                        Err(e) => {
                            reload_state
                                .metrics
                                .schema_reload_errors_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            error!(
                                error = %e,
                                path = %reload_path.display(),
                                "Schema reload failed via SIGUSR1 — keeping previous schema"
                            );
                        },
                    }
                }
            });
            info!(
                path = %schema_path.display(),
                "SIGUSR1 schema reload handler installed"
            );
        }

        // Start observer runtime if configured, wiring CDC events to EventBridge
        #[cfg(feature = "observers")]
        if let Some(ref runtime) = self.observer_runtime {
            info!("Starting observer runtime...");

            // Create EventBridge to forward CDC events to GraphQL subscriptions,
            // and — since #1309 — to the entity-event fan-out the REST
            // `/{resource}/stream` mount reads. The runtime feeds it from this
            // replica's own read of the changes (#1503), never from a per-request
            // subscription: `EventTransport::subscribe` is a competing consumer, so a
            // browser tab subscribing would have taken events away from dispatch.
            let mut bridge =
                EventBridge::new(self.subscription_manager.clone(), EventBridgeConfig::new());
            if let Some(ref fanout) = self.entity_event_fanout {
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

            match guard.start().await {
                Ok(()) => {
                    info!("Observer runtime started");
                    // The bridge runs on the server's JoinSet, so shutdown stops it
                    // with every other task instead of leaving it detached.
                    self.tasks.spawn(bridge.run());
                    info!("EventBridge started — CDC events will be forwarded to subscriptions");
                },
                Err(e) => {
                    // A broker-backed transport (NATS) was an explicit operator
                    // choice; refusing to boot in production rather than silently
                    // coming up without it is the #350 dead-broker contract. The
                    // default PostgreSQL transport keeps the resilient
                    // log-and-continue behaviour (and development downgrades the
                    // NATS failure to the same warning).
                    if guard.transport_requires_broker()
                        && crate::ServerConfig::is_production_mode()
                    {
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
                },
            }
            drop(guard);
        }

        // Arrow Flight, on its own port.
        #[cfg(feature = "arrow")]
        if entry == EntryPoint::Http {
            if let Some(mut flight_service) = self.flight_service.take() {
                let flight_addr = self.config.flight_bind_addr;

                // #954: attach the policy seam, never a bare executor. This is the
                // first point where both halves exist — `create_flight_service` builds
                // the service before there is any `AppState` to enforce policy from —
                // so the wiring lives here rather than in the constructor. Without it
                // Flight is the one transport that skips tenant resolution, the
                // suspended-tenant gate, per-tenant quotas and trusted documents.
                let app_state = app_state.clone();

                // #1349: the same wiring point, for the same reason. Flight builds its own
                // principal from a session token and, until now, dispatched it unresolved —
                // so `[identity.enrichment]`'s "every authenticated request resolves and
                // fail-closes" was true of six transports out of seven, and this one was
                // refused outright by the engine's backstop on any enrichment-declaring
                // deployment.
                #[cfg(feature = "auth")]
                if !app_state.identity.is_empty() {
                    flight_service
                        .set_identity_enricher(std::sync::Arc::new(app_state.identity.clone()));
                    info!("Arrow Flight resolves identity before dispatch (#1349)");
                }

                flight_service
                    .set_executor(crate::arrow::policy_seam::policy_gated_executor(app_state));
                info!(
                    "Arrow Flight GraphQL execution enabled through the policy seam (tenant \
                 dispatch, trusted documents, per-tenant quotas)"
                );

                info!("Arrow Flight server listening on grpc://{}", flight_addr);

                // Spawn Flight server in background, registered on the server's
                // JoinSet. The set's `shutdown` step abort-then-awaits the gRPC
                // server when the HTTP server exits.
                self.tasks.spawn(async move {
                    if let Err(e) = tonic::transport::Server::builder()
                        .add_service(flight_service.into_server())
                        .serve(flight_addr)
                        .await
                    {
                        error!(error = %e, "Arrow Flight server terminated with error");
                    }
                });
            }
        }

        Ok(())
    }

    /// Stop what [`Self::start_capabilities`] started that needs more than an abort,
    /// within the shutdown timeout. The `JoinSet`'s tasks are aborted after this.
    pub(super) async fn stop_capabilities(&self) {
        let timeout = std::time::Duration::from_secs(self.config.shutdown_timeout_secs);
        let stopped = tokio::time::timeout(timeout, async {
            #[cfg(feature = "observers")]
            if let Some(ref runtime) = self.observer_runtime {
                let mut guard = runtime.write().await;
                if guard.health().running {
                    match guard.stop().await {
                        Ok(()) => info!("Observer runtime stopped cleanly"),
                        Err(e) => warn!("Observer runtime shutdown error: {e}"),
                    }
                }
            }
        })
        .await;
        if stopped.is_err() {
            warn!(
                timeout_secs = self.config.shutdown_timeout_secs,
                "Shutdown drain timed out; forcing exit"
            );
        } else {
            info!("Graceful shutdown complete");
        }
    }
}
