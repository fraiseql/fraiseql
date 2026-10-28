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
//! 3. `fraiseql.toml` `[server]` / `[database]` sections, and the server configuration file
//!    `FRAISEQL_CONFIG` names, read as the `fraiseql-server` binary reads it. A setting may be made
//!    in either, never in both.
//! 4. Built-in defaults (`0.0.0.0:8080`, pool 2-20)

use std::{
    collections::BTreeSet,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::{
    Server, ServerConfig,
    schema::loader::FunctionsConfig,
    server_config::{DatabaseTlsConfig, TlsServerConfig},
    url_guard::{DatabaseScheme, parse_database_url},
};
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use tracing::info;

use super::compile::{CompileOptions, compile_to_schema};
use crate::config::{
    ConfigSource, DatabaseRuntimeConfig, ServerRuntimeConfig, TomlProjectConfig, TomlSchema,
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
    let server_file = ServerFile::from_env()?;

    let (db_url, bind_addr, server_cfg, db_cfg) =
        resolve_runtime_config(&input_path, database, port, bind, server_file.as_ref())?;

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
            server_file.as_ref(),
        ))
        .await
    } else {
        Box::pin(run_once(
            &input_path,
            &db_url,
            bind_addr,
            introspection,
            &server_cfg,
            &db_cfg,
            server_file.as_ref(),
        ))
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
    server_file: Option<&ServerFile>,
) -> Result<()> {
    let scheme = parse_database_url(db_url)?;
    let compiled = compile_schema(input_path).await?;
    let config =
        build_config_from(db_url, bind_addr, server_cfg, db_cfg, introspection, server_file)?;

    println!("Server ready at http://{bind_addr}/graphql");
    println!("   Press Ctrl+C to stop");
    println!();

    Box::pin(dispatch_serve(scheme, config, compiled, None)).await
}

