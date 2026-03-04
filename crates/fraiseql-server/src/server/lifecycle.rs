//! Server lifecycle: serve, serve_with_shutdown, and shutdown_signal.

use super::*;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Start server and listen for requests.
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
    pub async fn serve_with_shutdown<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let app = self.build_router();

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
            // Flight server runs on port 50051
            let flight_addr = "0.0.0.0:50051".parse().expect("Valid Flight address");
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
        let app = self.build_router();
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
}
