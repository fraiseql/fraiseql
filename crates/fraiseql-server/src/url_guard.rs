//! Startup-time validation that the database URL scheme is PostgreSQL.
//!
//! `fraiseql-server` and `fraiseql run` validate the URL scheme at startup.
//! FraiseQL is PostgreSQL-only: MySQL, SQLite and SQL Server support was
//! removed (G2 decision on #374; the dialects were never production-correct —
//! see #721/#799 and the issues they reference).
//!
//! Without this guard, pointing the binary at a URL whose scheme is unknown
//! would produce an opaque error from deep inside the driver layer
//! (connection refused, protocol mismatch, or worse). This module fails fast
//! at startup with a diagnostic naming the observed scheme so an operator can
//! correct the configuration. URLs for the *removed* engines get a dedicated
//! message naming the removal, so an operator upgrading across the de-scope
//! release learns what happened rather than guessing at a typo.

/// Operator-facing sentinel embedded in every guard error message.
///
/// Tests assert against this prefix so the diagnostic stays grep-able from
/// logs even if surrounding wording is reflowed.
pub const GUARD_MESSAGE_PREFIX: &str = "fraiseql-server: unsupported database URL";

/// Database schemes that the `fraiseql-server` binary can dispatch to.
///
/// The enum is exhaustive: every variant corresponds to an adapter that
/// `main()` / `fraiseql run` know how to construct. New schemes require an
/// explicit code change here and matching dispatch arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseScheme {
    /// `postgresql://` or `postgres://` — the only supported database.
    Postgres,
}

/// Parse the URL scheme from a database URL and return the matching
/// [`DatabaseScheme`].
///
/// # Errors
///
/// Returns `anyhow::Error` whose message starts with [`GUARD_MESSAGE_PREFIX`]
/// when the URL has no scheme, an empty scheme, or a scheme other than
/// PostgreSQL. The removed engines (`mysql://`, `sqlite://`, `sqlserver://`)
/// get a message naming the removal; anything else names the observed scheme
/// so the operator can correct their `fraiseql.toml` or `DATABASE_URL`.
pub fn parse_database_url(url: &str) -> anyhow::Result<DatabaseScheme> {
    // `split("://").next()` on a string with no separator returns the whole
    // string, so a bare `"postgres"` used to parse as a valid PostgreSQL URL and
    // the failure resurfaced later as the opaque driver error this guard exists
    // to prevent (#731). Require the separator.
    let scheme = url.split_once("://").map_or("", |(scheme, _)| scheme);
    match scheme {
        "postgresql" | "postgres" => Ok(DatabaseScheme::Postgres),
        removed @ ("mysql" | "sqlite" | "sqlserver") => anyhow::bail!(
            "{GUARD_MESSAGE_PREFIX} — {removed}:// support was removed: FraiseQL is \
             PostgreSQL-only (G2 decision on #374; the non-PostgreSQL dialects were never \
             production-correct, see #721/#799). Use a postgresql:// URL."
        ),
        "" => anyhow::bail!(
            "{GUARD_MESSAGE_PREFIX} — the URL has no scheme. Expected \
             postgresql:// or postgres://."
        ),
        other => anyhow::bail!(
            "{GUARD_MESSAGE_PREFIX} (observed URL scheme: {other:?}). The \
             fraiseql-server binary supports postgresql:// | postgres:// only."
        ),
    }
}

#[cfg(test)]
mod tests;
