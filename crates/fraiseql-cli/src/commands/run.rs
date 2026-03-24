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
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_server::{Server, ServerConfig, server_config::TlsServerConfig};
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
    read_only: bool,
) -> Result<()> {
    // CLI flag or FRAISEQL_READ_ONLY env var
    let read_only =
        read_only || std::env::var("FRAISEQL_READ_ONLY").is_ok_and(|v| v == "true" || v == "1");

    let input_path = resolve_input(input)?;

    let (db_url, bind_addr, server_cfg, db_cfg) =
        resolve_runtime_config(&input_path, database, port, bind)?;

    println!("FraiseQL");
    println!("   Schema: {}", input_path.display());
    println!("   Server: http://{bind_addr}/graphql");
    if read_only {
        println!("   Mode:   read-only (mutations disabled)");
    }
    println!();

    if watch {
        run_watch_loop(&input_path, &db_url, bind_addr, introspection, read_only, &server_cfg, &db_cfg)
            .await
    } else {
        run_once(&input_path, &db_url, bind_addr, introspection, read_only, &server_cfg, &db_cfg)
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
    read_only: bool,
    server_cfg: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
) -> Result<()> {
    let schema = compile_schema(input_path).await?;
    let mut config = build_config_from(db_url, bind_addr, server_cfg, db_cfg, introspection);
    config.read_only = read_only;

    let adapter = Arc::new(
        PostgresAdapter::with_pool_config(db_url, config.pool_min_size, config.pool_max_size)
            .await
            .context("Failed to connect to database")?,
    );

    println!("Server ready at http://{bind_addr}/graphql");
    println!("   Press Ctrl+C to stop");
    println!();

    let server: Server<PostgresAdapter> = Server::new(config, schema, adapter, None)
        .await
        .context("Failed to initialize server")?;

    server.serve().await.context("Server error")
}

/// Watch-loop: compile -> serve -> detect file change -> restart.
async fn run_watch_loop(
    input_path: &Path,
    db_url: &str,
    bind_addr: SocketAddr,
    introspection: bool,
    read_only: bool,
    server_cfg: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
) -> Result<()> {
    loop {
        let schema = compile_schema(input_path).await?;
        let mut config = build_config_from(db_url, bind_addr, server_cfg, db_cfg, introspection);
        config.read_only = read_only;

        let adapter = Arc::new(
            PostgresAdapter::with_pool_config(db_url, config.pool_min_size, config.pool_max_size)
                .await
                .context("Failed to connect to database")?,
        );

        println!("Server ready at http://{bind_addr}/graphql");
        println!("   Watching {} for changes...  (Ctrl+C to stop)", input_path.display());
        println!();

        let server: Server<PostgresAdapter> = Server::new(config, schema, adapter, None)
            .await
            .context("Failed to initialize server")?;

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

        server
            .serve_with_shutdown(async move {
                tokio::select! {
                    () = Server::<PostgresAdapter>::shutdown_signal() => {},
                    result = change_rx => {
                        if result.is_err() {
                            // Sender dropped without sending — treat as OS signal
                        }
                    },
                }
            })
            .await
            .context("Server error")?;

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
fn build_config_from(
    db_url: &str,
    bind_addr: SocketAddr,
    server: &ServerRuntimeConfig,
    db_cfg: &DatabaseRuntimeConfig,
    introspection: bool,
) -> ServerConfig {
    let tls = server.tls.enabled.then(|| build_tls_config(&server.tls));

    ServerConfig {
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
    }
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
fn resolve_input(input: Option<&str>) -> Result<PathBuf> {
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
fn auto_detect_input(base: &Path) -> Result<PathBuf> {
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tempfile::TempDir;

    use super::*;

    // ── resolve_input: explicit path ─────────────────────────────────────────

    #[test]
    fn test_resolve_input_explicit_existing_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("fraiseql.toml");
        std::fs::write(&file, "").unwrap();

        let result = resolve_input(Some(file.to_str().unwrap()));
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert_eq!(result.unwrap(), file);
    }

    #[test]
    fn test_resolve_input_explicit_missing_returns_helpful_error() {
        let result = resolve_input(Some("/nonexistent/path/schema.json"));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not found"), "expected 'not found' in: {msg}");
        assert!(msg.contains("/nonexistent/path/schema.json"), "expected path in: {msg}");
    }

    // ── auto_detect_input ────────────────────────────────────────────────────

    #[test]
    fn test_auto_detect_prefers_toml_over_json() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("fraiseql.toml"), "").unwrap();
        std::fs::write(dir.path().join("schema.json"), "{}").unwrap();

        let result = auto_detect_input(dir.path()).unwrap();
        assert_eq!(result, dir.path().join("fraiseql.toml"));
    }

    #[test]
    fn test_auto_detect_falls_back_to_schema_json() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("schema.json"), "{}").unwrap();

        let result = auto_detect_input(dir.path()).unwrap();
        assert_eq!(result, dir.path().join("schema.json"));
    }

    #[test]
    fn test_auto_detect_no_files_returns_helpful_error() {
        let dir = TempDir::new().unwrap();

        let result = auto_detect_input(dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("No input file found"), "expected hint in: {msg}");
        assert!(msg.contains("fraiseql run <INPUT>"), "expected usage in: {msg}");
    }

    // ── build_config_from ─────────────────────────────────────────────────────

    #[test]
    fn test_build_config_sets_db_url() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &DatabaseRuntimeConfig::default(),
            false,
        );
        assert_eq!(config.database_url, "postgres://localhost/test");
    }

    #[test]
    fn test_build_config_sets_bind_addr() {
        let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &DatabaseRuntimeConfig::default(),
            false,
        );
        assert_eq!(config.bind_addr, addr);
    }

    #[test]
    fn test_build_config_introspection_enabled() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &DatabaseRuntimeConfig::default(),
            true,
        );
        assert!(config.introspection_enabled);
        // Must not require auth — this is a dev-convenience flag
        assert!(!config.introspection_require_auth);
    }

    #[test]
    fn test_build_config_introspection_disabled() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &DatabaseRuntimeConfig::default(),
            false,
        );
        assert!(!config.introspection_enabled);
    }

    #[test]
    fn test_build_config_pool_sizes_from_db_cfg() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let db_cfg = DatabaseRuntimeConfig {
            pool_min: 5,
            pool_max: 50,
            ..Default::default()
        };
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &db_cfg,
            false,
        );
        assert_eq!(config.pool_min_size, 5);
        assert_eq!(config.pool_max_size, 50);
    }

    #[test]
    fn test_build_config_cors_origins_from_server_cfg() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let server_cfg = ServerRuntimeConfig {
            cors: crate::config::runtime::CorsRuntimeConfig {
                origins:     vec!["https://example.com".to_string()],
                credentials: false,
            },
            ..Default::default()
        };
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &server_cfg,
            &DatabaseRuntimeConfig::default(),
            false,
        );
        assert_eq!(config.cors_origins, ["https://example.com"]);
    }

    // ── resolve_runtime_config ───────────────────────────────────────────────

    #[test]
    fn test_resolve_runtime_config_database_url_from_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://toml-host/testdb"
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let (db_url, _addr, _srv, _db) =
                resolve_runtime_config(&toml_path, None, None, None).unwrap();
            assert_eq!(db_url, "postgresql://toml-host/testdb");
        });
    }

    #[test]
    fn test_resolve_runtime_config_cli_db_overrides_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://toml-host/testdb"
