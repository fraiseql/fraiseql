//! HTTP handlers for the changelog and checkpoint REST endpoints.
//!
//! These endpoints expose the `tb_entity_change_log` table and the
//! `observer_checkpoints` table over HTTP, enabling external consumers
//! (e.g. the Python `ChangelogConsumer` SDK) to poll for changes and
//! persist cursor state without direct database access.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ── State ────────────────────────────────────────────────────────────────────

/// Shared state for changelog HTTP handlers.
#[derive(Clone)]
pub struct ChangelogState {
    /// PostgreSQL pool for reading `tb_entity_change_log` and
    /// `observer_checkpoints`.
    pub pool: PgPool,
}

// ── Request / Response types ─────────────────────────────────────────────────

/// Query parameters for `GET /api/observers/changelog`.
#[derive(Debug, Deserialize)]
pub struct ChangelogQuery {
    /// Return entries with `pk_entity_change_log > after_cursor`.
    /// Defaults to 0 (start from beginning).
    #[serde(default)]
    pub after_cursor: i64,
    /// Maximum number of entries to return (default 100, max 1000).
    #[serde(default = "default_changelog_limit")]
    pub limit: i64,
    /// Optional filter by `object_type`.
    pub object_type: Option<String>,
}

const fn default_changelog_limit() -> i64 {
    100
}

/// Maximum entries a single request can fetch.
const MAX_CHANGELOG_LIMIT: i64 = 1_000;

/// A single changelog entry returned over HTTP.
#[derive(Debug, Serialize)]
pub struct ChangelogEntryResponse {
    /// Monotonic bigint cursor — used for polling.
    pub cursor: i64,
    /// Public UUID identity.
    pub id: String,
    /// Organization / tenant (nullable).
    pub org_id: Option<String>,
    /// User who made the change (nullable).
    pub user_id: Option<String>,
    /// Entity type (e.g. "Order").
    pub object_type: String,
    /// Entity instance ID.
    pub object_id: String,
    /// INSERT, UPDATE, DELETE, or NOOP.
    pub modification_type: String,
    /// Change status (nullable).
    pub status: Option<String>,
    /// Raw Debezium envelope `{op, before, after}`.
    pub object_data: serde_json::Value,
    /// Extra metadata (nullable).
    pub metadata: Option<serde_json::Value>,
    /// When the change was recorded (ISO 8601).
    pub created_at: Option<String>,
}

/// Response wrapper for the changelog list endpoint.
#[derive(Debug, Serialize)]
pub struct ChangelogListResponse {
    /// Changelog entries ordered by cursor ASC.
    pub entries: Vec<ChangelogEntryResponse>,
    /// The cursor of the last entry (for use as `after_cursor` on the next poll).
    /// `None` when the result set is empty.
    pub next_cursor: Option<i64>,
}

/// Checkpoint state returned / accepted by the checkpoint endpoints.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointResponse {
    /// Listener identifier.
    pub listener_id: String,
    /// Last successfully processed cursor value.
    pub last_cursor: i64,
    /// When the checkpoint was last updated.
    pub updated_at: Option<String>,
}

/// Request body for `PUT /api/observers/checkpoint/:listener_id`.
#[derive(Debug, Deserialize)]
pub struct SaveCheckpointRequest {
    /// The cursor value to persist.
    pub last_cursor: i64,
}

// ── Row type for sqlx ────────────────────────────────────────────────────────

