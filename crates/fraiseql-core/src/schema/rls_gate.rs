//! Whether declared Row-Level Security binds the roles that actually read.
//!
//! A schema that declares `[security.rls]` isolates tenants only through PostgreSQL's
//! policies, and PostgreSQL applies a table's policies to a reader only when:
//!
//! * the reader is neither a superuser nor `BYPASSRLS`;
//! * the reader does not own the table (directly or through a role it inherits), unless the table
//!   is `FORCE ROW LEVEL SECURITY`;
//! * every view between the reader and the table is `security_invoker` — a default view runs as its
//!   owner;
//! * the reader is not running a `SECURITY DEFINER` function, which runs as its owner.
//!
//! [`refuse_unbound_rls_sources`] checks each of these against the live catalog, as the
//! role of every pool a read can be served from, and names every source that fails.

use std::collections::HashMap;

use serde_json::Value;

use super::{CompiledSchema, SourceKind, SourceProbe, regclass_literal, sql_source_probes};
use crate::{
    db::DatabaseAdapter,
    error::{FraiseQLError, Result},
};

/// One catalog row, as an adapter returns it.
pub(super) type Row = HashMap<String, Value>;

/// The reading role and whether row level security can bind it.
pub(super) const ROLE_SQL: &str = "SELECT current_user::text AS role_name, r.rolsuper AS superuser, \
                        r.rolbypassrls AS bypassrls \
                        FROM pg_roles r WHERE r.rolname = current_user";

/// Refuse a schema whose declared Row-Level Security does not bind the roles that read.
///
/// Returns `Ok` without a database round trip for a schema with no SQL sources at all:
/// there is nothing to read, so nothing to isolate. A schema with sources is checked on
/// every read pool (see [`DatabaseAdapter::execute_raw_query_on_read_pools`]), and is
/// refused when:
///
/// * a pool's role cannot be read, is a superuser, or has `BYPASSRLS`;
/// * a relation source does not exist;
/// * a relation source, or a relation a view source reads at any depth, is a view that is not
///   `security_invoker`, a table without row level security or without a policy, or a table the
///   pool's role owns (or inherits ownership of) without `FORCE ROW LEVEL SECURITY`;
/// * a function source is `SECURITY DEFINER` and its owner is a superuser or `BYPASSRLS`, or owns
///   (or inherits ownership of) a relation this check reads without `FORCE ROW LEVEL SECURITY`.
///
/// A `SECURITY INVOKER` function runs as the reader, so the reader's checks cover it; the
/// relations its body reads cannot be known from the catalog, and a warning says so. The
/// same holds for a `SECURITY DEFINER` function's body: it is refused only on what can be
/// read from the catalog about its owner.
///
/// # Errors
///
/// [`FraiseQLError::Configuration`] naming every failure; [`FraiseQLError::Database`] if
/// the catalog cannot be read.
pub async fn refuse_unbound_rls_sources<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<()> {
    let sources = sql_source_probes(schema);
    if sources.is_empty() {
        return Ok(());
    }

    let mut failures: Vec<String> = Vec::new();
    for (pool, rows) in adapter.execute_raw_query_on_read_pools(ROLE_SQL).await? {
        if let Some(failure) = role_failure(&pool, rows.first()) {
            failures.push(failure);
        }
    }

    // Tables this check reads, by oid, for the definer-function owner rule below.
    let mut tables: Vec<String> = Vec::new();
    for source in sources.iter().filter(|p| p.kind == SourceKind::Relation) {
        let display = source.display_name();
        let sql = relation_chain_sql(source);
        for (pool, rows) in adapter.execute_raw_query_on_read_pools(&sql).await? {
            if rows.is_empty() {
                push_once(&mut failures, format!("{display} — relation does not exist"));
                continue;
            }
            for row in &rows {
                if let Some(oid) = text(row, "oid") {
                    if !is_view(row) && !tables.iter().any(|t| t == oid) {
                        tables.push(oid.to_string());
                    }
                }
                if let Some(reason) = relation_failure(row) {
                    let relation = text(row, "relation").unwrap_or(&display);
                    let subject = if flag(row, "is_source") || relation == display {
                        display.clone()
                    } else {
                        format!("{display} — reads {relation}")
                    };
                    let on = if pool == "primary" {
                        String::new()
                    } else {
                        format!(" (as {pool}'s role)")
                    };
                    push_once(&mut failures, format!("{subject} — {reason}{on}"));
                }
            }
        }
    }

    let mut unverifiable: Vec<String> = Vec::new();
    for source in sources.iter().filter(|p| p.kind == SourceKind::Function) {
        let display = source.display_name();
        let rows = primary_rows(adapter, &function_sql(source, &tables)).await?;
        for row in &rows {
            let function = text(row, "function").unwrap_or(&display).to_string();
            if !flag(row, "definer") {
                unverifiable.push(function);
                continue;
            }
            let owner = text(row, "owner").unwrap_or("?");
            if flag(row, "owner_superuser") || flag(row, "owner_bypassrls") {
                let what = if flag(row, "owner_superuser") {
                    "a superuser"
                } else {
                    "a BYPASSRLS role"
                };
                push_once(
                    &mut failures,
                    format!(
                        "{function} — SECURITY DEFINER function owned by `{owner}`, {what}: it \
                         runs as that role, which row level security never binds"
                    ),
                );
            }
            if let Some(owned) = text(row, "owned_unforced").filter(|s| !s.is_empty()) {
                push_once(
                    &mut failures,
                    format!(
                        "{function} — SECURITY DEFINER function owned by `{owner}`, which owns \
                         {owned} without FORCE ROW LEVEL SECURITY: it runs as that owner, so \
                         those policies do not apply to it"
                    ),
                );
            }
        }
    }
    if !unverifiable.is_empty() {
        tracing::warn!(
            functions = ?unverifiable,
            "Row-Level Security is declared, and these SECURITY INVOKER function sources run \
             as the reader, but which relations their bodies read cannot be checked from the \
             catalog: make sure each is RLS-protected."
        );
    }

    if failures.is_empty() {
        return Ok(());
    }
    Err(FraiseQLError::Configuration {
        message: format!(
            "Row-Level Security is declared for this multi-tenant schema but does not bind \
             the role(s) that read {} of its source(s), so tenants are not isolated:\n  - {}\n\
             Connect as a role that is neither a superuser nor BYPASSRLS, make every table \
             it owns `FORCE ROW LEVEL SECURITY` (or have another role own them), enable RLS \
             and declare a policy on each source table (`ALTER TABLE … ENABLE ROW LEVEL \
             SECURITY` + `CREATE POLICY …`), define views `WITH (security_invoker = true)` so \
             they honour the caller's policies, and give SECURITY DEFINER functions an owner \
             that row level security binds. Or remove `[security.rls] enabled = true` if \
             this deployment does not rely on database RLS.",
            failures.len(),
            failures.join("\n  - ")
        ),
    })
}

