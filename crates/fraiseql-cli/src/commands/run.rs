//! `fraiseql run` — compile schema in-memory and serve the GraphQL API.
//!
//! This command compiles the schema without writing any artifacts to disk and
//! immediately starts the HTTP server.  With `--watch`, the schema file is
//! monitored for changes and the server is hot-reloaded on every save.
//!
//! ## Configuration resolution
//!
//! Settings are resolved in descending priority order:
//!
//! 1. CLI flags (`--port`, `--bind`, `--database`)
//! 2. Environment variables (`DATABASE_URL`, `FRAISEQL_PORT`, `FRAISEQL_HOST`)
//! 3. `fraiseql.toml` `[server]` / `[database]` sections
//! 4. Built-in defaults (`0.0.0.0:8080`, pool 2-20)

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use fraiseql_core::{
    cache::CachedDatabaseAdapter,
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    schema::CompiledSchema,
};
use fraiseql_server::{
    Server, ServerConfig,
    server_config::TlsServerConfig,
    url_guard::{DatabaseScheme, parse_database_url},
};
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use tracing::info;

use super::compile::{CompileOptions, compile_to_schema};
use crate::config::{
    DatabaseRuntimeConfig, ServerRuntimeConfig, TomlProjectConfig, TomlSchema,
    runtime::TlsRuntimeConfig,
};

