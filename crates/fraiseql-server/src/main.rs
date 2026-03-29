//! FraiseQL Server binary.

use std::{env, path::Path, sync::Arc};

#[cfg(feature = "wire-backend")]
use fraiseql_core::db::FraiseWireAdapter;
#[cfg(not(feature = "wire-backend"))]
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::{CompiledSchemaLoader, Server, ServerConfig, middleware::RateLimitConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// ── Helper functions ──────────────────────────────────────────────────────

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

    if enabled_raw.is_none()
        && rps_ip_raw.is_none()
        && rps_user_raw.is_none()
        && burst_raw.is_none()
    {
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

/// Set up tracing subscriber with `RUST_LOG` env filter.
fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fraiseql_server=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Load config from file/defaults, apply all env var overrides, then validate.
fn load_and_validate_config() -> anyhow::Result<ServerConfig> {
    let config_path = env::var("FRAISEQL_CONFIG").ok();
    let mut config = load_config(config_path.as_deref())?;

    // Override configuration from environment variables if set.
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

    // Metrics configuration from environment.
    if let Ok(metrics_enabled) = env::var("FRAISEQL_METRICS_ENABLED") {
        warn_if_unrecognised_bool("FRAISEQL_METRICS_ENABLED", &metrics_enabled);
        config.metrics_enabled = parse_bool_env(&metrics_enabled);
    }
    if let Ok(metrics_token) = env::var("FRAISEQL_METRICS_TOKEN") {
        config.metrics_token = Some(metrics_token);
    }

    // Admin API configuration from environment.
    if let Ok(admin_enabled) = env::var("FRAISEQL_ADMIN_API_ENABLED") {
        warn_if_unrecognised_bool("FRAISEQL_ADMIN_API_ENABLED", &admin_enabled);
        config.admin_api_enabled = parse_bool_env(&admin_enabled);
    }
    if let Ok(admin_token) = env::var("FRAISEQL_ADMIN_TOKEN") {
        config.admin_token = Some(admin_token);
    }

    // Introspection configuration from environment.
    if let Ok(introspection_enabled) = env::var("FRAISEQL_INTROSPECTION_ENABLED") {
        warn_if_unrecognised_bool("FRAISEQL_INTROSPECTION_ENABLED", &introspection_enabled);
        config.introspection_enabled = parse_bool_env(&introspection_enabled);
    }
    if let Ok(introspection_require_auth) = env::var("FRAISEQL_INTROSPECTION_REQUIRE_AUTH") {
        warn_if_unrecognised_bool(
            "FRAISEQL_INTROSPECTION_REQUIRE_AUTH",
            &introspection_require_auth,
        );
        config.introspection_require_auth = parse_bool_env(&introspection_require_auth);
    }

    // Rate limiting configuration from environment — all four vars handled atomically.
    apply_rate_limit_overrides(&mut config);

    if let Err(e) = config.validate() {
        tracing::error!(error = %e, "Configuration validation failed");
        anyhow::bail!(e);
    }

    Ok(config)
}

/// Load and validate the compiled schema from the path in `config`.
async fn load_schema(config: &ServerConfig) -> anyhow::Result<CompiledSchema> {
    validate_schema_path(&config.schema_path)?;
    let schema_loader = CompiledSchemaLoader::new(&config.schema_path);
    let schema = schema_loader.load().await?;
    tracing::info!("Compiled schema loaded successfully");
    Ok(schema)
}

/// Initialize security configuration from the compiled schema (auth feature only).
///
/// Without `[auth]` configured, this is a no-op and RBAC/admin endpoints are
/// unprotected by OIDC — use `admin_token` or network controls as defence-in-depth.
#[cfg(feature = "auth")]
fn init_security(schema: &CompiledSchema) -> anyhow::Result<()> {
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
    if let Err(e) = fraiseql_server::auth::validate_security_config(&security_config) {
        tracing::error!(error = %e, "Security configuration validation failed");
        anyhow::bail!(e);
    }
    fraiseql_server::auth::log_security_config(&security_config);
    Ok(())
}

#[cfg(not(feature = "auth"))]
fn init_security(_schema: &CompiledSchema) -> anyhow::Result<()> {
    Ok(())
}

/// Create the database adapter — PostgreSQL (default) or FraiseQL Wire (`wire-backend`).
#[cfg(not(feature = "wire-backend"))]
async fn build_adapter(config: &ServerConfig) -> anyhow::Result<Arc<PostgresAdapter>> {
    tracing::info!(
        database_url = %config.database_url,
        pool_min_size = config.pool_min_size,
        pool_max_size = config.pool_max_size,
        "Initializing PostgreSQL database adapter"
    );
    let adapter = PostgresAdapter::with_pool_config(
        &config.database_url,
        config.pool_min_size,
        config.pool_max_size,
    )
    .await?;
    tracing::info!("PostgreSQL adapter initialized successfully with connection pooling");
    Ok(Arc::new(adapter))
}

#[cfg(feature = "wire-backend")]
async fn build_adapter(config: &ServerConfig) -> anyhow::Result<Arc<FraiseWireAdapter>> {
    tracing::info!(
        database_url = %config.database_url,
        "Initializing FraiseQL Wire database adapter (low-memory streaming)"
    );
    let adapter = FraiseWireAdapter::new(&config.database_url);
    tracing::info!("FraiseQL Wire adapter initialized successfully");
    Ok(Arc::new(adapter))
}

/// Create a dedicated PostgreSQL pool for the observer runtime.
///
/// Observers require their own pool because the LISTEN/NOTIFY connection
/// occupies a persistent slot that must not be shared with request-serving
/// connections (request connections need to be available for concurrent queries).
#[cfg(feature = "observers")]
async fn build_observer_pool(config: &ServerConfig) -> anyhow::Result<Option<sqlx::PgPool>> {
    use sqlx::postgres::PgPoolOptions;
    #[allow(clippy::cast_possible_truncation)] // Reason: pool sizes are always far below u32::MAX in practice
    let pool = PgPoolOptions::new()
        .min_connections(config.pool_min_size as u32)
        .max_connections(config.pool_max_size as u32)
        .connect(&config.database_url)
        .await?;
    Ok(Some(pool))
}

#[cfg(not(feature = "observers"))]
async fn build_observer_pool(_config: &ServerConfig) -> anyhow::Result<Option<sqlx::PgPool>> {
    Ok(None)
}

/// Initialize the secrets manager backend if `FRAISEQL_SECRETS_BACKEND` is set.
#[cfg(feature = "secrets")]
async fn build_secrets_manager()
-> anyhow::Result<Option<Arc<fraiseql_server::secrets_manager::SecretsManager>>> {
    if env::var("FRAISEQL_SECRETS_BACKEND").is_err() {
        tracing::debug!("Secrets manager disabled (set FRAISEQL_SECRETS_BACKEND to enable)");
        return Ok(None);
    }
    tracing::info!("Initializing secrets manager from environment configuration");
    let cfg = fraiseql_server::secrets_manager::SecretsBackendConfig::Env;
    match fraiseql_server::secrets_manager::create_secrets_manager(cfg).await {
        Ok(manager) => Ok(Some(manager)),
        Err(e) => {
            tracing::error!(error = %e, "Failed to initialize secrets manager");
            anyhow::bail!("Secrets manager initialization failed: {}", e)
        },
    }
}

#[cfg(not(feature = "secrets"))]
async fn build_secrets_manager() -> anyhow::Result<Option<std::convert::Infallible>> {
    Ok(None)
}

// ── Entry point ───────────────────────────────────────────────────────────

/// Entry point.
///
/// Initialization sequence:
/// 1. **Tracing** — set up `tracing_subscriber` with `RUST_LOG` env filter.
/// 2. **Config** — load `ServerConfig` from file (via `FRAISEQL_CONFIG`) or defaults, then apply
///    env var overrides for database URL, bind address, schema path, metrics, admin API,
///    introspection, and rate limiting.
/// 3. **Schema** — validate the compiled schema file exists and load it.
/// 4. **Security** — (auth feature) initialize and validate security config from schema.
/// 5. **Database** — create the PostgreSQL or Wire database adapter.
/// 6. **Observers / Secrets** — optionally create sqlx pool for observers and initialize the
///    secrets manager backend.
/// 7. **Server** — construct `Server` (with optional Arrow Flight service), optionally attach
///    secrets manager, then call `serve()` (or `serve_mcp_stdio()`).
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    tracing::info!("FraiseQL Server v{}", env!("CARGO_PKG_VERSION"));

    let config = load_and_validate_config()?;
    tracing::info!(
        bind_addr = %config.bind_addr,
        database_url = %config.database_url,
        graphql_path = %config.graphql_path,
        health_path = %config.health_path,
        introspection_path = %config.introspection_path,
        metrics_enabled = config.metrics_enabled,
        "Server configuration loaded"
    );

    let schema = load_schema(&config).await?;
    init_security(&schema)?;

    let adapter = build_adapter(&config).await?;
    let db_pool = build_observer_pool(&config).await?;

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
    if let Some(mgr) = build_secrets_manager().await? {
        server.set_secrets_manager(mgr);
    }
    #[cfg(not(feature = "secrets"))]
    let _ = build_secrets_manager().await?;

    // Serve MCP over stdio if requested, otherwise start HTTP server.
    #[cfg(feature = "mcp")]
    if env::var("FRAISEQL_MCP_STDIO").is_ok() {
        tracing::info!("FraiseQL MCP stdio mode starting");
        server.serve_mcp_stdio().await?;
        return Ok(());
    }

    #[cfg(feature = "arrow")]
    tracing::info!("FraiseQL Server {} starting (HTTP + Arrow Flight)", env!("CARGO_PKG_VERSION"));
    #[cfg(not(feature = "arrow"))]
    tracing::info!("FraiseQL Server {} starting (HTTP only)", env!("CARGO_PKG_VERSION"));

    // Wire-backend adapters are read-only — use serve() which mounts only
    // query routes.  Full adapters (PostgreSQL, MySQL, SQL Server) use
    // serve_mut() to include REST mutation routes.
    #[cfg(feature = "wire-backend")]
    server.serve().await?;
    #[cfg(not(feature = "wire-backend"))]
    server.serve_mut().await?;
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
        for val in &[
            "true", "TRUE", "1", "yes", "YES", "on", "ON", "false", "FALSE", "0", "no", "NO",
            "off", "OFF",
        ] {
            warn_if_unrecognised_bool("TEST_VAR", val);
        }
        // Unrecognised values — function emits a warning but must not panic.
        for val in &["enabled", "active", "2", "", "maybe"] {
            warn_if_unrecognised_bool("TEST_VAR", val);
        }
    }
}