/// Watch-loop: compile -> serve -> detect file change -> restart.
async fn run_watch_loop(
    input_path: &Path,
    db_url: &str,
    bind_addr: SocketAddr,
    introspection: bool,
    server_cfg: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
    server_file: Option<&ServerFile>,
) -> Result<()> {
    let scheme = parse_database_url(db_url)?;

    loop {
        let compiled = compile_schema(input_path).await?;
        let config =
            build_config_from(db_url, bind_addr, server_cfg, db_cfg, introspection, server_file)?;

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
            compiled,
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
    compiled: Compiled,
    shutdown: ShutdownFuture,
) -> Result<()> {
    // Box::pin: the adapter-init future exceeds clippy's `large_futures`
    // 16-KiB stack threshold. Heap-allocating once at server startup is free.
    match scheme {
        DatabaseScheme::Postgres => Box::pin(serve_postgres(config, compiled, shutdown)).await,
    }
}

async fn serve_postgres(
    config: ServerConfig,
    compiled: Compiled,
    shutdown: ShutdownFuture,
) -> Result<()> {
    let server = Box::pin(build_server(config, compiled.schema, compiled.functions)).await?;
    Box::pin(finish_serve(server, shutdown)).await
}

/// Build the server `fraiseql run` serves: the same assembly the `fraiseql-server` binary
/// runs, so `run` starts every capability its build compiled in.
pub(crate) async fn build_server(
    config: ServerConfig,
    schema: CompiledSchema,
    functions: Option<FunctionsConfig>,
) -> Result<Server> {
    // Box the assembly future: it exceeds clippy's `large_futures` stack threshold once
    // the platform features are compiled in (`--all-features`).
    Box::pin(fraiseql_server::boot::build_postgres_server(config, schema, functions))
        .await
        .context("Failed to initialize server")
}

async fn finish_serve(server: Server, shutdown: ShutdownFuture) -> Result<()> {
    if let Some(shutdown) = shutdown {
        server.serve_with_shutdown(shutdown).await.context("Server error")
    } else {
        // Boxed for the same reason as `fraiseql-server`'s own main: `Server` is large
        // and this future carries it, past clippy::large_futures' 16 KiB threshold as of
        // #1102. One allocation, once, on a future that runs for the process's life.
        Box::pin(server.serve()).await.context("Server error")
    }
}

/// Resolve all runtime configuration, applying the override precedence chain.
///
/// Priority (highest first):
/// 1. CLI flags (`db_cli`, `port_cli`, `bind_cli`)
/// 2. Standard environment variables (`DATABASE_URL`, `FRAISEQL_PORT`, `FRAISEQL_HOST`)
/// 3. `fraiseql.toml` `[server]` / `[database]` sections, or the server configuration file
///    (`server_file`); a setting made in both is refused
/// 4. Built-in defaults
///
/// Returns `(db_url, bind_addr, server_runtime_config, database_runtime_config)`.
pub(crate) fn resolve_runtime_config(
    input_path: &Path,
    db_cli: Option<String>,
    port_cli: Option<u16>,
    bind_cli: Option<String>,
    server_file: Option<&ServerFile>,
) -> Result<(String, SocketAddr, ServerRuntimeConfig, DatabaseRuntimeConfig)> {
    // 1. Load [server] and [database] from TOML (fatal for primary .toml, best-effort otherwise)
    let (server_cfg, db_cfg, project) = load_runtime_config_from_toml(input_path)?;
    if let Some(file) = server_file {
        file.refuse_settings_made_twice(&project)?;
    }
    let from_file = |key: &str| server_file.filter(|file| file.sets(key)).map(|file| &file.config);

    // 2. Resolve database URL Priority: CLI flag > DATABASE_URL env > TOML [database].url or the
    //    server file's `database_url`
    let db_url = db_cli
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .or_else(|| db_cfg.url.clone())
        .or_else(|| from_file("database_url").map(|config| config.database_url.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No database URL provided. Use --database, set DATABASE_URL env var, \
                 or set [database].url in fraiseql.toml."
            )
        })?;

    // 3. Resolve host and port Priority: CLI flags > FRAISEQL_HOST/FRAISEQL_PORT env > TOML
    //    [server].* or the server file's `bind_addr` > defaults
    let file_bind = from_file("bind_addr").map(|config| config.bind_addr);
    let host = bind_cli
        .or_else(|| std::env::var("FRAISEQL_HOST").ok())
        .or_else(|| file_bind.map(|addr| addr.ip().to_string()))
        .unwrap_or_else(|| server_cfg.host.clone());

    let port = port_cli
        .or_else(|| std::env::var("FRAISEQL_PORT").ok().and_then(|v| v.parse::<u16>().ok()))
        .or_else(|| file_bind.map(|addr| addr.port()))
        .unwrap_or(server_cfg.port);

    let bind_addr: SocketAddr = format!("{host}:{port}").parse().context("Invalid bind address")?;

    server_cfg.validate()?;
    db_cfg.validate()?;

    Ok((db_url, bind_addr, server_cfg, db_cfg))
}

/// Load `[server]` and `[database]` runtime config from the input file.
///
/// For `.toml` input files the sections are embedded directly. For `.json` input
/// files the project config is the one [`ConfigSource::resolve`] names — the same
/// file the compile step reads (#1387) — loaded as `TomlProjectConfig`. Defaults
/// apply only when no config applies; one that fails to parse is an error.
fn load_runtime_config_from_toml(
    input_path: &Path,
) -> Result<(ServerRuntimeConfig, DatabaseRuntimeConfig, BTreeSet<String>)> {
    let ext = input_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext == "toml" {
        // Workflow A: the input IS the fraiseql.toml — parse errors are fatal
        let schema =
            TomlSchema::from_file(input_path.to_str().unwrap_or("")).with_context(|| {
                format!("Failed to load runtime config from {}", input_path.display())
            })?;
        info!("Loaded [server] and [database] config from {}", input_path.display());
        return Ok((schema.server, schema.database, settings_made(input_path)?));
    }

    // Workflow B: input is schema.json — the config the compile step reads too.
    let source = ConfigSource::resolve(input_path, None)?;
    let Some(toml_path) = source.project_config() else {
        info!("Project config: {source}");
        return Ok((
            ServerRuntimeConfig::default(),
            DatabaseRuntimeConfig::default(),
            BTreeSet::new(),
        ));
    };
    let cfg = TomlProjectConfig::from_file(toml_path.to_str().unwrap_or_default())
        .with_context(|| format!("Failed to load runtime config from {}", toml_path.display()))?;
    info!("Loaded [server] and [database] config from {}", toml_path.display());
    Ok((cfg.server, cfg.database, settings_made(toml_path)?))
}

