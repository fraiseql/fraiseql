//! Shared helpers for native SQL column handling in aggregation queries.
//!
//! Native columns are SQL columns that exist directly on a view or table (not inside
//! a JSONB `data` column). When a query's filter, GROUP BY, or ORDER BY references such
//! a column, FraiseQL should emit a direct column reference (`"col"`) rather than JSONB
//! extraction (`data->>'col'`).

use std::collections::HashMap;

use crate::compiler::fact_table::{FilterColumn, SqlType};

/// Map a PostgreSQL column type name to the cast suffix used in parameterized queries.
///
/// Returns `""` for text-like types that require no explicit cast, since PostgreSQL
/// infers the type from context. Returns the canonical short name (e.g. `"int8"`,
/// `"uuid"`) for all typed columns so bind parameters can be written as `$N::type`.
#[must_use]
pub(crate) fn pg_type_to_cast(data_type: &str) -> &'static str {
    match data_type.to_lowercase().as_str() {
        "uuid" => "uuid",
        "integer" | "int" | "int4" => "int4",
        "bigint" | "int8" => "int8",
        "smallint" | "int2" => "int2",
        "boolean" | "bool" => "bool",
        "numeric" | "decimal" => "numeric",
        "double precision" | "float8" => "float8",
        "real" | "float4" => "float4",
        "timestamp without time zone" | "timestamp" => "timestamp",
        "timestamp with time zone" | "timestamptz" => "timestamptz",
        "date" => "date",
        "time without time zone" | "time" => "time",
        // text, varchar, char(n), jsonb, etc. — no cast needed.
        _ => "",
    }
}

/// Convert a [`SqlType`] to the PostgreSQL cast suffix used in parameterized queries.
///
/// Returns `""` for text-like types that require no explicit cast.
#[must_use]
pub(crate) fn sql_type_to_pg_cast(sql_type: &SqlType) -> &'static str {
    match sql_type {
        SqlType::Uuid => "uuid",
        SqlType::Int => "int4",
        SqlType::BigInt => "int8",
        SqlType::Decimal => "numeric",
        SqlType::Float => "float8",
        SqlType::Boolean => "bool",
        SqlType::Timestamp => "timestamptz",
        SqlType::Date => "date",
        // text, varchar, jsonb, json, other — no cast needed.
        SqlType::Text | SqlType::Jsonb | SqlType::Json | SqlType::Other(_) => "",
    }
}

/// Build a `native_columns` map from a list of denormalized filter columns.
///
/// The returned map has the column name as key and the PostgreSQL cast suffix as value.
/// An empty cast string (`""`) means no cast is needed for that column type.
///
/// This map is passed to [`crate::runtime::AggregateQueryParser::parse`] so that the
/// parser can emit [`crate::db::where_clause::WhereClause::NativeField`] and
/// [`crate::compiler::aggregation::GroupBySelection::NativeDimension`] variants for
/// native columns instead of JSONB extraction variants.
#[must_use]
pub(crate) fn filter_columns_to_native_map(
    filters: &[FilterColumn],
) -> HashMap<String, String> {
    filters
        .iter()
        .map(|f| (f.name.clone(), sql_type_to_pg_cast(&f.sql_type).to_string()))
        .collect()
}
