//! `FlightService` trait implementation for `FraiseQLFlightService`.
//!
//! This file contains only the thin `#[tonic::async_trait]` impl block that
//! satisfies the `FlightService` trait.  All business logic lives in the
//! sub-modules declared below; each handler simply delegates to the
//! corresponding `handle` (or named) function.

mod actions;
mod do_exchange;
mod do_get;
mod do_put;
mod metadata;
mod send_helpers;

use arrow_flight::{
    Action, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo, HandshakeRequest, PollInfo,
    SchemaResult, Ticket,
    flight_service_server::FlightService,
};
use tonic::{Request, Response, Status, Streaming};

use super::{
    ActionResultStream, ActionTypeStream, FlightDataStream, FlightInfoStream,
    FraiseQLFlightService, HandshakeStream, PutResultStream,
};

#[tonic::async_trait]
impl FlightService for FraiseQLFlightService {
    type DoActionStream = ActionResultStream;
    type DoExchangeStream = FlightDataStream;
    type DoGetStream = FlightDataStream;
    type DoPutStream = PutResultStream;
    type HandshakeStream = HandshakeStream;
    type ListActionsStream = ActionTypeStream;
    type ListFlightsStream = FlightInfoStream;

    /// Handshake for JWT authentication.
    ///
    /// Extracts JWT token from client request and validates it.
    /// Returns a session token on success for authenticated Flight requests.
    ///
    /// # Request Format
    ///
    /// Client sends `HandshakeRequest` with payload in "Bearer <`JWT_TOKEN`>" format.
    ///
    /// # Response
    ///
    /// Returns `HandshakeResponse` with:
    /// - `protocol_version`: Arrow Flight protocol version
    /// - `payload`: Session token for authenticated requests
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - JWT token is missing or malformed
    /// - Token signature is invalid
    /// - Token is expired
    async fn handshake(
        &self,
        request: Request<Streaming<HandshakeRequest>>,
    ) -> std::result::Result<Response<Self::HandshakeStream>, Status> {
        metadata::handshake(self, request).await
    }

    /// List available datasets/queries.
    ///
    /// Returns information about available pre-compiled Arrow views and optimized queries.
    async fn list_flights(
        &self,
        request: Request<Criteria>,
    ) -> std::result::Result<Response<Self::ListFlightsStream>, Status> {
        metadata::list_flights(self, request).await
    }

    /// Get schema for a dataset without fetching data.
    ///
    /// This is used by clients to inspect the schema before fetching data.
    async fn get_schema(
        &self,
        request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<SchemaResult>, Status> {
        metadata::get_schema(self, request).await
    }

    /// Fetch data stream (main data retrieval method).
    ///
    /// Requires authenticated session token from handshake.
    /// All queries require valid session tokens and pass security context to executor for RLS.
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> std::result::Result<Response<Self::DoGetStream>, Status> {
        do_get::handle(self, request).await
    }

    /// Upload data stream (for client-to-server data transfer).
    ///
    /// Requires authenticated session token from handshake.
    /// Authenticated data uploads with RLS checks.
    async fn do_put(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoPutStream>, Status> {
        do_put::handle(self, request).await
    }

    /// Execute an action (RPC method for operations beyond data transfer).
    ///
    /// Requires authenticated session token from handshake.
    /// Admin operations require appropriate scopes.
    ///
    /// Supported actions:
    /// - `ClearCache`: Clear all cached query results (requires "admin" scope)
    /// - `RefreshSchemaRegistry`: Reload schema definitions (requires "admin" scope)
    /// - `HealthCheck`: Return service health status (public, no auth required beyond session
    ///   token)
    async fn do_action(
        &self,
        request: Request<Action>,
    ) -> std::result::Result<Response<Self::DoActionStream>, Status> {
        actions::do_action(self, request).await
    }

    /// List available actions.
    ///
    /// Returns the list of supported Flight actions for admin operations.
    async fn list_actions(
        &self,
        request: Request<Empty>,
    ) -> std::result::Result<Response<Self::ListActionsStream>, Status> {
        actions::list_actions(self, request).await
    }

    /// Bidirectional streaming with correlation ID matching.
    ///
    /// Supports request/response operations in a single bidirectional stream:
    /// - Query: Execute GraphQL queries and return Arrow-encoded results
    /// - Upload: Insert data batches into tables
    /// - Subscribe: Stream entity change events (deferred to v2.1)
    ///
    /// Each message includes a `correlation_id` to match requests to responses.
    async fn do_exchange(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoExchangeStream>, Status> {
        do_exchange::handle(self, request).await
    }

    /// Get flight info for a descriptor (metadata about available data).
    ///
    /// This method provides metadata about what data is available without
    /// actually fetching it.
    ///
    /// Returns `FlightInfo` containing schema and endpoint information for a specified
    /// dataset (view, query, or observer events).
    ///
    /// # Request Format
    ///
    /// `FlightDescriptor` containing encoded `FlightTicket` in the path
    ///
    /// # Response
    ///
    /// `FlightInfo` with:
    /// - `schema`: Arrow schema in IPC format
    /// - `flight_descriptor`: Echo of request descriptor
    /// - `endpoint`: Empty (data retrieved via `DoGet` with same descriptor)
    /// - `total_records`: -1 (unknown until executed)
    /// - `total_bytes`: -1 (unknown until executed)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Descriptor path is empty
    /// - Ticket cannot be decoded
    /// - Schema not found for requested view
    async fn get_flight_info(
        &self,
        request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<FlightInfo>, Status> {
        metadata::get_flight_info(self, request).await
    }

    /// Poll for flight info (synchronous implementation).
    ///
    /// Executes the query inline and returns a completed `PollInfo` with
    /// `flight_descriptor = None`, which signals to the client that the result
    /// is immediately available and no further polling is needed.
    ///
    /// `progress` is set to `1.0` (100 %) to reflect that execution is complete.
    ///
    /// This satisfies the Arrow Flight SQL 1.2 contract for synchronous servers:
    /// a server that always completes within the first call is fully spec-compliant.
    async fn poll_flight_info(
        &self,
        request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<PollInfo>, Status> {
        metadata::poll_flight_info(self, request).await
    }
}
