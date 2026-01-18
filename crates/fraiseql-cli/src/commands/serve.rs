//! Development server command with hot-reload
//!
//! Watches schema.json for changes and auto-recompiles

use anyhow::{Context, Result};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tracing::{error, info};

/// Run the serve command (development server with hot-reload)
///
/// # Arguments
///
/// * `schema` - Path to schema.json file to watch
/// * `port` - Port to listen on (for future GraphQL server integration)
///
/// # Behavior
///
/// 1. Compiles the initial schema
/// 2. Watches schema.json for file changes
/// 3. Auto-recompiles on save
/// 4. Provides compilation feedback
///
/// # Future Enhancement
///
/// Will integrate with fraiseql-server to provide hot-reload of the GraphQL endpoint
pub async fn run(schema: &str, port: u16) -> Result<()> {
    info!("Starting development server");
    println!("🚀 FraiseQL Dev Server");
    println!("   Schema: {schema}");
    println!("   Port:   {port} (GraphQL server integration coming soon)");
    println!("   Watching for changes...\n");

    // Verify schema file exists
    let schema_path = Path::new(schema);
    if !schema_path.exists() {
        anyhow::bail!("Schema file not found: {schema}");
    }

    // Compile initial schema
    println!("📦 Initial compilation:");
    match compile_schema(schema).await {
        Ok(()) => println!("   ✓ Schema compiled successfully\n"),
        Err(e) => {
            error!("Initial compilation failed: {e}");
            println!("   ❌ Compilation failed: {e}\n");
            println!("   Fix errors and save to retry...\n");
        },
    }

    // Set up file watcher
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )
    .context("Failed to create file watcher")?;

    // Watch the schema file
    watcher
        .watch(schema_path, RecursiveMode::NonRecursive)
        .context("Failed to watch schema file")?;

    // Watch for file changes
    loop {
        match rx.recv() {
            Ok(event) => {
                // Only recompile on write/modify events
                if matches!(event.kind, EventKind::Modify(_)) {
                    info!("Schema file modified, recompiling...");
                    println!("🔄 Schema changed, recompiling...");

                    // Small delay to ensure file write is complete
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    match compile_schema(schema).await {
                        Ok(()) => {
                            info!("Recompilation successful");
                            println!("   ✓ Recompiled successfully\n");
                        },
                        Err(e) => {
                            error!("Recompilation failed: {e}");
                            println!("   ❌ Compilation failed: {e}\n");
                        },
                    }
                }
            },
            Err(e) => {
                error!("Watch error: {e}");
                anyhow::bail!("File watch error: {e}");
            },
        }
    }
}

/// Compile schema (used by file watcher)
async fn compile_schema(input: &str) -> Result<()> {
    let output = input.replace(".json", ".compiled.json");

    // Use the compile command logic (no database validation for dev server)
    super::compile::run(input, &output, false, None).await
}
