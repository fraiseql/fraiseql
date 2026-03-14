//! FraiseQL Server binary.

use std::{env, path::Path, sync::Arc};

#[cfg(feature = "wire-backend")]
use fraiseql_core::db::FraiseWireAdapter;
#[cfg(not(feature = "wire-backend"))]
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_server::{CompiledSchemaLoader, Server, ServerConfig, middleware::RateLimitConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Load configuration from file or use defaults.
fn load_config(config_path: Option<&str>) -> anyhow::Result<ServerConfig> {
    if let Some(path) = config_path {
        tracing::info!(path = %path, "Loading configuration from file");
        let contents = std::fs::read_to_string(path)?;
        let config: ServerConfig = toml::from_str(&contents)?;
        Ok(config)
    } else {
        tracing::info!("Using default server configuration");
        Ok(ServerConfig::default())
    }
}

/// Validate that schema file exists.
fn validate_schema_path(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!(
            "Schema file not found: {}. \
             Please compile schema first with: fraiseql-cli compile schema.json",
            path.display()
        );
    }
    Ok(())
}

#[tokio::main]
#[cfg_attr(
    not(any(feature = "secrets", feature = "mcp", feature = "arrow")),
    allow(unused_mut) // Reason: `mut` is needed when secrets/mcp/arrow features are enabled
)]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fraiseql_server=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("FraiseQL Server v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config_path = env::var("FRAISEQL_CONFIG").ok();
    let mut config = load_config(config_path.as_deref())?;

    // Override configuration from environment variables if set
    if let Ok(db_url) = env::var("DATABASE_URL") {
        config.database_url = db_url;
    }
    if let Ok(bind_addr) = env::var("FRAISEQL_BIND_ADDR") {
        if let Ok(addr) = bind_addr.parse() {
            config.bind_addr = addr;
        } else {
            tracing::warn!(bind_addr = %bind_addr, "Invalid FRAISEQL_BIND_ADDR, using default");
        }
    }
    if let Ok(schema_path) = env::var("FRAISEQL_SCHEMA_PATH") {
        config.schema_path = schema_path.into();
    }

    // Metrics configuration from environment
    if let Ok(metrics_enabled) = env::var("FRAISEQL_METRICS_ENABLED") {
        config.metrics_enabled = metrics_enabled == "true" || metrics_enabled == "1";
    }
    if let Ok(metrics_token) = env::var("FRAISEQL_METRICS_TOKEN") {
        config.metrics_token = Some(metrics_token);
    }

    // Admin API configuration from environment
    if let Ok(admin_enabled) = env::var("FRAISEQL_ADMIN_API_ENABLED") {
        config.admin_api_enabled = admin_enabled == "true" || admin_enabled == "1";
    }
    if let Ok(admin_token) = env::var("FRAISEQL_ADMIN_TOKEN") {
        config.admin_token = Some(admin_token);
    }

    // Introspection configuration from environment
    if let Ok(introspection_enabled) = env::var("FRAISEQL_INTROSPECTION_ENABLED") {
        config.introspection_enabled =
            introspection_enabled == "true" || introspection_enabled == "1";
    }
    if let Ok(introspection_require_auth) = env::var("FRAISEQL_INTROSPECTION_REQUIRE_AUTH") {
        config.introspection_require_auth =
            introspection_require_auth != "false" && introspection_require_auth != "0";
    }

    // Rate limiting configuration from environment
    if let Ok(rate_limiting_enabled) = env::var("FRAISEQL_RATE_LIMITING_ENABLED") {
        let enabled = rate_limiting_enabled == "true" || rate_limiting_enabled == "1";
        if let Some(ref mut rate_limit) = config.rate_limiting {
            rate_limit.enabled = enabled;
        } else {
            config.rate_limiting = Some(RateLimitConfig {
                enabled,
                rps_per_ip: 100,
                rps_per_user: 1000,
                burst_size: 500,
                cleanup_interval_secs: 300,
                trust_proxy_headers: false,
                trusted_proxy_cidrs: Vec::new(),
            });
        }
    }
    if let Ok(rps_per_ip) = env::var("FRAISEQL_RATE_LIMIT_RPS_PER_IP") {
        if let Ok(value) = rps_per_ip.parse() {
            let mut rate_config = config.rate_limiting.take().unwrap_or(RateLimitConfig {
                enabled:               true,
                rps_per_ip:            100,
                rps_per_user:          1000,
                burst_size:            500,
                cleanup_interval_secs: 300,
                trust_proxy_headers:   false,
                trusted_proxy_cidrs:   Vec::new(),
            });
            rate_config.rps_per_ip = value;
            config.rate_limiting = Some(rate_config);
        }
    }
    if let Ok(rps_per_user) = env::var("FRAISEQL_RATE_LIMIT_RPS_PER_USER") {
        if let Ok(value) = rps_per_user.parse() {
            let mut rate_config = config.rate_limiting.take().unwrap_or(RateLimitConfig {
                enabled:               true,
                rps_per_ip:            100,
                rps_per_user:          1000,
                burst_size:            500,
                cleanup_interval_secs: 300,
                trust_proxy_headers:   false,
                trusted_proxy_cidrs:   Vec::new(),
            });
            rate_config.rps_per_user = value;
            config.rate_limiting = Some(rate_config);
        }
    }
    if let Ok(burst_size) = env::var("FRAISEQL_RATE_LIMIT_BURST_SIZE") {
        if let Ok(value) = burst_size.parse() {
            let mut rate_config = config.rate_limiting.take().unwrap_or(RateLimitConfig {
                enabled:               true,
                rps_per_ip:            100,
                rps_per_user:          1000,
                burst_size:            500,
                cleanup_interval_secs: 300,
                trust_proxy_headers:   false,
                trusted_proxy_cidrs:   Vec::new(),
            });
            rate_config.burst_size = value;
            config.rate_limiting = Some(rate_config);
        }
    }

    // Validate configuration
    if let Err(e) = config.validate() {
        tracing::error!(error = %e, "Configuration validation failed");
        anyhow::bail!(e);
    }

    tracing::info!(
        bind_addr = %config.bind_addr,
        database_url = %config.database_url,
        graphql_path = %config.graphql_path,
        health_path = %config.health_path,
        introspection_path = %config.introspection_path,
        metrics_enabled = config.metrics_enabled,
        "Server configuration loaded"
    );

    // Validate schema file exists
    validate_schema_path(&config.schema_path)?;

    // Load compiled schema
    let schema_loader = CompiledSchemaLoader::new(&config.schema_path);
    let schema = schema_loader.load().await?;
    tracing::info!("Compiled schema loaded successfully");

    // Initialize security configuration from schema
    #[cfg(feature = "auth")]
    {
        tracing::info!("Initializing security configuration from schema");
        let schema_json_str = schema.to_json().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to serialize schema to JSON");
            "{}".to_string()
        });
        let security_config = fraiseql_server::auth::init_security_config(&schema_json_str)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to load security config from schema, using defaults");
                fraiseql_server::auth::init_default_security_config()
            });

        // Validate security configuration
        if let Err(e) = fraiseql_server::auth::validate_security_config(&security_config) {
            tracing::error!(error = %e, "Security configuration validation failed");
            anyhow::bail!(e);
        }

        // Log security configuration for observability
        fraiseql_server::auth::log_security_config(&security_config);
    }

    // Initialize database adapter
    #[cfg(not(feature = "wire-backend"))]
    {
        tracing::info!(
            database_url = %config.database_url,
            pool_min_size = config.pool_min_size,
            pool_max_size = config.pool_max_size,
            pool_timeout_secs = config.pool_timeout_secs,
            "Initializing PostgreSQL database adapter"
        );
    }

    #[cfg(feature = "wire-backend")]
    {
        tracing::info!(
            database_url = %config.database_url,
            "Initializing FraiseQL Wire database adapter (low-memory streaming)"
        );
    }

    #[cfg(not(feature = "wire-backend"))]
    let adapter = Arc::new(
        PostgresAdapter::with_pool_config(
            &config.database_url,
            config.pool_min_size,
            config.pool_max_size,
        )
        .await?,
    );

    #[cfg(feature = "wire-backend")]
    let adapter = Arc::new(FraiseWireAdapter::new(&config.database_url));

    #[cfg(not(feature = "wire-backend"))]
    tracing::info!("PostgreSQL adapter initialized successfully with connection pooling");

    #[cfg(feature = "wire-backend")]
    tracing::info!("FraiseQL Wire adapter initialized successfully");

    // Create sqlx pool for observers (if enabled)
    #[cfg(feature = "observers")]
    let db_pool = {
        use sqlx::postgres::PgPoolOptions;
        #[allow(clippy::cast_possible_truncation)]
        // Reason: pool sizes are validated to be small (< 1000)
        let pool_min = config.pool_min_size as u32;
        #[allow(clippy::cast_possible_truncation)]
        // Reason: pool sizes are validated to be small (< 1000)
        let pool_max = config.pool_max_size as u32;
        let pool = PgPoolOptions::new()
            .min_connections(pool_min)
            .max_connections(pool_max)
            .connect(&config.database_url)
            .await?;
        Some(pool)
    };
    #[cfg(not(feature = "observers"))]
    let db_pool: Option<sqlx::PgPool> = None;

    // Initialize secrets manager if configured via environment
    // For development/testing, use ENV backend if FRAISEQL_SECRETS_BACKEND env var is set
    #[cfg(feature = "secrets")]
    let secrets_manager = if env::var("FRAISEQL_SECRETS_BACKEND").is_ok() {
        tracing::info!("Initializing secrets manager from environment configuration");
        let config = fraiseql_server::secrets_manager::SecretsBackendConfig::Env;
        match fraiseql_server::secrets_manager::create_secrets_manager(config).await {
            Ok(manager) => Some(manager),
            Err(e) => {
                tracing::error!(error = %e, "Failed to initialize secrets manager");
                anyhow::bail!("Secrets manager initialization failed: {}", e);
            },
        }
    } else {
        tracing::debug!("Secrets manager disabled (set FRAISEQL_SECRETS_BACKEND to enable)");
        None
    };

    // Create and start server
    #[cfg(feature = "arrow")]
    {
        use fraiseql_server::arrow::create_flight_service;

        // Create Flight service with real database adapter
        let flight_service = create_flight_service(adapter.clone());
        tracing::info!("Arrow Flight service initialized with real database adapter");

        let mut server =
            Server::with_flight_service(config, schema, adapter, db_pool, Some(flight_service))
                .await?;

        // Attach secrets manager if configured
        #[cfg(feature = "secrets")]
        if let Some(mgr) = secrets_manager {
            server.set_secrets_manager(mgr);
        }

        // Serve MCP over stdio if requested, otherwise start HTTP server.
        #[cfg(feature = "mcp")]
        if env::var("FRAISEQL_MCP_STDIO").is_ok() {
            tracing::info!("FraiseQL MCP stdio mode starting");
            server.serve_mcp_stdio().await?;
            return Ok(());
        }

        tracing::info!(
            "FraiseQL Server {} starting (HTTP + Arrow Flight)",
            env!("CARGO_PKG_VERSION")
        );

        server.serve().await?;
    }

    #[cfg(not(feature = "arrow"))]
    {
        let mut server = Server::new(config, schema, adapter, db_pool).await?;

        // Attach secrets manager if configured
        #[cfg(feature = "secrets")]
        if let Some(mgr) = secrets_manager {
            server.set_secrets_manager(mgr);
        }

        // Serve MCP over stdio if requested, otherwise start HTTP server.
        #[cfg(feature = "mcp")]
        if env::var("FRAISEQL_MCP_STDIO").is_ok() {
            tracing::info!("FraiseQL MCP stdio mode starting");
            server.serve_mcp_stdio().await?;
            return Ok(());
        }

        tracing::info!("FraiseQL Server {} starting (HTTP only)", env!("CARGO_PKG_VERSION"));

        server.serve().await?;
    }

    Ok(())
}
