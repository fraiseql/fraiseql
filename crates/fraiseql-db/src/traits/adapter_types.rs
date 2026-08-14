//! Supporting types for the `DatabaseAdapter` trait family.
//!
//! Extracted from the main `traits` module to keep the trait definition file
//! focused on method signatures.

use std::{pin::Pin, sync::Arc};

use fraiseql_error::Result;
use futures::Stream;

use super::DatabaseAdapter;
use crate::{
    types::{
        ColumnValue, DatabaseType, JsonbValue,
        sql_hints::{OrderByClause, SqlProjectionHint},
    },
    where_clause::WhereClause,
};

/// Result from a relay pagination query, containing rows and an optional total count.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RelayPageResult {
    /// The page of JSONB rows (already trimmed to the requested page size).
    pub rows:        Vec<JsonbValue>,
    /// Total count of matching rows (only populated when requested via `include_total_count`).
    pub total_count: Option<u64>,
}

impl RelayPageResult {
    /// Creates a new `RelayPageResult`.
    #[must_use]
    pub const fn new(rows: Vec<JsonbValue>, total_count: Option<u64>) -> Self {
        Self { rows, total_count }
    }

    /// Returns a reference to the page of JSONB rows.
    #[must_use]
    pub fn rows(&self) -> &[JsonbValue] {
        &self.rows
    }

    /// Consumes the result and returns the rows.
    #[must_use]
    pub fn into_rows(self) -> Vec<JsonbValue> {
        self.rows
    }

    /// Returns the total count of matching rows, if requested.
    #[must_use]
    pub const fn total_count(&self) -> Option<u64> {
        self.total_count
    }
}

/// A snapshot of an adapter's query-result cache, for the operator surface.
///
/// Exists because the admin API could see only the Arrow Flight query cache, and so
/// answered "Cache is not configured" for a server whose sibling endpoint reported the
/// query result cache active in the same breath (#941). An adapter that caches nothing
/// returns `None` from
/// [`DatabaseAdapter::result_cache_stats`](super::DatabaseAdapter::result_cache_stats)
/// — "no such cache" and "a cache holding zero entries" are different answers and the
/// endpoint must not conflate them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultCacheStats {
    /// Entries currently held.
    pub entries:       usize,
    /// Lookups served from cache.
    pub hits:          u64,
    /// Lookups that reached the database.
    pub misses:        u64,
    /// Invalidations performed (mutation sweeps, entity evictions, admin clears).
    pub invalidations: u64,
    /// Estimated memory held, in bytes.
    pub memory_bytes:  usize,
    /// Configured default entry TTL, in seconds.
    pub ttl_seconds:   u64,
    /// Configured entry ceiling.
    pub max_entries:   usize,
}

/// Database capabilities and feature support.
///
/// Describes what features a database backend supports, allowing the runtime
/// to adapt behavior based on database limitations.
#[derive(Debug, Clone, Copy)]
pub struct DatabaseCapabilities {
    /// Database type.
    pub database_type: DatabaseType,

    /// Supports locale-specific collations.
    pub supports_locale_collation: bool,

    /// Requires custom collation registration.
    pub requires_custom_collation: bool,

    /// Recommended collation provider.
    pub recommended_collation: Option<&'static str>,
}

impl DatabaseCapabilities {
    /// Create capabilities from database type.
    #[must_use]
    pub const fn from_database_type(db_type: DatabaseType) -> Self {
        match db_type {
            DatabaseType::PostgreSQL => Self {
                database_type:             db_type,
                supports_locale_collation: true,
                requires_custom_collation: false,
                recommended_collation:     Some("icu"),
            },
        }
    }

    /// Get collation strategy description.
    #[must_use]
    pub const fn collation_strategy(&self) -> &'static str {
        match self.database_type {
            DatabaseType::PostgreSQL => "ICU collations (locale-specific)",
        }
    }
}

/// Strategy used by an adapter for executing mutations.
///
/// The PostgreSQL adapter uses stored database functions (`FunctionCall`).
/// `DirectSql` remains for adapters that generate INSERT/UPDATE/DELETE SQL
/// directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MutationStrategy {
    /// Mutations execute via stored database functions (`SELECT * FROM fn_create_user($1, $2)`).
    FunctionCall,
    /// Mutations execute via direct SQL (`INSERT INTO ... RETURNING *`).
    DirectSql,
}

/// The kind of direct mutation operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DirectMutationOp {
    /// `INSERT INTO ... RETURNING *`
    Insert,
    /// `UPDATE ... SET ... WHERE pk = ? RETURNING *`
    Update,
    /// `DELETE FROM ... WHERE pk = ? RETURNING *`
    Delete,
}

