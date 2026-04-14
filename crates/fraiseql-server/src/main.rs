//! FraiseQL Server binary.

use std::{path::Path, sync::Arc};

use clap::Parser;
#[cfg(feature = "wire-backend")]
use fraiseql_core::db::FraiseWireAdapter;
#[cfg(not(feature = "wire-backend"))]
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::{Cli, CompiledSchemaLoader, Server, ServerConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// ── Helper functions ──────────────────────────────────────────────────────

/// Load configuration from file or use defaults.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or is not valid TOML.
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
///
/// # Errors
///
/// Returns an error with a user-friendly message if the file does not exist.
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

/// Set up tracing subscriber with `RUST_LOG` env filter and optional OTLP export.
///
/// When `FRAISEQL_LOG_FORMAT=json` (case-insensitive), logs are emitted as
/// newline-delimited JSON — suitable for structured log aggregators such as
/// Datadog, Loki, or `CloudWatch`. Otherwise the default human-readable format
/// is used.
///
/// If an OTLP endpoint is configured (via `TracingConfig.otlp_endpoint` or the
/// `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable), an `OpenTelemetry` span
/// exporter is added as an additional tracing layer.  When no endpoint is set,
/// no gRPC connection is attempted and there is zero overhead.
fn init_tracing(config: &ServerConfig, is_json: bool) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "fraiseql_server=info,tower_http=info,axum=info".into());

    if is_json {
        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json());

        #[cfg(feature = "tracing-opentelemetry")]
        let subscriber = subscriber.with(build_otlp_layer(config));

        #[cfg(not(feature = "tracing-opentelemetry"))]
        let _ = config;

        subscriber.init();
    } else {
        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer());

        #[cfg(feature = "tracing-opentelemetry")]
        let subscriber = subscriber.with(build_otlp_layer(config));

        #[cfg(not(feature = "tracing-opentelemetry"))]
        let _ = config;

        subscriber.init();
    }
}

/// Resolve the OTLP endpoint from config or environment, returning `None` if
/// neither is set (meaning OTLP export should be skipped entirely).
#[cfg(feature = "tracing-opentelemetry")]
fn resolve_otlp_endpoint(config: &ServerConfig) -> Option<String> {
    config
        .otlp_endpoint
        .clone()
        .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
}

/// Build an optional `OpenTelemetry` tracing layer.
///
/// Returns `Some(layer)` when an OTLP endpoint is configured, `None` otherwise.
/// Failures during OTLP setup are logged to stderr (tracing is not yet initialized)
/// and result in `None` — the server continues without OTLP export.
#[cfg(feature = "tracing-opentelemetry")]
fn build_otlp_layer<S>(
    config: &ServerConfig,
) -> Option<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::SdkTracerProvider;

    let endpoint = resolve_otlp_endpoint(config)?;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(&endpoint)
        .with_timeout(std::time::Duration::from_secs(config.otlp_export_timeout_secs))
        .build()
        .map_err(|e| eprintln!("Failed to build OTLP exporter for {endpoint}: {e}"))
        .ok()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(config.tracing_service_name.clone())
                .build(),
        )
        .build();

    let tracer = provider.tracer("fraiseql");
    eprintln!(
        "OTLP tracing export enabled: endpoint={endpoint}, service_name={}",
        config.tracing_service_name
    );

    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// Load config from file/defaults, apply all CLI/env overrides, then validate.
