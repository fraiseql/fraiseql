use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use axum::Router;

use crate::config::RuntimeConfig;
use crate::runtime_state::AppState;
use crate::lifecycle::shutdown::{ShutdownCoordinator, ShutdownConfig, shutdown_signal};
use crate::runtime_middleware::admission::AdmissionLayer;

pub mod router;

pub struct RuntimeServer {
    config: RuntimeConfig,
}

impl RuntimeServer {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub async fn run(self) -> Result<(), ServerError> {
        // Initialize tracing
        init_tracing(&self.config);

        // Build shutdown coordinator
        let shutdown_config = ShutdownConfig {
            timeout: self.config.lifecycle.as_ref()
                .and_then(|l| parse_duration(&l.shutdown_timeout))
                .unwrap_or(std::time::Duration::from_secs(30)),
            delay: self.config.lifecycle.as_ref()
                .and_then(|l| parse_duration(&l.shutdown_delay))
                .unwrap_or(std::time::Duration::from_secs(5)),
        };
        let shutdown = ShutdownCoordinator::new(shutdown_config);

        // Build application state
        #[cfg(feature = "database")]
        let state = Arc::new(AppState::new_with_database(self.config.clone(), shutdown.clone()).await?);

        #[cfg(not(feature = "database"))]
        let state = Arc::new(AppState::new(self.config.clone(), shutdown.clone()));

        // Build router
        let router = router::RuntimeRouter::new(state.clone()).build();

        // Apply middleware stack
        let app = self.apply_middleware(router, &state);

        // Create listener
        let addr: SocketAddr = format!("{}:{}", self.config.server.host, self.config.server.port)
            .parse()
            .map_err(|e| ServerError::AddressParse(e))?;

        let listener = TcpListener::bind(addr).await?;
        tracing::info!("FraiseQL runtime listening on {}", addr);

        // Spawn shutdown signal handler
        let shutdown_coordinator = shutdown.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown_coordinator.shutdown().await;
        });

        // Run server with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown.subscribe().recv().await;
            })
            .await?;

        Ok(())
    }

    fn apply_middleware(&self, router: Router, state: &Arc<AppState>) -> Router {
        let mut app = router;

        // CORS (outermost - must be first)
        #[cfg(feature = "cors")]
        if let Some(_cors_config) = &self.config.cors {
            // TODO: Build CORS layer from config
            // app = app.layer(build_cors_layer(cors_config));
        }

        // Request tracing
        app = app.layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &http::Request<_>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                })
        );

        // Admission control (backpressure)
        if let Some(limits) = &self.config.server.limits {
            app = app.layer(AdmissionLayer::new(
                limits.max_concurrent_requests,
                limits.max_queue_depth,
                state.shutdown.clone(),
            ));
        }

        // Compression
        app = app.layer(CompressionLayer::new());

        // Request size limit
        if let Some(limits) = &self.config.server.limits {
            if let Some(max_size) = parse_size(&limits.max_request_size) {
                app = app.layer(RequestBodyLimitLayer::new(max_size));
            }
        }

        // Timeout
        if let Some(limits) = &self.config.server.limits {
            if let Some(timeout) = parse_duration(&limits.request_timeout) {
                app = app.layer(TimeoutLayer::with_status_code(http::StatusCode::REQUEST_TIMEOUT, timeout));
            }
        }

        app
    }
}

fn init_tracing(config: &RuntimeConfig) {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            if let Some(tracing_config) = &config.tracing {
                EnvFilter::new(&tracing_config.level)
            } else {
                EnvFilter::new("info")
            }
        });

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim().to_lowercase();

    let (num_str, multiplier_ms) = if s.ends_with("ms") {
        (&s[..s.len()-2], 1u64)
    } else if s.ends_with('s') {
        (&s[..s.len()-1], 1000)
    } else if s.ends_with('m') {
        (&s[..s.len()-1], 60 * 1000)
    } else if s.ends_with('h') {
        (&s[..s.len()-1], 60 * 60 * 1000)
    } else {
        return None;
    };

    let num: u64 = num_str.trim().parse().ok()?;
    Some(std::time::Duration::from_millis(num * multiplier_ms))
}

fn parse_size(s: &str) -> Option<usize> {
    let s = s.trim();
    let s_upper = s.to_uppercase();

    let (num_str, multiplier) = if s_upper.ends_with("GB") {
        (&s[..s.len()-2], 1024 * 1024 * 1024)
    } else if s_upper.ends_with("MB") {
        (&s[..s.len()-2], 1024 * 1024)
    } else if s_upper.ends_with("KB") {
        (&s[..s.len()-2], 1024)
    } else if s_upper.ends_with("B") {
        (&s[..s.len()-1], 1)
    } else {
        (s, 1)
    };

    let num: usize = num_str.trim().parse().ok()?;
    num.checked_mul(multiplier)
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Failed to bind to address: {0}")]
    Bind(#[from] std::io::Error),

    #[error("Failed to parse address: {0}")]
    AddressParse(#[from] std::net::AddrParseError),

    #[error("Runtime error: {0}")]
    Runtime(#[from] fraiseql_error::RuntimeError),
}
