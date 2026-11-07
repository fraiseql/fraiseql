//! Whether a fact table declared `row_scope = "relation"` is filtered by its relation.
//!
//! The server adds no tenant predicate to such a table's aggregate and window roots: the
//! declaration says the relation filters its own rows, on a session variable the server
//! sets per request. [`refuse_unfiltered_relation_scopes`] reads the live catalog, as the
//! role of every pool a read can be served from, and refuses each relation-scoped fact
//! table whose relation shows no such filter.
//!
//! A relation filters on a setting when it refers to it as `current_setting('<name>'…)`,
//! where `<name>` is a session variable sourced from the caller's token or an enriched
//! identity field — never a request header, which the caller controls. It refers to it:
//!
//! * in a view's own definition, which binds every reader, whatever role it reads as;
//! * in a table's row level security policies, which bind a reader only when row level security
//!   applies to it: it is neither a superuser nor `BYPASSRLS`, and it does not own the table (or a
//!   role it inherits does) unless the table is `FORCE ROW LEVEL SECURITY`. Among the policies that
//!   apply to the reader's reads, a restrictive one referring to the setting filters every row;
//!   otherwise every permissive one must, as permissive policies are combined with `OR`;
//! * through a `security_invoker` view, every relation of which filters.
//!
//! The check confirms that the filter refers to the setting, not that the filter is
//! correct: what the expression does with the value is the author's.

use std::collections::HashMap;

use serde_json::Value;

use super::{
    CompiledSchema, SessionVariableSource, regclass_literal,
    rls_gate::{ROLE_SQL, Row, flag, push_once, role_failure, text},
};
use crate::{
    compiler::fact_table::FactRowScope,
    db::DatabaseAdapter,
    error::{FraiseQLError, Result},
};

