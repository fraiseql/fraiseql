//! Server lifecycle: serve, `serve_with_shutdown`, and `shutdown_signal`.

use tokio::net::TcpListener;
use tracing::{error, info, warn};

use super::{DatabaseAdapter, Result, Server, ServerError, TlsSetup};

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
    pub async fn serve_with_shutdown<F>(self, shutdown: F) -> Result<()>
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

        let (app, app_state) = self.build_router();

        // Spawn SIGUSR1 schema reload handler when running on Unix.
        // The handler loops forever, reloading on each signal, until the
        // server process exits.
        #[cfg(unix)]
        if let Some(ref schema_path) = app_state.schema_path {
            let reload_state = app_state.clone();
            let reload_path = schema_path.clone();
            tokio::spawn(async move {
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

        // Initialize TLS setup
        let tls_setup = TlsSetup::new(self.config.tls.clone(), self.config.database_tls.clone())?;

        info!(
            bind_addr = %self.config.bind_addr,
            graphql_path = %self.config.graphql_path,
            tls_enabled = tls_setup.is_tls_enabled(),
            "Starting FraiseQL server"
        );

        // Start observer runtime if configured
        #[cfg(feature = "observers")]
        if let Some(ref runtime) = self.observer_runtime {
            info!("Starting observer runtime...");
            let mut guard = runtime.write().await;

            match guard.start().await {
                Ok(()) => info!("Observer runtime started"),
                Err(e) => {
                    error!("Failed to start observer runtime: {}", e);
                    warn!("Server will continue without observers");
                },
            }
            drop(guard);
        }

        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?;

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

        // Log TLS configuration
        if tls_setup.is_tls_enabled() {
            // Verify TLS setup is valid (will error if certificates are missing/invalid)
            let _ = tls_setup.create_rustls_config()?;
            info!(
                cert_path = ?tls_setup.cert_path(),
                key_path = ?tls_setup.key_path(),
                mtls_required = tls_setup.is_mtls_required(),
                "Server TLS configuration loaded (note: use reverse proxy for server-side TLS termination)"
            );
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
        if let Some(flight_service) = self.flight_service {
            let flight_addr = self.config.flight_bind_addr;
            info!("Arrow Flight server listening on grpc://{}", flight_addr);

            // Spawn Flight server in background
            let flight_server = tokio::spawn(async move {
                tonic::transport::Server::builder()
                    .add_service(flight_service.into_server())
                    .serve(flight_addr)
                    .await
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
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_with_cleanup)
                .await
                .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

            // Abort Flight server after HTTP server exits
            flight_server.abort();
        }

        // HTTP-only server (when arrow feature not enabled)
        #[cfg(not(feature = "arrow"))]
        {
            axum::serve(listener, app)
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
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;
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
