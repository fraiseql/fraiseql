//! WHERE clause helpers and cache key computation for query execution.
//!
//! Pure functions that build `WhereClause` values from inject params and
//! explicit query arguments, and compute response cache keys.

use crate::{
    db::{WhereClause, WhereOperator},
    error::{FraiseQLError, Result},
};

/// Auto-wired argument names that are handled by the `auto_params` system.
/// These are never treated as explicit WHERE filters.
pub const AUTO_PARAM_NAMES: &[&str] = &[
    "where", "limit", "offset", "orderBy", "first", "last", "after", "before",
];

/// Build a `WhereClause` for a single inject param, respecting `native_columns`.
pub fn inject_param_where_clause(
    col: &str,
    value: serde_json::Value,
    native_columns: &std::collections::HashMap<String, String>,
) -> WhereClause {
    if let Some(pg_type) = native_columns.get(col) {
        WhereClause::NativeField {
            // `native_columns` is keyed by the GraphQL-surface name (camelCase
            // under `naming_convention = "camelCase"`); the SQL column it was
            // resolved from is snake_case. Recase like the JSONB path below —
            // idempotent for as-authored snake_case names.
            column: crate::utils::to_snake_case(col),
            pg_cast: pg_type_to_cast(pg_type).to_string(),
            operator: WhereOperator::Eq,
            value,
        }
    } else {
        WhereClause::Field {
            // Recase the JSONB key to snake_case so the predicate matches the stored
            // key (parity with the WHERE-input / mutation-input paths, #486/#456).
            // Idempotent for the common snake-from-config case.
            path: vec![crate::utils::to_snake_case(col)],
            operator: WhereOperator::Eq,
            value,
        }
    }
}

/// Convert PostgreSQL `information_schema.data_type` to a safe SQL cast suffix.
///
/// Returns an empty string for types that need no cast (e.g. `text`, `varchar`).
/// Normalise a database type name for use as the `pg_cast` hint in
/// `WhereClause::NativeField`.
///
/// The returned string is the **canonical PostgreSQL type name** (e.g. `"uuid"`,
/// `"int4"`, `"timestamp"`).  It is passed to `SqlDialect::cast_native_param`
/// which translates it into the dialect-appropriate cast expression:
/// - PostgreSQL: `$1::text::uuid`  (two-step to avoid binary wire-format mismatch)
/// - MySQL:      `CAST(? AS CHAR)`
/// - SQLite:     `CAST(? AS TEXT)`
/// - SQL Server: `CAST(@p1 AS UNIQUEIDENTIFIER)`
///
/// Returns `""` for text-like types that need no cast.
pub fn pg_type_to_cast(data_type: &str) -> &'static str {
    crate::runtime::native_columns::pg_type_to_cast(data_type)
}

/// Estimate the payload reduction percentage from projecting N fields.
///
/// Uses a simple heuristic: each projected field saves proportional space
/// relative to a baseline of 20 typical JSONB fields per row. Clamped to
/// [10, 90] so the hint is never misleadingly extreme.
pub fn compute_projection_reduction(projected_field_count: usize) -> u32 {
    // Baseline: assume a typical type has 20 fields.
    const BASELINE_FIELD_COUNT: usize = 20;
    let requested = projected_field_count.min(BASELINE_FIELD_COUNT);
    let saved = BASELINE_FIELD_COUNT.saturating_sub(requested);
    // saved / BASELINE * 100, clamped to [10, 90]
    #[allow(clippy::cast_possible_truncation)] // Reason: result is in 0..=100, fits u32
    let percent = ((saved * 100) / BASELINE_FIELD_COUNT) as u32;
    percent.clamp(10, 90)
}

