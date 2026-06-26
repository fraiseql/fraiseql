//! Opt-in fail-fast `sql_source` existence check at server boot (#487).
//!
//! Turns a declared-but-unbacked `sql_source` from a silent-until-hit per-request
//! 500 into a **loud-early** boot failure. Default **OFF** — when disabled the boot
//! path is byte-for-byte unchanged.
//!
//! Postgres-only. It executes the shared
//! [`fraiseql_core::schema::sql_source_probes`] work-list — the *same* definition
//! of "backed" the CLI `validate --against-db` gate uses — through the live
//! [`DatabaseAdapter`], so the two cannot drift. The probe SQL embeds quoted
//! identifiers (the adapter's raw-SQL entry point takes no bind parameters) and
//! resolves them verbatim, exactly as the runtime does.

use fraiseql_core::{
    db::{DatabaseAdapter, quote_postgres_identifier},
    schema::{CompiledSchema, SourceKind, SourceProbe, sql_source_probes},
};

/// SQL returning a single boolean column `source_exists` for one probe.
///
/// A relation is resolved with `to_regclass` on the case-sensitively-quoted
/// identifier (`quote_postgres_identifier` — the runtime's own quoting), embedded
/// as a string literal because [`DatabaseAdapter::execute_raw_query`] takes no bind
/// parameters. A function is resolved via `pg_proc` (`prokind IN ('f','p')`),
/// schema-qualified or `current_schemas`-scoped. Identifiers come from the trusted
/// compiled schema but single quotes are still doubled defensively.
fn existence_sql(probe: &SourceProbe) -> String {
    match probe.kind {
        SourceKind::Relation => {
            let ident = match &probe.schema {
                Some(s) => format!(
                    "{}.{}",
                    quote_postgres_identifier(s),
                    quote_postgres_identifier(&probe.name)
                ),
                None => quote_postgres_identifier(&probe.name),
            };
            let literal = ident.replace('\'', "''");
            format!("SELECT to_regclass('{literal}') IS NOT NULL AS source_exists")
        },
        SourceKind::Function => {
            let name = probe.name.replace('\'', "''");
            match &probe.schema {
                Some(s) => {
                    let schema = s.replace('\'', "''");
                    format!(
                        "SELECT EXISTS(SELECT 1 FROM pg_proc p \
                           JOIN pg_namespace n ON n.oid = p.pronamespace \
                           WHERE n.nspname = '{schema}' AND p.proname = '{name}' \
                           AND p.prokind IN ('f','p')) AS source_exists"
                    )
                },
                None => format!(
                    "SELECT EXISTS(SELECT 1 FROM pg_proc p \
                       JOIN pg_namespace n ON n.oid = p.pronamespace \
                       WHERE p.proname = '{name}' \
                       AND n.nspname = ANY(current_schemas(false)) \
                       AND p.prokind IN ('f','p')) AS source_exists"
                ),
            }
        },
    }
}

/// Probe every declared `sql_source` and return the ones **not** backed by a live
/// database object, in declaration order — empty means every source is backed.
///
/// # Errors
///
/// Returns a [`fraiseql_core::FraiseQLError`] if a probe query fails — including on
/// adapters with no raw-SQL path (the wire backend), where this check is never
/// enabled.
pub async fn find_unbacked_sources<A: DatabaseAdapter>(
    schema: &CompiledSchema,
    adapter: &A,
) -> fraiseql_core::Result<Vec<SourceProbe>> {
    let mut unbacked = Vec::new();
    for probe in sql_source_probes(schema) {
        let rows = adapter.execute_raw_query(&existence_sql(&probe)).await?;
        let exists = rows
            .first()
            .and_then(|r| r.get("source_exists"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !exists {
            unbacked.push(probe);
        }
    }
    Ok(unbacked)
}

/// Render an unbacked-source list as a boot diagnostic. Shape is kept stable so the
/// release-smoke harness can assert on it.
#[must_use]
pub fn format_unbacked(unbacked: &[SourceProbe]) -> String {
    use std::fmt::Write as _;

    let mut out = String::from(
        "fail-fast sql_source validation failed — declared sources are not backed by the database:",
    );
    for probe in unbacked {
        let kind = match probe.kind {
            SourceKind::Relation => "relation",
            SourceKind::Function => "function",
        };
        let _ = write!(out, "\n  - {} ({kind}) does not exist", probe.display_name());
    }
    out
}

#[cfg(test)]
#[path = "sql_source_check_tests.rs"]
mod sql_source_check_tests;
