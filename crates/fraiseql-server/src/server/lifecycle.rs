//! Server lifecycle: serve, `serve_with_shutdown`, and `shutdown_signal`.

use std::net::SocketAddr;

use axum::serve::ListenerExt;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

use super::{DatabaseAdapter, Result, Server, ServerError, TlsSetup};
#[cfg(feature = "observers")]
use crate::subscriptions::event_bridge::{EventBridge, EventBridgeConfig};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Start server and listen for requests.
    ///
    /// Uses SIGUSR1-aware shutdown signal when a schema path is configured,
    /// enabling zero-downtime schema reloads via `kill -USR1 <pid>`.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve(self) -> Result<()> {
        self.serve_with_shutdown(Self::shutdown_signal()).await
    }

    /// Start server with a custom shutdown future.
    ///
    /// Enables programmatic shutdown (e.g., for `--watch` hot-reload) by accepting any
    /// future that resolves when the server should stop.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    #[allow(clippy::cognitive_complexity)] // Reason: server lifecycle with TLS/non-TLS binding, signal handling, and graceful shutdown
    pub async fn serve_with_shutdown<F>(mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        // Ensure RBAC schema exists before the router mounts RBAC endpoints.
        // Must run here (async context) rather than inside build_router() (sync).
        #[cfg(feature = "observers")]
        if let Some(ref db_pool) = self.db_pool {
            if self.config.admin_token.is_some() {
                let rbac_backend =
                    crate::api::rbac_management::db_backend::RbacDbBackend::new(db_pool.clone());
                rbac_backend.ensure_schema().await.map_err(|e| {
                    ServerError::ConfigError(format!("Failed to initialize RBAC schema: {e}"))
                })?;
            }
        }

        // Ensure the inbound-ingestion tables exist before the router mounts
        // POST /webhooks/{provider}. Like RBAC, this must run here (async context)
        // rather than in the sync build_router(). The spine holds normalized
        // messages; the idempotency ledger backs the pipeline's atomic claim.
        #[cfg(feature = "inbound")]
        if let Some(ref db_pool) = self.db_pool {
            if !self.config.webhooks.is_empty() {
                crate::inbound::WebhookInboundState::init_spine(db_pool).await.map_err(|e| {
                    ServerError::ConfigError(format!(
                        "Failed to initialize inbound spine schema: {e}"
                    ))
                })?;
                fraiseql_webhooks::PostgresIdempotencyStore::new(db_pool.clone())
                    .init()
                    .await
                    .map_err(|e| {
                        ServerError::ConfigError(format!(
                            "Failed to initialize webhook idempotency schema: {e}"
                        ))
                    })?;
                info!(
                    routes = self.config.webhooks.len(),
                    "Inbound ingestion schema ready (spine + idempotency ledger)"
                );
            }
        }

        // Initialize usage persistence backend if configured.
        // Must run before build_router() so the aggregator is populated before
        // serving requests, but after the DB pool is available (async context).
        if let Some(ref usage_cfg) = self.config.usage.clone() {
            use std::time::Duration;

            use sqlx::postgres::PgPoolOptions;
            use tokio::time::MissedTickBehavior;

            use crate::usage::aggregator::{PostgresBackend, global_aggregator};

            match PgPoolOptions::new()
                .max_connections(2) // small dedicated pool — only used for periodic flushes
                .connect(&self.config.database_url)
                .await
            {
                Ok(pool) => {
                    match PostgresBackend::new(pool).await {
                        Ok(backend) => {
                            let backend = std::sync::Arc::new(backend);
                            // Upgrade global aggregator's backend from NoopBackend.
                            global_aggregator().set_backend(backend.clone());
                            // Restore persisted counters before serving requests.
                            if let Err(e) = global_aggregator().load_from_backend().await {
                                warn!(error = %e, "Usage persistence: startup load failed — continuing with in-memory counters");
                            } else {
                                info!("Usage persistence: loaded counters from PostgreSQL");
                            }
                            // Spawn background flush task on the server's JoinSet
                            // so graceful shutdown can await its termination.
                            let flush_interval = Duration::from_secs(usage_cfg.flush_interval_secs);
                            let agg = std::sync::Arc::clone(global_aggregator());
                            self.tasks.spawn(async move {
                                let mut ticker = tokio::time::interval(flush_interval);
                                ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
                                ticker.tick().await; // skip immediate first tick
                                loop {
                                    ticker.tick().await;
                                    if let Err(e) = agg.flush_to_backend().await {
                                        warn!(error = %e, "Usage persistence: background flush failed");
                                    }
                                }
                            });
                            info!(
                                flush_interval_secs = usage_cfg.flush_interval_secs,
                                "Usage persistence: PostgreSQL backend active"
                            );
                        },
                        Err(e) => {
                            warn!(
                                error = %e,
                                "Usage persistence: PostgresBackend initialization failed — \
                                 continuing with in-memory (NoopBackend)"
                            );
                        },
                    }
                },
                Err(e) => {
                    warn!(
                        error = %e,
                        "Usage persistence: failed to connect to PostgreSQL — \
                         continuing with in-memory (NoopBackend)"
                    );
                },
            }
        }

        // Prepare functions-runtime dispatch (load modules, register runtimes,
        // attach the send_email wiring) before the router is built, so
        // `build_app_state` mounts the before-mutation hooks. Async + fail-loud,
        // like the RBAC/inbound schema init above; a no-op when no functions are
        // declared or the feature is off.
        #[cfg(feature = "functions-runtime")]
        self.prepare_functions_runtime().await?;

        let (app, app_state) = self.build_router();

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

                let sink = self.storage_backend.as_ref().map(|backend| {
                    std::sync::Arc::new(email::LegacyStorageSink::new(backend.clone()))
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
                let workers = email::build_workers(
                    &self.config.mailbox,
                    db_pool,
                    hooks.as_ref(),
                    sink.as_ref(),
                    Some(&correlator),
                    address_hash_key.as_ref(),
                    self.config.send.challenge_suppress_after,
                    |name| std::env::var(name).ok(),
                );
                let started = workers.len();
                for (worker, interval) in workers {
                    self.tasks.spawn(async move { worker.poll_forever(interval).await });
                }
                info!(mailboxes = started, "poll-IMAP email workers started");
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

        // Initialize TLS setup (database connection TLS; server-side TLS is unsupported).
        let tls_setup = TlsSetup::new(self.config.tls.clone(), self.config.database_tls.clone());

        // Refuse to boot if server-side `[tls]` is enabled. FraiseQL does not terminate TLS
        // itself — it serves plaintext and expects a reverse proxy / load balancer / service
        // mesh to terminate TLS in front of it. Previously an enabled `[tls]` built a rustls
        // config that was silently discarded while the server kept serving plaintext and
        // logged `mtls_required = true` (M-tls-enforce); failing loud is honest.
        if tls_setup.is_tls_enabled() {
            return Err(ServerError::ConfigError(
                "[tls] (server-side TLS termination) is enabled but not supported: FraiseQL \
                 serves plaintext HTTP and expects TLS to be terminated by a reverse proxy, \
                 load balancer, or service mesh. Remove the [tls] section (or set its \
                 `enabled = false`) and terminate TLS in front of the server. Database \
                 connection TLS ([database_tls]) is unaffected."
                    .to_string(),
            ));
        }

        info!(
            bind_addr = %self.config.bind_addr,
            graphql_path = %self.config.graphql_path,
            tls_enabled = tls_setup.is_tls_enabled(),
            "Starting FraiseQL server"
        );

        // Start observer runtime if configured, wiring CDC events to EventBridge
        #[cfg(feature = "observers")]
        #[allow(unused_variables)]
        // Reason: _bridge_handle is kept alive to prevent task cancellation
        let _bridge_handle = {
            let mut handle: Option<tokio::task::JoinHandle<()>> = None;
            if let Some(ref runtime) = self.observer_runtime {
                info!("Starting observer runtime...");

                // Create EventBridge to forward CDC events to GraphQL subscriptions
                let bridge =
                    EventBridge::new(self.subscription_manager.clone(), EventBridgeConfig::new());
                let sender = bridge.sender();

                let mut guard = runtime.write().await;
                guard.set_event_bridge_sender(sender);

                match guard.start().await {
                    Ok(()) => {
                        info!("Observer runtime started");
                        // Spawn EventBridge after observer runtime is running
                        handle = Some(bridge.spawn());
                        info!(
                            "EventBridge started — CDC events will be forwarded to subscriptions"
                        );
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
            handle
        };

        // Explicitly enable TCP_NODELAY (disable Nagle's algorithm) on every
        // accepted connection to minimise latency for small GraphQL responses.
        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?
            .tap_io(|tcp_stream| {
                if let Err(err) = tcp_stream.set_nodelay(true) {
                    warn!("failed to set TCP_NODELAY: {err:#}");
                }
            });

        // Warn if the process file descriptor limit is below the recommended minimum.
        // A low limit causes "too many open files" errors under load.
        #[cfg(target_os = "linux")]
        {
            if let Ok(limits) = std::fs::read_to_string("/proc/self/limits") {
                for line in limits.lines() {
                    if line.starts_with("Max open files") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if let Some(soft) = parts.get(3) {
                            if let Ok(n) = soft.parse::<u64>() {
                                if n < 65_536 {
                                    warn!(
                                        current_fd_limit = n,
                                        recommended = 65_536,
                                        "File descriptor limit is low; consider raising ulimit -n"
                                    );
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }

        // Log database TLS configuration
        info!(
            postgres_ssl_mode = tls_setup.postgres_ssl_mode(),
            redis_ssl = tls_setup.redis_ssl_enabled(),
            clickhouse_https = tls_setup.clickhouse_https_enabled(),
            elasticsearch_https = tls_setup.elasticsearch_https_enabled(),
            "Database connection TLS configuration applied"
        );

        info!("Server listening on http://{}", self.config.bind_addr);

        // Start both HTTP and gRPC servers concurrently if Arrow Flight is enabled
        #[cfg(feature = "arrow")]
        if let Some(flight_service) = self.flight_service.take() {
            let flight_addr = self.config.flight_bind_addr;
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

            // Wrap the user-supplied shutdown future so we can also stop observer runtime
            #[cfg(feature = "observers")]
            let observer_runtime = self.observer_runtime.clone();

            let shutdown_with_cleanup = async move {
                shutdown.await;
                #[cfg(feature = "observers")]
                if let Some(ref runtime) = observer_runtime {
                    info!("Shutting down observer runtime");
                    let mut guard = runtime.write().await;
                    if let Err(e) = guard.stop().await {
                        #[cfg(feature = "observers")]
                        error!("Error stopping runtime: {}", e);
                    } else {
                        info!("Runtime stopped cleanly");
                    }
                }
            };

            // Run HTTP server with graceful shutdown
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(shutdown_with_cleanup)
                .await
                .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

            // Abort and await every lifecycle task (Flight server, SIGUSR1
            // handler, PKCE cleanup, trusted-docs reload, usage flush, …).
            drain_lifecycle_tasks(self.tasks, self.config.shutdown_timeout_secs).await;
        }

        // HTTP-only server (when arrow feature not enabled)
        #[cfg(not(feature = "arrow"))]
        {
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(shutdown)
                .await
                .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

            let shutdown_timeout =
                std::time::Duration::from_secs(self.config.shutdown_timeout_secs);
            info!(
                timeout_secs = self.config.shutdown_timeout_secs,
                "HTTP server stopped, draining remaining work"
            );

            let drain = tokio::time::timeout(shutdown_timeout, async {
                #[cfg(feature = "observers")]
                if let Some(ref runtime) = self.observer_runtime {
                    let mut guard = runtime.write().await;
                    match guard.stop().await {
                        Ok(()) => info!("Observer runtime stopped cleanly"),
                        Err(e) => warn!("Observer runtime shutdown error: {e}"),
                    }
                }
            })
            .await;

            if drain.is_err() {
                warn!(
                    timeout_secs = self.config.shutdown_timeout_secs,
                    "Shutdown drain timed out; forcing exit"
                );
            } else {
                info!("Graceful shutdown complete");
            }

            // Abort and await every lifecycle task (SIGUSR1 handler, PKCE
            // cleanup, trusted-docs reload, usage flush, …).
            drain_lifecycle_tasks(self.tasks, self.config.shutdown_timeout_secs).await;
        }

        Ok(())
    }

    /// Start server on an externally created listener.
    ///
    /// Used in tests to discover the bound port before serving.
    /// Skips TLS, Flight, and observer startup — suitable for unit/integration tests only.
    ///
    /// # Errors
    ///
    /// Returns error if the server encounters a runtime error.
    pub async fn serve_on_listener<F>(self, listener: TcpListener, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let (app, _app_state) = self.build_router();
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;
        // Abort and await any lifecycle tasks spawned during construction
        // (e.g. PKCE cleanup, trusted-docs reload) so the test path doesn't
        // leak background work into the next test.
        drain_lifecycle_tasks(self.tasks, self.config.shutdown_timeout_secs).await;
        Ok(())
    }

    /// Listen for shutdown signals (Ctrl+C or SIGTERM)
    pub async fn shutdown_signal() {
        use tokio::signal;

        let ctrl_c = async {
            match signal::ctrl_c().await {
                Ok(()) => {},
                Err(e) => {
                    warn!(error = %e, "Failed to install Ctrl+C handler");
                    std::future::pending::<()>().await;
                },
            }
        };

        #[cfg(unix)]
        let terminate = async {
            match signal::unix::signal(signal::unix::SignalKind::terminate()) {
                Ok(mut s) => {
                    s.recv().await;
                },
                Err(e) => {
                    warn!(error = %e, "Failed to install SIGTERM handler");
                    std::future::pending::<()>().await;
                },
            }
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            () = ctrl_c => info!("Received Ctrl+C"),
            () = terminate => info!("Received SIGTERM"),
        }
    }
}

/// Abort every lifecycle task on the supplied [`tokio::task::JoinSet`] and await
/// the resulting `JoinError`s so the runtime is fully drained before
/// `serve_with_shutdown` returns.
///
/// Tasks are awaited under an outer timeout so a stuck task cannot prevent
/// process exit. A `JoinError::is_cancelled()` after `JoinSet::abort_all` is
/// the expected case — only unexpected panics are logged.
pub(super) async fn drain_lifecycle_tasks(
    mut tasks: tokio::task::JoinSet<()>,
    shutdown_timeout_secs: u64,
) {
    if tasks.is_empty() {
        return;
    }

    tasks.abort_all();
    let timeout = std::time::Duration::from_secs(shutdown_timeout_secs);
    let drained = tokio::time::timeout(timeout, async {
        while let Some(res) = tasks.join_next().await {
            if let Err(e) = res {
                if !e.is_cancelled() {
                    warn!(error = %e, "Lifecycle task terminated with a non-cancellation error");
                }
            }
        }
    })
    .await;
    if drained.is_err() {
        warn!(
            timeout_secs = shutdown_timeout_secs,
            "Lifecycle task drain timed out; some background tasks did not stop in time"
        );
    } else {
        info!("All lifecycle background tasks drained");
    }
}
