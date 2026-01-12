//! FraiseQL Server binary.

// use fraiseql_server::{Server, ServerConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fraiseql_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("FraiseQL Server v{}", env!("CARGO_PKG_VERSION"));

    // TODO: Load configuration from file or environment
    // TODO: Load compiled schema from file
    // TODO: Initialize database adapter
    // For now, this is a placeholder

    tracing::warn!("Server startup requires compiled schema and database connection");
    tracing::warn!("This binary is a placeholder - use as library for now");

    Ok(())
}
