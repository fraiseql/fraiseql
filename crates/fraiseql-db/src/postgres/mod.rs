//! PostgreSQL database adapter.
//!
//! Provides connection pooling and query execution for PostgreSQL.

mod adapter;
mod introspector;
mod server_version;
mod tls;
mod where_generator;

#[cfg(feature = "test-postgres")]
#[doc(hidden)]
pub use adapter::test_hooks;
pub use adapter::{
    HnswIterativeScan, IvfflatIterativeScan, PoolPrewarmConfig, PostgresAdapter, ReadReplicaConfig,
    ReadReplicaPolicy, SearchPath, VectorScanConfig,
};
pub use introspector::{IndexInfo, PostgresIntrospector};
pub use server_version::{
    MINIMUM_SERVER_VERSION_NUM, check_server_version, require_supported_server,
};
pub use tls::{PostgresConnector, PostgresSslMode, PostgresTlsConfig};
pub use where_generator::PostgresWhereGenerator;

/// The human-readable half of a `tokio_postgres::Error` (#888).
///
/// `Display` for a server-side failure is the literal string `db error`: the message
/// that names the relation, the column or the constraint lives on the `DbError` behind
/// [`as_db_error`](tokio_postgres::Error::as_db_error). Every `map_err` on a query path
/// formats through this instead of `{e}`, so `Query execution failed: db error` becomes
/// `Query execution failed: relation "v_order" does not exist`.
///
/// Deliberately the primary message only, **not** `DbError`'s own `Display` (which
/// appends `DETAIL:` and `HINT:`). Postgres puts row values in `DETAIL` — `Key
/// (email)=(alice@example.com) already exists` — and keeping row values and internal
/// surrogate keys out of an error string is worth doing regardless of what happens
/// downstream. The primary message still names the constraint, which is the diagnostic.
///
/// This paragraph used to add that classes 22 and 23 "reach the client unsanitized …
/// which `ErrorSanitizer` passes through by design". That was an accurate description of
/// a defect, read as a specification. Since #1153 the sanitizer routes on **provenance**:
/// `BAD_USER_INPUT` and `CONSTRAINT_VIOLATION` carry database-written text and are
/// replaced like any other database message when sanitization is enabled. This string is
/// still the server-side log line, so it must stay diagnostic — but do not treat it as
/// client-visible.
///
/// Falls back to `Display` for client-side failures (connection closed, TLS, encode),
/// where there is no server error to read and `Display` is already descriptive.
pub(crate) fn pg_detail(e: &tokio_postgres::Error) -> String {
    e.as_db_error().map_or_else(|| e.to_string(), |d| d.message().to_string())
}

/// The [`FraiseQLError::Database`] a driver error becomes: `message`, the error's SQLSTATE,
/// and the constraint it names (#1531).
///
/// One conversion for every site, so none can drop the constraint again. PostgreSQL reports
/// `CONSTRAINT NAME`, `SCHEMA NAME`, `TABLE NAME` and `COLUMN NAME` with an integrity-constraint
/// violation (a unique index under its index name; a not-null violation names its column
/// only); they are schema identifiers, never row values.
/// The `DETAIL` (which carries the row's values) is not read.
pub(crate) fn database_error(
    message: String,
    e: &tokio_postgres::Error,
) -> fraiseql_error::FraiseQLError {
    let constraint = e.as_db_error().and_then(|d| {
        (d.constraint().is_some() || d.column().is_some()).then(|| {
            Box::new(fraiseql_error::ConstraintViolation {
                name:   d.constraint().map(str::to_string),
                schema: d.schema().map(str::to_string),
                table:  d.table().map(str::to_string),
                column: d.column().map(str::to_string),
            })
        })
    });
    fraiseql_error::FraiseQLError::Database {
        message,
        sql_state: e.code().map(|c| c.code().to_string()),
        constraint,
    }
}
