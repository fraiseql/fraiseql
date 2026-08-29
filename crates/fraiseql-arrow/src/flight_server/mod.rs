//! FraiseQL Arrow Flight service implementation.
//!
//! This module provides the core gRPC service that handles Flight RPC calls,
//! enabling high-performance columnar data transfer for GraphQL queries.
//!
//! # Authentication
//!
//! **Authenticated Query Execution**:
//! - `handshake()` validates JWT tokens and returns 5-minute HMAC-SHA256 session tokens
//! - `do_get()`, `do_action()`, `do_put()`, `do_exchange()` require valid session tokens via
//!   "Authorization: Bearer" header
//! - Session tokens are validated using `validate_session_token()` helper
//! - Extracted tokens come from `extract_session_token()` helper
//! - `SecurityContext` created for each request to enable Row-Level Security (RLS)
//! - Admin operations (cache invalidation, schema refresh) require "admin" scope
//! - All failed auth attempts return descriptive errors guiding users to re-handshake if needed
//!
//! # Deferred Features (v2.1+)
//!
//! The following features are intentionally deferred to v2.1 to focus on core functionality:
//!
//! | Feature | Status | Reason |
//! |---------|--------|--------|
//! | Subscribe (`do_exchange`) | v2.1 | Real-time event streaming requires observer integration |
//! | `BulkExport` GetSchema/GetFlightInfo | — | Schema varies by table; use `do_get` directly |
//! | `GraphQLQuery` GetSchema/GetFlightInfo | — | No result schema is derived ahead of execution; read it from the `do_get` stream header |
//! | `RefreshSchemaRegistry` action | v2.1 | Requires safe schema update mechanism for running queries |
//! | Observer events (`do_get`) | v2.1 | Requires observer system integration |
//! | `PollFlightInfo` | ✅ v2.1 | Synchronous: delegates to `get_flight_info`, returns `progress=1.0` |
//! | Zero-copy Arrow conversion | v2.1 | Significant complexity for moderate performance gain |
//!
//! All deferred features return `Status::unimplemented()` or descriptive error messages
//! indicating they will be available in a future release.

mod auth;
mod convert;
mod handlers;
mod service;
#[cfg(test)]
mod tests;

use std::{pin::Pin, sync::Arc};

use arrow_flight::{ActionType, FlightData, FlightInfo, HandshakeResponse, PutResult};
use async_trait::async_trait;
use fraiseql_core::security::OidcValidator;
use futures::Stream; // Stream required for type aliases
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tonic::{Code, Status};

// Re-export auth helpers for use across submodules
pub(crate) use self::auth::{
    create_session_token, extract_session_token, map_security_error_to_status,
    validate_session_token,
};
// Public under `testing` so the executed-SQL integration suite (#715,
// tests/insert_sql_pg.rs) can run the generated INSERT against real Postgres.
#[cfg(feature = "testing")]
pub use self::convert::build_insert_query;
#[cfg(not(feature = "testing"))]
pub(crate) use self::convert::build_insert_query;
#[cfg(any(test, feature = "testing"))]
pub(crate) use self::convert::execute_placeholder_query;
// Re-export convert functions for use across submodules
pub(crate) use self::convert::{
    build_optimized_sql, decode_flight_data_to_batch, decode_upload_batch,
    encode_json_to_arrow_batch, record_batch_to_flight_data, schema_to_flight_data,
};
use crate::{
    cache::QueryCache, db::ArrowDatabaseAdapter, event_storage::ArrowEventStorage,
    metadata::SchemaRegistry, subscription::SubscriptionManager,
};