///
/// # Errors
///
/// Returns an error if configuration loading fails (file I/O, parse errors) or
/// if the resulting configuration is invalid.
fn load_and_validate_config(cli: &Cli) -> anyhow::Result<ServerConfig> {
    let mut config = load_config(cli.server.config.as_deref())?;

    // Apply all CLI flag and env var overrides in one pass.
    cli.server.apply_to_config(&mut config);

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
        pool_min_size     = config.pool_min_size,
        pool_max_size     = config.pool_max_size,
        pool_timeout_secs = config.pool_timeout_secs,
        "Initializing PostgreSQL connection pool"
    );
    let adapter = PostgresAdapter::with_pool_config(
        &config.database_url,
        fraiseql_core::db::postgres::PoolPrewarmConfig {
            min_size:     config.pool_min_size,
            max_size:     config.pool_max_size,
            timeout_secs: Some(config.pool_timeout_secs),
        },
    )
    .await?;
    tracing::info!("PostgreSQL adapter ready");
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
///
/// The observer pool is configured independently via `[observers.pool]` in
/// `fraiseql.toml`. When absent, observer-specific defaults are used (smaller
/// than the application pool — observers need far fewer connections).
#[cfg(feature = "observers")]
async fn build_observer_pool(config: &ServerConfig) -> anyhow::Result<Option<sqlx::PgPool>> {
    use std::time::Duration;

    use sqlx::postgres::PgPoolOptions;

    let pool_cfg = config
        .observers
        .as_ref()
        .map(|o| o.pool.clone())
        .unwrap_or_default();

    tracing::info!(
        min = pool_cfg.min_connections,
        max = pool_cfg.max_connections,
        timeout_secs = pool_cfg.acquire_timeout_secs,
        "Initializing observer PostgreSQL pool"
    );

    let pool = PgPoolOptions::new()
        .min_connections(pool_cfg.min_connections)
        .max_connections(pool_cfg.max_connections)
        .acquire_timeout(Duration::from_secs(pool_cfg.acquire_timeout_secs))
        .connect(&config.database_url)
        .await?;

    Ok(Some(pool))
}

#[cfg(not(feature = "observers"))]
async fn build_observer_pool(_config: &ServerConfig) -> anyhow::Result<Option<sqlx::PgPool>> {
    Ok(None)
}

/// Initialize the secrets manager backend if `--secrets-backend` / `FRAISEQL_SECRETS_BACKEND` is
/// set.
#[cfg(feature = "secrets")]
async fn build_secrets_manager()
-> anyhow::Result<Option<Arc<fraiseql_server::secrets_manager::SecretsManager>>> {
    if std::env::var("FRAISEQL_SECRETS_BACKEND").is_err() {
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
/// 1. **CLI** — parse command-line flags and env var overrides via clap.
/// 2. **Config** — load `ServerConfig` from file (via `--config` / `FRAISEQL_CONFIG`) or defaults,
///    then apply CLI/env overrides for database URL, bind address, schema path, metrics, admin API,
///    introspection, and rate limiting.
/// 3. **Tracing** — set up `tracing_subscriber` with `RUST_LOG` env filter.
/// 4. **Schema** — validate the compiled schema file exists and load it.
/// 5. **Security** — (auth feature) initialize and validate security config from schema.
/// 6. **Database** — create the PostgreSQL or Wire database adapter.
/// 7. **Observers / Secrets** — optionally create sqlx pool for observers and initialize the
///    secrets manager backend.
/// 8. **Server** — construct `Server` (with optional Arrow Flight service), optionally attach
///    secrets manager, then call `serve()` (or `serve_mcp_stdio()`).
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load config first so tracing can include the OTLP layer if configured.
    // Tracing calls in load_and_validate_config are silently discarded (no
    // subscriber yet); critical errors surface via the Result return.
    let config = load_and_validate_config(&cli)?;
    init_tracing(&config, cli.server.is_json_log_format());
    tracing::info!("FraiseQL Server v{}", env!("CARGO_PKG_VERSION"));
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
    // Use relay-capable server when the schema has relay queries (fraiseql/fraiseql#191).
    // wire-backend uses FraiseWireAdapter which does not implement RelayDatabaseAdapter,
    // so relay auto-detection is skipped and Server::new is used unconditionally there.
    #[cfg(not(any(feature = "arrow", feature = "wire-backend")))]
    let has_relay_queries = schema.queries.iter().any(|q| q.relay);
    #[cfg(not(any(feature = "arrow", feature = "wire-backend")))]
    let server = if has_relay_queries {
        Server::with_relay_pagination(config, schema, adapter, db_pool).await?
    } else {
        Server::new(config, schema, adapter, db_pool).await?
    };
    #[cfg(all(not(feature = "arrow"), feature = "wire-backend"))]
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
    if cli.mcp_stdio.is_some() {
        tracing::info!("FraiseQL MCP stdio mode starting");
        server.serve_mcp_stdio().await?;
        return Ok(());
    }

    #[cfg(feature = "arrow")]
    tracing::info!("FraiseQL Server {} starting (HTTP + Arrow Flight)", env!("CARGO_PKG_VERSION"));
    #[cfg(not(feature = "arrow"))]
    tracing::info!("FraiseQL Server {} starting (HTTP only)", env!("CARGO_PKG_VERSION"));

    server.serve().await?;
    Ok(())
}