/// The `[server]` and `[database]` settings the project file makes, as `section.key`, and
/// `section.table.key` for a key in a nested table (`server.cors.origins`).
fn settings_made(toml_path: &Path) -> Result<BTreeSet<String>> {
    let text = std::fs::read_to_string(toml_path)
        .with_context(|| format!("Failed to read {}", toml_path.display()))?;
    let table: toml::Table = text
        .parse()
        .with_context(|| format!("Failed to parse {}", toml_path.display()))?;
    let mut made = BTreeSet::new();
    for section in ["server", "database"] {
        let Some(toml::Value::Table(keys)) = table.get(section) else {
            continue;
        };
        for (key, value) in keys {
            made.insert(format!("{section}.{key}"));
            if let toml::Value::Table(nested) = value {
                made.extend(nested.keys().map(|inner| format!("{section}.{key}.{inner}")));
            }
        }
    }
    Ok(made)
}

/// The server configuration file `FRAISEQL_CONFIG` names, loaded as the `fraiseql-server`
/// binary loads it (#1803): every section the server reads reaches `fraiseql run` too.
pub(crate) struct ServerFile {
    path:   String,
    /// The keys it sets at its top level.
    keys:   BTreeSet<String>,
    config: ServerConfig,
}

/// Each `fraiseql.toml` setting `fraiseql run` reads, and the server configuration key it
/// sets. A setting may be made in either file, never in both: neither could win silently.
const SETTINGS_IN_BOTH: &[(&str, &str)] = &[
    ("server.host", "bind_addr"),
    ("server.port", "bind_addr"),
    ("server.cors.origins", "cors_origins"),
    ("server.tls", "tls"),
    ("database.url", "database_url"),
    ("database.pool_min", "pool_min_size"),
    ("database.pool_max", "pool_max_size"),
    ("database.connect_timeout_ms", "pool_timeout_secs"),
    ("database.ssl_mode", "database_tls"),
];

impl ServerFile {
    /// The file `FRAISEQL_CONFIG` names, or `None` when it names none.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the server's loader refuses it.
    pub(crate) fn from_env() -> Result<Option<Self>> {
        std::env::var("FRAISEQL_CONFIG").ok().map(|path| Self::load(&path)).transpose()
    }

    /// Load `path` with the server's own loader, [`ServerConfig::from_file`].
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the server's loader refuses it.
    pub(crate) fn load(path: &str) -> Result<Self> {
        let config = ServerConfig::from_file(path)
            .map_err(|e| anyhow::anyhow!("failed to parse config file `{path}`: {e}"))?;
        let table: toml::Table = std::fs::read_to_string(path)?.parse()?;
        Ok(Self {
            path: path.to_owned(),
            keys: table.keys().cloned().collect(),
            config,
        })
    }