/// Trait for executing GraphQL queries with security context (RLS filtering).
///
/// This trait abstracts over the generic `Executor<A>` type (where `A` is the database adapter),
/// allowing `FraiseQLFlightService` to execute queries without knowing the specific database
/// adapter type.
///
/// **Architecture Note:**
/// The Executor in fraiseql-core is generic over the database adapter type A.
/// This trait provides a type-erased interface that:
/// 1. Accepts GraphQL queries as strings
/// 2. Applies Row-Level Security (RLS) policies based on `SecurityContext`
/// 3. Returns JSON results that can be converted to Arrow `RecordBatches`
// Reason: used as dyn Trait (Arc<dyn QueryExecutor>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait QueryExecutor: Send + Sync {
    /// Execute a GraphQL query with security context (RLS filtering).
    ///
    /// # Arguments
    /// * `query` - GraphQL query string
    /// * `variables` - Optional GraphQL variables as JSON
    /// * `security_context` - Security context from `fraiseql_core` for RLS policy evaluation
    ///
    /// # Returns
    /// * `Ok(serde_json::Value)` - JSON result from query execution
    /// * `Err(FraiseQLError)` - the **typed** failure
    ///
    /// # Why typed (#1201)
    ///
    /// This used to be `Err(String)`, and the stringification was where the
    /// Flight transport's error classification died: by the time
    /// `execute_graphql_query` saw the failure there was nothing left to match
    /// on, so every one of them — a query naming a field the schema no longer
    /// has, an unauthenticated caller, an exhausted quota — became gRPC
    /// `INTERNAL` (13). gRPC clients and proxies treat `INTERNAL` as a server
    /// fault and retry it, and a retried validation error can never succeed.
    ///
    /// [`fraiseql_core::error::FraiseQLError::status_code`] is the classification the HTTP
    /// transport already routes on, so carrying the error here lets both transports answer
    /// from one source rather than two that can drift.
    async fn execute_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &fraiseql_core::security::SecurityContext,
    ) -> Result<serde_json::Value, fraiseql_core::error::FraiseQLError>;
}

/// The gRPC status a failure should be reported as, derived from the same
/// classification the HTTP transport routes on (#1201).
///
/// Derived rather than re-matched on purpose. A second variant-by-variant match
/// is a second classification, and two classifications drift — which is how the
/// Flight transport came to report a client's parse error as a server fault
/// while HTTP called the same error `PARSE_ERROR`. Deriving also means a new
/// `FraiseQLError` variant arrives here already classified, and
/// [`fraiseql_core::error::FraiseQLError::status_code`] fails closed on one it does not know
/// (`500`), so an unmapped variant is reported as `INTERNAL` — conservative, and the same
/// answer HTTP gives.
#[must_use]
pub const fn grpc_code_for(error: &fraiseql_core::error::FraiseQLError) -> Code {
    match error.status_code() {
        400 => Code::InvalidArgument,
        401 => Code::Unauthenticated,
        403 => Code::PermissionDenied,
        404 => Code::NotFound,
        // A timeout and a cancellation are both "the deadline ran out".
        408 => Code::DeadlineExceeded,
        // `Aborted`, not `AlreadyExists`: the latter is specifically a creation
        // collision, while `Conflict` here is the general "retry at a higher
        // level" case gRPC gives `Aborted` for.
        409 => Code::Aborted,
        429 => Code::ResourceExhausted,
        501 => Code::Unimplemented,
        503 => Code::Unavailable,
        // 500 and anything unmapped. Server-side, and the only class for which
        // `INTERNAL` was ever the right answer.
        _ => Code::Internal,
    }
}

/// Render a typed failure as the gRPC `Status` a client should act on (#1201).
#[must_use]
pub fn grpc_status(context: &str, error: &fraiseql_core::error::FraiseQLError) -> Status {
    Status::new(grpc_code_for(error), format!("{context}: {error}"))
}

pub(crate) type HandshakeStream =
    Pin<Box<dyn Stream<Item = std::result::Result<HandshakeResponse, Status>> + Send>>;
pub(crate) type FlightInfoStream =
    Pin<Box<dyn Stream<Item = std::result::Result<FlightInfo, Status>> + Send>>;
pub(crate) type FlightDataStream =
    Pin<Box<dyn Stream<Item = std::result::Result<FlightData, Status>> + Send>>;
pub(crate) type PutResultStream =
    Pin<Box<dyn Stream<Item = std::result::Result<PutResult, Status>> + Send>>;
pub(crate) type ActionResultStream =
    Pin<Box<dyn Stream<Item = std::result::Result<arrow_flight::Result, Status>> + Send>>;
pub(crate) type ActionTypeStream =
    Pin<Box<dyn Stream<Item = std::result::Result<ActionType, Status>> + Send>>;

