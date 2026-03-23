//! Server lifecycle: serve, serve_with_shutdown, and shutdown_signal.

use super::*;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Start a read-only server.
    ///
    /// Mounts all routes except REST mutations (POST, PUT, PATCH, DELETE).
    /// GraphQL mutations are dispatched at runtime and will return an error
    /// for adapters that do not support them.
    ///
    /// For full mutation support including REST mutations, use `serve_mut`
    /// (requires `A: MutationCapable`).
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve(self) -> Result<()> {
        self.serve_inner(Self::shutdown_signal(), Self::build_router).await
    }

    /// Start a read-only server with a custom shutdown future.
    ///
    /// See `serve` for details. Enables programmatic shutdown (e.g., for
    /// `--watch` hot-reload) by accepting any future that resolves when the
    /// server should stop.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve_with_shutdown<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.serve_inner(shutdown, Self::build_router).await
    }

    /// Core server startup logic shared by all `serve*` variants.
    ///
    /// Accepts a `router_fn` so that [`serve_mut`] can supply
    /// [`build_mutation_router`] while the base [`serve`] uses [`build_router`].
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    #[allow(unused_mut)] // Reason: `self` is mutated only when the `grpc` feature is enabled (.take())
    async fn serve_inner<F>(
        mut self,
        shutdown: F,
        router_fn: fn(&Self) -> (axum::Router, crate::routes::graphql::AppState<A>),
    ) -> Result<()>
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

        let (app, app_state) = router_fn(&self);

        // Spawn SIGUSR1 schema reload listener (Unix only)
        #[cfg(unix)]
        let _reload_handle = Self::spawn_schema_reload_listener(
            self.config.schema_path.clone(),
            app_state,
        );

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

        // Spawn gRPC transport server in background (if configured).
        #[cfg(feature = "grpc")]
        let grpc_server_handle = if let Some(grpc_service) = self.grpc_service.clone() {
            let grpc_addr = self.config.grpc_bind_addr;
            let reflection_bytes = self.grpc_reflection_bytes.take();
            info!("gRPC transport server listening on grpc://{}", grpc_addr);

            let max_msg_size = self
                .executor
                .schema()
                .grpc_config
                .as_ref()
                .map_or(4 * 1024 * 1024, |c| c.max_message_size_bytes);

            // Build reflection service from descriptor bytes (if available).
            let reflection_svc = reflection_bytes.and_then(|bytes| {
                match tonic_reflection::server::Builder::configure()
                    .register_encoded_file_descriptor_set(&bytes)
                    .build_v1()
                {
                    Ok(svc) => Some(svc),
                    Err(e) => {
                        warn!("Failed to build gRPC reflection service: {e}");
                        None
                    },
                }
            });

            Some(tokio::spawn(async move {
                let mut builder = tonic::transport::Server::builder()
                    .max_frame_size(Some(max_msg_size as u32));

                let router = builder.add_service(grpc_service);

                if let Some(reflection) = reflection_svc {
                    router.add_service(reflection).serve(grpc_addr).await
                } else {
                    router.serve(grpc_addr).await
                }
            }))
        } else {
            None
        };

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

            // Abort gRPC transport server after HTTP server exits
            #[cfg(feature = "grpc")]
            if let Some(handle) = grpc_server_handle {
                handle.abort();
            }
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

            match drain {
                Ok(()) => info!("Graceful shutdown complete"),
                Err(_) => warn!(
                    timeout_secs = self.config.shutdown_timeout_secs,
                    "Shutdown drain timed out; forcing exit"
                ),
            }

            // Abort gRPC transport server after HTTP server exits
            #[cfg(feature = "grpc")]
            if let Some(handle) = grpc_server_handle {
                handle.abort();
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
        let (app, _state) = self.build_router();
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;
        Ok(())
    }

    /// Listen for shutdown signals (Ctrl+C or SIGTERM).
    ///
    /// # Panics
    ///
    /// Panics if the Ctrl+C or SIGTERM signal handler cannot be installed.
    pub async fn shutdown_signal() {
        use tokio::signal;

        let ctrl_c = async {
            signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => info!("Received Ctrl+C"),
            _ = terminate => info!("Received SIGTERM"),
        }
    }

    /// Spawn a background task that listens for SIGUSR1 and hot-reloads the
    /// compiled schema from `schema_path`.
    ///
    /// The task runs until the returned [`tokio::task::JoinHandle`] is aborted
    /// (usually at server shutdown).
    ///
    /// # Panics
    ///
    /// Panics if the SIGUSR1 signal handler cannot be installed.
    #[cfg(unix)]
    pub fn spawn_schema_reload_listener(
        schema_path: std::path::PathBuf,
        state: crate::routes::graphql::AppState<A>,
    ) -> tokio::task::JoinHandle<()> {
        use std::sync::Arc as StdArc;
        use fraiseql_core::{runtime::Executor, schema::CompiledSchema};

        tokio::spawn(async move {
            let mut sig = tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::user_defined1(),
            )
            .expect("Failed to install SIGUSR1 handler");

            loop {
                sig.recv().await;
                info!(path = %schema_path.display(), "SIGUSR1 received — reloading schema");

                let json = match std::fs::read_to_string(&schema_path) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!("Schema reload failed (read error): {e}");
                        continue;
                    },
                };

                let new_schema = match CompiledSchema::from_json(&json) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Schema reload failed (parse error): {e}");
                        continue;
                    },
                };

                let old = state.executor.load_full();
                let adapter = StdArc::clone(old.adapter());
                let config = old.config().clone();
                let new_exec = StdArc::new(Executor::with_config(new_schema, adapter, config));
                let hash = new_exec.schema().content_hash();
                state.swap_executor(new_exec);

                info!(schema_hash = %hash, "Schema hot-reloaded via SIGUSR1");
            }
        })
    }
}