/// Refuse a schema whose relation-scoped fact tables are not filtered by their relations.
///
/// Returns `Ok` without a database round trip when no fact table declares
/// `row_scope = "relation"`. Otherwise each such table's relation is read from the
/// catalog on every read pool (see
/// [`DatabaseAdapter::execute_raw_query_on_read_pools`]), and refused when it does not
/// exist, or does not filter on a session variable sourced from the token or an enriched
/// field (see the module documentation for what counts).
///
/// # Errors
///
/// [`FraiseQLError::Configuration`] naming every relation-scoped fact table its relation
/// does not filter; [`FraiseQLError::Database`] if the catalog cannot be read.
pub async fn refuse_unfiltered_relation_scopes<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<()> {
    let mut tables: Vec<&str> = schema
        .fact_tables
        .values()
        .filter(|table| table.row_scope == FactRowScope::Relation)
        .map(|table| table.table_name.as_str())
        .collect();
    if tables.is_empty() {
        return Ok(());
    }
    tables.sort_unstable();

    let settings: Vec<String> = schema
        .session_variables
        .variables
        .iter()
        .filter(|variable| {
            matches!(
                variable.source,
                SessionVariableSource::Jwt { .. } | SessionVariableSource::Enrichment { .. }
            )
        })
        .map(|variable| variable.name.to_lowercase())
        .collect();

    let mut failures: Vec<String> = Vec::new();
    if settings.is_empty() {
        for table in &tables {
            failures.push(format!(
                "fact table '{table}' — no session variable is sourced from the token (`jwt`) \
                 or an enriched field (`enrichment`), so there is no setting to filter on"
            ));
        }
        return Err(refusal(&failures, &settings));
    }

    let mut roles: HashMap<String, Option<String>> = HashMap::new();
    for (pool, rows) in adapter.execute_raw_query_on_read_pools(ROLE_SQL).await? {
        let failure = role_failure(&pool, rows.first());
        roles.insert(pool, failure);
    }

    for table in tables {
        let sql = relation_sql(table);
        for (pool, rows) in adapter.execute_raw_query_on_read_pools(&sql).await? {
            let check = Check {
                relations: rows
                    .iter()
                    .filter_map(|row| text(row, "oid").map(|oid| (oid.to_string(), row)))
                    .collect(),
                settings:  &settings,
                role:      roles.get(&pool).and_then(Option::as_deref),
            };
            let Some(source) = rows.iter().find(|row| flag(row, "is_source")) else {
                push_once(&mut failures, format!("fact table '{table}' — relation does not exist"));
                continue;
            };
            if let Err(fault) = check.filters(source, 0) {
                let subject =
                    if fault.relation.as_str() == text(source, "relation").unwrap_or(table) {
                        format!("fact table '{table}'")
                    } else {
                        format!("fact table '{table}' — reads {}", fault.relation)
                    };
                let on = if pool == "primary" {
                    String::new()
                } else {
                    format!(" (as {pool}'s role)")
                };
                push_once(&mut failures, format!("{subject} — {}{on}", fault.reason));
            }
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    Err(refusal(&failures, &settings))
}

/// The refusal naming every `failure`, and the settings a relation may filter on.
fn refusal(failures: &[String], settings: &[String]) -> FraiseQLError {
    let accepted = if settings.is_empty() {
        "a session variable sourced from the token or an enriched field".to_string()
    } else {
        settings.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ")
    };
    FraiseQLError::Configuration {
        message: format!(
            "{} fact table(s) declare `row_scope = \"relation\"`, so the server adds no tenant \
             predicate to their aggregate and window roots, but their relation does not filter \
             their rows per request:\n  - {}\nFilter each relation on {accepted}, read with \
             `current_setting('<name>', true)`: in a view's own WHERE clause, or in a row level \
             security policy on each table it reads (enabled, applying to the reading role, \
             which must be neither a superuser nor BYPASSRLS, with FORCE ROW LEVEL SECURITY \
             when that role owns the table), read through `security_invoker` views. Or scope \
             the fact table with `inject_params`.",
            failures.len(),
            failures.join("\n  - ")
        ),
    }
}

/// Views nest at most this deep before the check gives up on a chain.
const MAX_DEPTH: usize = 32;

/// Why a relation of a chain does not filter its rows.
struct Fault {
    relation: String,
    reason:   String,
}

/// One pool's answer for one fact table's relation chain.
struct Check<'a> {
    /// Every relation of the chain, by oid.
    relations: HashMap<String, &'a Row>,
    /// The settings a relation may filter on, lowercased.
    settings:  &'a [String],
    /// Why row level security does not bind this pool's role, if it does not.
    role:      Option<&'a str>,
}

impl Check<'_> {
    /// Whether `row`'s relation filters its rows on one of the settings.
    fn filters(&self, row: &Row, depth: usize) -> std::result::Result<(), Fault> {
        let relation = text(row, "relation").unwrap_or("?").to_string();
        let fault = |reason: String| Fault {
            relation: relation.clone(),
            reason,
        };
        match text(row, "kind") {
            Some("v") => self.view_filters(row, depth).map_err(|reason| match reason {
                ViewFault::Own(reason) => fault(reason),
                ViewFault::Base(inner) => inner,
            }),
            Some("r" | "p") => self.table_filters(row).map_err(fault),
            _ => Err(fault(
                "it is neither a table nor a view, and only those can filter rows per request"
                    .to_string(),
            )),
        }
    }

    fn view_filters(&self, row: &Row, depth: usize) -> std::result::Result<(), ViewFault> {
        if text(row, "definition").is_some_and(|definition| self.refers(definition)) {
            return Ok(());
        }
        if !flag(row, "security_invoker") {
            return Err(ViewFault::Own(format!(
                "view is not `security_invoker` and does not filter on {} in its own \
                 definition, so it reads its relations with its owner's privileges and \
                 returns every row",
                self.named()
            )));
        }
        let reads = strings(row, "reads");
        if reads.is_empty() {
            return Err(ViewFault::Own(format!(
                "view reads no relation this check can see and does not filter on {} in its \
                 own definition",
                self.named()
            )));
        }
        if depth >= MAX_DEPTH {
            return Err(ViewFault::Own("views nest too deeply to check".to_string()));
        }
        for oid in reads {
            let Some(base) = self.relations.get(&oid) else {
                return Err(ViewFault::Own(format!(
                    "view reads a relation (oid {oid}) the catalog did not return"
                )));
            };
            self.filters(base, depth + 1).map_err(ViewFault::Base)?;
        }
        Ok(())
    }

    fn table_filters(&self, row: &Row) -> std::result::Result<(), String> {
        if !flag(row, "rls_enabled") {
            return Err("row level security is not enabled on it".to_string());
        }
        if let Some(role) = self.role {
            return Err(role.to_string());
        }
        if flag(row, "reader_owns") && !flag(row, "forced") {
            return Err("owned by the reading role (or a role it inherits) without FORCE ROW \
                        LEVEL SECURITY, so its policies do not apply to that role"
                .to_string());
        }
        let permissive = strings(row, "permissive");
        if permissive.is_empty() {
            return Err("no row level security policy applies to the reading role's reads \
                        (a permissive policy for SELECT or ALL, to PUBLIC or a role it is a \
                        member of)"
                .to_string());
        }
        if strings(row, "restrictive").iter().any(|qual| self.refers(qual)) {
            return Ok(());
        }
        if permissive.iter().all(|qual| self.refers(qual)) {
            return Ok(());
        }
        Err(format!(
            "a policy that applies to its reads does not filter on {}, and permissive \
             policies are combined with OR",
            self.named()
        ))
    }

    /// Whether `expression` reads one of the settings with `current_setting`.
    fn refers(&self, expression: &str) -> bool {
        let expression = expression.to_lowercase();
        self.settings
            .iter()
            .any(|setting| expression.contains(&format!("current_setting('{setting}'")))
    }

    fn named(&self) -> String {
        self.settings.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(" or ")
    }
}

