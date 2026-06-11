//! Capture-trigger DDL generation for `@subscribable` types (#366).
//!
//! Turns a compiled schema's [`SubscribableEntity`] declarations into the
//! PostgreSQL DDL that installs the shipped external-write capture trigger
//! (`core.fn_entity_change_log_capture`) on each underlying base table — so
//! "installing capture is one command, not hand-written SQL."
//!
//! The triggers are **statement-level** with transition tables (`OLD TABLE` /
//! `NEW TABLE`), one each for INSERT / UPDATE / DELETE, so a bulk statement
//! captures all its rows in a single set-based INSERT. Each trigger passes the
//! GraphQL `entity_type` (→ `object_type`), the UUID public-id column, and the
//! tenant column as `TG_ARGV`, so the capture function needs no schema knowledge.
//!
//! This module emits only the per-table triggers (it is dependency-free). The
//! `core.fn_entity_change_log_capture` function itself ships as
//! `fraiseql_observers::migrations::entity_change_log_capture_trigger_sql`; the
//! `fraiseql generate capture-triggers` CLI command concatenates the two into a
//! self-contained install script.

#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use super::compiled::{CompiledSchema, SubscribableEntity};

/// The public-id (UUID) column the capture trigger reads for `object_id`.
///
/// `@subscribable` tables must expose this column as a UUID — the change-log
/// reader decodes `object_id` as a non-null `uuid`, and the capture function
/// skips rows whose value is not UUID-shaped (see the trigger's UUID guard).
const PK_COLUMN: &str = "id";

/// The tenant column the capture trigger reads for `tenant_id` (falling back to
/// the `fraiseql.tenant_id` session GUC when the column is absent).
const TENANT_COLUMN: &str = "tenant_id";

/// PostgreSQL identifier length limit, in bytes.
const PG_IDENT_MAX: usize = 63;

/// Generate the per-table capture-trigger install DDL for a compiled schema's
/// `@subscribable` declarations (#366).
///
/// For each declared table the output contains an idempotent
/// `DROP TRIGGER IF EXISTS … ; CREATE TRIGGER …` for INSERT, UPDATE, and DELETE,
/// each wired to `core.fn_entity_change_log_capture(entity_type, 'id',
/// 'tenant_id')`. Returns an empty string when nothing is subscribable.
///
/// The output assumes `core.fn_entity_change_log_capture` and the contract table
/// `core.tb_entity_change_log` already exist (the CLI command prepends the
/// function). A table name that is not a plain or schema-qualified identifier is
/// skipped with a `-- WARNING:` comment rather than emitting unsafe SQL.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{CompiledSchema, SubscribableEntity, generate_capture_trigger_ddl};
///
/// let mut schema = CompiledSchema::default();
/// schema.subscribable.push(SubscribableEntity {
///     entity_type: "Post".to_string(),
///     tables:      vec!["tb_post".to_string()],
/// });
/// let ddl = generate_capture_trigger_ddl(&schema);
/// assert!(ddl.contains("CREATE TRIGGER"));
/// assert!(ddl.contains("core.fn_entity_change_log_capture('Post', 'id', 'tenant_id')"));
/// ```
#[must_use]
pub fn generate_capture_trigger_ddl(schema: &CompiledSchema) -> String {
    if schema.subscribable.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(
        "-- FraiseQL external-write capture triggers (#366) — generated.\n\
         -- Requires core.tb_entity_change_log + core.fn_entity_change_log_capture().\n\n",
    );
    for entity in &schema.subscribable {
        out.push_str(&entity_ddl(entity));
    }
    out
}

/// Emit the trigger DDL for one `@subscribable` entity (all its tables).
fn entity_ddl(entity: &SubscribableEntity) -> String {
    let mut out = String::new();
    for table in &entity.tables {
        match split_qualified_ident(table) {
            Some((schema_part, table_part)) => {
                out.push_str(&table_ddl(&entity.entity_type, schema_part.as_deref(), &table_part));
            },
            None => {
                let _ = write!(
                    out,
                    "-- WARNING: skipped @subscribable table {table:?} on type {:?} \
                     (not a plain or schema-qualified identifier)\n\n",
                    entity.entity_type
                );
            },
        }
    }
    out
}

