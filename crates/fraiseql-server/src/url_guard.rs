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
    /// `sqlite://` — requires `sqlite` Cargo feature. Read-only (no
    /// `SupportsMutations` impl); schemas with mutations are rejected at
    /// startup.
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

/// Refuse to start a SQLite-backed server when the compiled schema declares
/// any mutations.
///
/// `SqliteAdapter` deliberately does not implement `SupportsMutations` (the
/// adapter is read-only by design — see `crates/fraiseql-db/src/sqlite/`).
/// Without this guard the server would start and then fail every mutation
/// request at runtime; the diagnostic below tells the operator why and names
/// the first few offending mutations.
///
/// # Errors
///
/// Returns `anyhow::Error` when the schema contains one or more mutations.
/// Callers should invoke this *before* constructing a SQLite adapter; the
/// PostgreSQL / MySQL / SQL Server paths must not call it.
pub fn guard_sqlite_mutations(
    schema: &fraiseql_core::schema::CompiledSchema,
) -> anyhow::Result<()> {
    if schema.mutations.is_empty() {
        return Ok(());
    }
    let sample: Vec<&str> = schema.mutations.iter().take(3).map(|m| m.name.as_str()).collect();
    let suffix = if schema.mutations.len() > sample.len() {
        format!(", … (+{} more)", schema.mutations.len() - sample.len())
    } else {
        String::new()
    };
    anyhow::bail!(
        "fraiseql-server: SQLite is a read-only runtime adapter, but the compiled schema declares \
         {} mutation(s) which cannot be executed against a SQLite database. Use a postgresql:// / \
         mysql:// / sqlserver:// URL, or remove the mutations from the schema. Affected: {}{}",
        schema.mutations.len(),
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
mod tests {
    use super::*;

    #[test]
    fn accepts_postgresql_scheme() {
        assert_eq!(
            parse_database_url("postgresql://user@localhost/db").expect("accepted"),
            DatabaseScheme::Postgres,
        );
    }

    #[test]
    fn accepts_postgres_alias() {
        assert_eq!(
            parse_database_url("postgres://user@localhost/db").expect("accepted"),
            DatabaseScheme::Postgres,
        );
    }

    #[test]
    fn accepts_postgresql_with_query_string() {
        assert_eq!(
            parse_database_url("postgresql://user:pw@host:5432/db?sslmode=require")
                .expect("query-string parameters must not affect scheme parsing"),
            DatabaseScheme::Postgres,
        );
    }

    #[test]
    fn accepts_mysql_scheme() {
        assert_eq!(
            parse_database_url("mysql://localhost:3306/mydb").expect("accepted"),
            DatabaseScheme::MySql,
        );
    }

    #[test]
    fn accepts_sqlite_scheme() {
        assert_eq!(
            parse_database_url("sqlite://./mydb.db").expect("accepted"),
            DatabaseScheme::Sqlite,
        );
    }

    #[test]
    fn accepts_sqlserver_scheme() {
        assert_eq!(
            parse_database_url("sqlserver://localhost:1433").expect("accepted"),
            DatabaseScheme::SqlServer,
        );
    }

    #[test]
    fn rejects_unknown_scheme_with_clear_message() {
        let err = parse_database_url("redis://localhost:6379")
            .expect_err("redis:// is not a supported database scheme")
            .to_string();
        assert!(
            err.starts_with(GUARD_MESSAGE_PREFIX),
            "diagnostic must start with the operator-facing prefix: {err}"
        );
        assert!(err.contains("\"redis\""), "missing observed-scheme reproduction: {err}");
    }

    #[test]
    fn rejects_empty_string() {
        let err = parse_database_url("").expect_err("empty URL must be rejected").to_string();
        assert!(err.starts_with(GUARD_MESSAGE_PREFIX), "{err}");
    }

    #[test]
    fn rejects_url_without_scheme() {
        let err = parse_database_url("localhost:5432")
            .expect_err("URL without a scheme must be rejected")
            .to_string();
        assert!(err.contains("\"localhost:5432\""), "{err}");
    }

    #[test]
    fn required_feature_matrix() {
        assert_eq!(DatabaseScheme::Postgres.required_feature(), None);
        assert_eq!(DatabaseScheme::MySql.required_feature(), Some("mysql"));
        assert_eq!(DatabaseScheme::Sqlite.required_feature(), Some("sqlite"));
        assert_eq!(DatabaseScheme::SqlServer.required_feature(), Some("sqlserver"));
    }

    mod sqlite_guard {
        use fraiseql_core::schema::{CompiledSchema, MutationDefinition};

        use super::super::guard_sqlite_mutations;

        fn mutation(name: &str) -> MutationDefinition {
            MutationDefinition::new(name, "MutationResponse")
        }

        #[test]
        fn accepts_schema_with_no_mutations() {
            let schema = CompiledSchema::default();
            guard_sqlite_mutations(&schema).expect("read-only schema must be allowed on SQLite");
        }

        #[test]
        fn rejects_schema_with_mutations_and_names_them() {
            let mut schema = CompiledSchema::default();
            schema.mutations.push(mutation("createUser"));
            schema.mutations.push(mutation("deletePost"));

            let err = guard_sqlite_mutations(&schema)
                .expect_err("SQLite + mutations must be rejected at startup")
                .to_string();

            assert!(err.contains("SQLite is a read-only"), "missing read-only callout: {err}");
            assert!(err.contains("createUser"), "missing first mutation name: {err}");
            assert!(err.contains("deletePost"), "missing second mutation name: {err}");
            assert!(err.contains("2 mutation"), "missing mutation count: {err}");
        }

        #[test]
        fn truncates_long_mutation_lists_with_suffix() {
            let mut schema = CompiledSchema::default();
            for i in 0..5 {
                schema.mutations.push(mutation(&format!("m{i}")));
            }

            let err = guard_sqlite_mutations(&schema)
                .expect_err("any mutations must be rejected on SQLite")
                .to_string();

            assert!(err.contains("m0"), "missing first sample: {err}");
            assert!(err.contains("m2"), "missing third sample: {err}");
            assert!(!err.contains("m4"), "must truncate beyond sample window: {err}");
            assert!(err.contains("+2 more"), "missing overflow suffix: {err}");
        }
    }
}