    fn sets(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    /// Refuse a setting the project file (`project`, from [`settings_made`]) and this file
    /// both make.
    fn refuse_settings_made_twice(&self, project: &BTreeSet<String>) -> Result<()> {
        for (setting, key) in SETTINGS_IN_BOTH {
            let made = project
                .iter()
                .any(|made| made == setting || made.starts_with(&format!("{setting}.")));
            if made && self.sets(key) {
                let (section, name) = setting.split_once('.').unwrap_or(("", setting));
                anyhow::bail!(
                    "`[{section}] {name}` in fraiseql.toml and `{key}` in {} (FRAISEQL_CONFIG) \
                     both set the same thing: set it in one of them",
                    self.path
                );
            }
        }
        Ok(())
    }
}

/// Build a `ServerConfig` from resolved runtime parameters.
///
/// The server configuration file (`server_file`) is the base, as the `fraiseql-server` binary
/// loads it. The TOML-derived values and CLI overrides apply over it, except where the file
/// sets the same key: there it holds, since [`resolve_runtime_config`] has already refused a
/// setting made in both files. The server-production env var overrides (metrics, admin, rate
/// limiting) apply last, via [`fraiseql_server::ServerArgs`].
pub(crate) fn build_config_from(
    db_url: &str,
    bind_addr: SocketAddr,
    server: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
    introspection: bool,
    server_file: Option<&ServerFile>,
) -> Result<ServerConfig> {
    let mut config = server_file.map_or_else(ServerConfig::default, |file| file.config.clone());
    let unset = |key: &str| !server_file.is_some_and(|file| file.sets(key));

    config.database_url = db_url.to_string();
    config.bind_addr = bind_addr;
    if unset("cors_enabled") {
        config.cors_enabled = true;
    }
    if unset("cors_origins") {
        config.cors_origins.clone_from(&server.cors.origins);
    }
    if unset("tls") {
        config.tls = server.tls.enabled.then(|| build_tls_config(&server.tls));
    }
    // `[database] ssl_mode` is carried as a `[database_tls]` section so both the
    // `fraiseql run` launcher and the `fraiseql-server` binary reach the pool
    // through one seam (`ServerConfig::postgres_tls`) rather than two that can
    // disagree — which is how one of them came to drop the setting entirely (#824).
    if unset("database_tls") {
        config.database_tls = Some(DatabaseTlsConfig {
            postgres_ssl_mode:   db_cfg.ssl_mode.clone(),
            verify_certificates: true,
            ca_bundle_path:      None,
            redis_ssl:           None,
            clickhouse_https:    None,
            elasticsearch_https: None,
        });
    }
    if unset("pool_min_size") {
        config.pool_min_size = db_cfg.pool_min;
    }
    if unset("pool_max_size") {
        config.pool_max_size = db_cfg.pool_max;
    }
    if unset("pool_timeout_secs") {
        config.pool_timeout_secs = db_cfg.pool_timeout_secs();
    }
    // `--introspection` serves it without requiring auth (development convenience;
    // production setups use fraiseql-server directly). Without the flag, the server file
    // decides, and `fraiseql run` keeps it off.
    if introspection {
        config.introspection_enabled = true;
        config.introspection_require_auth = false;
    } else {
        if unset("introspection_enabled") {
            config.introspection_enabled = false;
        }
        if unset("introspection_require_auth") {
            config.introspection_require_auth = false;
        }
    }

    // Apply any server-production env var overrides (metrics, admin, rate
    // limiting, etc.) via the shared ServerArgs struct.  This picks up env
    // vars like FRAISEQL_METRICS_ENABLED without duplicating the parsing
    // logic here.
    let server_args = fraiseql_server::ServerArgs::from_env()
        .map_err(|e| anyhow::anyhow!("invalid server environment variable: {e}"))?;
    server_args.apply_to_config(&mut config);

    Ok(config)
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

/// A compiled schema and the `functions` section the compiler emits beside it.
#[derive(Debug)]
pub(crate) struct Compiled {
    pub(crate) schema:    CompiledSchema,
    pub(crate) functions: Option<FunctionsConfig>,
}

/// Compile the schema at `path`, printing progress to stdout.
///
/// Refuses a section the linked server was built without before connecting to the
/// database: `fraiseql run` hands the compiled schema to the server in memory, past the
/// loader's boot refusal.
pub(crate) async fn compile_schema(path: &Path) -> Result<Compiled> {
    let input = path.to_str().ok_or_else(|| anyhow::anyhow!("Input path is not valid UTF-8"))?;

    println!("Compiling schema...");

    let (artifact, _report) = compile_to_schema(CompileOptions::new(input))
        .await
        .context("Schema compilation failed")?;
    fraiseql_server::boot::refuse_functions_this_build_cannot_run(artifact.functions.as_ref())?;
    let schema = artifact.schema;
    let raw = serde_json::to_value(&schema).context("Failed to serialize the compiled schema")?;
    fraiseql_server::schema::loader::refuse_unservable(&raw)?;

    println!(
        "   Schema compiled ({} types, {} queries, {} mutations)",
        schema.types.len(),
        schema.queries.len(),
        schema.mutations.len(),
    );
    println!();

    Ok(Compiled {
        schema,
        functions: artifact.functions,
    })
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
