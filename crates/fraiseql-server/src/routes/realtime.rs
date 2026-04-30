//! Realtime REST endpoints: broadcast publish.

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
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
        Ok(receivers) => {
            (StatusCode::OK, Json(BroadcastResponse { receivers })).into_response()
        }
        Err(e) => {
            let status = StatusCode::from_u16(e.status_code())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            (status, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use axum::{
        Router,
        body::Body,
        http::Request,
        routing::post,
    };
    use tower::ServiceExt;

    use super::*;
    use crate::subscriptions::BroadcastConfig;

    fn test_app() -> Router {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));
        let state = BroadcastState::new(manager);
        Router::new()
            .route("/realtime/v1/broadcast", post(broadcast_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_broadcast_publish_ok() {
        let app = test_app();

        let body = serde_json::json!({
            "channel": "room:1",
            "event": "message",
            "payload": {"text": "hello"}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["receivers"], 0); // no subscribers
    }

    #[tokio::test]
    async fn test_broadcast_empty_channel_rejected() {
        let app = test_app();

        let body = serde_json::json!({
            "channel": "",
            "event": "message",
            "payload": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_broadcast_empty_event_rejected() {
        let app = test_app();

        let body = serde_json::json!({
            "channel": "room:1",
            "event": "",
            "payload": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_broadcast_with_subscriber() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));
        let state = BroadcastState::new(manager.clone());
        let app = Router::new()
            .route("/realtime/v1/broadcast", post(broadcast_handler))
            .with_state(state);

        // Subscribe first
        let _rx = manager.subscribe("room:1").await.unwrap();

        let body = serde_json::json!({
            "channel": "room:1",
            "event": "message",
            "payload": {"text": "hello"}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["receivers"], 1);
    }
}