/// Run the `fraiseql run` command.
///
/// # Arguments
///
/// * `input`         - Path to input file; `None` triggers auto-detection.
/// * `database`      - Database URL override; falls back to `DATABASE_URL` env var, then to
///   `[database].url` in `fraiseql.toml`.
/// * `port`          - TCP port override; `None` means fall back to TOML / default.
/// * `bind`          - Bind host override; `None` means fall back to TOML / default.
/// * `watch`         - Watch input file for changes and hot-reload.
/// * `introspection` - Enable the `/introspection` endpoint (no auth).
///
/// # Errors
///
/// Returns error if the input file cannot be found, the schema fails to compile,
/// the database URL is missing from all sources, or the server cannot bind.
pub async fn run(
    input: Option<&str>,
    database: Option<String>,
    port: Option<u16>,
    bind: Option<String>,
    watch: bool,
    introspection: bool,
) -> Result<()> {
    let input_path = resolve_input(input)?;

    let (db_url, bind_addr, server_cfg, db_cfg) =
        resolve_runtime_config(&input_path, database, port, bind)?;

    println!("FraiseQL");
    println!("   Schema: {}", input_path.display());
    println!("   Server: http://{bind_addr}/graphql");
    println!();

    // Box::pin both branches: each holds the per-adapter init futures the
    // dispatch helper splits into. Heap-allocating once per `fraiseql run`
    // invocation keeps clippy's `large_futures` lint satisfied.
    if watch {
        Box::pin(run_watch_loop(
            &input_path,
            &db_url,
            bind_addr,
            introspection,
            &server_cfg,
            &db_cfg,
        ))
        .await
    } else {
        Box::pin(run_once(&input_path, &db_url, bind_addr, introspection, &server_cfg, &db_cfg))
            .await
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Compile the schema and start the server once (no watch).
async fn run_once(
    input_path: &Path,
    db_url: &str,
    bind_addr: SocketAddr,
    introspection: bool,
    server_cfg: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
) -> Result<()> {
    let scheme = parse_database_url(db_url)?;
    let schema = compile_schema(input_path).await?;
    let config = build_config_from(db_url, bind_addr, server_cfg, db_cfg, introspection);

    println!("Server ready at http://{bind_addr}/graphql");
    println!("   Press Ctrl+C to stop");
    println!();

    Box::pin(dispatch_serve(scheme, config, schema, None)).await
}

/// Watch-loop: compile -> serve -> detect file change -> restart.
async fn run_watch_loop(
    input_path: &Path,
    db_url: &str,
    bind_addr: SocketAddr,
    introspection: bool,
    server_cfg: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
) -> Result<()> {
    let scheme = parse_database_url(db_url)?;

    loop {
        let schema = compile_schema(input_path).await?;
        let config = build_config_from(db_url, bind_addr, server_cfg, db_cfg, introspection);

        println!("Server ready at http://{bind_addr}/graphql");
        println!("   Watching {} for changes...  (Ctrl+C to stop)", input_path.display());
        println!();

        // oneshot channel: file watcher signals server to shut down
        let (change_tx, change_rx) = tokio::sync::oneshot::channel::<()>();

        // AtomicBool: was the shutdown triggered by a file change?
        let restarting = Arc::new(AtomicBool::new(false));
        let restarting_for_watcher = restarting.clone();

        // Spawn file watcher on a blocking thread (notify uses std channels)
        let watch_path = input_path.to_path_buf();
        let _watcher_guard = spawn_file_watcher(watch_path, move |_event| {
            restarting_for_watcher.store(true, Ordering::SeqCst);
            let _ = change_tx.send(());
        })?;

        Box::pin(dispatch_serve(
            scheme,
            config,
            schema,
            Some(Box::pin(async move {
                tokio::select! {
                    () = sigint_signal() => {},
                    result = change_rx => {
                        if result.is_err() {
                            // Sender dropped without sending — treat as OS signal
                        }
                    },
                }
            })),
        ))
        .await?;

        if !restarting.load(Ordering::SeqCst) {
            // Shutdown was triggered by Ctrl+C / SIGTERM — exit cleanly
            break;
        }

        // Small delay to ensure the file write is complete before re-reading
        tokio::time::sleep(Duration::from_millis(200)).await;
        println!("Schema changed, recompiling...");
    }

    Ok(())
}

/// Wait for the OS-level shutdown signal (SIGINT / Ctrl-C), wrapped so that
/// each [`dispatch_serve`] call can wire it into a `serve_with_shutdown`
/// future without taking a turbofish on the concrete `Server<…>` type.
async fn sigint_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Type alias for the per-loop shutdown future passed to
/// [`dispatch_serve`]. `None` means "use the default `serve()` shutdown
/// handling"; `Some(fut)` is forwarded to `serve_with_shutdown(fut)`.
type ShutdownFuture =
    Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>>;

/// Dispatch the configured database URL scheme to the matching adapter and
/// hand the resulting `Server` to either `serve()` or `serve_with_shutdown()`.
async fn dispatch_serve(
    scheme: DatabaseScheme,
    config: ServerConfig,
    schema: CompiledSchema,
    shutdown: ShutdownFuture,
) -> Result<()> {
    // Box::pin each arm: the adapter-init futures (SQLite in particular) exceed
    // clippy's `large_futures` 16-KiB stack threshold. Heap-allocating once at
    // server startup is free; boxing uniformly keeps the arms symmetric.
    match scheme {
        DatabaseScheme::Postgres => Box::pin(serve_postgres(config, schema, shutdown)).await,
        DatabaseScheme::MySql => Box::pin(serve_mysql(config, schema, shutdown)).await,
        DatabaseScheme::Sqlite => Box::pin(serve_sqlite(config, schema, shutdown)).await,
        DatabaseScheme::SqlServer => Box::pin(serve_sqlserver(config, schema, shutdown)).await,
    }
}

async fn serve_postgres(
    config: ServerConfig,
    schema: CompiledSchema,
    shutdown: ShutdownFuture,
) -> Result<()> {
    let adapter = Arc::new(
        PostgresAdapter::with_pool_config(
            &config.database_url,
            fraiseql_core::db::postgres::PoolPrewarmConfig {
                min_size:     config.pool_min_size,
                max_size:     config.pool_max_size,
                timeout_secs: Some(config.pool_timeout_secs),
            },
        )
        .await
        .context("Failed to connect to database")?,
    );
    let server: Server<CachedDatabaseAdapter<PostgresAdapter>> =
        Server::new(config, schema, adapter, None)
            .await
            .context("Failed to initialize server")?;
    finish_serve(server, shutdown).await
}

#[cfg(feature = "mysql")]
async fn serve_mysql(
    config: ServerConfig,
    schema: CompiledSchema,
    shutdown: ShutdownFuture,
) -> Result<()> {
    let adapter = Arc::new(
        fraiseql_core::db::mysql::MySqlAdapter::with_pool_config(
            &config.database_url,
            u32::try_from(config.pool_min_size).unwrap_or(u32::MAX),
            u32::try_from(config.pool_max_size).unwrap_or(u32::MAX),
        )
        .await
        .context("Failed to connect to MySQL")?,
    );
    let server: Server<CachedDatabaseAdapter<fraiseql_core::db::mysql::MySqlAdapter>> =
        Server::new(config, schema, adapter, None)
            .await
            .context("Failed to initialize server")?;
    finish_serve(server, shutdown).await
}

#[cfg(not(feature = "mysql"))]
async fn serve_mysql(_: ServerConfig, _: CompiledSchema, _: ShutdownFuture) -> Result<()> {
    anyhow::bail!(scheme_feature_off("mysql"))
}

#[cfg(feature = "sqlite")]
async fn serve_sqlite(
    config: ServerConfig,
    schema: CompiledSchema,
    shutdown: ShutdownFuture,
) -> Result<()> {
    fraiseql_server::url_guard::guard_sqlite_mutations(&schema)?;
    let adapter = Arc::new(
        fraiseql_core::db::sqlite::SqliteAdapter::with_pool_config(
            &config.database_url,
            u32::try_from(config.pool_min_size).unwrap_or(u32::MAX),
            u32::try_from(config.pool_max_size).unwrap_or(u32::MAX),
        )
        .await
        .context("Failed to connect to SQLite")?,
    );
    let server: Server<CachedDatabaseAdapter<fraiseql_core::db::sqlite::SqliteAdapter>> =
        Server::new(config, schema, adapter, None)
            .await
            .context("Failed to initialize server")?;
    finish_serve(server, shutdown).await
}

#[cfg(not(feature = "sqlite"))]
async fn serve_sqlite(_: ServerConfig, _: CompiledSchema, _: ShutdownFuture) -> Result<()> {
    anyhow::bail!(scheme_feature_off("sqlite"))
}

#[cfg(feature = "sqlserver")]
async fn serve_sqlserver(
    config: ServerConfig,
    schema: CompiledSchema,
    shutdown: ShutdownFuture,
) -> Result<()> {
    let adapter = Arc::new(
        fraiseql_core::db::sqlserver::SqlServerAdapter::with_pool_config(
            &config.database_url,
            u32::try_from(config.pool_min_size).unwrap_or(u32::MAX),
            u32::try_from(config.pool_max_size).unwrap_or(u32::MAX),
        )
        .await
        .context("Failed to connect to SQL Server")?,
    );
    let server: Server<CachedDatabaseAdapter<fraiseql_core::db::sqlserver::SqlServerAdapter>> =
        Server::new(config, schema, adapter, None)
            .await
            .context("Failed to initialize server")?;
    finish_serve(server, shutdown).await
}

#[cfg(not(feature = "sqlserver"))]
async fn serve_sqlserver(_: ServerConfig, _: CompiledSchema, _: ShutdownFuture) -> Result<()> {
    anyhow::bail!(scheme_feature_off("sqlserver"))
}

async fn finish_serve<X>(server: Server<X>, shutdown: ShutdownFuture) -> Result<()>
where
    X: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    if let Some(shutdown) = shutdown {
        server.serve_with_shutdown(shutdown).await.context("Server error")
    } else {
        server.serve().await.context("Server error")
    }
}

#[cfg(any(
    not(feature = "mysql"),
    not(feature = "sqlite"),
    not(feature = "sqlserver"),
))]
fn scheme_feature_off(feature: &str) -> String {
    format!(
        "fraiseql run: {feature}:// URL provided but the CLI was built without the `{feature}` \
         feature. Rebuild with `cargo install fraiseql --features {feature}` (or enable both \
         `run-server` and `{feature}` in your downstream crate)."
    )
}

