//! Action handlers for the Arrow Flight service.
//!
//! Contains handlers for `do_action` and `list_actions` RPC methods,
//! covering admin operations such as cache clearing and schema refresh.

use arrow_flight::{Action, ActionType, Empty};
use tonic::{Request, Response, Status};
use tracing::info;

use super::super::{
    ActionResultStream, ActionTypeStream, FraiseQLFlightService, extract_session_token,
    validate_session_token,
};

/// `do_action` handler: executes a named admin operation on behalf of an authenticated client.
pub(super) async fn do_action(
    svc: &FraiseQLFlightService,
    request: Request<Action>,
) -> std::result::Result<Response<ActionResultStream>, Status> {
    // Validate session token for admin operations
    let session_token = extract_session_token(&request)?;
    let secret = svc
        .session_secret
        .as_deref()
        .ok_or_else(|| Status::internal("FLIGHT_SESSION_SECRET not configured"))?;
    let authenticated_user = validate_session_token(&session_token, secret)?;

    let action = request.into_inner();
    info!(
        user_id = %authenticated_user.user_id,
        action_type = action.r#type,
        "Authenticated do_action request"
    );

    let stream = match action.r#type.as_str() {
        "ClearCache" => {
            // Admin-only action - verify "admin" scope
            if !authenticated_user.scopes.contains(&"admin".to_string()) {
                return Err(Status::permission_denied(
                    "Cache invalidation requires 'admin' scope",
                ));
            }

            svc.handle_clear_cache()
        },
        "RefreshSchemaRegistry" => {
            // Admin-only action - verify "admin" scope
            if !authenticated_user.scopes.contains(&"admin".to_string()) {
                return Err(Status::permission_denied(
                    "Schema registry refresh requires 'admin' scope",
                ));
            }

            svc.handle_refresh_schema_registry()
        },
        "GetSchemaVersions" => {
            // Admin-only action - verify "admin" scope
            if !authenticated_user.scopes.contains(&"admin".to_string()) {
                return Err(Status::permission_denied(
                    "GetSchemaVersions requires 'admin' scope",
                ));
            }

            svc.handle_get_schema_versions()
        },
        "HealthCheck" => {
            // Public action - no special authorization needed beyond authentication
            svc.handle_health_check()
        },
        _ => {
            return Err(Status::invalid_argument(format!(
                "Unknown action: {}",
                action.r#type
            )));
        },
    };

    Ok(Response::new(Box::pin(stream)))
}

/// `list_actions` handler: returns the set of supported Flight actions.
pub(super) async fn list_actions(
    _svc: &FraiseQLFlightService,
    _request: Request<Empty>,
) -> std::result::Result<Response<ActionTypeStream>, Status> {
    info!("ListActions called");

    let actions = vec![
        Ok(ActionType {
            r#type:      "ClearCache".to_string(),
            description: "Clear all cached query results".to_string(),
        }),
        Ok(ActionType {
            r#type:      "RefreshSchemaRegistry".to_string(),
            description: "Reload schema definitions from database".to_string(),
        }),
        Ok(ActionType {
            r#type:      "GetSchemaVersions".to_string(),
            description: "Get current schema versions and metadata".to_string(),
        }),
        Ok(ActionType {
            r#type:      "HealthCheck".to_string(),
            description: "Return service health status".to_string(),
        }),
    ];

    let stream = futures::stream::iter(actions);
    Ok(Response::new(Box::pin(stream)))
}
