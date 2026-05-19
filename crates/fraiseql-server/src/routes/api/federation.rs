//! Federation API endpoints.
//!
//! Provides endpoints for:
//! - Listing subgraphs and their health status
//! - Exporting federation dependency graphs in multiple formats (JSON, DOT, Mermaid)

use axum::{
    Json,
    extract::{Query, State},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::{
    api::types::{ApiError, ApiResponse},
    graphql::AppState,
};

/// Response containing federation subgraph information.
#[derive(Debug, Serialize)]
pub struct SubgraphsResponse {
    /// List of federated subgraphs
    pub subgraphs: Vec<SubgraphInfo>,
}

/// Information about a single federated subgraph.
#[derive(Debug, Serialize, Clone)]
pub struct SubgraphInfo {
    /// Name of the subgraph
    pub name: String,
    /// GraphQL endpoint URL for the subgraph
    pub url: String,
    /// Entity types managed by this subgraph
    pub entities: Vec<String>,
    /// Health status of the subgraph
    pub healthy: bool,
}

/// Federation graph in various formats.
#[derive(Debug, Serialize)]
pub struct GraphResponse {
    /// Format of the graph (json, dot, or mermaid)
    pub format: String,
    /// Graph content in the specified format
    pub content: String,
}

/// Graph format query parameter for federation graph endpoint.
#[derive(Debug, Deserialize)]
pub struct GraphFormatQuery {
    /// Output format: json (default), dot, or mermaid
    #[serde(default = "default_format")]
    pub format: String,
}

pub(crate) fn default_format() -> String {
    "json".to_string()
}

/// Get list of federation subgraphs.
///
/// Returns information about this subgraph's federation configuration,
/// including the entity types it manages. For gateway-level subgraph
/// discovery, configure a federation gateway separately.
///
/// # Errors
///
/// This handler currently always succeeds; it is infallible.
pub async fn subgraphs_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<SubgraphsResponse>>, ApiError> {
    let executor = state.executor();
    let schema = executor.schema();
    let federation = schema.federation.as_ref();

    let subgraphs = match federation {
        Some(fed) if fed.enabled => {
            let service_name =
                fed.service_name.clone().unwrap_or_else(|| "this-service".to_string());
            let url = fed.schema_url.clone().unwrap_or_else(|| "/__subgraph_schema".to_string());
            let entities = fed.entities.iter().map(|e| e.name.clone()).collect();

            vec![SubgraphInfo {
                name: service_name,
                url,
                entities,
                healthy: true,
            }]
        },
        _ => vec![],
    };

    let response = SubgraphsResponse { subgraphs };
    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

/// Get federation dependency graph.
///
/// Exports the federation structure showing:
/// - Subgraph relationships
/// - Entity resolution paths
/// - Dependencies between subgraphs
///
/// Supports multiple output formats:
/// - **json**: Machine-readable federation structure
/// - **dot**: Graphviz format for visualization
/// - **mermaid**: Markdown-compatible graph syntax
///
/// # Errors
///
/// Returns `ApiError` with a validation error if `format` is not one of `json`, `dot`, or
/// `mermaid`.
pub async fn graph_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Query(query): Query<GraphFormatQuery>,
) -> Result<Json<ApiResponse<GraphResponse>>, ApiError> {
    // Validate format parameter
    let format = match query.format.as_str() {
        "json" | "dot" | "mermaid" => query.format,
        _ => return Err(ApiError::validation_error("format must be 'json', 'dot', or 'mermaid'")),
    };

    let executor = state.executor();
    let schema = executor.schema();
    let federation = schema.federation.as_ref();

    let content = generate_federation_graph(&format, federation);

    let response = GraphResponse { format, content };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

/// Generate federation graph in the specified format from actual schema data.
fn generate_federation_graph(
    format: &str,
    federation: Option<&fraiseql_core::schema::FederationConfig>,
) -> String {
    match format {
        "json" => generate_json_graph(federation),
        "dot" => generate_dot_graph(federation),
        "mermaid" => generate_mermaid_graph(federation),
        _ => "{}".to_string(),
    }
}

pub(crate) fn generate_json_graph(
    federation: Option<&fraiseql_core::schema::FederationConfig>,
) -> String {
    let subgraphs: Vec<serde_json::Value> = match federation {
        Some(fed) if fed.enabled => {
            let name = fed.service_name.clone().unwrap_or_else(|| "this-service".to_string());
            let url = fed.schema_url.clone().unwrap_or_else(|| "/__subgraph_schema".to_string());
            let entities: Vec<_> = fed.entities.iter().map(|e| e.name.as_str()).collect();
            vec![serde_json::json!({ "name": name, "url": url, "entities": entities })]
        },
        _ => vec![],
    };

    serde_json::to_string_pretty(&serde_json::json!({
        "subgraphs": subgraphs,
        "edges": []
    }))
    .unwrap_or_else(|_| r#"{"subgraphs":[],"edges":[]}"#.to_string())
}

pub(crate) fn generate_dot_graph(
    federation: Option<&fraiseql_core::schema::FederationConfig>,
) -> String {
    use std::fmt::Write as _;

    let mut dot =
        "digraph federation {\n  rankdir=LR;\n  node [shape=box, style=rounded];\n\n".to_string();

    if let Some(fed) = federation {
        if fed.enabled {
            let name = fed.service_name.clone().unwrap_or_else(|| "this_service".to_string());
            let entities: Vec<_> = fed.entities.iter().map(|e| e.name.as_str()).collect();
            let label = format!("{}\\n[{}]", name, entities.join(", "));
            let _ = writeln!(dot, "  {name} [label=\"{label}\"];");
        }
    }

    dot.push('}');
    dot
}

/// Query parameter for plan endpoint.
#[derive(Debug, Deserialize)]
pub struct PlanQuery {
    /// The GraphQL query to look up in the plan cache.
    pub query: String,
}

/// Response from the plan visualization endpoint.
#[derive(Debug, Serialize)]
pub struct PlanResponse {
    /// Whether a cached plan was found.
    pub cached: bool,
    /// Schema fingerprint at plan creation time (if cached).
    pub schema_fingerprint: String,
    /// The fetch operations in the plan (if cached).
    #[cfg(feature = "federation")]
    pub fetches: Option<Vec<fraiseql_core::federation::SubgraphFetch>>,
    /// The fetch operations in the plan (stub when federation disabled).
    #[cfg(not(feature = "federation"))]
    pub fetches: Option<serde_json::Value>,
}

/// Get federation query plan for a given query.
///
/// `GET /admin/v1/federation/plan?query=<url-encoded GraphQL query>`
///
/// Returns the cached plan breakdown showing subgraph fetches, or
/// `{"cached": false, "fetches": null}` if no plan is cached for the query.
///
/// # Errors
///
/// Returns `ApiError` if the `query` parameter is missing or too long.
#[cfg(feature = "federation")]
pub async fn plan_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Query(params): Query<PlanQuery>,
) -> Result<Json<ApiResponse<PlanResponse>>, ApiError> {
    if params.query.is_empty() {
        return Err(ApiError::validation_error("query parameter is required"));
    }

    if params.query.len() > state.max_get_query_bytes {
        return Err(ApiError::validation_error("query parameter too long"));
    }

    let Some(ref plan_cache) = state.federation_plan_cache else {
        let response = PlanResponse {
            cached: false,
            schema_fingerprint: String::new(),
            fetches: None,
        };
        return Ok(Json(ApiResponse {
            status: "success".to_string(),
            data: response,
        }));
    };

    // Normalize the query for cache lookup
    let normalized = fraiseql_core::federation::query_plan_cache::normalize_query(&params.query);

    // Try to find in cache — we need the schema fingerprint to look up.
    // Use an empty fingerprint first; if the cache stores plans keyed by normalized query,
    // iterate to find any matching plan.
    let executor = state.executor();
    let schema = executor.schema();
    let fingerprint =
        schema.federation.as_ref().and_then(|f| f.version.clone()).unwrap_or_default();

    let plan = plan_cache.get(&normalized, &fingerprint);

    let response = match plan {
        Some(plan) => PlanResponse {
            cached: true,
            schema_fingerprint: plan.schema_fingerprint.clone(),
            fetches: Some(plan.fetches),
        },
        None => PlanResponse {
            cached: false,
            schema_fingerprint: fingerprint,
            fetches: None,
        },
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

pub(crate) fn generate_mermaid_graph(
    federation: Option<&fraiseql_core::schema::FederationConfig>,
) -> String {
    use std::fmt::Write as _;

    let mut mermaid = "graph LR\n".to_string();

    if let Some(fed) = federation {
        if fed.enabled {
            let name = fed.service_name.clone().unwrap_or_else(|| "this-service".to_string());
            let entities: Vec<_> = fed.entities.iter().map(|e| e.name.as_str()).collect();
            let _ = writeln!(mermaid, "    {name}[\"{name}<br/>[{}]\"]", entities.join(", "));
        }
    }

    mermaid
}
