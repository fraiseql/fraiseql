//! FraiseQL Arrow Flight service implementation.
//!
//! This module provides the core gRPC service that handles Flight RPC calls.
//! In Phase 9.1, it implements the basic server skeleton with empty data streams.
//! Phase 9.2+ will add actual query execution and data streaming.

use crate::metadata::SchemaRegistry;
use crate::schema::{graphql_result_schema, observer_event_schema};
use crate::ticket::FlightTicket;
use arrow_flight::{
    flight_service_server::{FlightService, FlightServiceServer},
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo, PollInfo,
    HandshakeRequest, HandshakeResponse, PutResult, SchemaResult, Ticket,
};
use futures::Stream;
use std::pin::Pin;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

type HandshakeStream = Pin<Box<dyn Stream<Item = std::result::Result<HandshakeResponse, Status>> + Send>>;
type FlightInfoStream = Pin<Box<dyn Stream<Item = std::result::Result<FlightInfo, Status>> + Send>>;
type FlightDataStream = Pin<Box<dyn Stream<Item = std::result::Result<FlightData, Status>> + Send>>;
type PutResultStream = Pin<Box<dyn Stream<Item = std::result::Result<PutResult, Status>> + Send>>;
type ActionResultStream = Pin<Box<dyn Stream<Item = std::result::Result<arrow_flight::Result, Status>> + Send>>;
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
    // Future: Will hold references to query executor, observer system, etc.
}

impl FraiseQLFlightService {
    /// Create a new Flight service.
    #[must_use]
    pub fn new() -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults(); // Register av_orders, av_users, etc.

        Self { schema_registry }
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
    ) -> std::result::Result<
        impl Stream<Item = std::result::Result<FlightData, Status>>,
        Status,
    > {
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

    /// Execute optimized query on pre-compiled av_* view.
    ///
    /// Phase 9.3: Fast path for compiler-generated Arrow views.
    /// Uses pre-compiled Arrow schemas, eliminating runtime type inference.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "av_orders")
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
    ) -> std::result::Result<
        impl Stream<Item = std::result::Result<FlightData, Status>>,
        Status,
    > {
        // 1. Load pre-compiled Arrow schema from registry
        let _schema = self
            .schema_registry
            .get(view)
            .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}")))?;

        // 2. Build optimized SQL query
        let _sql = build_optimized_sql(view, filter, order_by, limit, offset);

        // 3. TODO: Execute query via database adapter
        //    let rows = db.query(&sql).await?;
        //
        // 4. TODO: Fast conversion (types already aligned)
        //    let batches = fast_convert(rows, schema);
        //
        // 5. TODO: Stream batches
        //    stream_batches(batches)

        // Placeholder: Return empty stream until database integration complete
        let stream = futures::stream::empty();
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
    type HandshakeStream = HandshakeStream;
    type ListFlightsStream = FlightInfoStream;
    type DoGetStream = FlightDataStream;
    type DoPutStream = PutResultStream;
    type DoActionStream = ActionResultStream;
    type ListActionsStream = ActionTypeStream;
    type DoExchangeStream = FlightDataStream;

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
                self.schema_registry
                    .get(&view)
                    .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}")))?
            }
            FlightTicket::BulkExport { .. } => {
                // Will be implemented in Phase 9.4
                return Err(Status::unimplemented("BulkExport not implemented yet"));
            }
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
            }
            FlightTicket::OptimizedView {
                view,
                filter,
                order_by,
                limit,
                offset,
            } => {
                // Phase 9.3: Optimized path using pre-compiled av_* views
                let stream = self
                    .execute_optimized_view(&view, filter, order_by, limit, offset)
                    .await?;
                Ok(Response::new(Box::pin(stream)))
            }
            FlightTicket::ObserverEvents { .. } => {
                // Phase 9.3: Will implement observer event streaming
                Err(Status::unimplemented(
                    "Observer events not implemented yet (Phase 9.3)",
                ))
            }
            FlightTicket::BulkExport { .. } => {
                // Phase 9.4: Will implement bulk exports
                Err(Status::unimplemented(
                    "Bulk export not implemented yet (Phase 9.4)",
                ))
            }
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

/// Build optimized SQL query for av_* view.
///
/// # Arguments
///
/// * `view` - View name (e.g., "av_orders")
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
///     "av_orders",
///     Some("created_at > '2026-01-01'"),
///     Some("created_at DESC"),
///     Some(100),
///     Some(0)
/// );
/// // Returns: "SELECT * FROM av_orders WHERE created_at > '2026-01-01' ORDER BY created_at DESC LIMIT 100 OFFSET 0"
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
