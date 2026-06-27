//! `fraiseql query` — execute one GraphQL operation in-process and print JSON.
//!
//! Boots the compiled schema directly on top of `fraiseql-core`'s `Executor`
//! (no long-lived server, no HTTP layer, no axum) against PostgreSQL, runs a
//! single query or mutation, prints the GraphQL JSON response to stdout, and
//! exits. A scriptable "does this actually resolve?" check that closes the gap
//! between `compile`/`validate` (static) and the full server (runtime).
//!
//! Exit code: non-zero on a resolution error — whether `execute` returns an
//! outright error or the response carries a non-empty top-level `errors` array.
//!
//! Mutations COMMIT unless `--dry-run` is given, in which case the mutation runs
//! inside a transaction the adapter rolls back (validate-bind-without-commit,
//! via [`RuntimeConfig::dry_run_mutations`]).

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, bail};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    runtime::{Executor, RuntimeConfig},
    schema::CompiledSchema,
};

/// Run the `fraiseql query` command.
///
/// # Arguments
///
/// * `query`       - The GraphQL operation to execute (a single query or mutation).
/// * `schema_path` - Path to `schema.compiled.json`.
/// * `database`    - Database URL override; falls back to the `DATABASE_URL` env var.
/// * `variables`   - Optional GraphQL variables as a JSON object string.
/// * `dry_run`     - Run mutations inside a rolled-back transaction (no commit).
///
/// # Errors
///
/// Returns an error if the schema file is missing or invalid, no PostgreSQL URL
/// is available, the URL is not a `postgres://` scheme, the `--variables` JSON is
/// malformed, the database connection fails, or the operation fails to execute.
pub async fn run(
    query: &str,
    schema_path: &Path,
    database: Option<String>,
    variables: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let schema = load_schema(schema_path)?;
    let db_url = resolve_db_url(database)?;
    ensure_postgres_url(&db_url)?;
    let variables = parse_variables(variables.as_deref())?;

    // Safety notice: a mutation without --dry-run commits against the live DB.
    if !dry_run && is_mutation(query) {
        eprintln!(
            "warning: executing a mutation — changes will be COMMITTED. \
             Pass --dry-run to validate the binding without committing."
        );
    }

    let adapter =
        Arc::new(PostgresAdapter::new(&db_url).await.context("failed to connect to PostgreSQL")?);
    let config = RuntimeConfig {
        dry_run_mutations: dry_run,
        ..RuntimeConfig::default()
    };
    let executor = Executor::with_config(schema, adapter, config);

    let result = executor.execute(query, variables.as_ref()).await.context("execution failed")?;

    println!(
        "{}",
        serde_json::to_string_pretty(&result).context("failed to serialize result")?
    );

    // A resolution error surfaced in-band (top-level `errors`) must fail the
    // process even though `execute` returned the response envelope as `Ok`.
    if has_errors(&result) {
        std::process::exit(1);
    }
    Ok(())
}

/// Read and parse the compiled schema file.
fn load_schema(path: &Path) -> Result<CompiledSchema> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read schema file {}", path.display()))?;
    // `strict_integrity = false`: tolerate a missing/legacy `_content_hash` so the
    // command runs against schemas compiled by any toolchain version.
    CompiledSchema::from_json(&text, false)
        .with_context(|| format!("failed to parse compiled schema {}", path.display()))
}

/// Resolve the database URL: CLI flag first, then the `DATABASE_URL` env var.
fn resolve_db_url(cli: Option<String>) -> Result<String> {
    cli.or_else(|| std::env::var("DATABASE_URL").ok()).ok_or_else(|| {
        anyhow::anyhow!(
            "No database URL provided. Pass --database or set the DATABASE_URL env var."
        )
    })
}

/// Guard: `fraiseql query` supports PostgreSQL only (slice 1).
pub(crate) fn ensure_postgres_url(url: &str) -> Result<()> {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        Ok(())
    } else {
        bail!(
            "`fraiseql query` currently supports PostgreSQL only. \
             Pass a postgres:// (or postgresql://) connection string."
        )
    }
}

/// Parse the optional `--variables` JSON string into a value.
///
/// GraphQL variables are an object; a non-object value is rejected up front so
/// the failure is a clear CLI error rather than an opaque execution error.
pub(crate) fn parse_variables(raw: Option<&str>) -> Result<Option<serde_json::Value>> {
    match raw {
        None => Ok(None),
        Some(s) => {
            let value: serde_json::Value =
                serde_json::from_str(s).context("--variables is not valid JSON")?;
            if !value.is_object() {
                bail!("--variables must be a JSON object (e.g. '{{\"id\": 42}}').");
            }
            Ok(Some(value))
        },
    }
}

/// Whether a GraphQL response carries a non-empty top-level `errors` array.
pub(crate) fn has_errors(result: &serde_json::Value) -> bool {
    result
        .get("errors")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|a| !a.is_empty())
}

/// Whether the operation parses as a mutation (best-effort; false on parse error).
fn is_mutation(query: &str) -> bool {
    fraiseql_core::graphql::parse_query(query).is_ok_and(|p| p.operation_type == "mutation")
}
