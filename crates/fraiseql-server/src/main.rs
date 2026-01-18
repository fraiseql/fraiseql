//! FraiseQL Server binary.

use std::{env, path::Path, sync::Arc};

use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_server::{CompiledSchemaLoader, Server, ServerConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Load configuration from file or use defaults.
fn load_config(config_path: Option<&str>) -> anyhow::Result<ServerConfig> {
    match config_path {
        Some(path) => {
            tracing::info!(path = %path, "Loading configuration from file");
            let contents = std::fs::read_to_string(path)?;
            let config: ServerConfig = toml::from_str(&contents)?;
            Ok(config)
        },
        None => {
            tracing::info!("Using default server configuration");
            Ok(ServerConfig::default())
        },
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

    // Initialize database adapter with pool configuration
    tracing::info!(
        database_url = %config.database_url,
        pool_min_size = config.pool_min_size,
        pool_max_size = config.pool_max_size,
        pool_timeout_secs = config.pool_timeout_secs,
        "Initializing database adapter"
    );
    let adapter = Arc::new(
        PostgresAdapter::with_pool_config(
            &config.database_url,
            config.pool_min_size,
            config.pool_max_size,
        )
        .await?,
    );
    tracing::info!("Database adapter initialized successfully with connection pooling");

    // Create and start server
    let server = Server::new(config, schema, adapter).await?;
    tracing::info!("FraiseQL Server {} starting", env!("CARGO_PKG_VERSION"));

    server.serve().await?;

    Ok(())
}