/// Resolve all runtime configuration, applying the override precedence chain.
///
/// Priority (highest first):
/// 1. CLI flags (`db_cli`, `port_cli`, `bind_cli`)
/// 2. Standard environment variables (`DATABASE_URL`, `FRAISEQL_PORT`, `FRAISEQL_HOST`)
/// 3. `fraiseql.toml` `[server]` / `[database]` sections
/// 4. Built-in defaults
///
/// Returns `(db_url, bind_addr, server_runtime_config, database_runtime_config)`.
pub(crate) fn resolve_runtime_config(
    input_path: &Path,
    db_cli: Option<String>,
    port_cli: Option<u16>,
    bind_cli: Option<String>,
) -> Result<(String, SocketAddr, ServerRuntimeConfig, DatabaseRuntimeConfig)> {
    // 1. Load [server] and [database] from TOML (fatal for primary .toml, best-effort otherwise)
    let (server_cfg, db_cfg) = load_runtime_config_from_toml(input_path)?;

    // 2. Resolve database URL Priority: CLI flag > DATABASE_URL env > TOML [database].url
    let db_url = db_cli
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .or_else(|| db_cfg.url.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No database URL provided. Use --database, set DATABASE_URL env var, \
                 or set [database].url in fraiseql.toml."
            )
        })?;

    // 3. Resolve host and port Priority: CLI flags > FRAISEQL_HOST/FRAISEQL_PORT env > TOML
    //    [server].*
    let host = bind_cli
        .or_else(|| std::env::var("FRAISEQL_HOST").ok())
        .unwrap_or_else(|| server_cfg.host.clone());

    let port = port_cli
        .or_else(|| std::env::var("FRAISEQL_PORT").ok().and_then(|v| v.parse::<u16>().ok()))
        .unwrap_or(server_cfg.port);

    let bind_addr: SocketAddr = format!("{host}:{port}").parse().context("Invalid bind address")?;

    server_cfg.validate()?;
    db_cfg.validate()?;

    Ok((db_url, bind_addr, server_cfg, db_cfg))
}

