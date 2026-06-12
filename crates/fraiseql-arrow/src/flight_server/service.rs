//! `FraiseQLFlightService` construction and state management methods.

use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow_flight::{FlightData, flight_service_server::FlightServiceServer};
use chrono::Utc;
use fraiseql_core::security::OidcValidator;
use futures::Stream;
use tokio::sync::{Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Response, Status};
use tracing::{debug, info, warn};

#[cfg(any(test, feature = "testing"))]
use super::execute_placeholder_query;
use super::{
    ActionResultStream, FlightDataStream, FraiseQLFlightService, QueryExecutor, SecurityContext,
    build_optimized_sql, encode_json_to_arrow_batch, record_batch_to_flight_data,
    schema_to_flight_data,
};
use crate::{
    cache::QueryCache,
    convert::{ConvertConfig, RowToArrowConverter},
    db::ArrowDatabaseAdapter,
    db_convert::convert_db_rows_to_arrow,
    event_storage::ArrowEventStorage,
    export::{BulkExporter, ExportFormat},
    metadata::SchemaRegistry,
    subscription::SubscriptionManager,
};

/// Split converted rows into Arrow record batches of at most `batch_size` rows.
///
/// Floors `batch_size` at 1: `slice::chunks(0)` panics, and the one call site
/// that derives the batch size from a client ticket could pass `limit = 0`
/// (audit H38, a remotely-triggerable abort in `do_get`). Routing every chunk
/// loop through this helper makes a zero-sized chunk impossible at any site.
fn chunk_into_batches(
    arrow_rows: &[Vec<Option<crate::convert::Value>>],
    converter: &RowToArrowConverter,
    batch_size: usize,
) -> std::result::Result<Vec<arrow::array::RecordBatch>, arrow::error::ArrowError> {
    arrow_rows
        .chunks(batch_size.max(1))
        .map(|chunk| converter.convert_batch(chunk.to_vec()))
        .collect()
}

/// Read `FLIGHT_SESSION_SECRET` from the environment once.
///
/// Returns `None` (and logs a warning) if the variable is unset or empty.
/// This is called at service construction so every request reuses the cached value.
fn read_flight_session_secret() -> Option<String> {
    match std::env::var("FLIGHT_SESSION_SECRET") {
        Ok(s) if s.is_empty() => {
            tracing::warn!(
                "FLIGHT_SESSION_SECRET is set but empty; Flight authentication will fail. \
                 Generate a secret with: openssl rand -hex 32"
            );
            None
        },
        Ok(s) => Some(s),
        Err(_) => {
            tracing::warn!(
                "FLIGHT_SESSION_SECRET is not set; Flight handshake authentication \
                 will return an error. Set this variable before starting the server."
            );
            None
        },
    }
}

/// Default maximum number of concurrent Arrow Flight `do_get` streams.
///
/// When all permits are taken, new `do_get` requests immediately receive
/// `Status::resource_exhausted` rather than being queued indefinitely.
const DEFAULT_MAX_CONCURRENT_STREAMS: usize = 50;

/// Channel buffer size for the per-stream `FlightData` producer task (F011).
///
/// Each in-flight Arrow Flight stream uses a bounded `mpsc` channel between the
/// `FlightData` encoder and the gRPC consumer. A small buffer keeps peak memory
/// per request to a few materialised `FlightData` payloads while still letting
/// the encoder run a few batches ahead of the network.
const FLIGHT_DATA_CHANNEL_BUFFER: usize = 4;

/// Spawn a `FlightData` producer task that encodes `RecordBatch`es into
/// `FlightData` messages and sends them through a bounded channel.
///
/// Returns a `ReceiverStream` whose `Stream::poll_next` exerts backpressure on
/// the producer via the bounded channel: once `FLIGHT_DATA_CHANNEL_BUFFER`
/// messages are queued, the producer's `send().await` parks until the
/// consumer pulls a message.
///
/// The encoder is `FnOnce` because the producer task takes ownership of any
/// captured state (batches, schema, exporter) and drops it when the stream
/// ends.
fn spawn_flight_data_stream<F, Fut>(
    producer: F,
) -> impl Stream<Item = std::result::Result<FlightData, Status>>
where
    F: FnOnce(mpsc::Sender<std::result::Result<FlightData, Status>>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, rx) =
        mpsc::channel::<std::result::Result<FlightData, Status>>(FLIGHT_DATA_CHANNEL_BUFFER);
    tokio::spawn(producer(tx));
    ReceiverStream::new(rx)
}

