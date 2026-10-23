//! Server lifecycle: serve, `serve_with_shutdown`, and `shutdown_signal`.

use std::net::SocketAddr;

use axum::serve::ListenerExt;
use tokio::net::TcpListener;
use tracing::{info, warn};

use super::{Result, Server, ServerError, TlsSetup};

/// Disable Nagle's algorithm on an accepted connection, to minimise latency for small
/// GraphQL responses.
type NodelayTap = fn(&mut tokio::net::TcpStream);

fn set_nodelay(tcp_stream: &mut tokio::net::TcpStream) {
    if let Err(err) = tcp_stream.set_nodelay(true) {
        warn!("failed to set TCP_NODELAY: {err:#}");
    }
}

impl Server {
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

    /// Provision the persistent schemas the router is about to depend on.
    ///
    /// Everything here is async, must happen before the first request is served, and
    /// must **fail the boot** rather than leave an endpoint mounted over a table that
    /// does not exist. `build_router` is synchronous, so it cannot do any of it.
    ///
    /// Reached only through `provisioned_app_state`, which every serve entry point —
    /// [`Self::serve_with_shutdown`], [`Self::serve_on_listener`] and `serve_mcp_stdio`
    /// — obtains its state from. It used to be inlined in the first only, so the
    /// in-process test harness — which serves on a listener — reached the routes
    /// without ever running the DDL behind them. #748 (a parse error in the RBAC schema
    /// that aborted boot outright) was therefore invisible to every integration test in
    /// the workspace: they got a happily-running server that answered `500` on the RBAC
    /// routes. A boot step that only some entry points perform is the same class of
    /// defect as a guard only some call sites consult; #1332 was the stdio transport
    /// skipping it.
    ///
    /// **Scope is deliberately "what the mounted routes need in place", not "every
    /// startup step".** Subsystem startup that opens external connections — the
    /// observer runtime, Flight, the IMAP/cron/source pollers — is
    /// `start_capabilities`, which every entry point calls after building its router.
    /// The functions runtime moved here in #896, once it stopped needing a file on
    /// disk: every entry point must mount the same before-mutation hooks, or the
    /// in-process one greens a dispatch surface that never ran.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] if any subsystem's schema cannot be created,
    /// and [`ServerError::ConfigError`] if a setting cannot be honoured. Usage
    /// persistence is the one deliberate exception: it degrades to in-memory counters
    /// with a warning, because losing metering is not worth refusing to serve traffic.
    pub(super) async fn provision_persistent_schemas(&mut self) -> Result<()> {
        // #1175: refused before anything mounts, so the router, the email pollers and the
        // dispatch sweep can read the ledger settings without failing.
        #[cfg(feature = "inbound")]
        self.config.inbound_dispatch.settings().map_err(ServerError::ConfigError)?;

        // Every table a mounted route depends on, one ordered step per configured
        // subsystem, each under the migration lock.
        super::provision::run(self.provision_steps()).await?;

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
                            // A failed load is NOT recoverable-by-continuing: the
                            // flush loop would then be writing to a store this
                            // process could not read. Revert to the in-memory
                            // NoopBackend and do not arm the loop at all, so
                            // billing counters are never touched by a process that
                            // does not know what they were (#861).
                            if let Err(e) = global_aggregator().load_from_backend().await {
                                tracing::error!(
                                    error = %e,
                                    "Usage persistence: startup load failed — persistence is \
                                     DISABLED for this process. Counters are in-memory only and \
                                     will not be written back. Restart once the store is \
                                     reachable."
                                );
                                global_aggregator().set_backend(std::sync::Arc::new(
                                    crate::usage::aggregator::NoopBackend,
                                ));
                            } else {
                                info!("Usage persistence: loaded counters from PostgreSQL");
                                // Spawn background flush task on the server's JoinSet
                                // so graceful shutdown can await its termination.
                                let flush_interval =
                                    Duration::from_secs(usage_cfg.flush_interval_secs);
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
                            }
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

        // Prepare functions-runtime dispatch (load modules, register runtimes, attach
        // the `send_email` wiring) before the router is built, so `build_app_state`
        // mounts the before-mutation hooks and after:mutation functions actually fire.
        //
        // This step lived on `serve_with_shutdown` alone until #896, because it
        // re-read the compiled schema from `config.schema_path` and `serve_on_listener`
        // is driven with an in-memory schema and no file on disk. It now reads the
        // functions section the server was built with, so it belongs here with every
        // other provisioning step — and the in-process entry point stops silently
        // mounting no functions at all.
        #[cfg(feature = "functions-runtime")]
        self.prepare_functions_runtime().await?;

        Ok(())
    }