/// Load `[server]` and `[database]` runtime config from the input file.
///
/// For `.toml` input files the sections are embedded directly.  For `.json`
/// input files we look for a sibling `fraiseql.toml` and load it as
/// `TomlProjectConfig`.  Falls back to defaults if no config is found.
fn load_runtime_config_from_toml(
    input_path: &Path,
) -> Result<(ServerRuntimeConfig, DatabaseRuntimeConfig)> {
    let ext = input_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext == "toml" {
        // Workflow A: the input IS the fraiseql.toml — parse errors are fatal
        let schema =
            TomlSchema::from_file(input_path.to_str().unwrap_or("")).with_context(|| {
                format!("Failed to load runtime config from {}", input_path.display())
            })?;
        info!("Loaded [server] and [database] config from {}", input_path.display());
        return Ok((schema.server, schema.database));
    }

    // Workflow B: input is schema.json — look for fraiseql.toml in the same directory
    let toml_path = input_path.parent().unwrap_or(Path::new(".")).join("fraiseql.toml");

    if toml_path.exists() {
        match TomlProjectConfig::from_file(toml_path.to_str().unwrap_or("fraiseql.toml")) {
            Ok(cfg) => {
                info!("Loaded [server] and [database] config from {}", toml_path.display());
                return Ok((cfg.server, cfg.database));
            },
            Err(e) => {
                info!("Could not parse TomlProjectConfig for runtime config: {e}");
            },
        }
    }

    Ok((ServerRuntimeConfig::default(), DatabaseRuntimeConfig::default()))
}