impl<A: DatabaseAdapter + MutationCapable + Clone + Send + Sync + 'static> Server<A> {
    /// Start a full server with REST mutation support.
    ///
    /// Includes all routes from `serve` plus REST mutation routes
    /// (POST, PUT, PATCH, DELETE) that require `MutationCapable`.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve_mut(self) -> Result<()> {
        self.serve_inner(Self::shutdown_signal(), Self::build_mutation_router).await
    }

    /// Start a full server with REST mutations and a custom shutdown future.
    ///
    /// See `serve_mut` for details. Enables programmatic shutdown (e.g., for
    /// `--watch` hot-reload) by accepting any future that resolves when the
    /// server should stop.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve_mut_with_shutdown<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.serve_inner(shutdown, Self::build_mutation_router).await
    }

    /// Start server on an externally created listener with full mutation support.
    ///
    /// Used in tests to discover the bound port before serving.
    /// Skips TLS, Flight, and observer startup — suitable for unit/integration tests only.
    ///
    /// # Errors
    ///
    /// Returns error if the server encounters a runtime error.
    pub async fn serve_mut_on_listener<F>(self, listener: TcpListener, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let (app, _state) = self.build_mutation_router();
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Compile-time assertions that read-only adapters can construct and
    //! (conceptually) serve a `Server` without requiring `MutationCapable`.
    //!
    //! These tests do not actually start a server — they verify that the
    //! type-level bounds are satisfied at compile time.

    use super::*;

    /// Prove that `Server<FraiseWireAdapter>` can call `serve()` (read-only)
    /// without requiring `MutationCapable`.
    ///
    /// This is the primary compile-time assertion for this module — it proves
    /// that the `MutationCapable` bound was correctly removed from `serve()`.
    #[cfg(feature = "wire-backend")]
    #[allow(dead_code, unreachable_code, clippy::diverging_sub_expression, unused_variables)]
    // Reason: compile-time type check only; body is never executed.
    fn static_assert_wire_adapter_can_serve() {
        let server: Server<fraiseql_core::db::FraiseWireAdapter> = todo!();
        drop(server.serve());
    }

    /// Prove that `Server<PostgresAdapter>` can call both `serve()` (read-only)
    /// and `serve_mut()` (full mutations).
    #[allow(dead_code, unreachable_code, clippy::diverging_sub_expression, unused_variables)]
    // Reason: compile-time type check only; body is never executed.
    fn static_assert_postgres_can_serve_and_serve_mut() {
        let server: Server<fraiseql_core::db::postgres::PostgresAdapter> = todo!();
        drop(server.serve());
        // Also verifiable but requires separate instance due to move:
        let server2: Server<fraiseql_core::db::postgres::PostgresAdapter> = todo!();
        drop(server2.serve_mut());
    }
}
