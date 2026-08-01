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

#[cfg(test)]
mod tests;
