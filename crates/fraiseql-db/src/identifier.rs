//! Database identifier quoting utilities.
//!
//! This module provides database-specific identifier quoting functions that handle
//! schema-qualified identifiers (e.g., `schema.table`, `catalog.schema.table`).
//!
//! Each function splits on `.` and quotes each component with the appropriate syntax
//! for the target database.

/// Quote a PostgreSQL identifier.
///
/// PostgreSQL uses double quotes for identifiers. Schema-qualified names
/// (e.g., `schema.table`) are split and quoted per component.
///
/// # Examples
///
/// ```rust
/// use fraiseql_db::quote_postgres_identifier;
/// assert_eq!(quote_postgres_identifier("v_user"), "\"v_user\"");
/// assert_eq!(quote_postgres_identifier("benchmark.v_user"), "\"benchmark\".\"v_user\"");
/// assert_eq!(
///     quote_postgres_identifier("catalog.schema.table"),
///     "\"catalog\".\"schema\".\"table\""
/// );
/// ```
#[inline]
#[must_use]
pub fn quote_postgres_identifier(identifier: &str) -> String {
    identifier
        .split('.')
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(".")
}

/// The SQL expression for the relation `schema` and `name` name, as a `regclass`, or `NULL`
/// when the current role cannot use `schema`.
///
/// `schema` and `name` are SQL expressions, such as the columns of a catalog row
/// (`g.schema`, `g.name`). `to_regclass` raises `permission denied for schema` for a schema
/// without `USAGE`, rather than returning `NULL`. A query resolving every row of a catalog
/// such as `tviews.registry` would then fail as a whole because of one relation the role
/// cannot read anyway. `CASE` decides first, so `to_regclass` never sees such a schema.
///
/// # Examples
///
/// ```rust
/// use fraiseql_db::usable_relation;
/// let sql = usable_relation("g.schema", "g.name");
/// assert!(sql.starts_with("CASE WHEN EXISTS"));
/// assert!(sql.ends_with("pg_catalog.format('%I.%I', g.schema, g.name)) END"));
/// ```
#[must_use]
pub fn usable_relation(schema: &str, name: &str) -> String {
    format!(
        "CASE WHEN EXISTS (SELECT 1 FROM pg_catalog.pg_namespace un \
         WHERE un.nspname = {schema} AND pg_catalog.has_schema_privilege(un.oid, 'USAGE')) \
         THEN pg_catalog.to_regclass(pg_catalog.format('%I.%I', {schema}, {name})) END"
    )
}

#[cfg(test)]
mod tests;
