//! FraiseQL Server binary.
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: server binary; pre-tracing startup banner and CLI errors go to stdout/stderr.

use std::{path::Path, sync::Arc};

use clap::Parser;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::{
    Cli, CompiledSchemaLoader, Server, ServerConfig,
    usage::{aggregator::global_aggregator, layer::MutationAuditLayer},
};
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
        let config = ServerConfig::from_file(path)
            .map_err(|e| anyhow::anyhow!("failed to parse config file `{path}`: {e}"))?;
        #[cfg(feature = "observers")]
        check_observer_config_layout(&config, path)?;
        Ok(config)
    } else {
        tracing::info!("Using default server configuration");
        Ok(ServerConfig::default())
    }
}

/// Reject the pre-#342 flat `[observers]` server-tuning layout with a clear
/// migration message.
///
/// As of v2.5.0 the server's runtime tuning (`poll_interval_ms`, `batch_size`,
/// `channel_capacity`, `auto_reload`, `reload_interval_secs`, `pool`) moved from
/// `[observers]` to `[observers.runtime]` so the `[observers]` table can be
/// shared with the compiler schema (#342). A flat key here is captured as a
/// migration trap rather than silently ignored — boot fails loud, the #342
/// contract.
///
/// # Errors
///
/// Returns an error naming the misplaced keys when a pre-#342 server-tuning key
/// is found directly under `[observers]`.
#[cfg(feature = "observers")]
fn check_observer_config_layout(config: &ServerConfig, path: &str) -> anyhow::Result<()> {
    if let Some(observers) = &config.observers {
        let misplaced = observers.misplaced_runtime_keys();
        if let Some(first) = misplaced.first() {
            anyhow::bail!(
                "config file `{path}`: the [observers] key(s) `{}` moved to \
                 [observers.runtime] in v2.5.0. Move them under an \
                 `[observers.runtime]` table, e.g.\n\n    [observers.runtime]\n    {first} = ...\n",
                misplaced.join("`, `"),
            );
        }
    }
    Ok(())
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

    // Audit layer is always installed; it only records events with the
    // `fraiseql::mutation_audit` target and is otherwise a zero-cost no-op.
    let audit_layer = MutationAuditLayer::new(Arc::clone(global_aggregator()));

    if is_json {
        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(audit_layer)
            .with(tracing_subscriber::fmt::layer().json());

        #[cfg(feature = "tracing-opentelemetry")]
        let subscriber = subscriber.with(build_otlp_layer(config));

        #[cfg(not(feature = "tracing-opentelemetry"))]
        let _ = config;

        subscriber.init();
    } else {
        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(audit_layer)
            .with(tracing_subscriber::fmt::layer());

        #[cfg(feature = "tracing-opentelemetry")]
        let subscriber = subscriber.with(build_otlp_layer(config));

        #[cfg(not(feature = "tracing-opentelemetry"))]
        let _ = config;

        subscriber.init();
    }
}

