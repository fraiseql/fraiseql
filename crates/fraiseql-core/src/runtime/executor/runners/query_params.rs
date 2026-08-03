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
    "where", "limit", "offset", "orderBy", "first", "last", "after", "before", "nearest",
];

/// Parse the `nearest` similarity-search argument (#386).
///
/// `nearest: {vector: [Float!], k: Int!, metric: "cosine"|"l2"|"inner_product"}`
/// on a list query whose return type declares exactly one vector field lowers to
/// `ORDER BY "<column>" <op> '[…]'::vector LIMIT k` — the pgvector ANN shape,
/// index-eligible because the backing view must expose the vector as a native
/// column (the storage contract; the JSONB `data` payload does not carry
/// embeddings).
///
/// Returns `Ok(None)` when no `nearest` argument is present; every malformed or
/// unsupported shape is a loud validation error — a similarity request that
/// silently ran unordered would read as "search worked".
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] when the query is not an eligible list
/// query, the type has no (or more than one) vector field, the vector's
/// dimension does not match the declared `vector_config.dimensions`, `k` is
/// missing or zero, the metric is unknown, or `nearest` is combined with
/// `limit` / `orderBy`.
pub fn nearest_order_and_limit(
    arguments: &std::collections::HashMap<String, serde_json::Value>,
    schema: &crate::schema::CompiledSchema,
    query_def: &crate::schema::QueryDefinition,
) -> Result<Option<(crate::db::OrderByClause, u32)>> {
    let Some(raw) = arguments.get("nearest") else {
        return Ok(None);
    };

    if query_def.relay {
        return Err(FraiseQLError::validation(
            "`nearest` is not supported on relay (connection) queries",
        ));
    }
    if !query_def.returns_list {
        return Err(FraiseQLError::validation("`nearest` requires a list-returning query"));
    }
    if arguments.contains_key("limit") {
        return Err(FraiseQLError::validation(
            "`nearest.k` sets the page size; do not combine `nearest` with `limit`",
        ));
    }
    if arguments.contains_key("orderBy") {
        return Err(FraiseQLError::validation(
            "`nearest` orders by vector distance; do not combine it with `orderBy`",
        ));
    }

    let type_def = schema.find_type(&query_def.return_type).ok_or_else(|| {
        FraiseQLError::validation(format!(
            "`nearest` target type '{}' is not defined",
            query_def.return_type
        ))
    })?;
    let mut vector_fields = type_def.fields.iter().filter(|f| f.vector_config.is_some());
    let Some(field) = vector_fields.next() else {
        return Err(FraiseQLError::validation(format!(
            "`nearest` requires a vector field on type '{}', and it declares none",
            type_def.name
        )));
    };
    if vector_fields.next().is_some() {
        return Err(FraiseQLError::validation(format!(
            "type '{}' declares more than one vector field; `nearest` does not yet \
             support selecting between them",
            type_def.name
        )));
    }
    let config = field.vector_config.as_ref().expect("filtered on vector_config.is_some() above");

    let shape_err = || {
        FraiseQLError::validation(
            "`nearest` takes {vector: [Float, ...], k: Int, metric: \
             \"cosine\"|\"l2\"|\"inner_product\"} (metric optional — defaults to the \
             field's declared distance metric)",
        )
    };
    let obj = raw.as_object().ok_or_else(shape_err)?;
    for key in obj.keys() {
        if !matches!(key.as_str(), "vector" | "k" | "metric") {
            return Err(FraiseQLError::validation(format!(
                "unknown `nearest` argument key '{key}' (expected vector, k, metric)"
            )));
        }
    }
    let vector = obj.get("vector").and_then(serde_json::Value::as_array).ok_or_else(shape_err)?;
    let k = obj
        .get("k")
        .and_then(serde_json::Value::as_u64)
        .and_then(|k| u32::try_from(k).ok())
        .filter(|k| *k > 0)
        .ok_or_else(|| FraiseQLError::validation("`nearest.k` must be a positive integer"))?;

    // Dimension check against the declared config — the request-time consumer
    // vector_config previously never had.
    let declared = config.dimensions as usize;
    if vector.len() != declared {
        return Err(FraiseQLError::validation(format!(
            "`nearest.vector` has {} components but '{}.{}' declares {} dimensions",
            vector.len(),
            type_def.name,
            field.name,
            declared
        )));
    }

    let metric = match obj.get("metric").and_then(serde_json::Value::as_str) {
        None => config.distance_metric,
        Some("cosine") => crate::schema::DistanceMetric::Cosine,
        Some("l2") => crate::schema::DistanceMetric::L2,
        Some("inner_product") => crate::schema::DistanceMetric::InnerProduct,
        Some(other) => {
            return Err(FraiseQLError::validation(format!(
                "unknown `nearest.metric` '{other}' (expected cosine, l2 or inner_product)"
            )));
        },
    };

    // pgvector text literal, built exclusively from finite numbers — this is
    // what makes the interpolated form injection-impossible (and the SQL
    // builder re-validates the character set as defence in depth).
    let mut literal = String::with_capacity(vector.len() * 8 + 2);
    literal.push('[');
    for (i, component) in vector.iter().enumerate() {
        let n = component.as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
            FraiseQLError::validation("`nearest.vector` components must all be finite numbers")
        })?;
        if i > 0 {
            literal.push(',');
        }
        // Reason: fmt::Write for String is infallible
        std::fmt::Write::write_fmt(&mut literal, format_args!("{n}"))
            .expect("write to String is infallible");
    }
    literal.push(']');

    // The storage contract: the view exposes the vector as a native snake_case
    // column. Validated as a bare identifier, then quoted.
    let column = crate::utils::to_snake_case(field.name.as_str());
    let valid_ident = column.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && column.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !valid_ident {
        return Err(FraiseQLError::validation(format!(
            "vector field name '{}' does not resolve to a bare SQL identifier",
            field.name
        )));
    }

    let mut clause =
        crate::db::OrderByClause::new(field.name.to_string(), crate::db::OrderDirection::Asc);
    clause.native_column = Some(format!("\"{column}\""));
    clause.vector = Some(crate::db::VectorDistanceOrder {
        operator:     metric.operator().to_string(),
        query_vector: literal,
    });

    Ok(Some((clause, k)))
}

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