impl FraiseQLFlightService {
    /// Create a new Flight service with placeholder data (for testing/development).
    #[must_use]
    pub fn new() -> Self {
        let schema_registry = Arc::new(SchemaRegistry::new());
        schema_registry.register_defaults(); // Register va_orders, va_users, ta_orders, ta_users, etc.

        Self {
            schema_registry,
            db_adapter: None,
            executor: None,
            cache: None,
            security_context: None,
            oidc_validator: None,
            event_storage: None,
            subscription_manager: Arc::new(SubscriptionManager::new()),
            allow_raw_sql: false,
            bulk_export_allowed_tables: None,
            session_secret: read_flight_session_secret(),
            stream_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_STREAMS)),
        }
    }

    /// Create a new Flight service connected to a database adapter.
    ///
    /// # Arguments
    ///
    /// * `db_adapter` - Database adapter for executing real queries
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: running PostgreSQL database and a ArrowDatabaseAdapter implementation.
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::ArrowDatabaseAdapter;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # struct MyAdapter;
    /// # #[async_trait::async_trait]
    /// # impl ArrowDatabaseAdapter for MyAdapter {
    /// #     async fn execute_raw_query(&self, _sql: &str) -> fraiseql_arrow::db::DatabaseResult<Vec<std::collections::HashMap<String, serde_json::Value>>> { panic!("example stub") }
    /// # }
    /// // In production, create a real PostgresAdapter from fraiseql-core
    /// // and wrap it to implement the local ArrowDatabaseAdapter trait
    /// let db_adapter: Arc<dyn ArrowDatabaseAdapter> = Arc::new(MyAdapter);
    ///
    /// let service = FraiseQLFlightService::new_with_db(db_adapter);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_with_db(db_adapter: Arc<dyn ArrowDatabaseAdapter>) -> Self {
        let schema_registry = Arc::new(SchemaRegistry::new());
        schema_registry.register_defaults(); // Register va_orders, va_users, ta_orders, ta_users, etc.

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            executor: None,
            cache: None,
            security_context: None,
            oidc_validator: None,
            event_storage: None,
            subscription_manager: Arc::new(SubscriptionManager::new()),
            allow_raw_sql: false,
            bulk_export_allowed_tables: None,
            session_secret: read_flight_session_secret(),
            stream_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_STREAMS)),
        }
    }

    /// Create a new Flight service with database adapter and query cache.
    ///
    /// # Arguments
    ///
    /// * `db_adapter` - Database adapter for executing real queries
    /// * `cache_ttl_secs` - Query result cache TTL in seconds
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: running PostgreSQL database and a ArrowDatabaseAdapter implementation.
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::ArrowDatabaseAdapter;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # struct MyAdapter;
    /// # #[async_trait::async_trait]
    /// # impl ArrowDatabaseAdapter for MyAdapter {
    /// #     async fn execute_raw_query(&self, _sql: &str) -> fraiseql_arrow::db::DatabaseResult<Vec<std::collections::HashMap<String, serde_json::Value>>> { panic!("example stub") }
    /// # }
    /// let db_adapter: Arc<dyn ArrowDatabaseAdapter> = Arc::new(MyAdapter);
    /// let service = FraiseQLFlightService::new_with_cache(db_adapter, 60); // 60-second cache
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_with_cache(db_adapter: Arc<dyn ArrowDatabaseAdapter>, cache_ttl_secs: u64) -> Self {
        let schema_registry = Arc::new(SchemaRegistry::new());
        schema_registry.register_defaults();

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            executor: None,
            cache: Some(Arc::new(QueryCache::new(cache_ttl_secs))),
            security_context: None,
            oidc_validator: None,
            event_storage: None,
            subscription_manager: Arc::new(SubscriptionManager::new()),
            allow_raw_sql: false,
            bulk_export_allowed_tables: None,
            session_secret: read_flight_session_secret(),
            stream_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_STREAMS)),
        }
    }

    /// Create a new Flight service with OIDC authentication.
    ///
    /// # Arguments
    ///
    /// * `db_adapter` - Database adapter for executing real queries
    /// * `cache_ttl_secs` - Query result cache TTL in seconds (optional)
    /// * `oidc_validator` - OIDC validator for JWT authentication
    ///
    /// # Panics
    ///
    /// Panics if `FLIGHT_SESSION_SECRET` environment variable is not set.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: running PostgreSQL database and OIDC provider for JWT validation.
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::ArrowDatabaseAdapter;
    /// use fraiseql_core::security::OidcValidator;
    /// use fraiseql_core::security::oidc::OidcConfig;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # struct MyAdapter;
    /// # #[async_trait::async_trait]
    /// # impl ArrowDatabaseAdapter for MyAdapter {
    /// #     async fn execute_raw_query(&self, _sql: &str) -> fraiseql_arrow::db::DatabaseResult<Vec<std::collections::HashMap<String, serde_json::Value>>> { panic!("example stub") }
    /// # }
    /// // Create your adapter and OIDC validator for JWT authentication
    /// let db_adapter: Arc<dyn ArrowDatabaseAdapter> = Arc::new(MyAdapter);
    /// # let config: OidcConfig = panic!("example stub");
    /// let validator: Arc<OidcValidator> = Arc::new(OidcValidator::new(config).await?);
    /// let service = FraiseQLFlightService::new_with_auth(
    ///     db_adapter,
    ///     Some(60),
    ///     validator
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_with_auth(
        db_adapter: Arc<dyn ArrowDatabaseAdapter>,
        cache_ttl_secs: Option<u64>,
        oidc_validator: Arc<OidcValidator>,
    ) -> Self {
        let schema_registry = Arc::new(SchemaRegistry::new());
        schema_registry.register_defaults();

        let cache = cache_ttl_secs.map(|ttl| Arc::new(QueryCache::new(ttl)));

        // Fail fast: authenticated Flight services require FLIGHT_SESSION_SECRET.
        // Without it, every handshake will fail with an opaque internal error at
        // request time.  Panicking here gives a clear startup message instead.
        let session_secret = read_flight_session_secret().unwrap_or_else(|| {
            // Reason: startup-time fail-fast on missing configuration; the
            // alternative (returning an opaque internal error per handshake)
            // is significantly worse.
            #[allow(clippy::panic)]
            {
                panic!(
                    "FLIGHT_SESSION_SECRET must be set when using authenticated Arrow Flight. \
                     Generate one with: openssl rand -hex 32"
                )
            }
        });

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            executor: None,
            cache,
            security_context: None,
            oidc_validator: Some(oidc_validator),
            event_storage: None,
            subscription_manager: Arc::new(SubscriptionManager::new()),
            allow_raw_sql: false,
            bulk_export_allowed_tables: None,
            session_secret: Some(session_secret),
            stream_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_STREAMS)),
        }
    }

    /// Pre-load schemas from database at startup.
    ///
    /// For production deployments, call this method after creating the service to pre-load
    /// all va_* and ta_* view schemas from the database. This reduces first-query latency
    /// by discovering schemas at startup instead of on first query.
    ///
    /// # Errors
    ///
    /// Returns error if database queries fail. Falls back to hardcoded defaults if
    /// no schemas are preloaded.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: running PostgreSQL database with va_* and ta_* views.
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::ArrowDatabaseAdapter;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # struct MyAdapter;
    /// # #[async_trait::async_trait]
    /// # impl ArrowDatabaseAdapter for MyAdapter {
    /// #     async fn execute_raw_query(&self, _sql: &str) -> fraiseql_arrow::db::DatabaseResult<Vec<std::collections::HashMap<String, serde_json::Value>>> { panic!("example stub") }
    /// # }
    /// let db_adapter: Arc<dyn ArrowDatabaseAdapter> = Arc::new(MyAdapter);
    /// let mut service = FraiseQLFlightService::new_with_db(db_adapter.clone());
    ///
    /// // Pre-load schemas from database at startup
    /// let preloaded = service.preload_schemas_from_db().await?;
    /// eprintln!("Preloaded {} schemas from database", preloaded);
    ///
    /// // Schemas now available immediately for queries
    /// // Without preloading, first query would trigger schema inference
    /// # Ok(())
    /// # }
    /// ```
    pub async fn preload_schemas_from_db(&self) -> crate::error::Result<usize> {
        if let Some(ref db_adapter) = self.db_adapter {
            self.schema_registry.preload_all_schemas(&**db_adapter).await
        } else {
            // No database adapter, use defaults
            self.schema_registry.register_defaults();
            Ok(0)
        }
    }

    /// Get a reference to the schema registry.
    ///
    /// Useful for testing and schema introspection.
    #[must_use]
    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    /// Set the HMAC-SHA256 secret used to sign Flight session tokens.
    ///
    /// Overrides the value read from the `FLIGHT_SESSION_SECRET` environment variable
    /// at construction. Use this in tests or when managing the secret programmatically.
    #[must_use]
    pub fn with_session_secret(mut self, secret: impl Into<String>) -> Self {
        self.session_secret = Some(secret.into());
        self
    }

    /// Enable raw SQL execution via `BatchedQueries` tickets.
    ///
    /// **SECURITY WARNING**: Only call this for trusted internal tooling.
    /// Enabling raw SQL allows authenticated clients to bypass RLS and execute
    /// arbitrary queries. It is disabled by default.
    #[must_use]
    pub const fn with_raw_sql_enabled(mut self) -> Self {
        self.allow_raw_sql = true;
        self
    }

    /// Allow-list the tables an authenticated client may `BulkExport`.
    ///
    /// **SECURITY WARNING**: `BulkExport` runs `SELECT * FROM "<table>"` and applies **no
    /// per-user RLS filtering** — every `BulkExport`-capable client receives the table's
    /// full contents. `BulkExport` is disabled by default (the allow-list is `None`); call
    /// this only for tables whose entire contents those clients are permitted to read.
    #[must_use]
    pub fn with_bulk_export_tables<I, S>(mut self, tables: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.bulk_export_allowed_tables = Some(
            tables
                .into_iter()
                .map(Into::into)
                .collect::<std::collections::HashSet<String>>(),
        );
        self
    }

    /// Set the query executor for GraphQL query execution.
    ///
    /// The executor must be passed as `Arc<Executor<A>>` wrapped in Arc for shared ownership.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a type implementing `fraiseql_arrow::flight_server::QueryExecutor`.
    /// use fraiseql_arrow::flight_server::QueryExecutor;
    /// use fraiseql_core::security::SecurityContext;
    /// use std::sync::Arc;
    ///
    /// # fn example(service: &mut fraiseql_arrow::flight_server::FraiseQLFlightService) {
    /// # struct MyExecutor;
    /// # #[async_trait::async_trait]
    /// # impl QueryExecutor for MyExecutor {
    /// #     async fn execute_with_security(&self, _query: &str, _variables: Option<&serde_json::Value>, _ctx: &SecurityContext) -> Result<serde_json::Value, String> { panic!("example stub") }
    /// # }
    /// let executor: Arc<dyn QueryExecutor> = Arc::new(MyExecutor);
    /// service.set_executor(executor);
    /// # }
    /// ```
    pub fn set_executor(&mut self, executor: Arc<dyn QueryExecutor>) {
        self.executor = Some(executor);
    }

    /// Get a reference to the query executor, if set.
    #[must_use]
    pub fn executor(&self) -> Option<&Arc<dyn QueryExecutor>> {
        self.executor.as_ref()
    }

    /// Check if executor is configured for real query execution.
    ///
    /// Returns true if an executor has been set via `set_executor()`.
    /// When false, queries return placeholder data.
    #[must_use]
    pub fn has_executor(&self) -> bool {
        self.executor.is_some()
    }

    /// Set the event storage for historical observer event queries.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: an ArrowEventStorage implementation (e.g., backed by a database or Redis).
    /// use fraiseql_arrow::event_storage::{ArrowEventStorage, HistoricalEvent};
    /// use chrono::{DateTime, Utc};
    /// use std::sync::Arc;
    ///
    /// # fn example(service: &mut fraiseql_arrow::flight_server::FraiseQLFlightService) {
    /// # struct MyEventStorage;
    /// # #[async_trait::async_trait]
    /// # impl ArrowEventStorage for MyEventStorage {
    /// #     async fn query_events(&self, _entity_type: &str, _start: Option<DateTime<Utc>>, _end: Option<DateTime<Utc>>, _limit: Option<usize>) -> Result<Vec<HistoricalEvent>, String> { panic!("example stub") }
    /// #     async fn count_events(&self, _entity_type: &str, _start: Option<DateTime<Utc>>, _end: Option<DateTime<Utc>>) -> Result<usize, String> { panic!("example stub") }
    /// # }
    /// // Provide your ArrowEventStorage implementation (e.g., backed by a database or Redis)
    /// let storage: Arc<dyn ArrowEventStorage> = Arc::new(MyEventStorage);
    /// service.set_event_storage(storage);
    /// # }
    /// ```
    pub fn set_event_storage(&mut self, event_storage: Arc<dyn ArrowEventStorage>) {
        self.event_storage = Some(event_storage);
    }

    /// Get a reference to the event storage, if set.
    #[must_use]
    pub fn event_storage(&self) -> Option<&Arc<dyn ArrowEventStorage>> {
        self.event_storage.as_ref()
    }

    /// Check if event storage is configured for historical event queries.
    ///
    /// Returns true if event storage has been set via `set_event_storage()`.
    #[must_use]
    pub fn has_event_storage(&self) -> bool {
        self.event_storage.is_some()
    }

    /// Get a reference to the subscription manager for real-time event subscriptions.
    #[must_use]
    pub const fn subscription_manager(&self) -> &Arc<SubscriptionManager> {
        &self.subscription_manager
    }

    /// Check if service has authenticated security context.
    ///
    /// Returns true if handshake was successful and security context is set.
    /// Subsequent Flight RPC calls require valid authentication.
    #[must_use]
    pub const fn is_authenticated(&self) -> bool {
        self.security_context.is_some()
    }

    /// Get security context if authenticated.
    ///
    /// Returns the current security context if authentication succeeded.
    /// Contains session token, user ID, and expiration information.
    #[must_use]
    pub const fn security_context(&self) -> Option<&SecurityContext> {
        self.security_context.as_ref()
    }

    /// Set security context after successful authentication.
    ///
    /// Called internally after handshake succeeds to establish authenticated session.
    /// In production, this would be called after JWT validation succeeds.
    pub fn set_security_context(&mut self, context: SecurityContext) {
        self.security_context = Some(context);
    }

    /// Set OIDC validator for JWT authentication.
    ///
    /// Enables JWT validation during the Flight handshake.
    pub fn set_oidc_validator(&mut self, validator: Arc<OidcValidator>) {
        self.oidc_validator = Some(validator);
    }

    /// Convert this service into a gRPC server.
    #[must_use]
    pub fn into_server(self) -> FlightServiceServer<Self> {
        FlightServiceServer::new(self)
    }

    /// Execute GraphQL query and stream Arrow batches.
    ///
    /// Converts GraphQL query results to Arrow Flight format for efficient columnar transfer.
    ///
    /// # Security Integration
    ///
    /// `SecurityContext` flows through the executor for row-level security (RLS):
    ///
    /// **Setup (in fraiseql-server)**:
    /// 1. Import `fraiseql_core::runtime::Executor`
    /// 2. Create `Executor::new(schema, adapter)` with database adapter
    /// 3. Wrap executor in Arc and cast as trait object: `Arc::new(executor) as Arc<dyn
    ///    QueryExecutor>`
    /// 4. Call `flight_service.set_executor(executor_trait_object)`
    ///
    /// **Query Execution with RLS**:
    /// 1. Check `has_executor()` - if true, real execution available
    /// 2. Downcast executor: `executor.downcast_ref::<Executor<A>>()`
    /// 3. Call `executor.execute_with_security(query, variables, &security_context).await`
    /// 4. Convert JSON to Arrow `RecordBatches`
    ///
    /// **Result Streaming**:
    /// 1. Schema message (first)
    /// 2. Data batches (`RecordBatch` messages)
    /// 3. Empty payload signals completion
    pub(crate) async fn execute_graphql_query(
        &self,
        query: &str,
        variables: Option<serde_json::Value>,
        security_context: &fraiseql_core::security::SecurityContext,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        info!(
            user_id = %security_context.user_id,
            "Executing GraphQL query with RLS: {}",
            query
        );

        // Execute query with RLS filtering via executor
        if let Some(executor) = self.executor() {
            // Call executor.execute_with_security() to get JSON result with RLS applied
            let parsed = executor
                .execute_with_security(query, variables.as_ref(), security_context)
                .await
                .map_err(|e| Status::internal(format!("Query execution failed: {e}")))?;

            // Convert JSON to Arrow RecordBatches
            let batches = self
                .convert_json_to_arrow_batches(&parsed)
                .map_err(|e| Status::internal(format!("Arrow conversion failed: {e}")))?;

            // Encode the schema header eagerly (must precede the batches so the
            // client can decode the rest of the stream).
            let schema_header =
                batches.first().map(|b| schema_to_flight_data(&b.schema())).transpose()?;

            // Stream the batches through a bounded mpsc channel so the encode
            // step pauses when the consumer is slow (F011 backpressure).
            let stream = spawn_flight_data_stream(move |tx| async move {
                if let Some(header) = schema_header {
                    if tx.send(Ok(header)).await.is_err() {
                        return;
                    }
                }
                for batch in batches {
                    let msg = record_batch_to_flight_data(&batch);
                    if tx.send(msg).await.is_err() {
                        return;
                    }
                }
            });
            Ok(stream)
        } else {
            // No executor configured: refuse rather than return unauthenticated fake data
            tracing::warn!(
                user_id = %security_context.user_id,
                "Arrow Flight query rejected: no database executor configured"
            );
            Err(Status::unavailable(
                "Arrow Flight server has no database executor configured. \
                 Initialize the flight server with FraiseFlightServer::with_executor().",
            ))
        }
    }

    /// Convert a GraphQL JSON result to Arrow `RecordBatch`es.
    ///
    /// Handles the standard GraphQL response envelope `{"data": {...}}`.
    /// Finds the first field inside `data` that is a non-empty array of objects,
    /// infers an Arrow schema from the first row, and converts all rows to columnar
    /// Arrow format.
    ///
    /// Falls back to wrapping the entire JSON as a single `result` string column when:
    /// - The result is a scalar (no array of objects found)
    /// - The `data` field contains only non-array values
    ///
    /// # Errors
    ///
    /// Returns an error string if schema inference or Arrow conversion fails.
    fn convert_json_to_arrow_batches(
        &self,
        json: &serde_json::Value,
    ) -> Result<Vec<RecordBatch>, String> {
        // Extract the data payload from a GraphQL response envelope.
        // Typical structure: {"data": {"field": [...]}, "errors": [...]}.
        let data = json.get("data").unwrap_or(json);

        // Find a non-empty array of objects to convert to columnar Arrow format.
        let rows: Vec<std::collections::HashMap<String, serde_json::Value>> = match data {
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|v| {
                    v.as_object()
                        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                })
                .collect(),
            serde_json::Value::Object(map) => map
                .values()
                .find_map(|v| {
                    if let serde_json::Value::Array(arr) = v {
                        let converted: Vec<_> = arr
                            .iter()
                            .filter_map(|item| {
                                item.as_object().map(|obj| {
                                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                                })
                            })
                            .collect();
                        (!converted.is_empty()).then_some(converted)
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
            _ => vec![],
        };

        if rows.is_empty() {
            // No columnar data found — wrap the entire JSON as a single string column.
            // This handles scalar results and error-only responses gracefully.
            return encode_json_to_arrow_batch(&json.to_string()).map(|b| vec![b]);
        }

        // Infer Arrow schema from the first row, then convert all rows.
        let schema = crate::schema_gen::infer_schema_from_rows(&rows)
            .map_err(|e| format!("Schema inference failed: {e}"))?;

        let arrow_rows = convert_db_rows_to_arrow(&rows, &schema)
            .map_err(|e| format!("Row conversion failed: {e}"))?;

        let config = ConvertConfig {
            batch_size: 10_000,
            max_rows:   None,
        };
        let converter = RowToArrowConverter::new(schema, config);
        chunk_into_batches(&arrow_rows, &converter, config.batch_size)
            .map_err(|e| format!("Arrow conversion failed: {e}"))
    }

    /// Execute optimized query on pre-compiled va_* view.
    ///
    /// Uses pre-compiled Arrow schemas, eliminating runtime type inference.
    /// Results are cached if caching is enabled.
    ///
    /// # Security — no per-user RLS filtering
    ///
    /// This path executes the built SQL directly against the raw database adapter
    /// (`db_adapter.execute_raw_query`); it does **not** run the RLS-aware query executor,
    /// so **no per-user row-level filtering is applied** here. `security_context` is used
    /// for audit logging only. Any row scoping must be enforced by the underlying `va_*`
    /// view itself (e.g. a view that filters on a session/tenant setting), not by this
    /// method. Do not expose `va_*` views over Flight that depend on FraiseQL's
    /// application-level RLS for isolation.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "`va_orders`")
    /// * `filter` - Optional WHERE clause
    /// * `order_by` - Optional ORDER BY clause
    /// * `limit` - Optional LIMIT
    /// * `offset` - Optional OFFSET for pagination
    /// * `security_context` - User's security context (audit logging only; not an RLS filter)
    ///
    /// # Implementation Status
    ///
    /// Currently functional with optimizations:
    /// - Pre-load and cache pre-compiled Arrow schemas from metadata
    /// - Schema optimization with registry
    /// - Database adapter for real data execution (fallback to placeholder if not configured)
    pub(crate) async fn execute_optimized_view(
        &self,
        view: &str,
        filter: Option<String>,
        order_by: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
        security_context: &fraiseql_core::security::SecurityContext,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        // Reject a zero limit fail-loud: it is a meaningless request and
        // previously produced a zero-sized batch chunk (`chunks(0)` panics).
        // The clamp at the batch-size site below is defence-in-depth (H38).
        if limit == Some(0) {
            return Err(Status::invalid_argument("OptimizedView `limit` must be greater than 0"));
        }

        // 1. Load pre-compiled Arrow schema from registry
        let schema = self
            .schema_registry
            .get(view)
            .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}")))?;

        // 2. Build optimized SQL query (validates filter/order_by for injection safety).
        let sql = build_optimized_sql(view, filter, order_by, limit, offset)?;
        info!(
            user_id = %security_context.user_id,
            "Executing optimized view (no per-user RLS on this path): {}",
            sql
        );

        // 3. Check cache before executing query
        let db_rows = if let Some(cache) = &self.cache {
            if let Some(cached_result) = cache.get(&sql) {
                debug!("Cache hit for query: {}", sql);
                (*cached_result).clone()
            } else {
                // Cache miss: execute query and cache result
                let result = self.execute_raw_query_and_cache(&sql).await?;
                result
            }
        } else {
            // No cache: execute query normally
            if let Some(db) = &self.db_adapter {
                db.execute_raw_query(&sql)
                    .await
                    .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
            } else {
                #[cfg(any(test, feature = "testing"))]
                {
                    execute_placeholder_query(view, limit)
                }
                #[cfg(not(any(test, feature = "testing")))]
                {
                    return Err(Status::failed_precondition(
                        "Arrow Flight server started without a database adapter. \
                         Configure a database adapter or enable the `testing` feature \
                         for development use.",
                    ));
                }
            }
        };

        // 4. Convert database rows to Arrow Values
        let arrow_rows = convert_db_rows_to_arrow(&db_rows, &schema)
            .map_err(|e| Status::internal(format!("Row conversion failed: {e}")))?;

        // 5. Convert to RecordBatches. Clamp the client-derived batch size to
        // [1, 10_000] so it can never be zero (defence-in-depth for H38).
        let config = ConvertConfig {
            batch_size: limit.unwrap_or(10_000).clamp(1, 10_000),
            max_rows:   limit,
        };
        let converter = RowToArrowConverter::new(schema.clone(), config);

        let batches = chunk_into_batches(&arrow_rows, &converter, config.batch_size)
            .map_err(|e| Status::internal(format!("Arrow conversion failed: {e}")))?;

        info!("Generated {} Arrow batches", batches.len());

        // 6. Convert batches to FlightData and stream to client.
        // Schema header is encoded eagerly so any encoding error surfaces
        // before the stream starts; data batches stream through a bounded
        // channel so the encode step backpressures on the consumer (F011).
        let schema_message = schema_to_flight_data(&schema)?;
        let stream = spawn_flight_data_stream(move |tx| async move {
            if tx.send(Ok(schema_message)).await.is_err() {
                return;
            }
            for batch in batches {
                let msg = record_batch_to_flight_data(&batch);
                if tx.send(msg).await.is_err() {
                    return;
                }
            }
        });
        Ok(stream)
    }

    /// Execute raw query and cache the result if caching is enabled.
    pub(crate) async fn execute_raw_query_and_cache(
        &self,
        sql: &str,
    ) -> std::result::Result<Vec<std::collections::HashMap<String, serde_json::Value>>, Status>
    {
        let result = if let Some(db) = &self.db_adapter {
            db.execute_raw_query(sql)
                .await
                .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
        } else {
            Vec::new()
        };

        // Store in cache if available
        if let Some(cache) = &self.cache {
            cache.put(sql.to_string(), Arc::new(result.clone()));
        }

        Ok(result)
    }

    /// Execute multiple SQL queries and stream combined results.
    ///
    /// Efficiently executes multiple queries in sequence and returns combined Arrow results.
    /// Improves throughput by 20-30% compared to individual requests.
    /// Results are cached if caching is enabled, improving throughput further for repeated batches.
    ///
    /// `SecurityContext` is passed through for RLS filtering.
    /// When executor is configured, each query respects RLS policies.
    ///
    /// # Arguments
    ///
    /// * `queries` - Vec of SQL query strings to execute
    /// * `security_context` - User's security context for RLS filtering
    ///
    /// # Returns
    ///
    /// Stream of `FlightData` with combined results from all queries
    #[allow(clippy::cognitive_complexity)] // Reason: multi-query batching with per-query error handling and result aggregation
    pub(crate) async fn execute_batched_queries(
        &self,
        queries: Vec<String>,
        security_context: &fraiseql_core::security::SecurityContext,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        // SECURITY: Raw SQL execution is disabled by default.
        // Clients can supply arbitrary SQL in BatchedQueries tickets, bypassing RLS.
        // Only allow this if explicitly enabled via `with_raw_sql_enabled()`.
        if !self.allow_raw_sql {
            return Err(Status::permission_denied(
                "BatchedQueries raw SQL execution is disabled. Enable with with_raw_sql_enabled().",
            ));
        }

        if queries.is_empty() {
            return Err(Status::invalid_argument("BatchedQueries must contain at least one query"));
        }

        info!(
            user_id = %security_context.user_id,
            query_count = queries.len(),
            "Executing batched queries with RLS"
        );

        // Execute all queries sequentially
        let mut all_messages: Vec<std::result::Result<FlightData, Status>> = Vec::new();
        let mut first_query = true;

        for query in &queries {
            debug!("Executing batched query: {}", query);

            // Try to get from cache first
            let db_rows = if let Some(cache) = &self.cache {
                if let Some(cached_result) = cache.get(query) {
                    debug!("Cache hit for batched query: {}", query);
                    (*cached_result).clone()
                } else {
                    // Cache miss: execute and cache
                    let result = self.execute_raw_query_and_cache(query).await?;
                    result
                }
            } else {
                // No cache: execute normally
                if let Some(db) = &self.db_adapter {
                    db.execute_raw_query(query)
                        .await
                        .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
                } else {
                    Vec::new()
                }
            };

            // Infer schema from first row
            if db_rows.is_empty() {
                continue;
            }

            let inferred_schema = crate::schema_gen::infer_schema_from_rows(&db_rows)
                .map_err(|e| Status::internal(format!("Schema inference failed: {e}")))?;

            // Convert to Arrow
            let arrow_rows = convert_db_rows_to_arrow(&db_rows, &inferred_schema)
                .map_err(|e| Status::internal(format!("Row conversion failed: {e}")))?;

            // Convert to RecordBatches
            let config = ConvertConfig {
                batch_size: 10_000,
                max_rows:   None,
            };
            let converter = RowToArrowConverter::new(inferred_schema.clone(), config);

            let batches = chunk_into_batches(&arrow_rows, &converter, config.batch_size)
                .map_err(|e| Status::internal(format!("Arrow conversion failed: {e}")))?;

            // Add schema message only for first query (schema is shared)
            if first_query {
                all_messages.push(schema_to_flight_data(&inferred_schema));
                first_query = false;
            }

            // Add batch messages
            for batch in batches {
                all_messages.push(record_batch_to_flight_data(&batch));
            }
        }

        if all_messages.is_empty() {
            return Err(Status::not_found("All batched queries returned empty results"));
        }

        // Stream the pre-encoded messages through a bounded channel so the
        // gRPC write half exerts backpressure on this task if the client
        // is slow (F011). The messages are already materialised here — the
        // backpressure window is between this task and the gRPC consumer.
        let stream = spawn_flight_data_stream(move |tx| async move {
            for msg in all_messages {
                if tx.send(msg).await.is_err() {
                    return;
                }
            }
        });
        Ok(stream)
    }

    /// Query historical observer events and stream as Arrow data.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - Entity type to filter (e.g., "Order", "User")
    /// * `start_date` - Optional ISO 8601 start date
    /// * `end_date` - Optional ISO 8601 end date
    /// * `limit` - Optional maximum number of events
    pub(crate) async fn execute_observer_events(
        &self,
        entity_type: &str,
        start_date: Option<String>,
        end_date: Option<String>,
        limit: Option<usize>,
    ) -> std::result::Result<Response<FlightDataStream>, Status> {
        // Check if event storage is configured
        let event_storage = self.event_storage.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "Event storage not configured - cannot query historical events",
            )
        })?;

        // Parse date strings to DateTime<Utc>
        let start = start_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let end = end_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        // Query events from storage
        let events = event_storage
            .query_events(entity_type, start, end, limit)
            .await
            .map_err(|e| Status::internal(format!("Failed to query events: {}", e)))?;

        info!(
            entity_type = %entity_type,
            event_count = events.len(),
            "Queried historical observer events"
        );

        // Build response as vector of FlightData messages
        let mut messages: Vec<std::result::Result<FlightData, Status>> = Vec::new();

        // Return events as JSON
        let json_data = serde_json::json!(events);
        let json_str = json_data.to_string();

        let flight_data = FlightData {
            data_body: json_str.into_bytes().into(),
            app_metadata: b"application/json".to_vec().into(),
            ..Default::default()
        };
        messages.push(Ok(flight_data));

        let stream = futures::stream::iter(messages);
        Ok(Response::new(Box::pin(stream)))
    }

    /// Export table data in bulk with multiple format support.
    ///
    /// # Arguments
    ///
    /// * `table` - Table name to export
    /// * `filter` - Optional WHERE clause filter
    /// * `limit` - Optional row limit
    /// * `format` - Export format: "parquet", "csv", or "json" (default: "parquet")
    /// * `security_context` - Security context for RLS
    #[allow(clippy::cognitive_complexity)] // Reason: multi-format export with format negotiation, query construction, and encoding
    pub(crate) async fn execute_bulk_export(
        &self,
        table: &str,
        filter: Option<String>,
        limit: Option<usize>,
        format: Option<String>,
        security_context: &fraiseql_core::security::SecurityContext,
    ) -> std::result::Result<Response<FlightDataStream>, Status> {
        // H39: BulkExport runs `SELECT * FROM "<table>"` with NO per-user RLS filtering, so
        // it is fail-closed — rejected unless the operator explicitly allow-lists the table
        // via `with_bulk_export_tables`. `None` ⇒ BulkExport disabled entirely.
        let Some(allowed_tables) = self.bulk_export_allowed_tables.as_ref() else {
            return Err(Status::permission_denied(
                "BulkExport is disabled. Enable specific tables with with_bulk_export_tables().",
            ));
        };
        if !allowed_tables.contains(table) {
            return Err(Status::permission_denied(format!(
                "BulkExport is not permitted for table '{table}'."
            )));
        }

        // Parse export format. Default is Parquet when the `parquet` feature is enabled,
        // otherwise JSON Lines (Parquet pulls in the unmaintained thrift 0.17 crate;
        // see deny.toml / security-vulnerabilities.md for context).
        let export_format = match format.as_deref() {
            Some(f) => f
                .parse::<ExportFormat>()
                .map_err(|e| Status::invalid_argument(format!("Invalid format: {}", e)))?,
            #[cfg(feature = "parquet")]
            None => ExportFormat::Parquet,
            #[cfg(not(feature = "parquet"))]
            None => ExportFormat::Json,
        };

        info!(
            user_id = %security_context.user_id,
            table = %table,
            format = ?export_format,
            "Starting bulk export"
        );

        // SECURITY: Reject raw WHERE clause filters — they allow SQL injection. BulkExport
        // applies no per-user row filtering of its own; access is controlled by the table
        // allow-list above, so an allow-listed table is exported in full.
        if filter.is_some() {
            return Err(Status::invalid_argument("BulkExport filter parameter is not supported."));
        }

        // Get database adapter
        let db_adapter = self.db_adapter.as_ref().ok_or_else(|| {
            Status::failed_precondition("Database adapter not configured - cannot export data")
        })?;

        // Build SQL query with quoted table identifier to prevent SQL injection.
        let quoted_table = format!("\"{}\"", table.replace('"', "\"\""));
        let mut sql = format!("SELECT * FROM {quoted_table}");

        if let Some(l) = limit {
            sql.push_str(" LIMIT ");
            sql.push_str(&l.to_string());
        }

        debug!(sql = %sql, "Executing export query");

        // Execute query
        let rows = db_adapter
            .execute_raw_query(&sql)
            .await
            .map_err(|e| Status::internal(format!("Query execution failed: {}", e)))?;

        if rows.is_empty() {
            info!(table = %table, "Export returned no rows");
            return Err(Status::not_found(format!(
                "No rows found for export from table: {}",
                table
            )));
        }

        info!(
            table = %table,
            row_count = rows.len(),
            "Query returned rows for export"
        );

        // Infer schema from rows
        let schema = crate::schema_gen::infer_schema_from_rows(&rows)
            .map_err(|e| Status::internal(format!("Schema inference failed: {}", e)))?;

        // Convert database rows to Arrow format
        let arrow_rows = convert_db_rows_to_arrow(&rows, &schema)
            .map_err(|e| Status::internal(format!("Row conversion failed: {}", e)))?;

        // Convert to RecordBatches
        let config = ConvertConfig {
            batch_size: 10_000,
            max_rows:   None,
        };
        let converter = RowToArrowConverter::new(schema, config);

        let batches: Vec<RecordBatch> =
            chunk_into_batches(&arrow_rows, &converter, config.batch_size)
                .map_err(|e| Status::internal(format!("Arrow conversion failed: {}", e)))?;

        if batches.is_empty() {
            return Err(Status::internal("No Arrow batches created".to_string()));
        }

        info!(
            table = %table,
            batch_count = batches.len(),
            format = ?export_format,
            "Bulk export started"
        );

        // Export and stream batches through a bounded channel so each
        // `BulkExporter::export_batch` call only runs when the consumer is
        // ready for it — peak memory holds the channel-buffer-sized window of
        // serialised payloads rather than the full Vec<FlightData> (F011).
        let mime = export_format.mime_type().as_bytes().to_vec();
        let stream = spawn_flight_data_stream(move |tx| async move {
            for (index, batch) in batches.iter().enumerate() {
                let msg = match BulkExporter::export_batch(batch, export_format) {
                    Ok(exported_bytes) => {
                        info!(
                            batch_index = index,
                            bytes_size = exported_bytes.len(),
                            "Exported batch"
                        );
                        Ok(FlightData {
                            data_body: exported_bytes.into(),
                            app_metadata: mime.clone().into(),
                            ..Default::default()
                        })
                    },
                    Err(e) => Err(Status::internal(format!("Export failed: {e}"))),
                };
                let is_err = msg.is_err();
                if tx.send(msg).await.is_err() || is_err {
                    return;
                }
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }

    /// Handle `ClearCache` action
    pub(crate) fn handle_clear_cache(&self) -> ActionResultStream {
        info!("ClearCache action triggered");

        // Clear cache if present
        if let Some(cache) = &self.cache {
            cache.clear();
        }

        let message = "Cache cleared successfully".to_string();
        let result = Ok(arrow_flight::Result {
            body: message.into_bytes().into(),
        });

        let stream = futures::stream::iter(vec![result]);
        Box::pin(stream)
    }

    /// Handle `RefreshSchemaRegistry` action.
    ///
    /// Admin-only action that safely reloads schema definitions from the database
    /// without disrupting running queries (Copy-on-Write via Arc<Schema>).
    ///
    /// Returns a JSON result with:
    /// - `success`: true/false
    /// - `reloaded_count`: number of successfully reloaded schemas
    /// - `message`: descriptive message
    pub(crate) fn handle_refresh_schema_registry(&self) -> ActionResultStream {
        info!("RefreshSchemaRegistry action triggered");

        let db_adapter = self.db_adapter.clone();
        let schema_registry = Arc::clone(&self.schema_registry);

        // Spawn background task to reload schemas
        if let Some(adapter) = db_adapter {
            tokio::spawn(async move {
                match schema_registry.reload_all_schemas(adapter.as_ref()).await {
                    Ok(count) => {
                        info!("Schema reload completed: {} schemas reloaded", count);
                    },
                    Err(e) => {
                        warn!("Schema reload failed: {}", e);
                    },
                }
            });
        }

        // Return immediate response to client
        let response = serde_json::json!({
            "success": true,
            "message": "Schema reload started (processing in background)",
        });

        let result = Ok(arrow_flight::Result {
            body: serde_json::to_vec(&response).unwrap_or_else(|_| b"{}".to_vec()).into(),
        });

        let stream = futures::stream::iter(vec![result]);
        Box::pin(stream)
    }

    /// Handle `GetSchemaVersions` action.
    ///
    /// Returns information about all registered schemas and their versions.
    /// Useful for debugging schema reload issues.
    ///
    /// Returns a JSON result with array of:
    /// - `view_name`: the view name (e.g., "`va_orders`")
    /// - `version`: current schema version number
    /// - `created_at`: when this version was created (ISO 8601)
    pub(crate) fn handle_get_schema_versions(&self) -> ActionResultStream {
        info!("GetSchemaVersions action triggered");

        let versions = self.schema_registry.get_all_versions();

        let schema_infos: Vec<serde_json::Value> = versions
            .iter()
            .map(|(view_name, version, created_at)| {
                serde_json::json!({
                    "view_name": view_name,
                    "version": version,
                    "created_at": created_at.to_rfc3339(),
                })
            })
            .collect();

        let response = serde_json::json!({
            "schemas": schema_infos,
            "count": versions.len(),
        });

        let result = Ok(arrow_flight::Result {
            body: serde_json::to_vec(&response).unwrap_or_else(|_| b"{}".to_vec()).into(),
        });

        let stream = futures::stream::iter(vec![result]);
        Box::pin(stream)
    }

    /// Handle `HealthCheck` action
    pub(crate) fn handle_health_check(&self) -> ActionResultStream {
        info!("HealthCheck action triggered");

        let health_status = serde_json::json!({
            "status": "healthy",
            "version": "2.0.0-a1",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        });

        let message = health_status.to_string();
        let result = Ok(arrow_flight::Result {
            body: message.into_bytes().into(),
        });

        let stream = futures::stream::iter(vec![result]);
        Box::pin(stream)
    }
}

impl Default for FraiseQLFlightService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod convert_tests;

#[cfg(test)]
mod backpressure_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use arrow_flight::FlightData;
    use futures::StreamExt;
    use tonic::Status;

    use super::{FLIGHT_DATA_CHANNEL_BUFFER, spawn_flight_data_stream};

    /// F011: when the consumer is slow, the producer must park on
    /// `tx.send()` once `FLIGHT_DATA_CHANNEL_BUFFER` messages are in flight
    /// rather than continuing to produce ahead.
    #[tokio::test]
    async fn producer_parks_when_channel_buffer_full() {
        // Total messages exceed the channel buffer by a healthy margin.
        let total = FLIGHT_DATA_CHANNEL_BUFFER + 6;
        let produced = Arc::new(AtomicUsize::new(0));
        let produced_clone = Arc::clone(&produced);

        let stream = spawn_flight_data_stream(move |tx| async move {
            for i in 0..total {
                let msg: std::result::Result<FlightData, Status> = Ok(FlightData {
                    data_body: vec![i as u8].into(),
                    ..Default::default()
                });
                if tx.send(msg).await.is_err() {
                    return;
                }
                produced_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        // Pin the stream so we can poll it incrementally.
        let mut stream = Box::pin(stream);

        // Let the producer run ahead as far as the channel allows.
        // ReceiverStream + bounded channel(N) means the producer can
        // enqueue up to N + 1 messages before parking (N in the channel,
        // 1 in the `send()` future).
        tokio::time::sleep(Duration::from_millis(50)).await;

        let ahead = produced.load(Ordering::SeqCst);
        assert!(
            ahead <= FLIGHT_DATA_CHANNEL_BUFFER + 1,
            "producer ran ahead by {ahead} messages (buffer is {FLIGHT_DATA_CHANNEL_BUFFER}); \
             backpressure is not engaged"
        );
        assert!(
            ahead >= FLIGHT_DATA_CHANNEL_BUFFER,
            "producer only produced {ahead} messages before parking; expected to fill the buffer"
        );

        // Consume the stream and verify we receive all messages.
        let mut received = 0_usize;
        while stream.next().await.is_some() {
            received += 1;
        }
        assert_eq!(received, total);
        assert_eq!(produced.load(Ordering::SeqCst), total);
    }

    /// If the consumer drops the stream, the producer must terminate cleanly
    /// without leaking the spawned task.
    #[tokio::test]
    async fn producer_exits_when_consumer_drops() {
        let produced = Arc::new(AtomicUsize::new(0));
        let produced_clone = Arc::clone(&produced);

        let stream = spawn_flight_data_stream(move |tx| async move {
            for i in 0..1_000_usize {
                let msg: std::result::Result<FlightData, Status> = Ok(FlightData {
                    data_body: vec![(i % 256) as u8].into(),
                    ..Default::default()
                });
                if tx.send(msg).await.is_err() {
                    return;
                }
                produced_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        let mut stream = Box::pin(stream);

        // Pull two messages, then drop the consumer.
        for _ in 0..2 {
            stream.next().await.unwrap().unwrap();
        }
        drop(stream);

        // Give the producer task a moment to observe the channel close
        // and exit.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // The producer should not have iterated the full 1000 items.
        let final_count = produced.load(Ordering::SeqCst);
        assert!(
            final_count < 1_000,
            "producer kept running after consumer dropped (produced {final_count})"
        );
    }
}

#[cfg(test)]
mod bulk_export_authz_tests {
    #![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

    use tonic::Code;

    use super::FraiseQLFlightService;

    /// Build a minimal authenticated core `SecurityContext` for the export call.
    fn test_security_context() -> fraiseql_core::security::SecurityContext {
        let user = fraiseql_core::security::auth_middleware::AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new("user-1".to_string()),
            scopes:       vec![],
            expires_at:   chrono::Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: std::collections::HashMap::new(),
        };
        fraiseql_core::security::SecurityContext::from_user(&user, "req-1".to_string())
    }

    #[test]
    fn bulk_export_disabled_by_default() {
        assert!(FraiseQLFlightService::new().bulk_export_allowed_tables.is_none());
    }

    /// `Response<FlightDataStream>` is not `Debug`, so `unwrap_err()` is unavailable;
    /// pull the `Status` out by hand.
    async fn export_err_code(svc: &FraiseQLFlightService, table: &str) -> Code {
        match svc.execute_bulk_export(table, None, None, None, &test_security_context()).await {
            Ok(_) => panic!("expected BulkExport to error for table '{table}'"),
            Err(status) => status.code(),
        }
    }

    #[tokio::test]
    async fn bulk_export_rejected_when_disabled() {
        let svc = FraiseQLFlightService::new();
        assert_eq!(
            export_err_code(&svc, "orders").await,
            Code::PermissionDenied,
            "H39: BulkExport must be fail-closed (disabled) by default"
        );
    }

    #[tokio::test]
    async fn bulk_export_rejected_for_non_allowlisted_table() {
        let svc = FraiseQLFlightService::new().with_bulk_export_tables(["orders"]);
        assert_eq!(
            export_err_code(&svc, "users").await,
            Code::PermissionDenied,
            "H39: a table not on the allow-list must be denied"
        );
    }

    #[tokio::test]
    async fn bulk_export_allowlisted_table_passes_authz_gate() {
        // An allow-listed table passes the authz gate; with no db adapter configured the
        // call then fails at the adapter check (failed_precondition) — proving the gate
        // itself did NOT deny (it would otherwise be permission_denied).
        let svc = FraiseQLFlightService::new().with_bulk_export_tables(["orders"]);
        assert_eq!(
            export_err_code(&svc, "orders").await,
            Code::FailedPrecondition,
            "an allow-listed table must pass the authz gate (then fail on the missing adapter)"
        );
    }
}

#[cfg(test)]
mod chunk_into_batches_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use arrow::datatypes::{DataType, Field, Schema};

    use super::chunk_into_batches;
    use crate::convert::{ConvertConfig, RowToArrowConverter, Value};

    fn converter(batch_size: usize) -> RowToArrowConverter {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, true),
        ]));
        RowToArrowConverter::new(
            schema,
            ConvertConfig {
                batch_size,
                max_rows: None,
            },
        )
    }

    #[test]
    fn zero_batch_size_does_not_panic_on_empty_input() {
        // `slice::chunks(0)` panics regardless of length — even on an empty
        // slice. The helper floors the size at 1, so a client `limit = 0`
        // (audit H38) can no longer abort the worker.
        let batches = chunk_into_batches(&[], &converter(0), 0).expect("must not panic");
        assert!(batches.is_empty());
    }

    #[test]
    fn zero_batch_size_still_returns_all_rows() {
        let rows = vec![
            vec![Some(Value::Int(1)), Some(Value::String("a".to_string()))],
            vec![Some(Value::Int(2)), Some(Value::String("b".to_string()))],
            vec![Some(Value::Int(3)), None],
        ];
        let batches = chunk_into_batches(&rows, &converter(0), 0).expect("must not panic");
        let total: usize = batches.iter().map(arrow::array::RecordBatch::num_rows).sum();
        assert_eq!(total, 3, "all rows convert even when batch_size is 0 (floored to 1)");
    }
}
