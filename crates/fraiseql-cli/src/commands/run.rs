//! `fraiseql run` — compile schema in-memory and serve the GraphQL API.
//!
//! This command compiles the schema without writing any artifacts to disk and
//! immediately starts the HTTP server.  With `--watch`, the schema file is
//! monitored for changes and the server is hot-reloaded on every save.

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
use fraiseql_server::{Server, ServerConfig};
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use tracing::info;

use super::compile::{CompileOptions, compile_to_schema};

/// Run the `fraiseql run` command.
///
/// # Arguments
///
/// * `input`         - Path to input file; `None` triggers auto-detection.
/// * `database`      - Database URL; falls back to `DATABASE_URL` env var.
/// * `port`          - TCP port to listen on.
/// * `bind`          - Bind address (e.g. `"0.0.0.0"`).
/// * `watch`         - Watch input file for changes and hot-reload.
/// * `introspection` - Enable the `/introspection` endpoint (no auth).
///
/// # Errors
///
/// Returns error if the input file cannot be found, the schema fails to compile,
/// the database URL is missing, or the server cannot bind to the requested address.
pub async fn run(
    input: Option<&str>,
    database: Option<String>,
    port: u16,
    bind: String,
    watch: bool,
    introspection: bool,
) -> Result<()> {
    let input_path = resolve_input(input)?;

    let db_url = database.or_else(|| std::env::var("DATABASE_URL").ok()).ok_or_else(|| {
        anyhow::anyhow!("No database URL provided. Use --database or set DATABASE_URL env var.")
    })?;

    let bind_addr: SocketAddr = format!("{bind}:{port}").parse().context("Invalid bind address")?;

    println!("FraiseQL");
    println!("   Schema: {}", input_path.display());
    println!("   Server: http://{bind_addr}/graphql");
    println!();

    if watch {
        run_watch_loop(&input_path, &db_url, bind_addr, introspection).await
    } else {
        run_once(&input_path, &db_url, bind_addr, introspection).await
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Compile the schema and start the server once (no watch).
async fn run_once(
    input_path: &Path,
    db_url: &str,
    bind_addr: SocketAddr,
    introspection: bool,
) -> Result<()> {
    let schema = compile_schema(input_path).await?;
    let config = build_config(db_url, bind_addr, introspection);

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
) -> Result<()> {
    loop {
        let schema = compile_schema(input_path).await?;
        let config = build_config(db_url, bind_addr, introspection);

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

/// Build a `ServerConfig` for the `run` command.
fn build_config(db_url: &str, bind_addr: SocketAddr, introspection: bool) -> ServerConfig {
    ServerConfig {
        database_url: db_url.to_string(),
        bind_addr,
        introspection_enabled: introspection,
        // When introspection is requested via CLI flag, serve it without requiring auth
        // (development convenience; production setups use fraiseql-server directly).
        introspection_require_auth: false,
        ..ServerConfig::default()
    }
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

    // ── build_config ─────────────────────────────────────────────────────────

    #[test]
    fn test_build_config_sets_db_url() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let config = build_config("postgres://localhost/test", addr, false);
        assert_eq!(config.database_url, "postgres://localhost/test");
    }

    #[test]
    fn test_build_config_sets_bind_addr() {
        let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let config = build_config("postgres://localhost/test", addr, false);
        assert_eq!(config.bind_addr, addr);
    }

    #[test]
    fn test_build_config_introspection_enabled() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let config = build_config("postgres://localhost/test", addr, true);
        assert!(config.introspection_enabled);
        // Must not require auth — this is a dev-convenience flag
        assert!(!config.introspection_require_auth);
    }

    #[test]
    fn test_build_config_introspection_disabled() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let config = build_config("postgres://localhost/test", addr, false);
        assert!(!config.introspection_enabled);
    }
}
