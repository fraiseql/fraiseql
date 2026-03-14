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
    pub name:     String,
    /// GraphQL endpoint URL for the subgraph
    pub url:      String,
    /// Entity types managed by this subgraph
    pub entities: Vec<String>,
    /// Health status of the subgraph
    pub healthy:  bool,
}

/// Federation graph in various formats.
#[derive(Debug, Serialize)]
pub struct GraphResponse {
    /// Format of the graph (json, dot, or mermaid)
    pub format:  String,
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

fn default_format() -> String {
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
    let schema = state.executor.schema();
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
        data:   response,
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

    let schema = state.executor.schema();
    let federation = schema.federation.as_ref();

    let content = generate_federation_graph(&format, federation);

    let response = GraphResponse { format, content };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
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

fn generate_json_graph(federation: Option<&fraiseql_core::schema::FederationConfig>) -> String {
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

fn generate_dot_graph(federation: Option<&fraiseql_core::schema::FederationConfig>) -> String {
    let mut dot =
        "digraph federation {\n  rankdir=LR;\n  node [shape=box, style=rounded];\n\n".to_string();

    if let Some(fed) = federation {
        if fed.enabled {
            let name = fed.service_name.clone().unwrap_or_else(|| "this_service".to_string());
            let entities: Vec<_> = fed.entities.iter().map(|e| e.name.as_str()).collect();
            let label = format!("{}\\n[{}]", name, entities.join(", "));
            dot.push_str(&format!("  {name} [label=\"{label}\"];\n"));
        }
    }

    dot.push('}');
    dot
}

fn generate_mermaid_graph(federation: Option<&fraiseql_core::schema::FederationConfig>) -> String {
    let mut mermaid = "graph LR\n".to_string();

    if let Some(fed) = federation {
        if fed.enabled {
            let name = fed.service_name.clone().unwrap_or_else(|| "this-service".to_string());
            let entities: Vec<_> = fed.entities.iter().map(|e| e.name.as_str()).collect();
            mermaid.push_str(&format!("    {name}[\"{name}<br/>[{}]\"]\n", entities.join(", ")));
        }
    }

    mermaid
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    #[test]
    fn test_default_format() {
        assert_eq!(default_format(), "json");
    }

    #[test]
    fn test_subgraph_info_creation() {
        let info = SubgraphInfo {
            name:     "test".to_string(),
            url:      "http://test.local".to_string(),
            entities: vec!["Entity1".to_string()],
            healthy:  true,
        };

        assert_eq!(info.name, "test");
        assert!(info.healthy);
    }

    #[test]
    fn test_subgraphs_response_creation() {
        let response = SubgraphsResponse { subgraphs: vec![] };

        assert!(response.subgraphs.is_empty());
    }

    #[test]
    fn test_graph_response_creation() {
        let response = GraphResponse {
            format:  "json".to_string(),
            content: "{}".to_string(),
        };

        assert_eq!(response.format, "json");
    }

    #[test]
    fn test_generate_json_graph_no_federation() {
        let json = generate_json_graph(None);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["subgraphs"].as_array().unwrap().is_empty());
        assert!(parsed["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_generate_dot_graph_no_federation() {
        let dot = generate_dot_graph(None);
        assert!(dot.contains("digraph"));
        assert!(dot.contains("rankdir"));
    }

    #[test]
    fn test_generate_mermaid_graph_no_federation() {
        let mermaid = generate_mermaid_graph(None);
        assert!(mermaid.contains("graph LR"));
    }
}