/// Why `pool`'s role is not bound by row level security, if it is not.
pub(super) fn role_failure(pool: &str, row: Option<&Row>) -> Option<String> {
    let Some(row) = row else {
        return Some(format!(
            "{pool} — could not read the role it connects as, so whether row level security \
             binds it is unknown"
        ));
    };
    let role = text(row, "role_name").unwrap_or("?");
    if flag(row, "superuser") {
        return Some(format!(
            "{pool} — connects as `{role}`, a superuser: row level security never applies to it"
        ));
    }
    if flag(row, "bypassrls") {
        return Some(format!(
            "{pool} — connects as `{role}`, which has BYPASSRLS: row level security never \
             applies to it"
        ));
    }
    None
}

/// Why one relation of a source's chain does not isolate rows for the reading role.
fn relation_failure(row: &Row) -> Option<&'static str> {
    if is_view(row) {
        return (!flag(row, "security_invoker")).then_some(
            "view is not `security_invoker`, so it runs with its owner's privileges and \
             is not bound by the caller's RLS policies",
        );
    }
    if !flag(row, "rls_enabled") {
        return Some("row level security is not enabled on this table");
    }
    if text(row, "policy_count").is_none_or(|n| n == "0") {
        return Some(
            "row level security is enabled but no policy is defined, so it isolates nothing",
        );
    }
    if flag(row, "reader_owns") && !flag(row, "forced") {
        return Some(
            "owned by the reading role (or a role it inherits) without FORCE ROW LEVEL \
             SECURITY, so its policies do not apply to that role",
        );
    }
    None
}

