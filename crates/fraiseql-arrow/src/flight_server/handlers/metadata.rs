//! Metadata handlers for the Arrow Flight service.
//!
//! Contains handlers for: `handshake`, `list_flights`, `get_schema`,
//! `get_flight_info`, and `poll_flight_info`.

use arrow::{
    datatypes::SchemaRef,
    ipc::writer::{DictionaryTracker, IpcDataGenerator, IpcWriteOptions},
};
use arrow_flight::{
    Criteria, FlightDescriptor, FlightInfo, HandshakeRequest, HandshakeResponse, PollInfo,
    SchemaResult,
};
use prost::bytes::Bytes;
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, warn};

use super::super::{
    FlightInfoStream, FraiseQLFlightService, HandshakeStream, create_session_token,
    map_security_error_to_status,
};
use crate::ticket::FlightTicket;

/// Resolve the Arrow schema for a `FlightTicket`, enforcing unsupported-ticket errors.
fn ticket_to_schema(
    svc: &FraiseQLFlightService,
    ticket: FlightTicket,
) -> std::result::Result<SchemaRef, Status> {
    match ticket {
        // #1037: this returned the hardcoded placeholder `{id: Utf8, data: Utf8}`,
        // which `do_get` never produces — that path emits either a single `result`
        // column or a schema derived from the result rows. A client planning off
        // GetSchema/GetFlightInfo bound columns that never arrived, and the only
        // test touching the shape asserted the placeholder's field names as ground
        // truth. Refusing is honest; advertising a schema the stream contradicts is
        // not.
        //
        // This refusal is about what the server can do today, not about GraphQL: a
        // result shape *is* determined by the query and the schema. Deriving it here
        // would need the projection ahead of execution, which `QueryExecutor` — type
        // erased, returning `serde_json::Value` — does not expose.
        FlightTicket::GraphQLQuery { .. } => Err(Status::unimplemented(
            "GetSchema for GraphQLQuery is not implemented: this server does not \
             derive a result schema ahead of execution. Read the schema from the \
             header of the do_get response stream.",
        )),
        // #1038: this advertised a concrete 8-field schema for a ticket the server
        // can never serve — `event_storage` is `None` at every constructor and
        // nothing calls the setter, so `do_get` short-circuits with
        // `failed_precondition`. The schema also disagreed with what
        // `HistoricalEvent` actually serializes in three places.
        FlightTicket::ObserverEvents { .. } => Err(Status::unimplemented(
            "ObserverEvents is not implemented: no event storage is wired to the \
             Flight server, so no event stream can be produced.",
        )),
        FlightTicket::OptimizedView { view, .. } => svc
            .schema_registry
            .get(&view)
            .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}"))),
        FlightTicket::BulkExport { .. } => Err(Status::unimplemented(
            "BulkExport schema introspection is not supported: the schema varies by \
             table and export format. Use do_get with a BulkExport ticket to export \
             data directly; the Arrow schema is included in the response stream.",
        )),
        FlightTicket::BatchedQueries { .. } => Err(Status::unimplemented(
            "GetSchema for BatchedQueries returns per-query schemas in the data stream",
        )),
    }
}

/// Serialize an Arrow schema to IPC bytes.
fn schema_to_ipc_bytes(schema: &SchemaRef) -> Bytes {
    let options = IpcWriteOptions::default();
    let data_gen = IpcDataGenerator::default();
    let mut dict_tracker = DictionaryTracker::new(false);
    data_gen
        .schema_to_bytes_with_dictionary_tracker(schema, &mut dict_tracker, &options)
        .ipc_message
        .into()
}

/// Handshake handler: JWT authentication that returns a short-lived session token.
#[allow(clippy::cognitive_complexity)] // Reason: authentication protocol with multiple validation steps and error branches
pub(super) async fn handshake(
    svc: &FraiseQLFlightService,
    mut request: Request<Streaming<HandshakeRequest>>,
) -> std::result::Result<Response<HandshakeStream>, Status> {
    info!("Handshake called - JWT authentication");

    // Get the first handshake request which contains the JWT
    let handshake_request = match request.get_mut().message().await {
        Ok(Some(req)) => req,
        Ok(None) => {
            warn!("Handshake: No request message received");
            return Err(Status::invalid_argument("No handshake request provided"));
        },
        Err(e) => {
            warn!("Handshake: Error reading request: {}", e);
            return Err(Status::internal(format!("Error reading handshake: {}", e)));
        },
    };

    // Extract JWT from payload
    let payload_str = String::from_utf8_lossy(&handshake_request.payload);

    // Extract token from "Bearer <token>" format
    let token = if let Some(t) = payload_str.strip_prefix("Bearer ") {
        t.to_string()
    } else {
        warn!("Handshake: Missing 'Bearer' prefix in authentication payload");
        return Err(Status::unauthenticated("Invalid authentication format"));
    };

    // CRITICAL: OIDC validator MUST be configured - authentication is mandatory
    let validator = svc.oidc_validator.as_ref().ok_or_else(|| {
        error!(
            "OIDC validator not configured - authentication is mandatory. \
             Set FLIGHT_OIDC_* environment variables."
        );
        Status::internal("Authentication not configured. Contact system administrator.")
    })?;

    // Validate JWT
    let authenticated_user = match validator.validate_token(&token).await {
        Ok(user) => {
            info!(user_id = %user.user_id, "JWT validation successful");
            user
        },
        Err(e) => {
            warn!(error = %e, "JWT validation failed");
            return Err(map_security_error_to_status(e));
        },
    };

    // Create session token using the secret cached at service startup
    let secret = svc.session_secret.as_deref().ok_or_else(|| {
        Status::internal(
            "FLIGHT_SESSION_SECRET not configured; set the environment variable \
             or call FraiseQLFlightService::with_session_secret() before use",
        )
    })?;
    let session_token = create_session_token(&authenticated_user, secret)?;
    info!(user_id = %authenticated_user.user_id, "Handshake complete");

    // Create response with session token
    let response = HandshakeResponse {
        protocol_version: 0,
        payload:          session_token.as_bytes().to_vec().into(),
    };

    let stream = futures::stream::once(async move { Ok(response) });
    Ok(Response::new(Box::pin(stream) as HandshakeStream))
}