/// Row shape returned by the changelog query.
type ChangelogRow = (
    i64,                    // pk_entity_change_log
    uuid::Uuid,             // id
    Option<String>,         // fk_customer_org
    Option<String>,         // fk_contact
    String,                 // object_type
    String,                 // object_id
    String,                 // modification_type
    Option<String>,         // change_status
    serde_json::Value,      // object_data
    Option<serde_json::Value>, // extra_metadata
    Option<DateTime<Utc>>,  // created_at
);

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/observers/changelog`
///
/// Poll for new changelog entries after a given cursor position.
pub async fn changelog_list_handler(
    State(state): State<ChangelogState>,
    Query(query): Query<ChangelogQuery>,
) -> impl IntoResponse {
    let limit = query.limit.clamp(1, MAX_CHANGELOG_LIMIT);

    let result = if let Some(ref object_type) = query.object_type {
        sqlx::query_as::<_, ChangelogRow>(
            r"
            SELECT
                pk_entity_change_log, id, fk_customer_org, fk_contact,
                object_type, object_id, modification_type, change_status,
                object_data, extra_metadata, created_at
            FROM core.tb_entity_change_log
            WHERE pk_entity_change_log > $1 AND object_type = $3
            ORDER BY pk_entity_change_log ASC
            LIMIT $2
            ",
        )
        .bind(query.after_cursor)
        .bind(limit)
        .bind(object_type)
        .fetch_all(&state.pool)
        .await
    } else {
        sqlx::query_as::<_, ChangelogRow>(
            r"
            SELECT
                pk_entity_change_log, id, fk_customer_org, fk_contact,
                object_type, object_id, modification_type, change_status,
                object_data, extra_metadata, created_at
            FROM core.tb_entity_change_log
            WHERE pk_entity_change_log > $1
            ORDER BY pk_entity_change_log ASC
            LIMIT $2
            ",
        )
        .bind(query.after_cursor)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
    };

    match result {
        Ok(rows) => {
            let entries: Vec<ChangelogEntryResponse> = rows
                .into_iter()
                .map(
                    |(pk, id, org, contact, obj_type, obj_id, mod_type, status, data, meta, ts)| {
                        ChangelogEntryResponse {
                            cursor: pk,
                            id: id.to_string(),
                            org_id: org,
                            user_id: contact,
                            object_type: obj_type,
                            object_id: obj_id,
                            modification_type: mod_type,
                            status,
                            object_data: data,
                            metadata: meta,
                            created_at: ts.map(|t| t.to_rfc3339()),
                        }
                    },
                )
                .collect();

            let next_cursor = entries.last().map(|e| e.cursor);

            (
                StatusCode::OK,
                Json(ChangelogListResponse {
                    entries,
                    next_cursor,
                }),
            )
                .into_response()
        },
        Err(e) => {
            tracing::error!("Failed to query changelog: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {e}") })),
            )
                .into_response()
        },
    }
}

/// `GET /api/observers/checkpoint/:listener_id`
///
/// Read the current checkpoint for a given listener.
pub async fn checkpoint_get_handler(
    State(state): State<ChangelogState>,
    Path(listener_id): Path<String>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, (String, i64, Option<DateTime<Utc>>)>(
        r"
        SELECT listener_id, last_processed_id, updated_at
        FROM observer_checkpoints
        WHERE listener_id = $1
        ",
    )
    .bind(&listener_id)
    .fetch_optional(&state.pool)
    .await;

    match result {
        Ok(Some((lid, cursor, updated))) => (
            StatusCode::OK,
            Json(CheckpointResponse {
                listener_id: lid,
                last_cursor: cursor,
                updated_at: updated.map(|t| t.to_rfc3339()),
            }),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Checkpoint not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to read checkpoint: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {e}") })),
            )
                .into_response()
        },
    }
}

/// `PUT /api/observers/checkpoint/:listener_id`
///
/// Create or update a checkpoint. Uses `ON CONFLICT … DO UPDATE` (upsert).
pub async fn checkpoint_save_handler(
    State(state): State<ChangelogState>,
    Path(listener_id): Path<String>,
    Json(body): Json<SaveCheckpointRequest>,
) -> impl IntoResponse {
    let result = sqlx::query(
        r"
        INSERT INTO observer_checkpoints
            (listener_id, last_processed_id, last_processed_at, batch_size, event_count, updated_at)
        VALUES ($1, $2, NOW(), 0, 0, NOW())
        ON CONFLICT (listener_id) DO UPDATE SET
            last_processed_id = $2,
            last_processed_at = NOW(),
            updated_at = NOW()
        ",
    )
    .bind(&listener_id)
    .bind(body.last_cursor)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "listener_id": listener_id,
                "last_cursor": body.last_cursor,
                "message": "Checkpoint saved"
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to save checkpoint: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {e}") })),
            )
                .into_response()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_entry_response_serializes() {
        let entry = ChangelogEntryResponse {
            cursor: 42,
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            org_id: Some("acme".to_string()),
            user_id: None,
            object_type: "Order".to_string(),
            object_id: "123".to_string(),
            modification_type: "INSERT".to_string(),
            status: None,
            object_data: serde_json::json!({"op": "c", "after": {"id": 1}}),
            metadata: None,
            created_at: None,
        };
        let json = serde_json::to_value(&entry).expect("serialize");
        assert_eq!(json["cursor"], 42);
        assert_eq!(json["object_type"], "Order");
    }

    #[test]
    fn changelog_list_response_serializes() {
        let response = ChangelogListResponse {
            entries: vec![],
            next_cursor: None,
        };
        let json = serde_json::to_value(&response).expect("serialize");
        assert!(json["next_cursor"].is_null());
        assert_eq!(json["entries"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn checkpoint_response_serializes() {
        let response = CheckpointResponse {
            listener_id: "my_app".to_string(),
            last_cursor: 100,
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_value(&response).expect("serialize");
        assert_eq!(json["last_cursor"], 100);
    }

    #[test]
    fn save_checkpoint_request_deserializes() {
        let json = r#"{"last_cursor": 42}"#;
        let req: SaveCheckpointRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.last_cursor, 42);
    }

    #[test]
    fn default_changelog_limit_is_100() {
        assert_eq!(default_changelog_limit(), 100);
    }

    #[test]
    fn max_limit_is_1000() {
        assert_eq!(MAX_CHANGELOG_LIMIT, 1_000);
    }
}