/// The source relation and every relation it reads through views, at any depth, with the
/// row-level-security facts each needs, as the connecting role sees them.
fn relation_chain_sql(source: &SourceProbe) -> String {
    let literal = regclass_literal(&source.display_name());
    format!(
        "WITH RECURSIVE rel(oid, kind) AS ( \
           SELECT c.oid, c.relkind FROM pg_class c WHERE c.oid = to_regclass('{literal}') \
           UNION \
           SELECT base.oid, base.relkind FROM rel \
           JOIN pg_rewrite r ON r.ev_class = rel.oid \
           JOIN pg_depend d ON d.classid = 'pg_rewrite'::regclass AND d.objid = r.oid \
                            AND d.refclassid = 'pg_class'::regclass \
           JOIN pg_class base ON base.oid = d.refobjid AND base.oid <> rel.oid \
           WHERE rel.kind = 'v' \
         ) \
         SELECT c.oid::text AS oid, \
                c.oid::regclass::text AS relation, \
                c.relkind::text AS kind, \
                c.oid = to_regclass('{literal}') AS is_source, \
                c.relrowsecurity AS rls_enabled, \
                c.relforcerowsecurity AS forced, \
                pg_has_role(current_user, c.relowner, 'USAGE') AS reader_owns, \
                (SELECT count(*) FROM pg_policy p WHERE p.polrelid = c.oid)::text \
                  AS policy_count, \
                COALESCE((SELECT bool_or(lower(o.option_value) IN ('true','on','1')) \
                          FROM pg_options_to_table(c.reloptions) o \
                          WHERE o.option_name = 'security_invoker'), false) \
                  AS security_invoker \
         FROM rel JOIN pg_class c ON c.oid = rel.oid \
         ORDER BY is_source DESC, relation"
    )
}

/// Every function a function source names (all overloads), whether it is `SECURITY
/// DEFINER`, its owner's attributes, and which of `tables` (oids) its owner owns, or
/// inherits ownership of, without `FORCE ROW LEVEL SECURITY`.
fn function_sql(source: &SourceProbe, tables: &[String]) -> String {
    let name = source.name.replace('\'', "''");
    let namespace = source.schema.as_ref().map_or_else(
        || "pg_function_is_visible(p.oid)".to_string(),
        |s| format!("p.pronamespace = to_regnamespace('{}')", s.replace('\'', "''")),
    );
    let oids = if tables.is_empty() {
        "ARRAY[]::oid[]".to_string()
    } else {
        format!("ARRAY[{}]::oid[]", tables.join(","))
    };
    format!(
        "SELECT p.oid::regprocedure::text AS function, p.prosecdef AS definer, \
                o.rolname::text AS owner, o.rolsuper AS owner_superuser, \
                o.rolbypassrls AS owner_bypassrls, \
                COALESCE((SELECT string_agg(c.oid::regclass::text, ', ' ORDER BY 1) \
                          FROM pg_class c \
                          WHERE c.oid = ANY({oids}) AND NOT c.relforcerowsecurity \
                            AND pg_has_role(p.proowner, c.relowner, 'USAGE')), '') \
                  AS owned_unforced \
         FROM pg_proc p JOIN pg_roles o ON o.oid = p.proowner \
         WHERE p.proname = '{name}' AND {namespace} \
         ORDER BY 1"
    )
}

/// The primary pool's rows for `sql`, uncached.
async fn primary_rows<A: DatabaseAdapter + ?Sized>(adapter: &A, sql: &str) -> Result<Vec<Row>> {
    Ok(adapter
        .execute_raw_query_on_read_pools(sql)
        .await?
        .into_iter()
        .next()
        .map_or_else(Vec::new, |(_, rows)| rows))
}

fn is_view(row: &Row) -> bool {
    text(row, "kind") == Some("v")
}

pub(super) fn text<'a>(row: &'a Row, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

pub(super) fn flag(row: &Row, key: &str) -> bool {
    row.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Push `failure` unless an identical one is already listed: every pool answers the
/// pool-independent questions too.
pub(super) fn push_once(failures: &mut Vec<String>, failure: String) {
    if !failures.contains(&failure) {
        failures.push(failure);
    }
}
