//! `fraiseql sources` — a read-only status view over scheduled ingress sources
//! (#573).
//!
//! Lists each compiled source with its schedule, `run_as` authority ceiling, and —
//! when a database is reachable — its durable cursor: the value, the compare-and-swap
//! version, and how stale the watermark is (seconds since the last advance). It is
//! the operator's answer to "read its cursor / measure lag"; the fire/skip/error
//! signals live in the `fraiseql_source_*` Prometheus metrics and the poller's
//! structured logs, since those are per-firing and per-replica, whereas the cursor is
//! the durable, cross-replica source of truth for progress.
//!
//! Leadership and last-fire are deliberately **not** reported here: a PostgreSQL
//! advisory lock exposes no holder, the single-firing lease is released between
//! ticks, and "last fired on this replica" is not "last fired" in a multi-replica
//! deployment — so the honest, durable signal is the cursor, and single-firing health
//! is the fleet-wide `fraiseql_source_skips_not_leader_total` metric.
//!
//! Architecture mirrors `perf`: a thin PostgreSQL [`reader`] plus pure, unit-tested
//! merge/format functions, so correctness is covered without a live database.

pub mod reader;

#[cfg(test)]
mod tests;

use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result};
use fraiseql_core::schema::{CompiledSchema, SourceDefinition};
use serde::Serialize;

use self::reader::{CursorRow, SourceCursorReader};
use crate::commands::migrate::resolve_database_url;

/// Arguments for `fraiseql sources`.
pub struct SourcesArgs {
    /// Path to the compiled schema (the source definitions).
    pub schema:   PathBuf,
    /// Explicit PostgreSQL URL for cursor reads; falls back to `fraiseql.toml` /
    /// `DATABASE_URL`. When no URL resolves, definitions are listed without cursor
    /// state.
    pub database: Option<String>,
    /// Emit machine-readable JSON instead of the human report.
    pub json:     bool,
}

/// The `run_as` authority ceiling of a source, flattened for display.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RunAsStatus {
    /// Whether a `run_as` ceiling is configured at all. `false` ⇒ the source is
    /// fail-closed: its background mutations are denied until a ceiling is granted.
    pub configured: bool,
    /// The granted roles (RBAC ceiling).
    pub roles:      Vec<String>,
    /// The granted scopes.
    pub scopes:     Vec<String>,
    /// The pinned tenant, if any. `None` ⇒ global/system or a per-message
    /// multi-tenant source.
    pub tenant:     Option<String>,
}

/// The durable cursor state of a source, if known.
#[derive(Debug, Serialize, PartialEq)]
pub struct CursorStatus {
    /// `"unknown"` (no database read), `"never_advanced"` (no row yet), or
    /// `"advanced"` (a watermark exists).
    pub state:       &'static str,
    /// The compare-and-swap generation counter (present only when advanced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version:     Option<i64>,
    /// The opaque cursor value (present only when advanced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value:       Option<serde_json::Value>,
    /// Last-advance timestamp (present only when advanced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at:  Option<String>,
    /// Seconds since the last advance — the staleness/lag (present only when
    /// advanced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<f64>,
}

impl CursorStatus {
    /// No database was reachable, so the cursor state is unknown.
    const fn unknown() -> Self {
        Self {
            state:       "unknown",
            version:     None,
            value:       None,
            updated_at:  None,
            age_seconds: None,
        }
    }

    /// The database was read but the source has no cursor row yet.
    const fn never_advanced() -> Self {
        Self {
            state:       "never_advanced",
            version:     None,
            value:       None,
            updated_at:  None,
            age_seconds: None,
        }
    }

    /// A durable watermark exists.
    fn advanced(row: &CursorRow) -> Self {
        Self {
            state:       "advanced",
            version:     Some(row.version),
            value:       row.value.clone(),
            updated_at:  Some(row.updated_at.clone()),
            age_seconds: Some(row.age_seconds),
        }
    }
}

/// One source's full status: its static definition plus its durable cursor.
#[derive(Debug, Serialize)]
pub struct SourceStatus {
    /// The source name (also the cursor key the runtime advances under).
    pub name:        String,
    /// The cron schedule the source fires on.
    pub schedule:    String,
    /// The connector function the source runs.
    pub function:    String,
    /// Whether the source is enabled (a disabled source is compiled but not
    /// scheduled).
    pub enabled:     bool,
    /// The declared cursor name. Equal to `name` unless an explicit `cursor` was set.
    pub cursor_name: String,
    /// The `run_as` authority ceiling.
    pub run_as:      RunAsStatus,
    /// The durable cursor state.
    pub cursor:      CursorStatus,
}

/// Merge the compiled source definitions with the cursor rows read from the
/// database into a per-source status list.
///
/// `cursors` is `None` when no database was reachable (every cursor is `unknown`),
/// or `Some(map)` keyed by the source name the runtime advances under — so a source
/// with no row reads as `never_advanced` and one with a row as `advanced`.
fn build_status(
    sources: &[SourceDefinition],
    cursors: Option<&HashMap<String, CursorRow>>,
) -> Vec<SourceStatus> {
    sources
        .iter()
        .map(|source| {
            let cursor = match cursors {
                None => CursorStatus::unknown(),
                Some(map) => match map.get(&source.name) {
                    Some(row) => CursorStatus::advanced(row),
                    None => CursorStatus::never_advanced(),
                },
            };
            SourceStatus {
                name: source.name.clone(),
                schedule: source.schedule.clone(),
                function: source.function.clone(),
                enabled: source.enabled,
                cursor_name: source.cursor_name().to_string(),
                run_as: run_as_status(source),
                cursor,
            }
        })
        .collect()
}

