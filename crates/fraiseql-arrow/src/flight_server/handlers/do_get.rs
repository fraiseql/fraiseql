//! Handler for the Arrow Flight `do_get` RPC method.
//!
//! Authenticates the caller via session token and dispatches to the appropriate
//! data-fetching path based on the decoded `FlightTicket` variant.

use arrow_flight::Ticket;
use tonic::{Request, Response, Status};
use tracing::info;

use super::super::{
    FlightDataStream, FraiseQLFlightService, extract_session_token, validate_session_token,
};
use crate::ticket::FlightTicket;

/// `do_get` handler: fetch a data stream identified by the supplied ticket.
pub(super) async fn handle(
    svc: &FraiseQLFlightService,
    request: Request<Ticket>,
) -> std::result::Result<Response<FlightDataStream>, Status> {
    // Enforce concurrent stream limit: non-blocking acquire so we immediately
    // reject when at capacity rather than queueing indefinitely.
    let _permit = svc
        .stream_semaphore
        .try_acquire()
        .map_err(|_| tonic::Status::resource_exhausted("Max concurrent Flight streams reached"))?;

    // Validate session token from metadata
    let session_token = extract_session_token(&request)?;
    let secret = svc
        .session_secret
        .as_deref()
        .ok_or_else(|| Status::internal("FLIGHT_SESSION_SECRET not configured"))?;
    let authenticated_user = validate_session_token(&session_token, secret)?;

    info!(
        user_id = %authenticated_user.user_id,
        scopes = ?authenticated_user.scopes,
        "Authenticated do_get request"
    );

    // Extract ticket
    let ticket_bytes = request.into_inner().ticket;
    let ticket = FlightTicket::decode(&ticket_bytes)
        .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

    info!("DoGet called (authenticated): {:?}", ticket);

    // Create security context for RLS filtering
    let security_context = fraiseql_core::security::SecurityContext::from_user(
        authenticated_user,
        uuid::Uuid::new_v4().to_string(),
    );

    match ticket {
        FlightTicket::GraphQLQuery { query, variables } => {
            // Pass security_context to execute_graphql_query for RLS
            let stream = svc.execute_graphql_query(&query, variables, &security_context).await?;
            Ok(Response::new(Box::pin(stream)))
        },
        FlightTicket::OptimizedView {
            view,
            filter,
            order_by,
            limit,
            offset,
        } => {
            // Pass security_context to execute_optimized_view for RLS
            let stream = svc
                .execute_optimized_view(&view, filter, order_by, limit, offset, &security_context)
                .await?;
            Ok(Response::new(Box::pin(stream)))
        },
        FlightTicket::ObserverEvents {
            entity_type,
            start_date,
            end_date,
            limit,
        } => svc.execute_observer_events(&entity_type, start_date, end_date, limit).await,
        FlightTicket::BulkExport {
            table,
            filter,
            limit,
            format,
        } => svc.execute_bulk_export(&table, filter, limit, format, &security_context).await,
        FlightTicket::BatchedQueries { queries } => {
            // Pass security_context for batched query execution with RLS
            let stream = svc.execute_batched_queries(queries, &security_context).await?;
            Ok(Response::new(Box::pin(stream)))
        },
    }
}
