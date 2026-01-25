//! FraiseQL Arrow Flight service implementation.
//!
//! This module provides the core gRPC service that handles Flight RPC calls.
//! In Phase 9.1, it implements the basic server skeleton with empty data streams.
//! Phase 9.2+ will add actual query execution and data streaming.

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
    /// Phase 9.2: Placeholder implementation returns empty stream.
    /// Phase 9.3+: Will integrate with fraiseql-core query executor.
    ///
    /// # TODO
    ///
    /// - Add query executor reference to FraiseQLFlightService struct
    /// - Call fraiseql_core::arrow_executor::execute_query_as_arrow()
    /// - Convert RecordBatches to FlightData
    /// - Stream batches to client
    async fn execute_graphql_query(
        &self,
        _query: &str,
        _variables: Option<serde_json::Value>,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        // TODO: Phase 9.3+ - Execute actual query
        // let executor = &self.query_executor;
        // let batches = fraiseql_core::arrow_executor::execute_query_as_arrow(
        //     executor,
        //     query,
        //     variables,
        //     10_000, // batch size
        // ).await
        //   .map_err(|e| Status::internal(format!("Query execution failed: {}", e)))?;
        //
        // Convert RecordBatches to FlightData and stream
        //  let stream = stream::iter(batches.into_iter().map(|batch| {
        //      let flight_data = ... // Convert RecordBatch to FlightData
        //      Ok(flight_data)
        //  }));

        // Placeholder: Return empty stream
        let stream = futures::stream::empty();
        Ok(stream)
    }

    /// Execute optimized query on pre-compiled va_* view.
    ///
    /// Phase 9.3: Fast path for compiler-generated Arrow views.
    /// Uses pre-compiled Arrow schemas, eliminating runtime type inference.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "va_orders")
    /// * `filter` - Optional WHERE clause
    /// * `order_by` - Optional ORDER BY clause
    /// * `limit` - Optional LIMIT
    /// * `offset` - Optional OFFSET for pagination
    ///
    /// # TODO
    ///
    /// - Load pre-compiled Arrow schema from metadata
    /// - Build optimized SQL query
    /// - Execute via database adapter
    /// - Minimal row → Arrow conversion (types already match)
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

        // 3. Execute query via database adapter
        let db_rows = if let Some(db) = &self.db_adapter {
            // Use real database adapter
            db.execute_raw_query(&sql)
                .await
                .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
        } else {
            // Fall back to placeholder (for backward compatibility and testing)
            execute_placeholder_query(view, limit)
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
    /// Will be implemented in Phase 10 with JWT/API key authentication.
    async fn handshake(
        &self,
        _request: Request<Streaming<HandshakeRequest>>,
    ) -> std::result::Result<Response<Self::HandshakeStream>, Status> {
        info!("Handshake called (not implemented)");
        Err(Status::unimplemented("Handshake not implemented yet"))
    }

    /// List available datasets/queries.
    ///
    /// In Phase 9.1, this returns an empty list for testing.
    /// In Phase 9.2+, this will list available GraphQL queries, observer events, etc.
    async fn list_flights(
        &self,
        _request: Request<Criteria>,
    ) -> std::result::Result<Response<Self::ListFlightsStream>, Status> {
        info!("ListFlights called");

        // TODO: Return actual available datasets
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
            FlightTicket::OptimizedView { view, .. } => {
                // Phase 9.3: Load pre-compiled Arrow schema for optimized view
                self.schema_registry.get(&view).map_err(|e| {
                    Status::not_found(format!("Schema not found for view {view}: {e}"))
                })?
            },
            FlightTicket::BulkExport { .. } => {
                // Will be implemented in Phase 9.4
                return Err(Status::unimplemented("BulkExport not implemented yet"));
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
    /// In Phase 9.1, this returns empty streams.
    /// In Phase 9.2+, this will execute queries and stream Arrow RecordBatches.
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
                // Phase 9.2: Execute query and stream batches (placeholder for now)
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
                // Phase 9.3: Optimized path using pre-compiled va_* views
                let stream =
                    self.execute_optimized_view(&view, filter, order_by, limit, offset).await?;
                Ok(Response::new(Box::pin(stream)))
            },
            FlightTicket::ObserverEvents { .. } => {
                // Phase 9.3: Will implement observer event streaming
                Err(Status::unimplemented("Observer events not implemented yet (Phase 9.3)"))
            },
            FlightTicket::BulkExport { .. } => {
                // Phase 9.4: Will implement bulk exports
                Err(Status::unimplemented("Bulk export not implemented yet (Phase 9.4)"))
            },
        }
    }

    /// Upload data stream (for client-to-server data transfer).
    ///
    /// Not needed for Phase 9.1-9.3 (we're focused on server→client).
    /// May be useful in Phase 9.4+ for bulk imports.
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
    /// actually fetching it. Will be implemented in Phase 9.2+.
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
/// TODO: Replace with actual database adapter when integrated with fraiseql-server.
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