/// Context for a direct SQL mutation (used by `DirectSql` strategy adapters).
///
/// All field references are borrowed from the caller to avoid allocation.
#[derive(Debug)]
pub struct DirectMutationContext<'a> {
    /// The mutation operation to perform.
    pub operation:      DirectMutationOp,
    /// Target table name (e.g., `"users"`).
    pub table:          &'a str,
    /// Client-supplied column names (in bind order).
    pub columns:        &'a [String],
    /// All bind values: client values first, then injected values.
    pub values:         &'a [serde_json::Value],
    /// Server-injected column names (e.g., RLS tenant columns), appended after client columns.
    pub inject_columns: &'a [String],
    /// GraphQL return type name (e.g., `"User"`), used in the mutation response envelope.
    pub return_type:    &'a str,
}

/// A typed cursor value for keyset (relay) pagination.
///
/// The cursor type is determined at compile time by `QueryDefinition::relay_cursor_type`
/// and used at runtime to choose the correct SQL comparison and cursor
/// encoding/decoding path.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CursorValue {
    /// BIGINT primary key cursor (default, backward-compatible).
    Int64(i64),
    /// UUID cursor — bound as text and cast to `uuid` in SQL.
    Uuid(String),
}

/// Parameters for an `execute_with_projection_arc` call (F043).
///
/// Consolidates the six positional parameters of the projection-execution path
/// into a single borrowed struct so adapters and callers cannot reorder them
/// by mistake. All fields borrow from the caller; the struct is constructed
/// per-request on the stack and discarded after the call.
///
/// # Field ordering
///
/// The field order mirrors a SQL `SELECT … FROM view WHERE … ORDER BY … LIMIT
/// … OFFSET …` clause, top-to-bottom, so that reading the struct mirrors the
/// query it produces.
///
/// Intentionally **not** `#[non_exhaustive]`: the struct is the *call shape*
/// of the trait method and any field addition is a breaking trait change
/// regardless. Callers construct it with a struct literal so that omitting a
/// field is a hard compile error.
#[derive(Debug, Clone, Copy)]
pub struct ProjectionRequest<'a> {
    /// View or table name (e.g. `"v_user"`).
    pub view:         &'a str,
    /// Projection hint (`SELECT` shape). `None` falls back to `SELECT data`.
    pub projection:   Option<&'a SqlProjectionHint>,
    /// WHERE clause AST. `None` means no filter.
    pub where_clause: Option<&'a WhereClause>,
    /// ORDER BY clauses. Empty slice (or `None`) means unordered.
    pub order_by:     Option<&'a [OrderByClause]>,
    /// Row limit. `None` means no limit.
    pub limit:        Option<u32>,
    /// Row offset. `None` means no offset.
    pub offset:       Option<u32>,
}

impl<'a> ProjectionRequest<'a> {
    /// Construct a `ProjectionRequest` from a view name with no filters,
    /// pagination or projection. Useful for tests and simple table scans.
    #[must_use]
    pub const fn new(view: &'a str) -> Self {
        Self {
            view,
            projection: None,
            where_clause: None,
            order_by: None,
            limit: None,
            offset: None,
        }
    }
}

/// JSONB rows delivered as the database produces them, rather than collected
/// into a `Vec` first (#958).
///
/// Returned by
/// [`DatabaseAdapter::stream_with_projection`](super::DatabaseAdapter::stream_with_projection).
/// Boxed because the trait is used through `dyn DatabaseAdapter`, and `Send`
/// because a response body is polled from whichever task the transport runs on.
///
/// # What the caller is holding
///
/// A stream is not a `Vec` that arrives late. On PostgreSQL it is an open portal
/// inside an open transaction, which means the read holds a pooled connection for
/// as long as the stream is alive. Consume it promptly, and drop it as soon as the
/// response is finished — dropping is what closes the transaction and returns the
/// connection.
pub type JsonbRowStream = Pin<Box<dyn Stream<Item = Result<JsonbValue>> + Send>>;

/// The column-shaped counterpart of [`JsonbRowStream`], for transports that
/// decode typed columns rather than a JSONB document (gRPC).
///
/// Returned by
/// [`DatabaseAdapter::stream_row_query`](super::DatabaseAdapter::stream_row_query).
/// The same connection-lifetime caveat applies.
pub type ColumnRowStream = Pin<Box<dyn Stream<Item = Result<Vec<ColumnValue>>> + Send>>;

/// Type alias for boxed dynamic database adapters.
///
/// Used to store database adapters without generic type parameters in collections
/// or struct fields. The adapter type is determined at runtime.
///
/// # Example
///
/// ```ignore
/// let adapter: BoxDatabaseAdapter = Box::new(postgres_adapter);
/// ```
pub type BoxDatabaseAdapter = Box<dyn DatabaseAdapter>;

/// Type alias for arc-wrapped dynamic database adapters.
///
/// Used for thread-safe, reference-counted storage of adapters in shared state.
///
/// # Example
///
/// ```ignore
/// let adapter: ArcDatabaseAdapter = Arc::new(postgres_adapter);
/// ```
pub type ArcDatabaseAdapter = Arc<dyn DatabaseAdapter>;
