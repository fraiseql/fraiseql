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
/// ```ignore
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
        .map(|part| format!("\"{}\"", part))
        .collect::<Vec<_>>()
        .join(".")
}

/// Quote a MySQL identifier.
///
/// MySQL uses backticks for identifiers. Schema-qualified names
/// (e.g., `database.table`) are split and quoted per component.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(quote_mysql_identifier("v_user"), "`v_user`");
/// assert_eq!(quote_mysql_identifier("mydb.v_user"), "`mydb`.`v_user`");
/// assert_eq!(
///     quote_mysql_identifier("catalog.schema.table"),
///     "`catalog`.`schema`.`table`"
/// );
/// ```
#[inline]
#[must_use]
pub fn quote_mysql_identifier(identifier: &str) -> String {
    identifier
        .split('.')
        .map(|part| format!("`{}`", part))
        .collect::<Vec<_>>()
        .join(".")
}

/// Quote a SQLite identifier.
///
/// SQLite uses double quotes for identifiers. Schema-qualified names
/// (e.g., `schema.table`) are split and quoted per component.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(quote_sqlite_identifier("v_user"), "\"v_user\"");
/// assert_eq!(quote_sqlite_identifier("main.v_user"), "\"main\".\"v_user\"");
/// assert_eq!(
///     quote_sqlite_identifier("catalog.schema.table"),
///     "\"catalog\".\"schema\".\"table\""
/// );
/// ```
#[inline]
#[must_use]
pub fn quote_sqlite_identifier(identifier: &str) -> String {
    identifier
        .split('.')
        .map(|part| format!("\"{}\"", part))
        .collect::<Vec<_>>()
        .join(".")
}

/// Quote a SQL Server identifier.
///
/// SQL Server uses square brackets for identifiers. Schema-qualified names
/// (e.g., `schema.table`) are split and quoted per component.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(quote_sqlserver_identifier("v_user"), "[v_user]");
/// assert_eq!(quote_sqlserver_identifier("dbo.v_user"), "[dbo].[v_user]");
/// assert_eq!(
///     quote_sqlserver_identifier("catalog.schema.table"),
///     "[catalog].[schema].[table]"
/// );
/// ```
#[inline]
#[must_use]
pub fn quote_sqlserver_identifier(identifier: &str) -> String {
    identifier
        .split('.')
        .map(|part| format!("[{}]", part))
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_simple_identifier() {
        assert_eq!(quote_postgres_identifier("v_user"), "\"v_user\"");
    }

    #[test]
    fn test_postgres_schema_qualified() {
        assert_eq!(quote_postgres_identifier("benchmark.v_user"), "\"benchmark\".\"v_user\"");
    }

    #[test]
    fn test_postgres_three_part_name() {
        assert_eq!(
            quote_postgres_identifier("catalog.schema.table"),
            "\"catalog\".\"schema\".\"table\""
        );
    }

    #[test]
    fn test_mysql_simple_identifier() {
        assert_eq!(quote_mysql_identifier("v_user"), "`v_user`");
    }

    #[test]
    fn test_mysql_schema_qualified() {
        assert_eq!(quote_mysql_identifier("mydb.v_user"), "`mydb`.`v_user`");
    }

    #[test]
    fn test_mysql_three_part_name() {
        assert_eq!(quote_mysql_identifier("catalog.schema.table"), "`catalog`.`schema`.`table`");
    }

    #[test]
    fn test_sqlite_simple_identifier() {
        assert_eq!(quote_sqlite_identifier("v_user"), "\"v_user\"");
    }

    #[test]
    fn test_sqlite_schema_qualified() {
        assert_eq!(quote_sqlite_identifier("main.v_user"), "\"main\".\"v_user\"");
    }

    #[test]
    fn test_sqlite_three_part_name() {
        assert_eq!(
            quote_sqlite_identifier("catalog.schema.table"),
            "\"catalog\".\"schema\".\"table\""
        );
    }

    #[test]
    fn test_sqlserver_simple_identifier() {
        assert_eq!(quote_sqlserver_identifier("v_user"), "[v_user]");
    }

    #[test]
    fn test_sqlserver_schema_qualified() {
        assert_eq!(quote_sqlserver_identifier("dbo.v_user"), "[dbo].[v_user]");
    }

    #[test]
    fn test_sqlserver_three_part_name() {
        assert_eq!(
            quote_sqlserver_identifier("catalog.schema.table"),
            "[catalog].[schema].[table]"
        );
    }
}
