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
    response::{IntoResponse, Response},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::{graphql::app_state::AppState, studio::not_implemented};

// ---------------------------------------------------------------------------
// Query types
// ---------------------------------------------------------------------------

/// Filter comparison operators for data browser queries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
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
#[non_exhaustive]
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
    pub op:    FilterOp,
    /// Value to compare against (JSON-typed).
    pub value: serde_json::Value,
}

/// A single sort directive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortClause {
    /// Entity field name to sort by.
    pub field: String,
    /// Sort direction.
    pub dir:   SortDir,
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
    pub page:      u32,
    /// Rows per page (default 50).
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// Optional filter predicates (AND-combined).
    #[serde(default)]
    pub filter:    Vec<FilterClause>,
    /// Optional sort directives (applied in order).
    #[serde(default)]
    pub sort:      Vec<SortClause>,
}

/// Mutation operation type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
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
    pub data:      serde_json::Value,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Paginated query response agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataQueryResponse {
    /// Rows matching the query on this page.
    pub rows:      Vec<serde_json::Value>,
    /// Total matching rows across all pages.
    pub total:     u64,
    /// Current page number (1-indexed).
    pub page:      u32,
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
    Json(_req): Json<DataBrowserQuery>,
) -> Response
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

    // `{"rows": [], "total": 0}` is the answer "this entity has no rows" — for a
    // browser pointed at a populated database, a false one. Until the query path is
    // wired, say what is true.
    not_implemented(
        "studio.data.query",
        "The data browser's query path is not wired to the executor; it cannot read \
         rows. Query the entity through /graphql.",
    )
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
) -> Response
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

    // This answered `{"success": true}` with the insert/update/delete never executed,
    // under a comment that said the read-only guard was "not yet wired... For now,
    // always allow" (#749) — so a data editor reported a saved row over an unchanged
    // database.
    //
    // Not wired up rather than merely reported: a generic admin row-mutation surface
    // is a new write path, and activating one is a decision to take deliberately, not
    // a side effect of repairing a fabricated response.
    not_implemented(
        "studio.data.mutate",
        "The data browser's mutation path is not wired to the executor; no row was \
         inserted, updated or deleted. Use a declared GraphQL mutation.",
    )
}
