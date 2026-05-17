//! Admin query-stats endpoints.
//!
//! Surfaces database-level query performance statistics via:
//! - `GET /api/v1/admin/query-stats` — top-N queries by execution time
//! - `GET /api/v1/admin/query-stats/{queryid}` — single query detail
//! - `POST /api/v1/admin/query-stats/reset` — reset statistics (PG only)

use axum::{
    Json,
    extract::{Path, Query, State},
};
use fraiseql_core::db::{DatabaseType, QueryStatEntry, traits::DatabaseAdapter};
use serde::{Deserialize, Serialize};

use crate::routes::{
    api::types::{ApiError, ApiResponse},
    graphql::AppState,
};

/// Query parameters for the stats listing endpoint.
#[derive(Debug, Deserialize)]
pub struct QueryStatsParams {
    /// Maximum number of entries to return (default 20, clamped to 1..=100).
    pub limit: Option<u32>,
}

/// Response payload for the query-stats listing endpoint.
#[derive(Debug, Serialize)]
pub struct QueryStatsResponse {
    /// Which database backend produced this data.
    pub database_type:   String,
    /// Whether this backend supports query stats at all.
    pub stats_available: bool,
    /// The query statistics entries.
    pub entries:         Vec<QueryStatEntry>,
    /// Optional informational message (e.g., extension not installed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message:         Option<String>,
}

/// Response payload for a single query detail.
#[derive(Debug, Serialize)]
pub struct QueryStatsDetailResponse {
    /// Which database backend produced this data.
    pub database_type: String,
    /// The query statistics entry.
    pub entry:         QueryStatEntry,
}

/// Response payload for the reset endpoint.
#[derive(Debug, Serialize)]
pub struct QueryStatsResetResponse {
    /// Confirmation message.
    pub message: String,
}

/// `GET /api/v1/admin/query-stats`
///
/// Returns top-N queries by total execution time.
///
/// # Errors
///
/// Returns `ApiError` with `INTERNAL_ERROR` if the database query fails.
pub async fn query_stats_handler<A: DatabaseAdapter + 'static>(
    State(state): State<AppState<A>>,
    Query(params): Query<QueryStatsParams>,
) -> Result<Json<ApiResponse<QueryStatsResponse>>, ApiError> {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let executor = state.executor();
    let adapter = executor.adapter();
    let db_type = adapter.database_type();

    let stats_available = !matches!(db_type, DatabaseType::SQLite);

    let entries = adapter
        .query_stats(limit)
        .await
        .map_err(|e| ApiError::internal_error(format!("Failed to fetch query stats: {e}")))?;

    let message = if !stats_available {
        Some("Query stats are not supported by SQLite".to_string())
    } else if entries.is_empty() {
        Some(
            "No query stats recorded yet (extension may not be installed or no queries executed)"
                .to_string(),
        )
    } else {
        None
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   QueryStatsResponse {
            database_type: db_type.to_string(),
            stats_available,
            entries,
            message,
        },
    }))
}

/// `GET /api/v1/admin/query-stats/{queryid}`
///
/// Returns detail for a single query by its ID.
///
/// # Errors
///
/// Returns `ApiError` with `NOT_FOUND` if the query ID is not found.
/// Returns `ApiError` with `INTERNAL_ERROR` if the database query fails.
pub async fn query_stats_detail_handler<A: DatabaseAdapter + 'static>(
    State(state): State<AppState<A>>,
    Path(queryid): Path<String>,
) -> Result<Json<ApiResponse<QueryStatsDetailResponse>>, ApiError> {
    let executor = state.executor();
    let adapter = executor.adapter();
    let db_type = adapter.database_type();

    let entry = adapter
        .query_stats_by_id(&queryid)
        .await
        .map_err(|e| ApiError::internal_error(format!("Failed to fetch query stats: {e}")))?;

    match entry {
        Some(entry) => Ok(Json(ApiResponse {
            status: "success".to_string(),
            data:   QueryStatsDetailResponse {
                database_type: db_type.to_string(),
                entry,
            },
        })),
        None => Err(ApiError::not_found(format!("No query stats found for id '{queryid}'"))),
    }
}

/// `POST /api/v1/admin/query-stats/reset`
///
/// Resets query performance statistics. Only PostgreSQL supports this.
///
/// # Errors
///
/// Returns 501 if the backend does not support reset.
/// Returns `ApiError` with `INTERNAL_ERROR` on other failures.
pub async fn query_stats_reset_handler<A: DatabaseAdapter + 'static>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<QueryStatsResetResponse>>, ApiError> {
    let executor = state.executor();
    let adapter = executor.adapter();

    match adapter.reset_query_stats().await {
        Ok(()) => Ok(Json(ApiResponse {
            status: "success".to_string(),
            data:   QueryStatsResetResponse {
                message: "Query statistics have been reset".to_string(),
            },
        })),
        Err(fraiseql_error::FraiseQLError::Unsupported { message }) => {
            Err(ApiError::new(message, "UNSUPPORTED_OPERATION".to_string()))
        },
        Err(e) => Err(ApiError::internal_error(format!("Failed to reset query stats: {e}"))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test assertions — panics are the intended failure mode
mod tests;