/// Build a `ServerConfig` from resolved runtime parameters.
///
/// Constructs the config from TOML-derived values and CLI overrides, then
/// applies any server-production overrides (metrics, admin, rate limiting)
/// from env vars via [`fraiseql_server::ServerArgs`].
pub(crate) fn build_config_from(
    db_url: &str,
    bind_addr: SocketAddr,
    server: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
    introspection: bool,
) -> ServerConfig {
    let tls = server.tls.enabled.then(|| build_tls_config(&server.tls));

    let mut config = ServerConfig {
        database_url: db_url.to_string(),
        bind_addr,
        cors_enabled: true,
        cors_origins: server.cors.origins.clone(),
        tls,
        pool_min_size: db_cfg.pool_min,
        pool_max_size: db_cfg.pool_max,
        pool_timeout_secs: db_cfg.connect_timeout_ms / 1000,
        introspection_enabled: introspection,
        // When introspection is requested via CLI flag, serve it without requiring auth
        // (development convenience; production setups use fraiseql-server directly).
        introspection_require_auth: false,
        ..ServerConfig::default()
    };

    // Apply any server-production env var overrides (metrics, admin, rate
    // limiting, etc.) via the shared ServerArgs struct.  This picks up env
    // vars like FRAISEQL_METRICS_ENABLED without duplicating the parsing
    // logic here.
    let server_args = fraiseql_server::ServerArgs::from_env();
    server_args.apply_to_config(&mut config);

    config
}

/// Convert `TlsRuntimeConfig` → `TlsServerConfig`.
fn build_tls_config(tls: &TlsRuntimeConfig) -> TlsServerConfig {
    TlsServerConfig {
        enabled:             true,
        cert_path:           tls.cert_file.clone().into(),
        key_path:            tls.key_file.clone().into(),
        min_version:         tls.min_version.clone(),
        require_client_cert: false,
        client_ca_path:      None,
    }
}

/// Compile the schema at `path`, printing progress to stdout.
async fn compile_schema(path: &Path) -> Result<fraiseql_core::schema::CompiledSchema> {
    let input = path.to_str().ok_or_else(|| anyhow::anyhow!("Input path is not valid UTF-8"))?;

    println!("Compiling schema...");

    let (schema, _report) = compile_to_schema(CompileOptions::new(input))
        .await
        .context("Schema compilation failed")?;

    println!(
        "   Schema compiled ({} types, {} queries, {} mutations)",
        schema.types.len(),
        schema.queries.len(),
        schema.mutations.len(),
    );
    println!();

    Ok(schema)
}

/// Spawn a file watcher that calls `on_change` once when a write event is detected.
///
/// Returns the watcher guard — drop it to stop watching.
fn spawn_file_watcher<F>(path: PathBuf, on_change: F) -> Result<RecommendedWatcher>
where
    F: FnOnce(Event) + Send + 'static,
{
    use std::sync::mpsc::channel;

    let (tx, rx) = channel::<Result<Event, notify::Error>>();

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        NotifyConfig::default().with_poll_interval(Duration::from_millis(500)),
    )
    .context("Failed to create file watcher")?;

    watcher
        .watch(&path, RecursiveMode::NonRecursive)
        .context("Failed to watch input file")?;

    // Drain events on a dedicated blocking thread — fires once then exits
    std::thread::spawn(move || {
        for event in rx.into_iter().flatten() {
            if matches!(event.kind, EventKind::Modify(_)) {
                info!("Schema file changed");
                on_change(event);
                break;
            }
        }
    });

    Ok(watcher)
}

/// Resolve the input file path from an optional argument or by auto-detection.
///
/// Priority: explicit arg -> `fraiseql.toml` -> `schema.json` -> error.
pub(crate) fn resolve_input(input: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = input {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        anyhow::bail!("Input file not found: {path}");
    }

    auto_detect_input(&std::env::current_dir().unwrap_or_default())
}

/// Search `base` for the first of `fraiseql.toml` / `schema.json`.
pub(crate) fn auto_detect_input(base: &Path) -> Result<PathBuf> {
    let candidates = ["fraiseql.toml", "schema.json"];
    for candidate in &candidates {
        let p = base.join(candidate);
        if p.exists() {
            info!("Auto-detected input file: {candidate}");
            return Ok(p);
        }
    }

    anyhow::bail!(
        "No input file found. Create a fraiseql.toml (or schema.json) in the current \
         directory, or pass an explicit path: fraiseql run <INPUT>"
    )
}