/// FraiseQL Arrow Flight service implementation.
///
/// This is the core gRPC service that handles Flight RPC calls.
/// It will be extended to support additional data fetching and streaming modes.
///
/// # Example
///
/// ```no_run
/// use fraiseql_arrow::flight_server::FraiseQLFlightService;
/// use tonic::transport::Server;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let service = FraiseQLFlightService::new();
///     let addr = "0.0.0.0:50051".parse()?;
///
///     Server::builder()
///         .add_service(service.into_server())
///         .serve(addr)
///         .await?;
///
///     Ok(())
/// }
/// ```
pub struct FraiseQLFlightService {
    /// Schema registry for pre-compiled Arrow views (Arc for safe sharing in async contexts)
    pub(crate) schema_registry: Arc<SchemaRegistry>,
    /// Optional database adapter for executing real queries.
    /// If None, placeholder queries are used (for testing/development).
    pub(crate) db_adapter: Option<Arc<dyn ArrowDatabaseAdapter>>,
    /// Optional query executor for executing GraphQL queries with RLS.
    /// Uses trait object to abstract over generic `Executor<A>` type.
    pub(crate) executor: Option<Arc<dyn QueryExecutor>>,
    /// Optional query result cache for improving throughput on repeated queries
    pub(crate) cache: Option<Arc<QueryCache>>,
    /// Optional security context for authenticated requests.
    /// Stores session information from successful handshake.
    pub(crate) security_context: Option<SecurityContext>,
    /// OIDC validator for JWT authentication during handshake
    pub(crate) oidc_validator: Option<Arc<OidcValidator>>,
    /// Optional event storage for historical observer event queries
    pub(crate) event_storage: Option<Arc<dyn ArrowEventStorage>>,
    /// Subscription manager for real-time event streaming
    pub(crate) subscription_manager: Arc<SubscriptionManager>,
    /// Allow clients to submit raw SQL via `BatchedQueries` tickets.
    ///
    /// **SECURITY**: Disabled by default. Enabling this allows authenticated clients
    /// to execute arbitrary SQL, which bypasses RLS and query-level authorization.
    /// Only enable for trusted internal tooling with explicit intent.
    pub(crate) allow_raw_sql: bool,
    /// Tables an authenticated client may `BulkExport` via Flight tickets.
    ///
    /// **SECURITY**: `None` (the default) disables `BulkExport` entirely. `BulkExport`
    /// runs `SELECT * FROM "<table>"` and applies **no per-user RLS filtering** (the
    /// `SecurityContext` is recorded for audit only), so it is fail-closed: a request is
    /// rejected unless its table is explicitly allow-listed here via
    /// `with_bulk_export_tables`. Only allow-list tables whose full contents every
    /// `BulkExport`-capable client is permitted to read.
    pub(crate) bulk_export_allowed_tables: Option<std::collections::HashSet<String>>,
    /// Tables an authenticated client may `Upload` into via `do_exchange` (#953).
    ///
    /// **SECURITY**: `None` (the default) disables `Upload` entirely. An Upload is a
    /// raw client-directed INSERT — it does not pass through the mutation pipeline, so
    /// it carries no operation authorizer and no field RBAC, and the target table is
    /// named by the *client*. Left ungated, a caller holding any valid Flight session
    /// token could write to every table the connection role can write, `_system`, audit
    /// and outbox tables included. It is therefore fail-closed: a request is refused
    /// unless its table is explicitly allow-listed here via
    /// [`with_upload_tables`](FraiseQLFlightService::with_upload_tables). Only
    /// allow-list tables every `Upload`-capable client is permitted to write.
    pub(crate) upload_allowed_tables: Option<std::collections::HashSet<String>>,
    /// HMAC-SHA256 secret used to sign and verify Flight session tokens.
    ///
    /// Read once at service construction from `FLIGHT_SESSION_SECRET` environment
    /// variable, or supplied via [`FraiseQLFlightService::with_session_secret`].
    pub(crate) session_secret: Option<String>,
    /// Semaphore limiting the number of concurrent `do_get` streams.
    ///
    /// `try_acquire()` is used (non-blocking): when all permits are taken, new
    /// `do_get` calls immediately return `Status::resource_exhausted` instead of
    /// queuing indefinitely. Default capacity: 50.
    pub(crate) stream_semaphore: Arc<Semaphore>,
}

/// Security context for authenticated Flight requests.
/// Stores session information from JWT validation during handshake.
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Session token returned from handshake
    pub session_token: String,
    /// User ID extracted from JWT
    pub user_id:       String,
    /// Token expiration time
    pub expiration:    Option<u64>,
}

/// Session token claims for JWT validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionTokenClaims {
    /// Subject (user ID)
    pub(crate) sub:          String,
    /// Expiration time (Unix timestamp)
    pub(crate) exp:          i64,
    /// Issued at (Unix timestamp)
    pub(crate) iat:          i64,
    /// Scopes from original token
    pub(crate) scopes:       Vec<String>,
    /// Session type marker
    pub(crate) session_type: String,
}
