//! Usage statistics API endpoint.
//!
//! Exposes in-memory mutation counters accumulated by the `MutationAuditLayer`
//! tracing subscriber.  Counters are keyed by `(tenant_id, period, entity_type)`
//! and reset to zero on process restart.
//!
//! ## Endpoint
//!
//! ```text
//! GET /api/v1/admin/usage?tenant_id=<str>&period=<YYYY-MM>
//! ```
//!
//! Protected by the admin bearer token (same as all other admin read routes).
//!
//! ## Response
//!
//! ```json
//! {
//!   "tenant_id": "acme",
//!   "period": "2026-05",
//!   "usage": {
//!     "mutations": { "User": 42, "Order": 7 }
//!   }
//! }
//! ```
//!
//! Unknown `tenant_id` / `period` combinations return 200 with
//! `"usage": { "mutations": {} }` — never 404.
//!
//! Invalid `period` (not `YYYY-MM`) returns 400 with
//! `{"error": "invalid period format"}`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::{
    routes::graphql::AppState,
    usage::aggregator::{UsageSummary, validate_period},
};

// ── Query parameters ───────────────────────────────────────────────────────

/// Query parameters for the usage endpoint.
#[non_exhaustive]
#[derive(Debug, Deserialize)]
pub struct UsageQueryParams {
    /// Tenant identifier to query.
    pub tenant_id: String,
    /// Billing period in `YYYY-MM` format (e.g. `"2026-05"`).
    pub period:    String,
}

// ── Response types ─────────────────────────────────────────────────────────

/// Successful usage query response.
#[non_exhaustive]
#[derive(Debug, Serialize)]
pub struct UsageResponse {
    /// The queried tenant identifier.
    pub tenant_id: String,
    /// The queried period (`YYYY-MM`).
    pub period:    String,
    /// Mutation counts for the queried period.
    pub usage:     UsageSummary,
}

// ── Handler ────────────────────────────────────────────────────────────────

/// Query mutation usage statistics for a tenant and period.
///
/// Returns 400 when `period` is not a valid `YYYY-MM` string.  Returns 200
/// with empty `mutations` for unknown tenant/period combinations.
///
/// # Errors
///
/// Returns `(400, {"error": "invalid period format"})` when the `period`
/// query parameter is not in `YYYY-MM` format.
pub async fn usage_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Query(params): Query<UsageQueryParams>,
) -> Result<Json<UsageResponse>, (StatusCode, Json<serde_json::Value>)> {
    if !validate_period(&params.period) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid period format"})),
        ));
    }

    let usage = state.usage.query(&params.tenant_id, &params.period);

    Ok(Json(UsageResponse {
        tenant_id: params.tenant_id,
        period:    params.period,
        usage,
    }))
}

// ── Tests ──────────────────────────────────────────────────────────────────
