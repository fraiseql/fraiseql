//! Generate external-write capture-trigger install DDL (#366).
//!
//! Reads a compiled schema and emits a self-contained, idempotent PostgreSQL
//! script that installs the shipped fallback-capture trigger function plus the
//! per-table statement-level triggers for every `@subscribable` type — so a raw
//! external write (psql / a migration / a third-party tool) to one of those
//! tables fans out to GraphQL subscribers, without double-emitting for writes
//! that already flow through FraiseQL's mutation executor.
//!
//! Pipe it straight into a database:
//!
//! ```bash
//! fraiseql generate-capture-triggers -s schema.compiled.json | psql "$DATABASE_URL"
//! ```

use std::fs;

use anyhow::{Context, Result};
use fraiseql_core::schema::{CompiledSchema, generate_capture_trigger_ddl};
use fraiseql_observers::migrations::entity_change_log_capture_trigger_sql;

use crate::output::OutputFormatter;

#[cfg(test)]
mod tests;

/// Configuration for the capture-trigger generator.
#[derive(Debug, Clone)]
pub struct GenerateCaptureTriggersConfig {
    /// Path to the compiled schema (`schema.compiled.json`).
    pub schema_path:      String,
    /// Output file path, or `None` to write the DDL to stdout (for piping).
    pub output:           Option<String>,
    /// Prepend the `core.fn_entity_change_log_capture()` function definition so
    /// the script is self-contained (default `true`).
    pub include_function: bool,
}

/// Build the install DDL for a compiled schema's `@subscribable` declarations.
///
/// Returns an empty string when nothing is subscribable. Otherwise the output is
/// the capture function (when `include_function`) followed by the per-table
/// `DROP TRIGGER IF EXISTS … ; CREATE TRIGGER …` statements — idempotent and safe
/// to re-run. The function and triggers both require the contract table
/// `core.tb_entity_change_log` (install it with the change-log contract migration
/// first).
#[must_use]
pub fn build_ddl(schema: &CompiledSchema, include_function: bool) -> String {
    let triggers = generate_capture_trigger_ddl(schema);
    if triggers.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    if include_function {
        out.push_str(entity_change_log_capture_trigger_sql());
        out.push('\n');
    }
    out.push_str(&triggers);
    out
}

/// Run the `generate-capture-triggers` command.
///
/// # Errors
///
/// Returns an error if the schema file cannot be read/parsed or the output file
/// cannot be written.
pub fn run(config: GenerateCaptureTriggersConfig, formatter: &OutputFormatter) -> Result<()> {
    let schema_json = fs::read_to_string(&config.schema_path)
        .with_context(|| format!("Failed to read schema '{}'", config.schema_path))?;
    let schema = CompiledSchema::from_json(&schema_json, false)
        .context("Failed to parse compiled schema")?;

    let ddl = build_ddl(&schema, config.include_function);

    if ddl.is_empty() {
        // Keep stdout empty (so a pipe into psql is a clean no-op) and explain on
        // the status channel.
        formatter.progress(
            "No @subscribable types in the schema — no capture triggers to generate. \
             Declare one with @fraiseql.type(subscribable_tables=[\"tb_post\"]).",
        );
        return Ok(());
    }

    let trigger_count = schema.subscribable.iter().map(|e| e.tables.len() * 3).sum::<usize>();

    match &config.output {
        Some(path) => {
            fs::write(path, &ddl)
                .with_context(|| format!("Failed to write output file '{path}'"))?;
            formatter.progress(&format!(
                "✓ Wrote {trigger_count} capture trigger(s) for {} subscribable type(s) to {path}",
                schema.subscribable.len()
            ));
        },
        // Stdout is the DDL only, so it can be piped straight into psql.
        None => print!("{ddl}"),
    }
    Ok(())
}
