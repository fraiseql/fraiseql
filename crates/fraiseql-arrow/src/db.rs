//! Database adapter trait for Arrow Flight service.
//!
//! This module defines a minimal database adapter interface for the Arrow Flight
//! layer: raw SQL reads, and the one atomic write path a gated `Upload` needs.
//!
//! # Note
//!
//! This trait is simpler than `fraiseql_core::db::DatabaseAdapter` and only includes
//! the methods needed for Arrow Flight streaming. In fraiseql-server, a wrapper
//! can implement both traits by delegating to the core adapter.

use std::collections::HashMap;

use async_trait::async_trait;

/// Error type for database operations.
///
/// This is a simplified error type that can be created from various
/// database drivers without requiring fraiseql-core dependencies.
#[derive(Debug, Clone)]
pub struct DatabaseError {
    message: String,
}

impl DatabaseError {
    /// Create a new database error.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DatabaseError {}

/// Result type for database operations.
pub type DatabaseResult<T> = Result<T, DatabaseError>;

/// Arrow Flight-specific database adapter for executing raw SQL queries.
///
/// This trait abstracts over different database backends (PostgreSQL, MySQL, SQLite, etc.)
/// and provides a minimal interface for executing raw SQL and returning results as JSON.
/// It is intentionally simpler than `fraiseql_db::DatabaseAdapter` — only the methods
/// needed for Arrow Flight streaming are required.
///
/// # Why a separate trait?
///
/// `ArrowDatabaseAdapter` carries only what the Flight layer needs, so a consumer can
/// back Flight with something that is not a full `fraiseql_db::DatabaseAdapter`;
/// `fraiseql-server` wraps a core adapter to satisfy both traits.
///
/// (This trait's separateness is **not** about avoiding a dependency: `fraiseql-arrow`
/// does depend on `fraiseql-core`, and the Flight handlers use `SecurityContext`
/// directly. The module docs said otherwise until #953.)
///
/// # Example
///
/// ```no_run
/// // Requires: a running database and an implementation of this trait.
/// use std::collections::HashMap;
/// use fraiseql_arrow::db::{ArrowDatabaseAdapter, DatabaseError, DatabaseResult};
///
/// struct MyAdapter { /* database connection */ }
///
/// #[async_trait::async_trait]
/// impl ArrowDatabaseAdapter for MyAdapter {
///     async fn execute_raw_query(
///         &self,
///         sql: &str,
///     ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>> {
///         // Execute SQL and return rows as JSON maps
///         panic!("connect to database and execute: {}", sql)
///     }
/// }
/// ```
// Reason: used as dyn Trait (Arc<dyn ArrowDatabaseAdapter>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait ArrowDatabaseAdapter: Send + Sync {
    /// Execute a raw SQL query and return rows as JSON objects.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL query string
    ///
    /// # Returns
    ///
    /// Vec of `HashMap` where each `HashMap` represents a row with column names as keys
    /// and column values as `serde_json::Value`
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError` if the query fails for any reason.
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>>;

    /// Execute one allow-listed Flight `Upload`: the client's rows **and** the
    /// `core.tb_entity_change_log` outbox rows that record them, in a **single
    /// transaction** (#953).
    ///
    /// # Why this is not `execute_raw_query`
    ///
    /// An `Upload` is a client-directed write that never passes through the mutation
    /// pipeline, so nothing else will write its Change Spine rows. Running the INSERT
    /// and the outbox write as two statements would let the rows commit while the
    /// outbox write failed — the split brain the Change Spine exists to prevent, and
    /// invisible to CDC and every observer downstream.
    ///
    /// # The default refuses, deliberately
    ///
    /// An adapter that has not implemented this **cannot** serve a gated Upload. The
    /// tempting default — run the statements one after another — is fail-open: it
    /// produces exactly the silent split brain above, on every adapter that forgot to
    /// override it. Refusing means an unimplemented adapter is loudly unable to
    /// Upload rather than quietly writing unrecorded rows.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError` when the adapter cannot write atomically (the default),
    /// or when the transaction fails — in which case **nothing** has been written.
    async fn execute_gated_upload(&self, upload: &GatedUpload<'_>) -> DatabaseResult<u64> {
        Err(DatabaseError::new(format!(
            "This database adapter cannot write a Flight Upload and its change-log row \
             atomically, so the Upload into '{}' is refused. An adapter must implement \
             ArrowDatabaseAdapter::execute_gated_upload before Upload can be allow-listed \
             for it.",
            upload.table
        )))
    }
}

/// One allow-listed Flight `Upload`, ready for the atomic write (#953).
///
/// Carries what the outbox row needs beyond the INSERT itself: who is writing, and
/// under which tenant. The `insert_sql` is already built and escaped by
/// `build_insert_query`; the adapter's job is to run it and the change-log write in
/// one transaction, not to re-derive it.
///
/// Intentionally **not** `#[non_exhaustive]`: this is the *call shape* of
/// [`ArrowDatabaseAdapter::execute_gated_upload`], and adding a field is a breaking
/// trait change regardless. Callers construct it with a struct literal so that
/// omitting a field — a new piece of provenance an outbox row must carry — is a hard
/// compile error at every call site rather than a silently defaulted NULL.
#[derive(Debug)]
pub struct GatedUpload<'a> {
    /// Target table — allow-listed by the operator, already verified by the caller.
    pub table:      &'a str,
    /// The multi-row `INSERT INTO "<table>" …` to execute.
    pub insert_sql: &'a str,
    /// Authenticated Flight session subject, recorded on the outbox row.
    pub user_id:    &'a str,
    /// Tenant this write belongs to, or `None` for a single-tenant deployment.
    pub tenant_id:  Option<&'a str>,
}
