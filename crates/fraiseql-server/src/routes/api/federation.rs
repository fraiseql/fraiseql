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
/// Returns information about all subgraphs in the federated schema,
/// including their URLs, managed entities, and health status.
///
/// When federation is enabled, reports this server as a subgraph with
/// its federated entity types. When disabled, returns an empty list.
pub async fn subgraphs_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<SubgraphsResponse>>, ApiError> {
    let schema = state.executor.schema();
    let subgraphs = extract_subgraph_info(schema);

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
    let subgraphs = extract_subgraph_info(schema);
    let content = generate_federation_graph(&format, &subgraphs);

    let response = GraphResponse { format, content };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

/// Extract subgraph information from the compiled schema.
///
/// When federation is enabled in the schema, extracts entity types and their
/// key fields to produce a self-describing subgraph entry. Each federated type
/// with `@key` directives is reported as an entity managed by this subgraph.
fn extract_subgraph_info(schema: &fraiseql_core::schema::CompiledSchema) -> Vec<SubgraphInfo> {
    let metadata = match schema.federation_metadata() {
        Some(meta) if meta.enabled => meta,
        _ => return vec![],
    };

    // Collect entity types (types that have at least one @key directive)
    let entities: Vec<String> = metadata
        .types
        .iter()
        .filter(|t| !t.keys.is_empty())
        .map(|t| t.name.clone())
        .collect();

    if entities.is_empty() {
        return vec![];
    }

    // Report this server as a single subgraph with its federated entities
    vec![SubgraphInfo {
        name: "self".to_string(),
        url: "http://localhost".to_string(),
        entities,
        healthy: true,
    }]
}

/// Generate federation graph in the specified format.
fn generate_federation_graph(format: &str, subgraphs: &[SubgraphInfo]) -> String {
    match format {
        "json" => generate_json_graph(subgraphs),
        "dot" => generate_dot_graph(subgraphs),
        "mermaid" => generate_mermaid_graph(subgraphs),
        _ => "{}".to_string(),
    }
}

/// Generate JSON representation of federation graph.
fn generate_json_graph(subgraphs: &[SubgraphInfo]) -> String {
    let subgraph_nodes: Vec<serde_json::Value> = subgraphs
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "url": s.url,
                "entities": s.entities,
                "healthy": s.healthy,
            })
        })
        .collect();

    serde_json::json!({
        "subgraphs": subgraph_nodes,
        "edges": []
    })
    .to_string()
}

/// Generate Graphviz (DOT) representation of federation graph.
fn generate_dot_graph(subgraphs: &[SubgraphInfo]) -> String {
    let mut dot =
        String::from("digraph federation {\n  rankdir=LR;\n  node [shape=box, style=rounded];\n\n");

    for sg in subgraphs {
        let entities_label = sg.entities.join(", ");
        dot.push_str(&format!("  {} [label=\"{}\\n[{}]\"];\n", sg.name, sg.name, entities_label));
    }

    dot.push('}');
    dot
}

/// Generate Mermaid diagram representation of federation graph.
fn generate_mermaid_graph(subgraphs: &[SubgraphInfo]) -> String {
    let mut mermaid = String::from("graph LR\n");

    for sg in subgraphs {
        let entities_label = sg.entities.join(", ");
        mermaid.push_str(&format!("    {}[\"{}<br/>[{}]\"]\n", sg.name, sg.name, entities_label));
    }

    mermaid
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_subgraphs() -> Vec<SubgraphInfo> {
        vec![SubgraphInfo {
            name:     "users".to_string(),
            url:      "http://users.local".to_string(),
            entities: vec!["User".to_string(), "Profile".to_string()],
            healthy:  true,
        }]
    }

    #[test]
    fn test_generate_json_graph_empty() {
        let json = generate_json_graph(&[]);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["subgraphs"].is_array());
        assert!(parsed["edges"].is_array());
    }

    #[test]
    fn test_generate_json_graph_with_subgraphs() {
        let json = generate_json_graph(&sample_subgraphs());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let subgraphs = parsed["subgraphs"].as_array().unwrap();
        assert_eq!(subgraphs.len(), 1);
        assert_eq!(subgraphs[0]["name"], "users");
        assert_eq!(subgraphs[0]["entities"][0], "User");
    }

    #[test]
    fn test_generate_dot_graph() {
        let dot = generate_dot_graph(&sample_subgraphs());

        assert!(dot.contains("digraph"));
        assert!(dot.contains("rankdir"));
        assert!(dot.contains("users"));
        assert!(dot.contains("User, Profile"));
    }

    #[test]
    fn test_generate_mermaid_graph() {
        let mermaid = generate_mermaid_graph(&sample_subgraphs());

        assert!(mermaid.contains("graph LR"));
        assert!(mermaid.contains("users"));
    }

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
    fn test_extract_subgraph_info_no_federation() {
        let schema = fraiseql_core::schema::CompiledSchema::new();
        let subgraphs = extract_subgraph_info(&schema);
        assert!(subgraphs.is_empty());
    }

    #[test]
    fn test_extract_subgraph_info_with_federation() {
        use fraiseql_core::federation::{FederatedType, FederationMetadata, KeyDirective};

        let mut schema = fraiseql_core::schema::CompiledSchema::new();
        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![FederatedType {
                name:             "User".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            }],
        };
        schema.federation = Some(serde_json::to_value(&metadata).unwrap());

        let subgraphs = extract_subgraph_info(&schema);
        assert_eq!(subgraphs.len(), 1);
        assert_eq!(subgraphs[0].name, "self");
        assert_eq!(subgraphs[0].entities, vec!["User".to_string()]);
    }
}