/// Redact credentials from an endpoint URL before logging.
///
/// If the URL contains userinfo (`user:pass@host`), the credentials are replaced
/// with `[REDACTED]`.  Non-URL strings and parse failures are returned as-is with
/// no credential risk (they don't contain structured userinfo).
#[cfg(feature = "tracing-opentelemetry")]
fn redact_endpoint_credentials(endpoint: &str) -> String {
    match url::Url::parse(endpoint) {
        Ok(mut parsed) => {
            if !parsed.username().is_empty() || parsed.password().is_some() {
                // Reason: infallible for valid URLs — set_username / set_password
                // only fail on `cannot-be-a-base` URLs which have a scheme.
                let _ = parsed.set_username("[REDACTED]");
                let _ = parsed.set_password(None);
            }
            parsed.to_string()
        },
        Err(_) => endpoint.to_string(),
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
        .map_err(|e| {
            eprintln!(
                "Failed to build OTLP exporter for {}: {e}",
                redact_endpoint_credentials(&endpoint)
            );
        })
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
        "OTLP tracing export enabled: endpoint={}, service_name={}",
        redact_endpoint_credentials(&endpoint),
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
async fn load_schema(config: &ServerConfig) -> anyhow::Result<LoadedSchema> {
    validate_schema_path(&config.schema_path)?;
    let schema_loader = CompiledSchemaLoader::new(&config.schema_path);
    // `load_extended` rather than `load`: the `functions` section is handed to the
    // server explicitly (#896) instead of being re-read from `schema_path` inside
    // `prepare_functions_runtime`, where it could name a different artifact than the
    // schema serving queries.
    let extended = schema_loader.load_extended().await?;
    // Install the schema's casing acronyms (on top of the built-in defaults) so the
    // runtime's `to_snake_case` JSONB-key resolution matches the compiled surface.
    fraiseql_core::utils::casing::set_runtime_acronyms(&extended.schema.naming_acronyms);
    tracing::info!("Compiled schema loaded successfully");
    Ok(LoadedSchema {
        schema: extended.schema,
        #[cfg(feature = "functions-runtime")]
        functions: extended.functions,
    })
}

/// The compiled schema plus the platform sections the server is given explicitly.
struct LoadedSchema {
    schema:    CompiledSchema,
    /// The `functions` section, handed to `Server::with_functions_config` (#896).
    #[cfg(feature = "functions-runtime")]
    functions: Option<fraiseql_server::schema::loader::FunctionsConfig>,
}

/// Initialize the secrets manager backend if `--secrets-backend` / `FRAISEQL_SECRETS_BACKEND` is
/// set.
///
/// `FRAISEQL_SECRETS_BACKEND` selects the backend: `env`, `file` or `vault`.
/// This used to accept **any** value and build the environment backend for all of
/// them — an operator who set `FRAISEQL_SECRETS_BACKEND=vault` got env-var
/// "secrets management" with a healthy startup log (#856's root cause). Unknown
/// values and missing per-backend settings now refuse to boot.
///
/// Backend settings, all read from the environment:
///
/// * `file` — `FRAISEQL_SECRETS_FILE_PATH` (directory of one-file-per-secret).
/// * `vault` — `VAULT_ADDR` plus either `VAULT_TOKEN` (static token) or `VAULT_ROLE_ID` +
///   `VAULT_SECRET_ID` (`AppRole`, with automatic token renewal); optional `VAULT_NAMESPACE`;
///   `VAULT_TLS_VERIFY=false` to accept invalid TLS certificates (refused in production).
#[cfg(feature = "secrets")]
async fn build_secrets_manager()
-> anyhow::Result<Option<Arc<fraiseql_server::secrets_manager::SecretsManager>>> {
    use fraiseql_server::secrets_manager::{SecretsBackendConfig, VaultAuth};

    let Ok(backend) = std::env::var("FRAISEQL_SECRETS_BACKEND") else {
        tracing::debug!("Secrets manager disabled (set FRAISEQL_SECRETS_BACKEND to enable)");
        return Ok(None);
    };

    let env_or_bail = |name: &str| -> anyhow::Result<String> {
        std::env::var(name).map_err(|_| {
            anyhow::anyhow!(
                "FRAISEQL_SECRETS_BACKEND={backend} requires {name} to be set; refusing to \
                 boot without it rather than running with a partially configured secrets \
                 backend"
            )
        })
    };

    let cfg = match backend.as_str() {
        "env" => SecretsBackendConfig::Env,
        "file" => SecretsBackendConfig::File {
            path: std::path::PathBuf::from(env_or_bail("FRAISEQL_SECRETS_FILE_PATH")?),
        },
        "vault" => {
            let addr = env_or_bail("VAULT_ADDR")?;
            let auth = match (std::env::var("VAULT_TOKEN"), std::env::var("VAULT_ROLE_ID")) {
                (Ok(token), _) => VaultAuth::Token(zeroize::Zeroizing::new(token)),
                (Err(_), Ok(role_id)) => VaultAuth::AppRole {
                    role_id,
                    secret_id: zeroize::Zeroizing::new(env_or_bail("VAULT_SECRET_ID")?),
                },
                (Err(_), Err(_)) => anyhow::bail!(
                    "FRAISEQL_SECRETS_BACKEND=vault requires either VAULT_TOKEN (static \
                     token) or VAULT_ROLE_ID + VAULT_SECRET_ID (AppRole login)"
                ),
            };
            SecretsBackendConfig::Vault {
                addr,
                auth,
                namespace: std::env::var("VAULT_NAMESPACE").ok(),
                // Anything but an explicit "false" verifies TLS; the factory refuses
                // `false` in production (#727).
                tls_verify: !std::env::var("VAULT_TLS_VERIFY")
                    .is_ok_and(|v| v.eq_ignore_ascii_case("false")),
            }
        },
        other => anyhow::bail!(
            "unknown FRAISEQL_SECRETS_BACKEND {other:?}; expected \"env\", \"file\" or \
             \"vault\". (Every value used to silently select the env backend, so a \
             deployment that asked for Vault ran on environment variables while its \
             startup log read healthy.)"
        ),
    };

    tracing::info!(backend = %backend, "Initializing secrets manager");
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
    // Fail loud (M-secrets-backend-stub): if an operator sets
    // FRAISEQL_SECRETS_BACKEND but this binary was built without the `secrets`
    // feature, returning `Ok(None)` silently runs with NO secrets manager — the
    // operator believes secrets are managed when they are not. Refuse to boot.
    if std::env::var("FRAISEQL_SECRETS_BACKEND").is_ok() {
        anyhow::bail!(
            "FRAISEQL_SECRETS_BACKEND is set, but this binary was built without the `secrets` \
             feature, so no secrets backend can be initialized. Rebuild with \
             `--features secrets`, or unset FRAISEQL_SECRETS_BACKEND to run without a secrets \
             manager."
        );
    }
    Ok(None)
}

/// Warn at startup when `[files.<name>]` sections are configured: file-upload
/// routes are not yet wired into the binary, so the sections are parsed but
/// otherwise ignored. Emitting a warning keeps them from being silently dropped.
fn warn_files_not_wired(config: &ServerConfig) {
    if config.files.is_empty() {
        return;
    }
    let mut names: Vec<&str> = config.files.keys().map(String::as_str).collect();
    names.sort_unstable();
    tracing::warn!(
        sections = %names.join(", "),
        "[files.<name>] sections are configured but file-upload routes are not yet wired into \
         the fraiseql-server binary; these sections are ignored."
    );
}

// #469's warn-only handling of `[observers]`-without-the-feature is gone: since
// `ServerConfig` denies unknown fields (#839), a build without the feature refuses
// the config at parse time and `ServerConfig::from_file` names the missing build
// feature in the error.

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

    // Install the global `metrics`-facade recorder so transitive crates
    // (e.g. fraiseql-core) have their facade emissions captured and exported via
    // the `/metrics` endpoint (audit H45). No-op unless the `metrics` feature is
    // built and metrics are enabled in config.
    #[cfg(feature = "metrics")]
    if config.metrics_enabled {
        fraiseql_server::metrics_recorder::install();
    }

    let loaded = load_schema(&config).await?;

    warn_files_not_wired(&config);
    // `warn_sources_feature_missing` used to live here (#573): a lean build logged a
    // warning and started no scheduler. Superseded by #1326 — `load_extended` now
    // REFUSES a schema declaring an enabled source this build cannot run, which happens
    // before this point, so the warning could only ever fire for an all-disabled
    // `sources` array. For that case its advice was wrong: the operator turned the
    // sources off, and "rebuild with `--features sources`" answers a question nobody
    // asked.

    // Box::pin: the per-scheme dispatch holds adapter init futures for all
    // enabled adapters, which combined exceeds clippy's `large_futures`
    // 16-KiB stack threshold. Heap-allocating once at startup is fine.
    Box::pin(dispatch_server(config, loaded, &cli)).await
}

/// Dispatch on the configured database URL scheme and run the matching
/// adapter-specific server entry point.
async fn dispatch_server(
    config: ServerConfig,
    loaded: LoadedSchema,
    cli: &Cli,
) -> anyhow::Result<()> {
    use fraiseql_server::url_guard::{DatabaseScheme, parse_database_url};

    // Box::pin: the server-setup future exceeds clippy's `large_futures`
    // 16-KiB threshold once optional subsystems (observers, MCP) are enabled.
    // Heap-allocating once at startup is fine.
    match parse_database_url(&config.database_url)? {
        DatabaseScheme::Postgres => Box::pin(run_postgres(config, loaded, cli)).await,
    }
}

/// PostgreSQL entry point — preserves the existing observer-pool, Arrow
/// Flight, and relay-detection behaviour.
async fn run_postgres(config: ServerConfig, loaded: LoadedSchema, cli: &Cli) -> anyhow::Result<()> {
    let LoadedSchema {
        schema,
        #[cfg(feature = "functions-runtime")]
        functions,
    } = loaded;
    // The loader refused a `functions` section this build cannot run.
    #[cfg(not(feature = "functions-runtime"))]
    let functions = None;

    // Boxed: the assembly future carries every subsystem's construction, past
    // clippy::large_futures' 16 KiB threshold. One allocation, once per process.
    let server =
        Box::pin(fraiseql_server::boot::build_postgres_server(config, schema, functions)).await?;
    finish_server(server, cli, cfg!(feature = "arrow")).await
}

/// Finalize startup for any constructed `Server`: attach the secrets
/// manager if configured, dispatch to MCP-stdio mode if requested, otherwise
/// start the HTTP server.
#[cfg_attr(not(feature = "mcp"), allow(unused_variables))]
async fn finish_server(server: Server, cli: &Cli, with_arrow: bool) -> anyhow::Result<()> {
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
        // Boxed: the stdio path now builds the full `AppState` (#858), which puts
        // this future over the `clippy::large_futures` threshold.
        Box::pin(server.serve_mcp_stdio()).await?;
        return Ok(());
    }

    if with_arrow {
        tracing::info!(
            "FraiseQL Server {} starting (HTTP + Arrow Flight)",
            env!("CARGO_PKG_VERSION")
        );
    } else {
        tracing::info!("FraiseQL Server {} starting (HTTP only)", env!("CARGO_PKG_VERSION"));
    }

    // Boxed because `Server` is large and `serve()`'s future carries it (#1102 pushed it
    // past clippy::large_futures' 16 KiB threshold). One heap allocation, once per
    // process, on a future that lives for the process's whole run.
    Box::pin(server.serve()).await?;
    Ok(())
}
