//! FraiseQL Server binary.

use std::{env, path::Path, sync::Arc};

#[cfg(feature = "wire-backend")]
use fraiseql_core::db::FraiseWireAdapter;
#[cfg(not(feature = "wire-backend"))]
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_server::{
    CompiledSchemaLoader, Server, ServerConfig, middleware::RateLimitConfig,
};
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

/// Apply rate-limiting env var overrides to config.
///
/// Reads all four `FRAISEQL_RATE_LIMIT_*` environment variables up front and
/// mutates `config.rate_limiting` exactly once, avoiding repeated
/// `.take().unwrap_or(default)` constructions when multiple variables are set.
fn apply_rate_limit_overrides(config: &mut ServerConfig) {
    let enabled_raw = std::env::var("FRAISEQL_RATE_LIMITING_ENABLED").ok();
    let rps_ip_raw = std::env::var("FRAISEQL_RATE_LIMIT_RPS_PER_IP").ok();
    let rps_user_raw = std::env::var("FRAISEQL_RATE_LIMIT_RPS_PER_USER").ok();
    let burst_raw = std::env::var("FRAISEQL_RATE_LIMIT_BURST_SIZE").ok();

    if enabled_raw.is_none() && rps_ip_raw.is_none() && rps_user_raw.is_none() && burst_raw.is_none() {
        return;
    }

    let mut rate_config = config.rate_limiting.take().unwrap_or_else(|| RateLimitConfig {
        enabled:               true,
        rps_per_ip:            100,
        rps_per_user:          1000,
        burst_size:            500,
        cleanup_interval_secs: 300,
        trust_proxy_headers:   false,
        trusted_proxy_cidrs:   Vec::new(),
    });

    if let Some(val) = enabled_raw {
        warn_if_unrecognised_bool("FRAISEQL_RATE_LIMITING_ENABLED", &val);
        rate_config.enabled = parse_bool_env(&val);
    }
    if let Some(val) = rps_ip_raw {
        if let Ok(v) = val.parse() {
            rate_config.rps_per_ip = v;
        }
    }
    if let Some(val) = rps_user_raw {
        if let Ok(v) = val.parse() {
            rate_config.rps_per_user = v;
        }
    }
    if let Some(val) = burst_raw {
        if let Ok(v) = val.parse() {
            rate_config.burst_size = v;
        }
    }

    config.rate_limiting = Some(rate_config);
}