"#,
        )
        .unwrap();

        let (db_url, _addr, _srv, _db) = resolve_runtime_config(
            &toml_path,
            Some("postgresql://cli-host/clidb".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(db_url, "postgresql://cli-host/clidb");
    }

    #[test]
    fn test_resolve_runtime_config_env_var_overrides_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://toml-host/testdb"
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", Some("postgresql://env-host/envdb"))], || {
            let (db_url, _addr, _srv, _db) =
                resolve_runtime_config(&toml_path, None, None, None).unwrap();
            assert_eq!(db_url, "postgresql://env-host/envdb");
        });
    }

    #[test]
    fn test_resolve_runtime_config_toml_port_used_when_cli_absent() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"

[server]
host = "127.0.0.1"
port = 9999
"#,
        )
        .unwrap();

        temp_env::with_vars(
            [
                ("DATABASE_URL", None::<&str>),
                ("FRAISEQL_HOST", None::<&str>),
                ("FRAISEQL_PORT", None::<&str>),
            ],
            || {
                let (_db_url, addr, _srv, _db) =
                    resolve_runtime_config(&toml_path, None, None, None).unwrap();
                assert_eq!(addr.port(), 9999);
                assert_eq!(addr.ip().to_string(), "127.0.0.1");
            },
        );
    }

    #[test]
    fn test_resolve_runtime_config_cli_port_overrides_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"

[server]
port = 9999
"#,
        )
        .unwrap();

        temp_env::with_vars(
            [
                ("DATABASE_URL", None::<&str>),
                ("FRAISEQL_PORT", None::<&str>),
            ],
            || {
                let (_db_url, addr, _srv, _db) =
                    resolve_runtime_config(&toml_path, None, Some(7777), None).unwrap();
                assert_eq!(addr.port(), 7777);
            },
        );
    }

    #[test]
    fn test_resolve_runtime_config_invalid_primary_toml_is_fatal() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(&toml_path, "this is [not valid toml !!!").unwrap();

        let result = resolve_runtime_config(&toml_path, None, None, None);
        assert!(result.is_err(), "invalid primary TOML must be fatal");
    }

    #[test]
    fn test_resolve_runtime_config_port_zero_rejected() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"

[server]
port = 0
"#,
        )
        .unwrap();

        temp_env::with_vars(
            [
                ("DATABASE_URL", None::<&str>),
                ("FRAISEQL_PORT", None::<&str>),
            ],
            || {
                let result = resolve_runtime_config(&toml_path, None, None, None);
                assert!(result.is_err());
                let msg = result.unwrap_err().to_string();
                assert!(msg.contains("port"), "got: {msg}");
            },
        );
    }

    #[test]
    fn test_resolve_runtime_config_pool_range_rejected() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"
pool_min = 50
pool_max = 10
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let result = resolve_runtime_config(&toml_path, None, None, None);
            assert!(result.is_err());
            let msg = result.unwrap_err().to_string();
            assert!(msg.contains("pool_min"), "got: {msg}");
        });
    }

    #[test]
    fn test_resolve_runtime_config_no_db_url_returns_error() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let result = resolve_runtime_config(&toml_path, None, None, None);
            assert!(result.is_err());
            let msg = result.unwrap_err().to_string();
            assert!(msg.contains("database URL"), "got: {msg}");
        });
    }
}