    /// Start the background tasks every serve entry point runs once provisioning is done.
    ///
    /// Spawned here rather than at construction so a built-but-never-served `Server`
    /// leaves no task (and no connection) behind.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::ConfigError`] if a change-feed connector cannot be built
    /// from the configured database URL and TLS settings.
    pub(super) fn start_provisioned_tasks(&mut self) -> Result<()> {
        // #1577: the result cache's feed to every other server. Here, like the refreshers
        // below, so every serve entry point starts it and a built-but-never-served Server
        // leaves no task (and no connection) behind; its cache stays suspended.
        if let Some(parts) = self.cache_feed.take() {
            let tls = self.config.postgres_tls().map_err(ServerError::ConfigError)?;
            let connector =
                fraiseql_core::db::postgres::NotifyConnector::new(&self.config.database_url, tls)
                    .map_err(|e| ServerError::ConfigError(e.to_string()))?;
            let instance = uuid::Uuid::new_v4().to_string();
            info!(%instance, "Query result cache: starting the feed to other servers");
            self.tasks.spawn(super::cache_feed::run(connector, parts, instance));
        }

        // #1589: the listener on pg_tviews' change feed, here for the same reason.
        if let Some(listener) = self.tviews_feed.take() {
            let tls = self.config.postgres_tls().map_err(ServerError::ConfigError)?;
            let connector =
                fraiseql_core::db::postgres::NotifyConnector::new(&self.config.database_url, tls)
                    .map_err(|e| ServerError::ConfigError(e.to_string()))?
                    .serving(fraiseql_core::cache::tviews_feed::CHANNEL);
            let instance = uuid::Uuid::new_v4().to_string();
            info!(%instance, "Query result cache: starting the listener on pg_tviews' changes");
            self.tasks.spawn(super::tviews_feed::run(connector, listener, instance));
        }

        // Keep stored IdPs current (#947). Writes through this server's own admin API
        // refresh synchronously, so the ticker exists to pick up another replica's changes
        // and to keep the certificate-expiry warning honest. Spawned here rather than at
        // construction so a built-but-never-served Server leaves no task behind.
        #[cfg(feature = "auth-saml")]
        if let (Some(saml), Some(cfg)) = (self.saml_state.as_ref(), self.config.saml.as_ref()) {
            if cfg.store_enabled {
                let registry = saml.registry().clone();
                let interval = std::time::Duration::from_secs(cfg.refresh_interval_secs);
                let warning_days = cfg.certificate_expiry_warning_days;
                self.tasks.spawn(registry.refresh_loop(interval, warning_days));
                info!(
                    refresh_interval_secs = cfg.refresh_interval_secs,
                    "SAML IdP store refresher started"
                );
            }
        }

        // Keep stored bucket policies current (#974), for the same reason the
        // IdP refresher above exists: a policy pushed at one replica applies
        // there immediately, and an access control that reached one replica out
        // of three is not an access control. This bounds how long the rest of
        // the deployment lags.
        if let Some(ref storage_state) = self.storage_state {
            let state = storage_state.clone();
            self.tasks.spawn(
                state.policy_refresh_loop(fraiseql_storage::DEFAULT_POLICY_REFRESH_INTERVAL),
            );
            info!(
                refresh_interval_secs = fraiseql_storage::DEFAULT_POLICY_REFRESH_INTERVAL.as_secs(),
                "Storage bucket policy refresher started"
            );
        }

        Ok(())
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
        // Server-side TLS is unsupported; database TLS is applied where the pool is built.
        let tls_setup = TlsSetup::new(self.config.tls.clone());

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

        let app_state = self.provisioned_app_state().await?;
        let app = self.build_router(&app_state);
        Box::pin(self.start_capabilities(&app_state, super::start::EntryPoint::Http)).await?;

        info!(
            bind_addr = %self.config.bind_addr,
            graphql_path = %self.config.graphql_path,
            "Starting FraiseQL server"
        );

        // Explicitly enable TCP_NODELAY (disable Nagle's algorithm) on every
        // accepted connection to minimise latency for small GraphQL responses.
        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?
            .tap_io(set_nodelay as NodelayTap);

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

        // Deliberately no "database TLS applied" line here. `serve()` does not build the
        // connection pool — it is handed one — so it is in no position to report what
        // transport security that pool negotiated. The line that used to be here read
        // `postgres_ssl_mode=verify-full … Database connection TLS configuration applied`
        // over a pool constructed with `NoTls`, which is the whole of #801. The pool now
        // reports its own mode from the site that builds it.

        info!("Server listening on http://{}", self.config.bind_addr);

        Box::pin(self.serve_until(listener, app, shutdown)).await
    }