/// A view's own fault, or the fault of a relation it reads.
enum ViewFault {
    Own(String),
    Base(Fault),
}

/// A JSON array of strings in `row[key]`, as the adapter decodes a `json` column.
fn strings(row: &Row, key: &str) -> Vec<String> {
    let value = match row.get(key) {
        Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or(Value::Null),
        Some(value) => value.clone(),
        None => Value::Null,
    };
    match value {
        Value::Array(items) => {
            items.into_iter().filter_map(|item| item.as_str().map(str::to_string)).collect()
        },
        _ => Vec::new(),
    }
}

/// The fact table's relation and every relation it reads through views, at any depth,
/// with what deciding whether each filters needs, as the connecting role sees them.
fn relation_sql(table: &str) -> String {
    let literal = regclass_literal(table);
    let applies = "p.polcmd IN ('r', '*') AND EXISTS (SELECT 1 FROM unnest(p.polroles) AS pr(r) \
                   WHERE CASE WHEN pr.r = 0 THEN true \
                         ELSE pg_has_role(current_user, pr.r, 'MEMBER') END)";
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
                COALESCE((SELECT bool_or(lower(o.option_value) IN ('true','on','1')) \
                          FROM pg_options_to_table(c.reloptions) o \
                          WHERE o.option_name = 'security_invoker'), false) \
                  AS security_invoker, \
                CASE WHEN c.relkind = 'v' THEN pg_get_viewdef(c.oid) END AS definition, \
                COALESCE((SELECT json_agg(DISTINCT d.refobjid::text) \
                          FROM pg_rewrite r \
                          JOIN pg_depend d ON d.classid = 'pg_rewrite'::regclass \
                                          AND d.objid = r.oid \
                                          AND d.refclassid = 'pg_class'::regclass \
                          WHERE r.ev_class = c.oid AND d.refobjid <> c.oid), '[]'::json) \
                  AS reads, \
                COALESCE((SELECT json_agg(COALESCE(pg_get_expr(p.polqual, p.polrelid), '')) \
                          FROM pg_policy p \
                          WHERE p.polrelid = c.oid AND p.polpermissive AND {applies}), \
                         '[]'::json) AS permissive, \
                COALESCE((SELECT json_agg(COALESCE(pg_get_expr(p.polqual, p.polrelid), '')) \
                          FROM pg_policy p \
                          WHERE p.polrelid = c.oid AND NOT p.polpermissive AND {applies}), \
                         '[]'::json) AS restrictive \
         FROM rel JOIN pg_class c ON c.oid = rel.oid \
         ORDER BY is_source DESC, relation"
    )
}

#[cfg(test)]
#[path = "relation_scope_gate_tests.rs"]
mod relation_scope_gate_tests;