/// Flatten a source's optional `run_as` into a display-ready [`RunAsStatus`].
fn run_as_status(source: &SourceDefinition) -> RunAsStatus {
    match &source.run_as {
        None => RunAsStatus {
            configured: false,
            roles:      Vec::new(),
            scopes:     Vec::new(),
            tenant:     None,
        },
        Some(run_as) => RunAsStatus {
            configured: true,
            roles:      run_as.roles.clone(),
            scopes:     run_as.scopes.clone(),
            tenant:     run_as.tenant.clone(),
        },
    }
}

/// Render the status list as a human-readable report.
#[must_use]
pub fn render_text(statuses: &[SourceStatus], database_connected: bool) -> String {
    use std::fmt::Write as _;

    if statuses.is_empty() {
        return "No sources declared in the compiled schema.\n".to_string();
    }

    let mut out = String::new();
    let _ = writeln!(out, "Sources ({})\n", statuses.len());
    for status in statuses {
        let marker = if status.enabled { '●' } else { '○' };
        let enabled = if status.enabled {
            "enabled"
        } else {
            "DISABLED"
        };
        let _ = writeln!(
            out,
            "{marker} {name}    schedule {schedule}    {enabled}",
            name = status.name,
            schedule = status.schedule,
        );
        let _ = writeln!(out, "    function   {}", status.function);
        let _ = writeln!(out, "    run_as     {}", render_run_as(&status.run_as));
        let _ =
            writeln!(out, "    cursor     {}", render_cursor(&status.cursor, database_connected));
        out.push('\n');
    }
    out
}

/// Render a `run_as` ceiling, calling out the fail-closed (unconfigured) case.
fn render_run_as(run_as: &RunAsStatus) -> String {
    if !run_as.configured {
        return "(none — fail-closed: mutations are denied until a run_as ceiling is granted)"
            .to_string();
    }
    let roles = if run_as.roles.is_empty() {
        "[]".to_string()
    } else {
        format!("{:?}", run_as.roles)
    };
    let scopes = if run_as.scopes.is_empty() {
        "[]".to_string()
    } else {
        format!("{:?}", run_as.scopes)
    };
    let tenant = run_as.tenant.as_deref().unwrap_or("(per-message / global)");
    format!("roles={roles} scopes={scopes} tenant={tenant}")
}

/// Render a cursor's durable state.
fn render_cursor(cursor: &CursorStatus, database_connected: bool) -> String {
    match cursor.state {
        "advanced" => {
            let version = cursor.version.unwrap_or_default();
            let age = cursor.age_seconds.unwrap_or_default();
            let when = cursor.updated_at.as_deref().unwrap_or("");
            let value =
                cursor.value.as_ref().map_or_else(|| "null".to_string(), ToString::to_string);
            format!("v{version} · advanced {age:.0}s ago · {when} · value {value}")
        },
        "never_advanced" => "never advanced (no watermark yet)".to_string(),
        _ if !database_connected => {
            "(no database connection — pass --db-url to read cursor state)".to_string()
        },
        _ => "unknown".to_string(),
    }
}

/// Run `fraiseql sources`: load the compiled schema, optionally read cursor state
/// from the database, and print the merged status as text or JSON.
///
/// # Errors
///
/// Returns an error if the compiled schema cannot be read/parsed, or if a database
/// URL resolves but the cursor read fails.
pub async fn run(args: SourcesArgs) -> Result<()> {
    let text = std::fs::read_to_string(&args.schema)
        .with_context(|| format!("cannot read compiled schema {}", args.schema.display()))?;
    let schema = CompiledSchema::from_json(&text, false).map_err(|e| {
        anyhow::anyhow!("cannot parse compiled schema {}: {e}", args.schema.display())
    })?;

    // A database read is best-effort: when no URL resolves, list definitions with
    // `unknown` cursor state rather than failing — the definitions are still useful.
    // When a URL *does* resolve, a read failure is a real error (the operator asked
    // for cursor state).
    let (cursors, database_connected) = match resolve_database_url(args.database.as_deref()) {
        Ok(db_url) => {
            let reader = SourceCursorReader::connect(&db_url)?;
            let rows = reader.load_cursors().await?;
            let map: HashMap<String, CursorRow> =
                rows.into_iter().map(|row| (row.source_name.clone(), row)).collect();
            (Some(map), true)
        },
        Err(_) => (None, false),
    };

    let statuses = build_status(&schema.sources, cursors.as_ref());

    if args.json {
        let payload = serde_json::json!({
            "database_connected": database_connected,
            "sources": statuses,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print!("{}", render_text(&statuses, database_connected));
    }
    Ok(())
}
