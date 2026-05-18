//! Realtime REST endpoints: broadcast publish.

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::subscriptions::BroadcastManager;

/// Shared state for the realtime broadcast endpoint.
#[derive(Clone)]
pub struct BroadcastState {
    manager: Arc<BroadcastManager>,
}

impl BroadcastState {
    /// Create new broadcast state wrapping the given manager.
    #[must_use]
    pub const fn new(manager: Arc<BroadcastManager>) -> Self {
        Self { manager }
    }
}

/// Request body for `POST /realtime/v1/broadcast`.
#[derive(Debug, Deserialize)]
pub struct BroadcastRequest {
    /// Named channel to publish to (e.g., "chat:room1").
    pub channel: String,

    /// Event name (e.g., "message", "typing").
    pub event: String,

    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
}

/// Response body for broadcast publish.
#[derive(Debug, Serialize)]
pub struct BroadcastResponse {
    /// Number of subscribers that received the message.
    pub receivers: usize,
}

/// `POST /realtime/v1/broadcast` — publish a message to a named channel.
pub async fn broadcast_handler(
    State(state): State<BroadcastState>,
    Json(req): Json<BroadcastRequest>,
) -> impl IntoResponse {
    if req.channel.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "channel must not be empty"})),
        )
            .into_response();
    }

    if req.event.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "event must not be empty"})),
        )
            .into_response();
    }

    match state.manager.publish(&req.channel, req.event, req.payload).await {
        Ok(receivers) => (StatusCode::OK, Json(BroadcastResponse { receivers })).into_response(),
        Err(e) => {
            let status =
                StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            (status, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        },
    }
}