/// `list_flights` handler: returns metadata about available pre-compiled Arrow views.
pub(super) async fn list_flights(
    svc: &FraiseQLFlightService,
    _request: Request<Criteria>,
) -> std::result::Result<Response<FlightInfoStream>, Status> {
    info!("ListFlights called");

    let mut flight_infos = Vec::new();

    for view_name in &["va_orders", "va_users", "ta_orders", "ta_users"] {
        if let Ok(schema) = svc.schema_registry.get(view_name) {
            let descriptor = FlightDescriptor {
                r#type: 1, // PATH
                path:   vec![(*view_name).to_string()],
                cmd:    b"".to_vec().into(),
            };

            let flight_info = FlightInfo {
                schema:            schema_to_ipc_bytes(&schema),
                flight_descriptor: Some(descriptor),
                endpoint:          vec![],
                total_records:     -1, // Unknown until executed
                total_bytes:       -1,
                ordered:           false,
                app_metadata:      vec![].into(),
            };

            flight_infos.push(Ok(flight_info));
        }
    }

    info!("ListFlights returning {} datasets", flight_infos.len());
    Ok(Response::new(Box::pin(futures::stream::iter(flight_infos))))
}

/// `get_schema` handler: returns the Arrow schema for a descriptor without fetching data.
pub(super) async fn get_schema(
    svc: &FraiseQLFlightService,
    request: Request<FlightDescriptor>,
) -> std::result::Result<Response<SchemaResult>, Status> {
    let descriptor = request.into_inner();
    info!("GetSchema called: {:?}", descriptor);

    if descriptor.path.is_empty() {
        return Err(Status::invalid_argument("Empty flight descriptor path"));
    }

    let ticket = FlightTicket::decode(descriptor.path[0].as_bytes())
        .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

    let schema = ticket_to_schema(svc, ticket)?;

    Ok(Response::new(SchemaResult {
        schema: schema_to_ipc_bytes(&schema),
    }))
}

/// `get_flight_info` handler: returns metadata about a dataset without fetching it.
pub(super) async fn get_flight_info(
    svc: &FraiseQLFlightService,
    request: Request<FlightDescriptor>,
) -> std::result::Result<Response<FlightInfo>, Status> {
    let descriptor = request.into_inner();
    info!("GetFlightInfo called: {:?}", descriptor);

    if descriptor.path.is_empty() {
        return Err(Status::invalid_argument("Empty flight descriptor path"));
    }

    let ticket = FlightTicket::decode(descriptor.path[0].as_bytes())
        .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

    info!("GetFlightInfo decoded ticket: {:?}", ticket);

    let schema = ticket_to_schema(svc, ticket)?;

    let flight_info = FlightInfo {
        schema:            schema_to_ipc_bytes(&schema),
        flight_descriptor: Some(descriptor),
        endpoint:          vec![], // Data retrieved via DoGet with same descriptor
        total_records:     -1,     // Unknown until executed
        total_bytes:       -1,     // Unknown until executed
        ordered:           false,
        app_metadata:      vec![].into(),
    };

    info!("GetFlightInfo returning schema for ticket");
    Ok(Response::new(flight_info))
}

/// `poll_flight_info` handler: synchronous implementation that delegates to `get_flight_info`.
///
/// Sets `progress = 1.0` and `flight_descriptor = None` to signal to the client
/// that the result is immediately available and no further polling is needed.
pub(super) async fn poll_flight_info(
    svc: &FraiseQLFlightService,
    request: Request<FlightDescriptor>,
) -> std::result::Result<Response<PollInfo>, Status> {
    let descriptor = request.into_inner();
    info!("PollFlightInfo called: {:?}", descriptor);

    // Reuse get_flight_info logic to build the FlightInfo.
    let flight_info = get_flight_info(svc, Request::new(descriptor)).await?.into_inner();

    // flight_descriptor = None signals "complete — no need to poll again".
    // progress = 1.0 confirms 100 % complete.
    Ok(Response::new(PollInfo {
        info:              Some(flight_info),
        flight_descriptor: None,
        progress:          Some(1.0),
        expiration_time:   None,
    }))
}
