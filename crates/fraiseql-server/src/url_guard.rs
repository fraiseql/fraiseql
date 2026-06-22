//! Startup-time validation that the database URL scheme matches a known
//! FraiseQL adapter, with feature-gated dispatch to the matching adapter.
//!
//! `fraiseql-server` and `fraiseql run` dispatch on the URL scheme at startup
//! to pick the right [`DatabaseAdapter`] implementation. PostgreSQL is always
//! available; MySQL, SQLite, and SQL Server are gated behind matching Cargo
//! features (`mysql`, `sqlite`, `sqlserver`).
//!
//! Without this guard, pointing the binary at a URL whose scheme is unknown
//! or whose adapter feature is not enabled would produce an opaque error from
//! deep inside the driver layer (connection refused, protocol mismatch, or
//! worse). This module fails fast at startup with a diagnostic naming the
//! observed scheme so an operator can correct the configuration or rebuild
//! with the required feature flag.
//!
//! [`DatabaseAdapter`]: fraiseql_core::db::DatabaseAdapter

/// Operator-facing sentinel embedded in every guard error message.
///
/// Tests assert against this prefix so the diagnostic stays grep-able from
/// logs even if surrounding wording is reflowed.
pub const GUARD_MESSAGE_PREFIX: &str = "fraiseql-server: unsupported database URL";

/// Database schemes that the `fraiseql-server` binary can dispatch to.
///
/// The enum is exhaustive: every variant corresponds to an adapter that
/// `main()` / `fraiseql run` know how to construct (subject to Cargo feature
/// flags). New schemes require an explicit code change here and matching
/// arms in `build_adapter` / `run_once` / `run_watch_loop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseScheme {
    /// `postgresql://` or `postgres://` — always available.
    Postgres,
    /// `mysql://` — requires `mysql` Cargo feature.
    MySql,
    /// `sqlite://` — requires `sqlite` Cargo feature. Supports direct-SQL
    /// Insert/Delete mutations; Update and custom / stored-procedure mutations
    /// are rejected at startup (see [`guard_sqlite_mutations`]).
    Sqlite,
    /// `sqlserver://` — requires `sqlserver` Cargo feature.
    SqlServer,
}

impl DatabaseScheme {
    /// The Cargo feature flag required to build the matching adapter into the
    /// server binary, or `None` for adapters that ship in the default feature
    /// set.
    #[must_use]
    pub const fn required_feature(self) -> Option<&'static str> {
        match self {
            Self::Postgres => None,
            Self::MySql => Some("mysql"),
            Self::Sqlite => Some("sqlite"),
            Self::SqlServer => Some("sqlserver"),
        }
    }
}

/// Refuse to start a SQLite-backed server when the compiled schema declares a
/// mutation that the SQLite (`DirectSql`) strategy cannot execute.
///
/// SQLite executes direct-SQL **Insert** and **Delete** mutations via the
/// executor (`MutationStrategy::DirectSql`). **Update** and custom /
/// stored-procedure (`fn_*`) mutations are not supported on SQLite. Without this
/// guard the server would start and then fail those requests at runtime; the
/// diagnostic below tells the operator why and names the first few offenders.
///
/// # Errors
///
/// Returns `anyhow::Error` when the schema contains an Update or custom mutation.
/// Callers should invoke this *before* constructing a SQLite adapter; the
/// PostgreSQL / MySQL / SQL Server paths must not call it.
pub fn guard_sqlite_mutations(
    schema: &fraiseql_core::schema::CompiledSchema,
) -> anyhow::Result<()> {
    use fraiseql_core::schema::MutationOperation;

    let unsupported: Vec<&str> = schema
        .mutations
        .iter()
        .filter(|m| {
            matches!(m.operation, MutationOperation::Update { .. } | MutationOperation::Custom)
        })
        .map(|m| m.name.as_str())
        .collect();
    if unsupported.is_empty() {
        return Ok(());
    }
    let sample: Vec<&str> = unsupported.iter().take(3).copied().collect();
    let suffix = if unsupported.len() > sample.len() {
        format!(", … (+{} more)", unsupported.len() - sample.len())
    } else {
        String::new()
    };
    anyhow::bail!(
        "fraiseql-server: SQLite supports only direct-SQL Insert/Delete mutations, but the \
         compiled schema declares {} Update or custom mutation(s) which cannot be executed \
         against a SQLite database. Use a postgresql:// / mysql:// / sqlserver:// URL, or remove \
         those mutations from the schema. Affected: {}{}",
        unsupported.len(),
        sample.join(", "),
        suffix,
    )
}

/// Parse the URL scheme from a database URL and return the matching
/// [`DatabaseScheme`].
///
/// # Errors
///
/// Returns `anyhow::Error` whose message starts with [`GUARD_MESSAGE_PREFIX`]
/// when the URL has no scheme, an empty scheme, or a scheme that is not one
/// of the supported four. The message names the observed scheme so the
/// operator can correct their `fraiseql.toml` or `DATABASE_URL`.
pub fn parse_database_url(url: &str) -> anyhow::Result<DatabaseScheme> {
    let scheme = url.split("://").next().unwrap_or("");
    match scheme {
        "postgresql" | "postgres" => Ok(DatabaseScheme::Postgres),
        "mysql" => Ok(DatabaseScheme::MySql),
        "sqlite" => Ok(DatabaseScheme::Sqlite),
        "sqlserver" => Ok(DatabaseScheme::SqlServer),
        "" => anyhow::bail!(
            "{GUARD_MESSAGE_PREFIX} — the URL has no scheme. Expected one of \
             postgresql:// | postgres:// | mysql:// | sqlite:// | sqlserver://."
        ),
        other => anyhow::bail!(
            "{GUARD_MESSAGE_PREFIX} (observed URL scheme: {other:?}). The \
             fraiseql-server binary dispatches on the URL scheme and supports \
             postgresql:// | postgres:// | mysql:// | sqlite:// | sqlserver:// \
             only."
        ),
    }
}

#[cfg(test)]
mod tests;
