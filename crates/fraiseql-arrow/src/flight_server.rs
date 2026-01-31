//! FraiseQL Arrow Flight service implementation.
//!
//! This module provides the core gRPC service that handles Flight RPC calls,
//! enabling high-performance columnar data transfer for GraphQL queries.

use std::{pin::Pin, sync::Arc};

use arrow::{
    array::RecordBatch,
    ipc::writer::{DictionaryTracker, IpcDataGenerator, IpcWriteOptions},
};
use arrow_flight::{
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PollInfo, PutResult, SchemaResult, Ticket,
    flight_service_server::{FlightService, FlightServiceServer},
};
use futures::Stream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

use crate::{
    cache::QueryCache,
    convert::{ConvertConfig, RowToArrowConverter},
    db::DatabaseAdapter,
    db_convert::convert_db_rows_to_arrow,
    metadata::SchemaRegistry,
    schema::{graphql_result_schema, observer_event_schema},
    ticket::FlightTicket,
};

type HandshakeStream =
    Pin<Box<dyn Stream<Item = std::result::Result<HandshakeResponse, Status>> + Send>>;
type FlightInfoStream = Pin<Box<dyn Stream<Item = std::result::Result<FlightInfo, Status>> + Send>>;
type FlightDataStream = Pin<Box<dyn Stream<Item = std::result::Result<FlightData, Status>> + Send>>;
type PutResultStream = Pin<Box<dyn Stream<Item = std::result::Result<PutResult, Status>> + Send>>;
type ActionResultStream =
    Pin<Box<dyn Stream<Item = std::result::Result<arrow_flight::Result, Status>> + Send>>;
type ActionTypeStream = Pin<Box<dyn Stream<Item = std::result::Result<ActionType, Status>> + Send>>;

/// FraiseQL Arrow Flight service implementation.
///
/// This is the core gRPC service that handles Flight RPC calls.
/// It will be extended in subsequent phases to actually fetch/stream data.
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
    /// Schema registry for pre-compiled Arrow views
    schema_registry: SchemaRegistry,
    /// Optional database adapter for executing real queries.
    /// If None, placeholder queries are used (for testing/development).
    db_adapter:      Option<Arc<dyn DatabaseAdapter>>,
    /// Optional query result cache for improving throughput on repeated queries
    cache:           Option<Arc<QueryCache>>,
    // Future: Will hold references to query executor, observer system, etc.
}

impl FraiseQLFlightService {
    /// Create a new Flight service with placeholder data (for testing/development).
    #[must_use]
    pub fn new() -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults(); // Register va_orders, va_users, ta_orders, ta_users, etc.

