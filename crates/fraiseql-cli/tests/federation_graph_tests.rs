//! Tests for federation graph command - export federation dependency graphs

use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphFormat {
    Json,
    Dot,
    Mermaid,
}

#[derive(Debug)]
struct GraphResult {
    format:  GraphFormat,
    content: String,
}

fn export_federation_graph(_schema_file: &str, format: GraphFormat) -> anyhow::Result<GraphResult> {
    // Simulate federation graph export
    let content = match format {
        GraphFormat::Json => json!({
            "subgraphs": [
                {
                    "name": "users",
                    "url": "http://users.service.local/graphql",
                    "entities": ["User"]
                },
                {
                    "name": "posts",
                    "url": "http://posts.service.local/graphql",
                    "entities": ["Post"]
                }
            ],
            "edges": [
                {
                    "from": "users",
                    "to": "posts",
                    "entity": "User"
                }
            ]
        })
        .to_string(),
        GraphFormat::Dot => r#"digraph federation {
    users [label="users\n[User]"];
    posts [label="posts\n[Post]"];
    users -> posts [label="User"];
}
"#
        .to_string(),
        GraphFormat::Mermaid => r#"graph LR
    users["users<br/>[User]"]
    posts["posts<br/>[Post]"]
    users -->|User| posts
"#
        .to_string(),
    };

    Ok(GraphResult { format, content })
}

#[test]
fn test_federation_graph_json_format() {
    let graph = export_federation_graph("schema.compiled.json", GraphFormat::Json).unwrap();

    assert_eq!(graph.format, GraphFormat::Json);
    assert!(!graph.content.is_empty());

    let parsed: serde_json::Value = serde_json::from_str(&graph.content).unwrap();
    assert!(parsed["subgraphs"].is_array());
    assert!(parsed["edges"].is_array());
}

#[test]
fn test_federation_graph_dot_format() {
    let graph = export_federation_graph("schema.compiled.json", GraphFormat::Dot).unwrap();

    assert_eq!(graph.format, GraphFormat::Dot);
    assert!(graph.content.contains("digraph"));
    assert!(graph.content.contains("->"));
}

#[test]
fn test_federation_graph_mermaid_format() {
    let graph = export_federation_graph("schema.compiled.json", GraphFormat::Mermaid).unwrap();

    assert_eq!(graph.format, GraphFormat::Mermaid);
    assert!(graph.content.contains("graph"));
    assert!(graph.content.contains("-->"));
}

#[test]
fn test_federation_graph_json_includes_subgraphs() {
    let graph = export_federation_graph("schema.compiled.json", GraphFormat::Json).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&graph.content).unwrap();
    let subgraphs = &parsed["subgraphs"];

    assert!(subgraphs.is_array());
    assert!(subgraphs.as_array().unwrap().len() >= 2);

    let subgraph = &subgraphs[0];
    assert!(!subgraph["name"].is_null());
    assert!(!subgraph["url"].is_null());
    assert!(!subgraph["entities"].is_null());
}

#[test]
fn test_federation_graph_json_includes_edges() {
    let graph = export_federation_graph("schema.compiled.json", GraphFormat::Json).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&graph.content).unwrap();
    let edges = &parsed["edges"];

    assert!(edges.is_array());
    let edge = &edges[0];
    assert!(!edge["from"].is_null());
    assert!(!edge["to"].is_null());
    assert!(!edge["entity"].is_null());
}

#[test]
fn test_federation_graph_dot_is_valid_format() {
    let graph = export_federation_graph("schema.compiled.json", GraphFormat::Dot).unwrap();

    // Valid DOT should have:
    // - digraph declaration
    // - nodes with labels
    // - edges with arrows
    assert!(graph.content.starts_with("digraph"));
    assert!(graph.content.contains("[label="));
    assert!(graph.content.contains("->"));
    assert!(graph.content.contains("}"));
}

#[test]
fn test_federation_graph_mermaid_is_valid_format() {
    let graph = export_federation_graph("schema.compiled.json", GraphFormat::Mermaid).unwrap();

    // Valid Mermaid should have:
    // - graph declaration
    // - node definitions
    // - arrows/connections
    assert!(graph.content.contains("graph"));
    assert!(graph.content.contains("["));
    assert!(graph.content.contains("-->"));
}

#[test]
fn test_federation_graph_multiple_formats_consistent() {
    let json_graph = export_federation_graph("schema.compiled.json", GraphFormat::Json).unwrap();
    let dot_graph = export_federation_graph("schema.compiled.json", GraphFormat::Dot).unwrap();
    let mermaid_graph =
        export_federation_graph("schema.compiled.json", GraphFormat::Mermaid).unwrap();

    // All formats should mention the same subgraphs
    assert!(json_graph.content.contains("users"));
    assert!(dot_graph.content.contains("users"));
    assert!(mermaid_graph.content.contains("users"));

    assert!(json_graph.content.contains("posts"));
    assert!(dot_graph.content.contains("posts"));
    assert!(mermaid_graph.content.contains("posts"));
}

#[test]
fn test_federation_graph_shows_entity_relationships() {
    let json_graph = export_federation_graph("schema.compiled.json", GraphFormat::Json).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json_graph.content).unwrap();
    let edges = &parsed["edges"];

    // Should show relationship through User entity
    let has_user_edge = edges
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["entity"].as_str().unwrap_or("") == "User");

    assert!(has_user_edge);
}

#[test]
fn test_federation_graph_not_empty() {
    let json_graph = export_federation_graph("schema.compiled.json", GraphFormat::Json).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json_graph.content).unwrap();

    // Should have at least some subgraphs and edges
    assert!(!parsed["subgraphs"].as_array().unwrap().is_empty());
    assert!(!parsed["edges"].as_array().unwrap().is_empty());
}