/// Emit the three statement-level triggers for one validated table.
fn table_ddl(entity_type: &str, schema_part: Option<&str>, table_part: &str) -> String {
    let target = match schema_part {
        Some(s) => format!("\"{s}\".\"{table_part}\""),
        None => format!("\"{table_part}\""),
    };
    let args = format!("{}, '{PK_COLUMN}', '{TENANT_COLUMN}'", sql_string_literal(entity_type));

    let mut out = String::new();
    for (suffix, when, referencing) in [
        ("ins", "INSERT", "NEW TABLE AS new_table"),
        ("upd", "UPDATE", "OLD TABLE AS old_table NEW TABLE AS new_table"),
        ("del", "DELETE", "OLD TABLE AS old_table"),
    ] {
        let trigger = trigger_name(suffix, schema_part, table_part);
        let _ = write!(
            out,
            "DROP TRIGGER IF EXISTS \"{trigger}\" ON {target};\n\
             CREATE TRIGGER \"{trigger}\" AFTER {when} ON {target}\n  \
             REFERENCING {referencing} FOR EACH STATEMENT\n  \
             EXECUTE FUNCTION core.fn_entity_change_log_capture({args});\n\n"
        );
    }
    out
}

/// Build a ≤ 63-byte trigger name, deterministic and collision-resistant.
///
/// `tr_cdc_capture_<op>_<sanitized>`; when the schema-qualified table identity is
/// long enough to overflow the 63-byte cap, the sanitized portion is truncated
/// and suffixed with a short hash of the full (`schema.table`) identity so two
/// distinct long tables never collide on a truncated name.
fn trigger_name(suffix: &str, schema_part: Option<&str>, table_part: &str) -> String {
    let prefix = format!("tr_cdc_capture_{suffix}_");
    let sanitized = sanitize_ident(table_part);
    let budget = PG_IDENT_MAX - prefix.len();
    if sanitized.len() <= budget {
        return format!("{prefix}{sanitized}");
    }
    // Disambiguate by the FULL identity (schema-qualified), so two long tables
    // that share a truncated prefix still get distinct trigger names.
    let full = match schema_part {
        Some(s) => format!("{s}.{table_part}"),
        None => table_part.to_string(),
    };
    let hash = format!("{:08x}", djb2(&full));
    let keep = budget.saturating_sub(hash.len() + 1);
    format!("{prefix}{}_{hash}", &sanitized[..keep])
}

/// Replace any non-`[A-Za-z0-9_]` byte with `_` so the result is a legal
/// (unquoted-safe) identifier fragment for a trigger name.
fn sanitize_ident(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Split a table reference into an optional schema and a table, validating each
/// part is a plain SQL identifier. Returns `None` for anything else (empty, more
/// than one dot, or a part with illegal characters) so the caller can skip it.
fn split_qualified_ident(table: &str) -> Option<(Option<String>, String)> {
    let parts: Vec<&str> = table.split('.').collect();
    match parts.as_slice() {
        [t] if is_plain_ident(t) => Some((None, (*t).to_string())),
        [s, t] if is_plain_ident(s) && is_plain_ident(t) => {
            Some((Some((*s).to_string()), (*t).to_string()))
        },
        _ => None,
    }
}

/// A plain unquoted SQL identifier: a leading letter/underscore then
/// letters/digits/underscores. Rejects empty, leading digits, and any character
/// that would need quoting (spaces, dots, quotes, `;`, …).
fn is_plain_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {},
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Quote a string for a single-quoted SQL literal (doubling embedded quotes).
fn sql_string_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A tiny deterministic string hash (djb2) for trigger-name disambiguation — no
/// external crate, no `Math.random`/time dependence, stable across runs.
fn djb2(s: &str) -> u32 {
    s.bytes().fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(u32::from(b)))
}