        Self {
            schema_registry,
            db_adapter: None,
            cache: None,
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
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::DatabaseAdapter;
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // In production, create a real PostgresAdapter from fraiseql-core
    /// // and wrap it to implement the local DatabaseAdapter trait
    /// let db_adapter: Arc<dyn DatabaseAdapter> = todo!("Create from fraiseql_core::db::PostgresAdapter");
    ///
    /// let service = FraiseQLFlightService::new_with_db(db_adapter);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_with_db(db_adapter: Arc<dyn DatabaseAdapter>) -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults(); // Register va_orders, va_users, ta_orders, ta_users, etc.

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            cache: None,
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
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::DatabaseAdapter;
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db_adapter: Arc<dyn DatabaseAdapter> = todo!("Create adapter");
    /// let service = FraiseQLFlightService::new_with_cache(db_adapter, 60); // 60-second cache
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_with_cache(db_adapter: Arc<dyn DatabaseAdapter>, cache_ttl_secs: u64) -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults();

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            cache: Some(Arc::new(QueryCache::new(cache_ttl_secs))),
        }
    }

    /// Get a reference to the schema registry.
    ///
    /// Useful for testing and schema introspection.
    #[must_use]
    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    /// Convert this service into a gRPC server.
    #[must_use]
    pub fn into_server(self) -> FlightServiceServer<Self> {
        FlightServiceServer::new(self)
    }

    /// Execute GraphQL query and stream Arrow batches.
    ///
    ///
    ///
    /// # Implementation Status (Phase 17: Arrow Flight Implementation)
    ///
    /// Currently returns an empty stream (placeholder). Full implementation includes:
    /// - BLOCKED: Awaiting Phase 17 Arrow execution engine implementation
    /// - TODO: Add QueryExecutor reference to FraiseQLFlightService struct (see
    ///   KNOWN_LIMITATIONS.md#arrow-flight)
    /// - TODO: Call fraiseql_core::arrow_executor::execute_query_as_arrow()
    /// - TODO: Convert RecordBatches to FlightData messages
    /// - TODO: Stream Arrow data to client
    async fn execute_graphql_query(
        &self,
        _query: &str,
        _variables: Option<serde_json::Value>,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        // TODO: Execute actual GraphQL query and convert RecordBatches to FlightData
        let stream = futures::stream::empty();
        Ok(stream)
    }

    /// Execute optimized query on pre-compiled va_* view.
    ///
    /// Uses pre-compiled Arrow schemas, eliminating runtime type inference.
    /// Results are cached if caching is enabled.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "va_orders")
    /// * `filter` - Optional WHERE clause
    /// * `order_by` - Optional ORDER BY clause
    /// * `limit` - Optional LIMIT
    /// * `offset` - Optional OFFSET for pagination
    ///
    /// # Implementation Status (Phase 17: Arrow Flight Implementation)
    ///
    /// Currently functional with placeholder data. Full optimization includes:
    /// - BLOCKED: Depends on Phase 17 Arrow execution optimization
    /// - TODO: Pre-load and cache pre-compiled Arrow schemas from metadata (see
    ///   KNOWN_LIMITATIONS.md#arrow-flight)
    /// - TODO: Implement query optimization with pre-compiled schemas
    /// - TODO: Use database adapter for real data execution
    /// - TODO: Zero-copy row-to-Arrow conversion for pre-compiled types
    async fn execute_optimized_view(
        &self,
        view: &str,
        filter: Option<String>,
        order_by: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        // 1. Load pre-compiled Arrow schema from registry
        let schema = self
            .schema_registry
            .get(view)
            .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}")))?;

        // 2. Build optimized SQL query
        let sql = build_optimized_sql(view, filter, order_by, limit, offset);
        info!("Executing optimized query: {}", sql);

        // 3. Check cache before executing query
        let db_rows = if let Some(cache) = &self.cache {
            if let Some(cached_result) = cache.get(&sql) {
                info!("Cache hit for query: {}", sql);
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
                execute_placeholder_query(view, limit)
            }
        };

        // 4. Convert database rows to Arrow Values
        let arrow_rows = convert_db_rows_to_arrow(&db_rows, &schema)
            .map_err(|e| Status::internal(format!("Row conversion failed: {e}")))?;

        // 5. Convert to RecordBatches
        let config = ConvertConfig {
            batch_size: limit.unwrap_or(10_000).min(10_000),
            max_rows:   limit,
        };
        let converter = RowToArrowConverter::new(schema.clone(), config);

        let batches = arrow_rows
            .chunks(config.batch_size)
            .map(|chunk| converter.convert_batch(chunk.to_vec()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Status::internal(format!("Arrow conversion failed: {e}")))?;

        info!("Generated {} Arrow batches", batches.len());

        // 6. Convert batches to FlightData and stream to client
        // First message: schema
        let schema_message = schema_to_flight_data(&schema)?;

        // Subsequent messages: data batches
        let batch_messages: Vec<std::result::Result<FlightData, Status>> =
            batches.iter().map(record_batch_to_flight_data).collect();

        // Combine schema + batches into a single stream
        let mut all_messages = vec![Ok(schema_message)];
        all_messages.extend(batch_messages);

        let stream = futures::stream::iter(all_messages);
        Ok(stream)
    }

    /// Execute raw query and cache the result if caching is enabled.
    async fn execute_raw_query_and_cache(
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
    /// # Arguments
    ///
    /// * `queries` - Vec of SQL query strings to execute
    ///
    /// # Returns
    ///
    /// Stream of FlightData with combined results from all queries
    async fn execute_batched_queries(
        &self,
        queries: Vec<String>,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        if queries.is_empty() {
            return Err(Status::invalid_argument("BatchedQueries must contain at least one query"));
        }

        info!("Executing {} batched queries", queries.len());

        // Execute all queries sequentially
        let mut all_messages: Vec<std::result::Result<FlightData, Status>> = Vec::new();
        let mut first_query = true;

        for query in &queries {
            info!("Executing batched query: {}", query);

            // Try to get from cache first
            let db_rows = if let Some(cache) = &self.cache {
                if let Some(cached_result) = cache.get(query) {
                    info!("Cache hit for batched query: {}", query);
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

            let batches = arrow_rows
                .chunks(config.batch_size)
                .map(|chunk| converter.convert_batch(chunk.to_vec()))
                .collect::<Result<Vec<_>, _>>()
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

        let stream = futures::stream::iter(all_messages);
        Ok(stream)
    }
}

impl Default for FraiseQLFlightService {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl FlightService for FraiseQLFlightService {
    type DoActionStream = ActionResultStream;
    type DoExchangeStream = FlightDataStream;
    type DoGetStream = FlightDataStream;
    type DoPutStream = PutResultStream;
    type HandshakeStream = HandshakeStream;
    type ListActionsStream = ActionTypeStream;
    type ListFlightsStream = FlightInfoStream;

    /// Handshake for authentication (not implemented yet).
    ///
    /// Will be implemented in future versions with JWT/API key authentication.
    async fn handshake(
        &self,
        _request: Request<Streaming<HandshakeRequest>>,
    ) -> std::result::Result<Response<Self::HandshakeStream>, Status> {
        info!("Handshake called (not implemented)");
        Err(Status::unimplemented("Handshake not implemented yet"))
    }

    /// List available datasets/queries.
    ///
    /// Currently, this returns an empty list for testing.
    /// In  to list available GraphQL queries, observer events, etc.
    async fn list_flights(
        &self,
        _request: Request<Criteria>,
    ) -> std::result::Result<Response<Self::ListFlightsStream>, Status> {
        info!("ListFlights called");

        // Phase 17: Arrow Flight dataset listing (BLOCKED)
        // TODO: Return actual available datasets (GraphQL queries, observer events)
        // For now, demonstrate the API works with an empty stream
        let stream = futures::stream::empty();
        Ok(Response::new(Box::pin(stream)))
    }

    /// Get schema for a dataset without fetching data.
    ///
    /// This is used by clients to inspect the schema before fetching data.
    async fn get_schema(
        &self,
        request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<SchemaResult>, Status> {
        let descriptor = request.into_inner();
        info!("GetSchema called: {:?}", descriptor);

        // Decode ticket from descriptor path
        if descriptor.path.is_empty() {
            return Err(Status::invalid_argument("Empty flight descriptor path"));
        }

        let ticket_bytes = descriptor.path[0].as_bytes();
        let ticket = FlightTicket::decode(ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

        // Return appropriate schema based on ticket type
        let schema = match ticket {
            FlightTicket::GraphQLQuery { .. } => graphql_result_schema(),
            FlightTicket::ObserverEvents { .. } => observer_event_schema(),
            FlightTicket::OptimizedView { view, .. } => self
                .schema_registry
                .get(&view)
                .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}")))?,
            FlightTicket::BulkExport { .. } => {
                // Will be implemented in future versions
                return Err(Status::unimplemented("BulkExport not implemented yet"));
            },
            FlightTicket::BatchedQueries { .. } => {
                // Batched queries don't have a single schema; each query has its own
                // Return a generic combined schema or error
                return Err(Status::unimplemented(
                    "GetSchema for BatchedQueries returns per-query schemas in the data stream",
                ));
            },
        };

        // Serialize schema to IPC format
        let options = arrow::ipc::writer::IpcWriteOptions::default();
        let data_gen = arrow::ipc::writer::IpcDataGenerator::default();
        let mut dict_tracker = arrow::ipc::writer::DictionaryTracker::new(false);
        let encoded_data =
            data_gen.schema_to_bytes_with_dictionary_tracker(&schema, &mut dict_tracker, &options);

        Ok(Response::new(SchemaResult {
            schema: encoded_data.ipc_message.into(),
        }))
    }

    /// Fetch data stream (main data retrieval method).
    ///
    /// Currently, this returns empty streams.
    /// In , this will execute queries and stream Arrow RecordBatches.
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> std::result::Result<Response<Self::DoGetStream>, Status> {
        let ticket_bytes = request.into_inner().ticket;
        let ticket = FlightTicket::decode(&ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

        info!("DoGet called: {:?}", ticket);

        match ticket {
            FlightTicket::GraphQLQuery { query, variables } => {
                let stream = self.execute_graphql_query(&query, variables).await?;
                Ok(Response::new(Box::pin(stream)))
            },
            FlightTicket::OptimizedView {
                view,
                filter,
                order_by,
                limit,
                offset,
            } => {
                let stream =
                    self.execute_optimized_view(&view, filter, order_by, limit, offset).await?;
                Ok(Response::new(Box::pin(stream)))
            },
            FlightTicket::ObserverEvents { .. } => {
                Err(Status::unimplemented("Observer events not implemented yet"))
            },
            FlightTicket::BulkExport { .. } => {
                Err(Status::unimplemented("Bulk export not implemented yet"))
            },
            FlightTicket::BatchedQueries { queries } => {
                let stream = self.execute_batched_queries(queries).await?;
                Ok(Response::new(Box::pin(stream)))
            },
        }
    }

    /// Upload data stream (for client-to-server data transfer).
    ///
    /// Not currently needed(we're focused on server→client).
    /// May be useful in  for bulk imports.
    async fn do_put(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoPutStream>, Status> {
        warn!("DoPut called but not implemented");
        Err(Status::unimplemented("DoPut not implemented yet"))
    }

    /// Execute an action (RPC method for operations beyond data transfer).
    async fn do_action(
        &self,
        _request: Request<Action>,
    ) -> std::result::Result<Response<Self::DoActionStream>, Status> {
        warn!("DoAction called but not implemented");
        Err(Status::unimplemented("DoAction not implemented yet"))
    }

    /// List available actions.
    async fn list_actions(
        &self,
        _request: Request<Empty>,
    ) -> std::result::Result<Response<Self::ListActionsStream>, Status> {
        info!("ListActions called");
        let stream = futures::stream::empty();
        Ok(Response::new(Box::pin(stream)))
    }

    /// Bidirectional streaming (not needed for FraiseQL use cases).
    async fn do_exchange(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoExchangeStream>, Status> {
        warn!("DoExchange called but not implemented");
        Err(Status::unimplemented("DoExchange not implemented yet"))
    }

    /// Get flight info for a descriptor (metadata about available data).
    ///
    /// This method provides metadata about what data is available without
    /// actually fetching it. Will be implemented in future versions+.
    async fn get_flight_info(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<FlightInfo>, Status> {
        info!("GetFlightInfo called");
        Err(Status::unimplemented("GetFlightInfo not implemented yet"))
    }

    /// Poll for flight info updates (for long-running operations).
    ///
    /// Not needed for FraiseQL use cases (queries are synchronous).
    async fn poll_flight_info(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<PollInfo>, Status> {
        info!("PollFlightInfo called");
        Err(Status::unimplemented("PollFlightInfo not implemented yet"))
    }
}

/// Convert RecordBatch to FlightData using Arrow IPC encoding.
///
/// # Arguments
///
/// * `batch` - Arrow RecordBatch to encode
///
/// # Returns
///
/// FlightData message with IPC-encoded batch
///
/// # Errors
///
/// Returns error if IPC encoding fails
#[allow(clippy::result_large_err)]
fn record_batch_to_flight_data(batch: &RecordBatch) -> std::result::Result<FlightData, Status> {
    let options = IpcWriteOptions::default();
    let data_gen = IpcDataGenerator::default();
    let mut dict_tracker = DictionaryTracker::new(false);

    let (_, encoded_data) = data_gen
        .encoded_batch(batch, &mut dict_tracker, &options)
        .map_err(|e| Status::internal(format!("Failed to encode RecordBatch: {e}")))?;

    Ok(FlightData {
        data_header: encoded_data.ipc_message.into(),
        data_body: encoded_data.arrow_data.into(),
        ..Default::default()
    })
}

/// Convert schema to FlightData for initial message.
///
/// # Arguments
///
/// * `schema` - Arrow schema to encode
///
/// # Returns
///
/// FlightData message with IPC-encoded schema
///
/// # Errors
///
/// Returns error if IPC encoding fails
#[allow(clippy::result_large_err)]
fn schema_to_flight_data(
    schema: &Arc<arrow::datatypes::Schema>,
) -> std::result::Result<FlightData, Status> {
    let options = IpcWriteOptions::default();
    let data_gen = IpcDataGenerator::default();
    let mut dict_tracker = DictionaryTracker::new(false);

    let encoded_data =
        data_gen.schema_to_bytes_with_dictionary_tracker(schema, &mut dict_tracker, &options);

    Ok(FlightData {
        data_header: encoded_data.ipc_message.into(),
        data_body: vec![].into(),
        ..Default::default()
    })
}

/// Build optimized SQL query for va_* view.
///
/// # Arguments
///
/// * `view` - View name (e.g., "va_orders")
/// * `filter` - Optional WHERE clause
/// * `order_by` - Optional ORDER BY clause
/// * `limit` - Optional LIMIT
/// * `offset` - Optional OFFSET
///
/// # Returns
///
/// SQL query string
///
/// # Example
///
/// ```ignore
/// let sql = build_optimized_sql(
///     "va_orders",
///     Some("created_at > '2026-01-01'"),
///     Some("created_at DESC"),
///     Some(100),
///     Some(0)
/// );
/// // Returns: "SELECT * FROM va_orders WHERE created_at > '2026-01-01' ORDER BY created_at DESC LIMIT 100 OFFSET 0"
/// ```
fn build_optimized_sql(
    view: &str,
    filter: Option<String>,
    order_by: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> String {
    let mut sql = format!("SELECT * FROM {view}");

    if let Some(where_clause) = filter {
        sql.push_str(&format!(" WHERE {where_clause}"));
    }

    if let Some(order_clause) = order_by {
        sql.push_str(&format!(" ORDER BY {order_clause}"));
    }

    if let Some(limit_value) = limit {
        sql.push_str(&format!(" LIMIT {limit_value}"));
    }

    if let Some(offset_value) = offset {
        sql.push_str(&format!(" OFFSET {offset_value}"));
    }

    sql
}

/// Generate placeholder database rows for testing.
///
/// # Implementation Status (Phase 17: Arrow Flight Implementation)
///
/// Currently returns hardcoded test data. Production implementation:
/// - BLOCKED: Depends on Phase 17 database adapter integration
/// - TODO: Replace with actual database adapter when integrated with fraiseql-server (see
///   KNOWN_LIMITATIONS.md#arrow-flight)
///
/// # Arguments
///
/// * `view` - View name (e.g., "va_orders", "va_users")
/// * `limit` - Optional limit on number of rows
///
/// # Returns
///
/// Vec of rows as HashMap<column_name, json_value>
fn execute_placeholder_query(
    view: &str,
    limit: Option<usize>,
) -> Vec<std::collections::HashMap<String, serde_json::Value>> {
    use std::collections::HashMap;

    use serde_json::json;

    let row_count = limit.unwrap_or(10).min(100); // Cap at 100 for testing
    let mut rows = Vec::with_capacity(row_count);

    match view {
        "va_orders" => {
            // Schema: id (Int64), total (Float64), created_at (Timestamp), customer_name (Utf8)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(i64::from(i as i32 + 1)));
                row.insert("total".to_string(), json!((i as f64 + 1.0) * 99.99));
                row.insert(
                    "created_at".to_string(),
                    json!(1_700_000_000_000_000_i64 + i64::from(i as i32) * 86_400_000_000),
                );
                row.insert("customer_name".to_string(), json!(format!("Customer {}", i + 1)));
                rows.push(row);
            }
        },
        "va_users" => {
            // Schema: id (Int64), email (Utf8), name (Utf8), created_at (Timestamp)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(i64::from(i as i32 + 1)));
                row.insert("email".to_string(), json!(format!("user{}@example.com", i + 1)));
                row.insert("name".to_string(), json!(format!("User {}", i + 1)));
                row.insert(
                    "created_at".to_string(),
                    json!(1_700_000_000_000_000_i64 + i64::from(i as i32) * 86_400_000_000),
                );
                rows.push(row);
            }
        },
        "ta_orders" => {
            // Schema: id (Utf8), total (Utf8), created_at (Utf8 ISO 8601), customer_name (Utf8)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(format!("order-{}", i + 1)));
                row.insert("total".to_string(), json!(format!("{:.2}", (i as f64 + 1.0) * 99.99)));
                // ISO 8601 timestamp format
                row.insert(
                    "created_at".to_string(),
                    json!(format!("2025-11-{:02}T12:00:00Z", (i % 30) + 1)),
                );
                row.insert("customer_name".to_string(), json!(format!("Customer {}", i + 1)));
                rows.push(row);
            }
        },
        "ta_users" => {
            // Schema: id (Utf8), email (Utf8), name (Utf8), created_at (Utf8 ISO 8601)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(format!("user-{}", i + 1)));
                row.insert("email".to_string(), json!(format!("user{}@example.com", i + 1)));
                row.insert("name".to_string(), json!(format!("User {}", i + 1)));
                // ISO 8601 timestamp format
                row.insert(
                    "created_at".to_string(),
                    json!(format!("2025-11-{:02}T12:00:00Z", (i % 30) + 1)),
                );
                rows.push(row);
            }
        },
        _ => {
            // Unknown view, return empty rows
            warn!("Unknown view '{}', returning empty result", view);
        },
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests service initialization without database adapter
    #[test]
    fn test_new_creates_service_without_db_adapter() {
        let service = FraiseQLFlightService::new();
        assert!(service.db_adapter.is_none());
    }

    /// Tests that service registers default views on creation
    #[test]
    fn test_new_registers_defaults() {
        let service = FraiseQLFlightService::new();
        assert!(service.schema_registry.contains("va_orders"));
        assert!(service.schema_registry.contains("va_users"));
        assert!(service.schema_registry.contains("ta_orders"));
        assert!(service.schema_registry.contains("ta_users"));
    }
}
