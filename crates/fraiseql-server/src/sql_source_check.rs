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
/// A bare relation is resolved with `to_regclass` on the case-sensitively-quoted
/// identifier (`quote_postgres_identifier` — the runtime's own quoting), embedded
/// as a string literal because [`DatabaseAdapter::execute_raw_query`] takes no bind
/// parameters. A schema-qualified relation is looked up in the catalogs by its exact
/// names instead: `to_regclass` raises "permission denied for schema" for a role
/// without `USAGE` on it, which aborted the whole check rather than reporting the
/// source (#1426). A function is resolved via `pg_proc` (`prokind IN ('f','p')`),
/// schema-qualified or `current_schemas`-scoped. Identifiers come from the trusted
/// compiled schema but single quotes are still doubled defensively.
fn existence_sql(probe: &SourceProbe) -> String {
    match probe.kind {
        SourceKind::Relation => {
            if let Some(s) = &probe.schema {
                format!(
                    "SELECT EXISTS(SELECT 1 FROM pg_class c \
                       JOIN pg_namespace n ON n.oid = c.relnamespace \
                       WHERE n.nspname = '{}' AND c.relname = '{}') AS source_exists",
                    s.replace('\'', "''"),
                    probe.name.replace('\'', "''")
                )
            } else {
                let literal = quote_postgres_identifier(&probe.name).replace('\'', "''");
                format!("SELECT to_regclass('{literal}') IS NOT NULL AS source_exists")
            }
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

/// SQL returning the privilege the connected role lacks to use one probe's source
/// (`missing`, or no row / NULL when it can use it), as `has_*_privilege` answers for
/// `current_user` (#1426). Checked in order: `USAGE` on the source's schema, then
/// `SELECT` on a relation or `EXECUTE` on a function (any overload of the name).
fn privilege_sql(probe: &SourceProbe) -> String {
    let name = probe.name.replace('\'', "''");
    let schema_filter = probe.schema.as_ref().map_or_else(
        || "n.nspname = ANY(current_schemas(false))".to_string(),
        |s| format!("n.nspname = '{}'", s.replace('\'', "''")),
    );
    match probe.kind {
        SourceKind::Relation => format!(
            "SELECT CASE \
               WHEN NOT bool_or(has_schema_privilege(n.oid, 'USAGE')) \
                 THEN 'USAGE on schema ' || min(n.nspname) \
               WHEN NOT bool_or(has_table_privilege(c.oid, 'SELECT')) THEN 'SELECT' \
             END AS missing \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE {schema_filter} AND c.relname = '{name}' HAVING count(*) > 0"
        ),
        SourceKind::Function => format!(
            "SELECT CASE \
               WHEN NOT bool_or(has_schema_privilege(n.oid, 'USAGE')) \
                 THEN 'USAGE on schema ' || min(n.nspname) \
               WHEN NOT bool_or(has_function_privilege(p.oid, 'EXECUTE')) THEN 'EXECUTE' \
             END AS missing \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE {schema_filter} AND p.proname = '{name}' AND p.prokind IN ('f','p') \
             HAVING count(*) > 0"
        ),
    }
}

/// Probe every declared `sql_source` **as the connected role** and return the ones it
/// may not use, each with the privilege it lacks (#1426).
///
/// A source that exists but is not granted (a view nobody granted `SELECT` on, a function whose
/// `EXECUTE` was revoked from `PUBLIC`) passed the existence check and failed its first request
/// with `permission denied`. Sources that do not exist are left to
/// [`find_unbacked_sources`].
///
/// # Errors
///
/// Returns a [`fraiseql_core::FraiseQLError`] if a probe query fails.
pub async fn find_unusable_sources<A: DatabaseAdapter>(
    schema: &CompiledSchema,
    adapter: &A,
) -> fraiseql_core::Result<Vec<(SourceProbe, String)>> {
    let mut unusable = Vec::new();
    for probe in sql_source_probes(schema) {
        let rows = adapter.execute_raw_query(&privilege_sql(&probe)).await?;
        let missing = rows
            .first()
            .and_then(|r| r.get("missing"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        if let Some(missing) = missing {
            unusable.push((probe, missing));
        }
    }
    Ok(unusable)
}

/// The role the adapter's connections run as (`current_user`), for the diagnostic.
///
/// # Errors
///
/// Returns a [`fraiseql_core::FraiseQLError`] if the query fails.
pub async fn connected_role<A: DatabaseAdapter>(adapter: &A) -> fraiseql_core::Result<String> {
    let rows = adapter.execute_raw_query("SELECT current_user::text AS role").await?;
    Ok(rows
        .first()
        .and_then(|r| r.get("role"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("the connected role")
        .to_string())
}

/// Render the missing and the unusable sources as one boot diagnostic. The missing
/// sources keep [`format_unbacked`]'s lines, which the release-smoke harness asserts on.
#[must_use]
pub fn format_source_problems(
    unbacked: &[SourceProbe],
    unusable: &[(SourceProbe, String)],
    role: &str,
) -> String {
    use std::fmt::Write as _;

    let mut out = if unbacked.is_empty() {
        String::from(
            "fail-fast sql_source validation failed — declared sources cannot be used by the \
             server's database role:",
        )
    } else {
        format_unbacked(unbacked)
    };
    for (probe, missing) in unusable {
        let kind = match probe.kind {
            SourceKind::Relation => "relation",
            SourceKind::Function => "function",
        };
        let _ =
            write!(out, "\n  - {} ({kind}): {missing} not granted to {role}", probe.display_name());
    }
    out
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
