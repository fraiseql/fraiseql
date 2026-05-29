//! Startup-time validation that the database URL is a PostgreSQL URL.
//!
//! The `fraiseql-server` binary only supports PostgreSQL at runtime. The
//! non-PostgreSQL adapters (`MySqlAdapter`, `SqliteAdapter`,
//! `SqlServerAdapter`) exist in `fraiseql-db` behind feature flags and
//! are exercised by the SQL planner via `database_target` schema hints,
//! but they are not wired into the server binary's adapter factory.
//!
//! Without this guard, pointing the binary at a non-PG URL produces an
//! opaque error from deep inside `tokio-postgres` (connection refused,
//! protocol mismatch, or worse). This module fails fast at startup with
//! a diagnostic naming the observed scheme so an operator can correct
//! the configuration immediately.

/// Operator-facing sentinel embedded in the guard's error message.
///
/// Tests assert against this prefix so the diagnostic stays grep-able
/// from logs even if surrounding wording is reflowed.
pub const GUARD_MESSAGE_PREFIX: &str = "fraiseql-server binary supports only PostgreSQL at runtime";

/// Reject database URLs whose scheme is not `postgresql://` or `postgres://`.
///
/// # Errors
///
/// Returns `anyhow::Error` whose message starts with
/// [`GUARD_MESSAGE_PREFIX`] when the URL scheme is anything other than
/// `postgresql` or `postgres`. The message names the observed scheme so
/// the operator can correct their `fraiseql.toml` or `DATABASE_URL`.
pub fn validate_postgres_url(url: &str) -> anyhow::Result<()> {
    let scheme = url.split("://").next().unwrap_or("");
    if matches!(scheme, "postgresql" | "postgres") {
        return Ok(());
    }
    anyhow::bail!(
        "{GUARD_MESSAGE_PREFIX} (observed URL scheme: {scheme:?}). The fraiseql-db crate \
         ships MySQL/SQLite/SQL Server adapters but they are not yet wired into the server \
         binary; multi-database runtime support is tracked separately. For now: use a \
         postgresql:// URL, or build a custom server that uses fraiseql_server::Server with \
         the adapter you want."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_postgresql_scheme() {
        validate_postgres_url("postgresql://user@localhost/db")
            .expect("postgresql:// must be accepted");
    }

    #[test]
    fn accepts_postgres_alias() {
        validate_postgres_url("postgres://user@localhost/db")
            .expect("postgres:// (alias) must be accepted");
    }

    #[test]
    fn accepts_postgresql_with_query_string() {
        validate_postgres_url("postgresql://user:pw@host:5432/db?sslmode=require")
            .expect("query-string parameters must not affect scheme validation");
    }

    #[test]
    fn rejects_mysql_url_with_clear_message() {
        let err = validate_postgres_url("mysql://localhost:3306/mydb")
            .expect_err("mysql:// must be rejected")
            .to_string();
        assert!(
            err.starts_with(GUARD_MESSAGE_PREFIX),
            "diagnostic must start with the operator-facing prefix: {err}"
        );
        assert!(err.contains("\"mysql\""), "missing observed-scheme reproduction: {err}");
    }

    #[test]
    fn rejects_sqlite_url() {
        let err = validate_postgres_url("sqlite://./mydb.db")
            .expect_err("sqlite:// must be rejected")
            .to_string();
        assert!(err.contains("\"sqlite\""), "missing observed-scheme: {err}");
    }

    #[test]
    fn rejects_sqlserver_url() {
        let err = validate_postgres_url("sqlserver://localhost:1433")
            .expect_err("sqlserver:// must be rejected")
            .to_string();
        assert!(err.contains("\"sqlserver\""), "missing observed-scheme: {err}");
    }

    #[test]
    fn rejects_empty_string() {
        // No `://` present — split fallback returns the whole string.
        validate_postgres_url("").expect_err("empty URL must be rejected");
    }

    #[test]
    fn rejects_url_without_scheme() {
        // A bare host:port string is not a valid URL; reject it.
        validate_postgres_url("localhost:5432")
            .expect_err("URL without a scheme must be rejected");
    }
}
