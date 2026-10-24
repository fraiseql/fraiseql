//! The database contract of a query read from a function.
//!
//! A query whose `sql_source_kind` is `function` is read as `SELECT … FROM fn($1, …)`: the
//! executor binds the query's own arguments (auto-params left out) in declaration order, then
//! one parameter per inject param, and reads the query's `jsonb_column` from each row. The
//! function must therefore exist with exactly that many input parameters, return a set, return
//! that column as `json`/`jsonb`, and not be `VOLATILE`: a read may be cached, retried or served
//! from a replica, and none of those is safe for a function that writes.
//!
//! [`check_function_root`] answers that from catalog rows, so it is tested without a database;
//! [`validate_function_roots`] reads the rows for every function root of a schema.

use anyhow::Result;
use fraiseql_core::schema::{CompiledSchema, QueryDefinition};

use crate::schema::pg_catalog::{PgCatalog, PgFunction};

/// The queries of `schema` that read from a function.
pub fn function_roots(schema: &CompiledSchema) -> impl Iterator<Item = &QueryDefinition> {
    schema.queries.iter().filter(|q| !q.sql_source_kind.is_relation())
}

/// What is wrong with the function `query` reads, given every overload of its name
/// (`candidates`); empty when the read will bind and run.
#[must_use]
pub fn check_function_root(query: &QueryDefinition, candidates: &[PgFunction]) -> Vec<String> {
    let source = query.sql_source.as_deref().unwrap_or_default();
    if candidates.is_empty() {
        return vec![format!(
            "no function `{source}` is visible to the connection"
        )];
    }
    let bound: Vec<&str> = query
        .function_arguments()
        .map(|arg| arg.name.as_str())
        .chain(query.inject_params.keys().map(String::as_str))
        .collect();
    let matching: Vec<&PgFunction> =
        candidates.iter().filter(|f| f.in_types.len() == bound.len()).collect();
    let function = match matching.as_slice() {
        [function] => *function,
        [] => {
            let mut arities: Vec<usize> = candidates.iter().map(|f| f.in_types.len()).collect();
            arities.sort_unstable();
            arities.dedup();
            let takes = arities.iter().map(ToString::to_string).collect::<Vec<_>>().join(" or ");
            return vec![format!(
                "the query binds {} parameter(s) ({}), and `{source}` takes {takes}",
                bound.len(),
                if bound.is_empty() {
                    "none".to_string()
                } else {
                    bound.join(", ")
                },
            )];
        },
        several => {
            return vec![format!(
                "{} overloads of `{source}` take {} parameter(s); the query's parameters are bound \
                 untyped, so the call would be ambiguous. Keep one.",
                several.len(),
                bound.len()
            )];
        },
    };

    let mut problems = Vec::new();
    if !function.returns_set {
        problems.push(format!(
            "`{source}` returns one value; a query reads from a function that returns a set \
             (RETURNS TABLE(…) or RETURNS SETOF …)"
        ));
    }
    if function.volatility == "v" {
        problems.push(format!(
            "`{source}` is VOLATILE; a read may be cached, retried or served from a replica. \
             Declare it STABLE, or make it a mutation if it writes"
        ));
    }
    let column = query.jsonb_column.as_str();
    match function.out_columns.iter().find(|c| c.name == column) {
        Some(c) if matches!(c.type_name.as_str(), "json" | "jsonb") => {},
        Some(c) => problems.push(format!(
            "`{source}` returns column `{column}` as {}; the query reads it as the row's \
             json/jsonb document",
            c.type_name
        )),
        None => problems.push(format!(
            "`{source}` returns no column `{column}`, the row's document the query reads"
        )),
    }
    problems
}

/// Check every function root of `schema` against the database, as
/// `query \`name\` (sql_source: fn): problem` lines.
///
/// # Errors
///
/// Returns an error if a catalog query fails.
pub async fn validate_function_roots(
    schema: &CompiledSchema,
    catalog: &PgCatalog,
) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    for query in function_roots(schema) {
        let source = query.sql_source.as_deref().unwrap_or_default();
        let candidates = catalog.resolve_functions(source).await?;
        problems.extend(
            check_function_root(query, &candidates)
                .into_iter()
                .map(|p| format!("query `{}` (sql_source: {source}): {p}", query.name)),
        );
    }
    Ok(problems)
}

#[cfg(test)]
#[path = "function_root_contract_tests.rs"]
mod tests;
