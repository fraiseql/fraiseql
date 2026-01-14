//! FraiseQL Server binary.

use fraiseql_server::{CompiledSchemaLoader, ServerConfig};
use std::env;
use std::path::Path;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Load configuration from file or use defaults.
fn load_config(config_path: Option<&str>) -> anyhow::Result<ServerConfig> {
    match config_path {
        Some(path) => {
            tracing::info!(path = %path, "Loading configuration from file");
            let contents = std::fs::read_to_string(path)?;
            let config: ServerConfig = toml::from_str(&contents)?;
            Ok(config)
        }
        None => {
            tracing::info!("Using default server configuration");
            Ok(ServerConfig::default())
        }
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
    let config = load_config(config_path.as_deref())?;

    tracing::info!(
        bind_addr = %config.bind_addr,
        graphql_path = %config.graphql_path,
        health_path = %config.health_path,
        introspection_path = %config.introspection_path,
        "Server configuration loaded"
    );

    // Validate schema file exists
    validate_schema_path(&config.schema_path)?;

    // Load compiled schema
    let schema_loader = CompiledSchemaLoader::new(&config.schema_path);
    let _schema = schema_loader.load().await?;
    tracing::info!("Compiled schema loaded successfully");

    // For now, log a message about database adapter initialization
    // The database adapter will be initialized based on the schema configuration
    // in a future phase when Phase 2 (Database & Cache) is implemented
    tracing::warn!("Database adapter initialization is not yet implemented");
    tracing::warn!("This phase focuses on HTTP server setup only");
    tracing::warn!("Database connectivity will be added in Phase 2");

    tracing::info!(
        "FraiseQL Server {} ready",
        env!("CARGO_PKG_VERSION")
    );

    Ok(())
}
