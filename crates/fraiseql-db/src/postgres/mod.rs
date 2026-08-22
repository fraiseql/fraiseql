//! PostgreSQL database adapter.
//!
//! Provides connection pooling and query execution for PostgreSQL.

mod adapter;
mod introspector;
mod tls;
mod where_generator;

pub use adapter::{
    PoolPrewarmConfig, PostgresAdapter, ReadReplicaConfig, ReadReplicaPolicy, SearchPath,
};
pub use introspector::PostgresIntrospector;
pub use tls::{PostgresConnector, PostgresSslMode, PostgresTlsConfig};
pub use where_generator::{IndexedColumnsCache, PostgresWhereGenerator};

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
