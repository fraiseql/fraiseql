//! Realtime monitor endpoints for the Studio dashboard.
//!
//! Routes under `/admin/v1/realtime/*` expose connection counts, broadcast
//! channels, presence rooms, and CDC replication lag. All routes read
//! from existing realtime state — no new state is introduced. Protected
//! by the admin bearer token middleware.

use axum::{Json, extract::State, response::IntoResponse};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::graphql::app_state::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Presence room summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceRoom {
    /// Room name / identifier.
    pub room: String,
    /// Current member count in the room.
    pub members: u32,
}

/// Realtime statistics snapshot agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeStatsResponse {
    /// Total active `WebSocket` connections.
    pub connections: u32,
    /// Active broadcast channel names.
    pub channels: Vec<String>,
    /// Active presence rooms with member counts.
    pub presence_rooms: Vec<PresenceRoom>,
    /// CDC replication lag in milliseconds, or `None` if CDC is not configured.
    pub cdc_lag_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /admin/v1/realtime/stats` — aggregated realtime snapshot.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn stats_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // Placeholder — not yet wired to PresenceManager / BroadcastManager.
    Json(RealtimeStatsResponse {
        connections: 0,
        channels: vec![],
        presence_rooms: vec![],
        cdc_lag_ms: None,
    })
}

/// `GET /admin/v1/realtime/broadcast` — active broadcast channels with subscriber counts.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn broadcast_channels_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(serde_json::json!({ "channels": [] }))
}

/// `GET /admin/v1/realtime/presence` — presence rooms with member counts.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn presence_rooms_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(serde_json::json!({ "presence_rooms": [] }))
}

/// `GET /admin/v1/realtime/cdc` — CDC replication lag from the event bridge.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn cdc_lag_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(serde_json::json!({ "cdc_lag_ms": null }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::*;

    #[test]
    fn test_realtime_stats_serializes() {
        let resp = RealtimeStatsResponse {
            connections: 5,
            channels: vec!["users".to_string()],
            presence_rooms: vec![PresenceRoom {
                room: "lobby".to_string(),
                members: 3,
            }],
            cdc_lag_ms: Some(10),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"connections\""));
        assert!(json.contains("\"channels\""));
        assert!(json.contains("\"presence_rooms\""));
        assert!(json.contains("\"cdc_lag_ms\""));
    }
}
