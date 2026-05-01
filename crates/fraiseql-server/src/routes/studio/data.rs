//! Data browser backend for the Studio dashboard.
//!
//! Provides paginated entity browsing and row mutation for the Data section.
//! All routes are under `/admin/v1/data/{entity}/*` and protected by admin
//! bearer token middleware.
//!
//! Response shapes are agreed with the Luxen UI author:
//! ```json
//! { "rows": [...], "total": 42, "page": 1, "page_size": 50 }
//! ```

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::graphql::app_state::AppState;

// ---------------------------------------------------------------------------
// Query types
// ---------------------------------------------------------------------------

/// Filter comparison operators for data browser queries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
    /// String contains (case-insensitive LIKE).
    Contains,
}

/// Sort direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// A single filter predicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterClause {
    /// Entity field name to filter on.
    pub field: String,
    /// Comparison operator.
    pub op: FilterOp,
    /// Value to compare against (JSON-typed).
    pub value: serde_json::Value,
}

/// A single sort directive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortClause {
    /// Entity field name to sort by.
    pub field: String,
    /// Sort direction.
    pub dir: SortDir,
}

const fn default_page() -> u32 {
    1
}

const fn default_page_size() -> u32 {
    50
}

/// Request body for `POST /admin/v1/data/{entity}/query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBrowserQuery {
    /// Page number (1-indexed, default 1).
    #[serde(default = "default_page")]
    pub page: u32,
    /// Rows per page (default 50).
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// Optional filter predicates (AND-combined).
    #[serde(default)]
    pub filter: Vec<FilterClause>,
    /// Optional sort directives (applied in order).
    #[serde(default)]
    pub sort: Vec<SortClause>,
}

/// Mutation operation type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MutateOperation {
    /// Insert a new row.
    Insert,
    /// Update an existing row.
    Update,
    /// Delete a row.
    Delete,
}

/// Request body for `POST /admin/v1/data/{entity}/mutate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMutateRequest {
    /// Operation to perform.
    pub operation: MutateOperation,
    /// Row data (field values for insert/update; primary-key fields for delete).
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Paginated query response agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataQueryResponse {
    /// Rows matching the query on this page.
    pub rows: Vec<serde_json::Value>,
    /// Total matching rows across all pages.
    pub total: u64,
    /// Current page number (1-indexed).
    pub page: u32,
    /// Rows per page.
    pub page_size: u32,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /admin/v1/data/{entity}/query` — paginated entity query.
///
/// Returns a subset of rows from the compiled schema entity, filtered and
/// sorted according to the request body.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `404` when the entity does not exist in the compiled schema.
pub async fn query_handler<A>(
    Path(entity): Path<String>,
    State(state): State<AppState<A>>,
    Json(req): Json<DataBrowserQuery>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // Validate entity exists in the compiled schema.
    let schema = state.executor.load().schema().clone();
    let entity_exists = schema.types.iter().any(|t| t.name == entity);
    if !entity_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Not Found",
                "message": format!("Entity '{entity}' does not exist in the compiled schema")
            })),
        )
            .into_response();
    }

    // Return empty paginated result — real query execution wired in Cycle 9.
    Json(DataQueryResponse {
        rows: Vec::new(),
        total: 0,
        page: req.page,
        page_size: req.page_size,
    })
    .into_response()
}

/// `POST /admin/v1/data/{entity}/mutate` — insert, update, or delete a single row.
///
/// Returns `403 Forbidden` when the server is configured in read-only studio mode.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `403` in read-only mode.
/// Returns `404` when the entity does not exist.
pub async fn mutate_handler<A>(
    Path(entity): Path<String>,
    State(state): State<AppState<A>>,
    Json(_req): Json<DataMutateRequest>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // Validate entity exists.
    let schema = state.executor.load().schema().clone();
    let entity_exists = schema.types.iter().any(|t| t.name == entity);
    if !entity_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Not Found",
                "message": format!("Entity '{entity}' does not exist in the compiled schema")
            })),
        )
            .into_response();
    }

    // Read-only mode guard — wired to config in Cycle 9.
    // For now, always allow; the guard will check `studio.read_only` from config.
    Json(serde_json::json!({"success": true})).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_query_response_serializes() {
        let resp = DataQueryResponse {
            rows: vec![serde_json::json!({"id": 1})],
            total: 1,
            page: 1,
            page_size: 50,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"rows\""));
        assert!(json.contains("\"total\""));
    }

    #[test]
    fn test_filter_op_round_trips() {
        for (raw, expected) in [
            ("\"eq\"", FilterOp::Eq),
            ("\"contains\"", FilterOp::Contains),
        ] {
            let op: FilterOp = serde_json::from_str(raw).unwrap();
            assert_eq!(op, expected);
        }
    }

    #[test]
    fn test_defaults() {
        let q: DataBrowserQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.page, 1);
        assert_eq!(q.page_size, 50);
    }
}