/// Parse a boolean environment variable value consistently.
///
/// Returns `true` for `"true"`, `"1"`, `"yes"`, `"on"` (case-insensitive);
/// returns `false` for all other values including empty string and unrecognised inputs.
fn parse_bool_env(val: &str) -> bool {
    matches!(val.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on")
}

/// Emit a warning if `val` is neither a recognised truthy nor a recognised falsy boolean string.
///
/// Recognised values: `true`, `1`, `yes`, `on`, `false`, `0`, `no`, `off` (case-insensitive).
/// Any other value (e.g. `"enabled"`, `"active"`) is silently treated as `false` by
/// `parse_bool_env`; this warning surfaces that silent mis-configuration.
fn warn_if_unrecognised_bool(var: &str, val: &str) {
    if !matches!(
        val.to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on" | "false" | "0" | "no" | "off"
    ) {
        tracing::warn!(
            variable = var,
            value = val,
            "Unrecognised boolean value; defaulting to false. \
             Use true/false, 1/0, yes/no, or on/off."
        );
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

/// Entry point.
///
/// Initialization sequence:
/// 1. **Tracing** — set up `tracing_subscriber` with `RUST_LOG` env filter.
/// 2. **Config** — load `ServerConfig` from file (via `FRAISEQL_CONFIG`) or defaults,
///    then apply env var overrides for database URL, bind address, schema path,
///    metrics, admin API, introspection, and rate limiting.
/// 3. **Schema** — validate the compiled schema file exists and load it.
/// 4. **Security** — (auth feature) initialize and validate security config from schema.
/// 5. **Database** — create the PostgreSQL or Wire database adapter.
/// 6. **Observers / Secrets** — optionally create sqlx pool for observers and
///    initialize the secrets manager backend.
/// 7. **Server** — construct `Server` (with optional Arrow Flight service),
///    optionally attach secrets manager, then call `serve()` (or `serve_mcp_stdio()`).
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
        warn_if_unrecognised_bool("FRAISEQL_METRICS_ENABLED", &metrics_enabled);
        config.metrics_enabled = parse_bool_env(&metrics_enabled);
    }
    if let Ok(metrics_token) = env::var("FRAISEQL_METRICS_TOKEN") {
        config.metrics_token = Some(metrics_token);
    }

    // Admin API configuration from environment
    if let Ok(admin_enabled) = env::var("FRAISEQL_ADMIN_API_ENABLED") {
        warn_if_unrecognised_bool("FRAISEQL_ADMIN_API_ENABLED", &admin_enabled);
        config.admin_api_enabled = parse_bool_env(&admin_enabled);
    }
    if let Ok(admin_token) = env::var("FRAISEQL_ADMIN_TOKEN") {
        config.admin_token = Some(admin_token);
    }

    // Introspection configuration from environment
    if let Ok(introspection_enabled) = env::var("FRAISEQL_INTROSPECTION_ENABLED") {
        warn_if_unrecognised_bool("FRAISEQL_INTROSPECTION_ENABLED", &introspection_enabled);
        config.introspection_enabled = parse_bool_env(&introspection_enabled);
    }
    if let Ok(introspection_require_auth) = env::var("FRAISEQL_INTROSPECTION_REQUIRE_AUTH") {
        warn_if_unrecognised_bool("FRAISEQL_INTROSPECTION_REQUIRE_AUTH", &introspection_require_auth);
        config.introspection_require_auth = parse_bool_env(&introspection_require_auth);
    }

    // Rate limiting configuration from environment — all four vars handled atomically.
    apply_rate_limit_overrides(&mut config);

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
        // Reason: pool sizes are always ≪ u32::MAX in practice
        let pool = PgPoolOptions::new()
            .min_connections(config.pool_min_size as u32)
            .max_connections(config.pool_max_size as u32)
            .connect(&config.database_url)
            .await?;
        Some(pool)
    };
    #[cfg(not(feature = "observers"))]
    let db_pool: Option<sqlx::PgPool> = None;

    // Initialize secrets manager if configured via environment
    #[cfg(feature = "secrets")]
    let secrets_manager = if env::var("FRAISEQL_SECRETS_BACKEND").is_ok() {
        tracing::info!("Initializing secrets manager from environment configuration");
        let cfg = fraiseql_server::secrets_manager::SecretsBackendConfig::Env;
        match fraiseql_server::secrets_manager::create_secrets_manager(cfg).await {
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

    // Create server — arrow path adds an Arrow Flight gRPC endpoint.
    #[cfg(feature = "arrow")]
    let server = {
        use fraiseql_server::arrow::create_flight_service;
        let flight_service = create_flight_service(adapter.clone());
        tracing::info!("Arrow Flight service initialized with real database adapter");
        Server::with_flight_service(config, schema, adapter, db_pool, Some(flight_service)).await?
    };
    #[cfg(not(feature = "arrow"))]
    let server = Server::new(config, schema, adapter, db_pool).await?;

    // Attach secrets manager if configured.
    #[cfg(feature = "secrets")]
    let mut server = server;
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

    #[cfg(feature = "arrow")]
    tracing::info!(
        "FraiseQL Server {} starting (HTTP + Arrow Flight)",
        env!("CARGO_PKG_VERSION")
    );
    #[cfg(not(feature = "arrow"))]
    tracing::info!("FraiseQL Server {} starting (HTTP only)", env!("CARGO_PKG_VERSION"));

    server.serve().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_bool_env, warn_if_unrecognised_bool};

    #[test]
    fn parse_bool_env_truthy_values() {
        assert!(parse_bool_env("true"));
        assert!(parse_bool_env("TRUE"));
        assert!(parse_bool_env("True"));
        assert!(parse_bool_env("1"));
        assert!(parse_bool_env("yes"));
        assert!(parse_bool_env("YES"));
        assert!(parse_bool_env("on"));
        assert!(parse_bool_env("ON"));
    }

    #[test]
    fn parse_bool_env_falsy_values() {
        assert!(!parse_bool_env("false"));
        assert!(!parse_bool_env("FALSE"));
        assert!(!parse_bool_env("0"));
        assert!(!parse_bool_env("no"));
        assert!(!parse_bool_env("off"));
        assert!(!parse_bool_env(""));
        assert!(!parse_bool_env("unexpected"));
        assert!(!parse_bool_env("2"));
    }

    #[test]
    fn warn_if_unrecognised_bool_does_not_panic_for_any_input() {
        // All recognised values — function must be a no-op (no panic).
        for val in &["true", "TRUE", "1", "yes", "YES", "on", "ON",
                     "false", "FALSE", "0", "no", "NO", "off", "OFF"] {
            warn_if_unrecognised_bool("TEST_VAR", val);
        }
        // Unrecognised values — function emits a warning but must not panic.
        for val in &["enabled", "active", "2", "", "maybe"] {
            warn_if_unrecognised_bool("TEST_VAR", val);
        }
    }
}