    /// Start server on an externally created listener.
    ///
    /// Used in tests to discover the bound port before serving. Starts what
    /// [`Self::serve_with_shutdown`] starts except Arrow Flight, which would bind a port
    /// of its own; skips the `[tls]` refusal, which concerns the binary's listener.
    ///
    /// It does **not** skip `provision_persistent_schemas`: the tables the
    /// mounted routes read from are created here exactly as they are under
    /// [`Self::serve_with_shutdown`], so a boot-time provisioning failure fails this
    /// entry point too. Skipping it was how #748 reached a release — the RBAC DDL did
    /// not parse, and every test in the workspace served the RBAC routes over tables
    /// that had never been created.
    ///
    /// # Errors
    ///
    /// Returns error if the server encounters a runtime error, or if
    /// `provision_persistent_schemas` cannot provision a configured subsystem.
    pub async fn serve_on_listener<F>(mut self, listener: TcpListener, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let app_state = self.provisioned_app_state().await?;
        let app = self.build_router(&app_state);
        // The same start as `serve_with_shutdown`, so a suite serving through here runs
        // the observers, pollers and schedulers its configuration enables.
        Box::pin(self.start_capabilities(&app_state, super::start::EntryPoint::Listener)).await?;
        let listener = listener.tap_io(set_nodelay as NodelayTap);
        Box::pin(self.serve_until(listener, app, shutdown)).await
    }

    /// Serve `app` until `shutdown` fires, then stop and drain everything started.
    async fn serve_until<F>(
        self,
        listener: axum::serve::TapIo<TcpListener, NodelayTap>,
        app: axum::Router,
        shutdown: F,
    ) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        // #571: signal live subscription connections as soon as shutdown fires, so they
        // can send their Complete/close frames while the graceful-shutdown window is
        // still open.
        let subscription_drain = std::sync::Arc::clone(&self.subscription_drain);
        let shutdown_with_drain = async move {
            shutdown.await;
            let _ = subscription_drain.send(true);
        };
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown_with_drain)
            .await
            .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

        info!(
            timeout_secs = self.config.shutdown_timeout_secs,
            "HTTP server stopped, draining remaining work"
        );
        self.stop_capabilities().await;
        // Abort and await every lifecycle task (Flight, the pollers, the SIGUSR1
        // handler, PKCE cleanup, trusted-docs reload, usage flush, …).
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