/// Convert explicit query arguments (e.g. `id`, `slug`, `email`) into
/// WHERE equality conditions and AND them onto `existing`.
///
/// Arguments whose names match auto-wired parameters (`where`, `limit`,
/// `offset`, `orderBy`, `first`, `last`, `after`, `before`) are skipped —
/// they are handled separately by the auto-params system.
///
/// When an argument has a matching entry in `native_columns`, a
/// `WhereClause::NativeField` is emitted (enabling B-tree index lookup via
/// `WHERE col = $N::type`).  Otherwise a `WhereClause::Field` is emitted
/// (JSONB extraction: `WHERE data->>'col' = $N`).
///
/// The JSONB key is `snake_case`d with [`crate::utils::to_snake_case`] — the same
/// caser the WHERE-input and mutation-input paths use — so a camelCase argument
/// (`organizationId`) resolves to the stored key (`organization_id`) rather than a
/// never-matching `organizationId` key (#486, mirrors the #456 mutation fix).
pub fn combine_explicit_arg_where(
    existing: Option<WhereClause>,
    defined_args: &[crate::schema::ArgumentDefinition],
    provided_args: &std::collections::HashMap<String, serde_json::Value>,
    native_columns: &std::collections::HashMap<String, String>,
) -> Option<WhereClause> {
    let explicit_conditions: Vec<WhereClause> = defined_args
        .iter()
        .filter(|arg| !AUTO_PARAM_NAMES.contains(&arg.name.as_str()))
        .filter_map(|arg| {
            provided_args.get(&arg.name).map(|value| {
                if let Some(pg_type) = native_columns.get(&arg.name) {
                    WhereClause::NativeField {
                        // Same recasing as the JSONB branch below: the map key is
                        // the GraphQL argument name, not the SQL column name.
                        // `comments(postId: …)` must emit `WHERE post_id = …`,
                        // never `WHERE "postId" = …` (column does not exist).
                        column:   crate::utils::to_snake_case(&arg.name),
                        pg_cast:  pg_type_to_cast(pg_type).to_string(),
                        operator: WhereOperator::Eq,
                        value:    value.clone(),
                    }
                } else {
                    WhereClause::Field {
                        // Recase the camelCase GraphQL arg name to the snake_case JSONB
                        // key so `orders(organizationId: "x")` builds
                        // `data->>'organization_id'` (matches) instead of
                        // `data->>'organizationId'` (always NULL → silent `[]`).
                        // Same caser as the WHERE-input path (#486, mirrors #456).
                        path:     vec![crate::utils::to_snake_case(&arg.name)],
                        operator: WhereOperator::Eq,
                        value:    value.clone(),
                    }
                }
            })
        })
        .collect();

    if explicit_conditions.is_empty() {
        return existing;
    }

    let mut all_conditions = Vec::new();
    if let Some(prev) = existing {
        all_conditions.push(prev);
    }
    all_conditions.extend(explicit_conditions);

    match all_conditions.len() {
        1 => Some(all_conditions.remove(0)),
        _ => Some(WhereClause::And(all_conditions)),
    }
}

/// Reject a pagination argument that exceeds the configured maximum page size.
///
/// Returns the value unchanged when it is within the ceiling, when no ceiling is
/// configured (`max` is `None`), or when no value was supplied. This is the
/// top-level row-count guard against unbounded pagination (#421): a client-supplied
/// `first`/`last`/`limit` is the one knob that sizes the DB result set and the
/// serialized response, so an arbitrarily large value is a denial-of-service lever.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] when `value > max`, naming the argument and
/// the ceiling.
pub fn enforce_max_page_size(
    value: Option<u32>,
    max: Option<u32>,
    arg_name: &str,
) -> Result<Option<u32>> {
    if let (Some(v), Some(m)) = (value, max) {
        if v > m {
            return Err(FraiseQLError::Validation {
                message: format!("`{arg_name}` {v} exceeds the maximum page size of {m}"),
                path:    Some(arg_name.to_string()),
            });
        }
    }
    Ok(value)
}

#[cfg(test)]
#[path = "query_params_tests.rs"]
mod query_params_tests;
