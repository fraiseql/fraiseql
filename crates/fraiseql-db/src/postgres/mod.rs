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
/// (email)=(alice@example.com) already exists` — and the errors most likely to carry one
/// are exactly the ones that reach the client unsanitized: SQLSTATE classes 22 and 23 map
/// to `BAD_USER_INPUT` / `CONSTRAINT_VIOLATION`, which `ErrorSanitizer` passes through by
/// design. The primary message still names the constraint, which is the diagnostic.
///
/// Falls back to `Display` for client-side failures (connection closed, TLS, encode),
/// where there is no server error to read and `Display` is already descriptive.
pub(crate) fn pg_detail(e: &tokio_postgres::Error) -> String {
    e.as_db_error().map_or_else(|| e.to_string(), |d| d.message().to_string())
}
