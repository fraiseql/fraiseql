//! `fraiseql watch` — recompile on schema-source change and live-reload a server.
//!
//! `fraiseql run --watch` already recompiles and restarts the server in-process on
//! every save. `watch` instead targets a *separately running* server: it recompiles
//! `schema.compiled.json` to disk and, when `--reload-url` is given, POSTs to the
//! server's `POST /api/v1/admin/reload-schema` admin endpoint, which swaps the
//! executor via `ArcSwap` — a zero-downtime reload (in-flight queries finish on the
//! old schema) with no process restart.
//!
//! ```text
//! $ fraiseql watch schema.json --reload-url http://localhost:8080 --admin-token $TOKEN
//! [watch] compiled schema.compiled.json
//! [watch] reloaded http://localhost:8080 (zero-downtime)
//! [watch] watching schema.json — press Ctrl+C to stop
//! ```

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::UnboundedReceiver;

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;

/// How long to wait for a quiet window before recompiling, collapsing the
/// double-write many editors emit on an atomic save into a single compile.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Build the admin reload endpoint URL from a server base URL, tolerating a
/// trailing slash on the base.
fn reload_endpoint(reload_url: &str) -> String {
    format!("{}/api/v1/admin/reload-schema", reload_url.trim_end_matches('/'))
}

/// Run the `fraiseql watch` command.
///
/// Compiles once up front (and reloads, if a server URL is given), then watches
/// `input` and repeats on every change until interrupted. Compile and reload
/// failures are reported but never stop the watch loop — the developer fixes the
/// source and saves again.
///
/// # Arguments
///
/// * `input`       - Schema source (`fraiseql.toml` or `schema.json`).
/// * `output`      - Compiled-schema output path (the server reads this on reload).
/// * `reload_url`  - Base URL of a running server; `None` skips the live reload.
/// * `admin_token` - Bearer token for the admin reload endpoint.
/// * `database`    - Optional database URL for compile-time validation.
///
/// # Errors
///
/// Returns an error only for unrecoverable setup failures (the file watcher cannot
/// be created or the input path cannot be watched). Per-change compile/reload
/// errors are logged and the loop continues.
pub async fn run(
    input: &str,
    output: &str,
    reload_url: Option<&str>,
    admin_token: Option<&str>,
    database: Option<&str>,
) -> Result<()> {
    // Compile + reload once so the loop starts from a known-good state.
    compile_once(input, output, database).await;
    if let Some(url) = reload_url {
        reload_server(url, output, admin_token).await;
    }

    // The notify callback runs on a watcher thread; forward each relevant event to
    // the async loop over an unbounded channel (a non-blocking send from sync code).
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    let _ = tx.send(());
                }
            }
        },
        NotifyConfig::default().with_poll_interval(Duration::from_millis(500)),
    )
    .context("Failed to create file watcher")?;
    watcher
        .watch(&PathBuf::from(input), RecursiveMode::NonRecursive)
        .with_context(|| format!("Failed to watch input file `{input}`"))?;

    println!("[watch] watching {input} — press Ctrl+C to stop");

    loop {
        tokio::select! {
            received = rx.recv() => {
                if received.is_none() {
                    break; // watcher dropped
                }
                debounce(&mut rx).await;
                compile_once(input, output, database).await;
                if let Some(url) = reload_url {
                    reload_server(url, output, admin_token).await;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n[watch] stopped");
                break;
            }
        }
    }

    Ok(())
}

/// Wait until `DEBOUNCE` elapses with no further change event, draining the burst.
async fn debounce(rx: &mut UnboundedReceiver<()>) {
    loop {
        tokio::select! {
            () = tokio::time::sleep(DEBOUNCE) => break,
            received = rx.recv() => {
                if received.is_none() {
                    break;
                }
            }
        }
    }
}

/// Compile `input` to `output`. Errors are reported but not propagated so a typo
/// in the schema source does not kill the watch loop.
async fn compile_once(input: &str, output: &str, database: Option<&str>) {
    match super::compile::run(
        input,
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        output,
        false, // check: false → write the output
        database,
        None,  // emit_ddl
        false, // check_migrations
        false, // skip_hash
    )
    .await
    {
        Ok(()) => println!("[watch] compiled {output}"),
        Err(e) => eprintln!("[watch] compile failed: {e:#}"),
    }
}

/// POST the compiled-schema path to the server's admin reload endpoint for a
/// zero-downtime `ArcSwap` reload. Failures are reported, not propagated.
async fn reload_server(reload_url: &str, schema_path: &str, admin_token: Option<&str>) {
    let url = reload_endpoint(reload_url);

    let client = match reqwest::Client::builder().timeout(Duration::from_secs(10)).build() {
        Ok(client) => client,
        Err(e) => {
            eprintln!("[watch] reload client error: {e}");
            return;
        },
    };

    let mut req = client.post(&url).json(&serde_json::json!({
        "schema_path": schema_path,
        "validate_only": false,
    }));
    if let Some(tok) = admin_token {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            println!("[watch] reloaded {reload_url} (zero-downtime)");
        },
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("[watch] reload failed: HTTP {status} {body}");
        },
        Err(e) => eprintln!("[watch] reload request failed: {e}"),
    }
}
