//! Federation graph export command
//!
//! Usage: fraiseql federation graph <schema.compiled.json> [--format=json|dot|mermaid]

use std::{fmt::Display, fs, str::FromStr};

use anyhow::Result;
use serde::Serialize;

use crate::output::CommandResult;

/// Export format for federation graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// JSON format (machine-readable)
    Json,
    /// DOT format (Graphviz)
    Dot,
    /// Mermaid format (documentation)
    Mermaid,
}

impl FromStr for GraphFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(GraphFormat::Json),
            "dot" => Ok(GraphFormat::Dot),
            "mermaid" => Ok(GraphFormat::Mermaid),
            other => Err(format!("Unknown format: {other}. Use json, dot, or mermaid")),
        }
    }
}

impl Display for GraphFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphFormat::Json => write!(f, "json"),
            GraphFormat::Dot => write!(f, "dot"),
            GraphFormat::Mermaid => write!(f, "mermaid"),
        }
    }
}

/// Federation graph data
#[derive(Debug, Serialize)]
pub struct FederationGraph {
    /// Subgraphs in the federation
    pub subgraphs: Vec<Subgraph>,

    /// Edges representing entity relationships
    pub edges: Vec<Edge>,
}

/// Subgraph in federation
#[derive(Debug, Serialize)]
pub struct Subgraph {
    /// Subgraph name
    pub name: String,

    /// Subgraph endpoint URL
    pub url: String,

    /// Entities provided by this subgraph
    pub entities: Vec<String>,
}

/// Edge representing entity relationship between subgraphs
#[derive(Debug, Serialize)]
pub struct Edge {
    /// Source subgraph
    pub from: String,

    /// Target subgraph
    pub to: String,

    /// Entity linking the subgraphs
    pub entity: String,
}

/// Run federation graph command
pub fn run(schema_path: &str, format: GraphFormat) -> Result<CommandResult> {
    // Load schema file
    let schema_content = fs::read_to_string(schema_path)?;

    // Parse as JSON to verify structure (validation only)
    let _schema: serde_json::Value = serde_json::from_str(&schema_content)?;

    // Build federation graph (simulated for now)
    let graph = FederationGraph {
        subgraphs: vec![
            Subgraph {
                name:     "users".to_string(),
                url:      "http://users.service/graphql".to_string(),
                entities: vec!["User".to_string()],
            },
            Subgraph {
                name:     "posts".to_string(),
                url:      "http://posts.service/graphql".to_string(),
                entities: vec!["Post".to_string()],
            },
            Subgraph {
                name:     "comments".to_string(),
                url:      "http://comments.service/graphql".to_string(),
                entities: vec!["Comment".to_string()],
            },
        ],
        edges:     vec![
            Edge {
                from:   "users".to_string(),
                to:     "posts".to_string(),
                entity: "User".to_string(),
            },
            Edge {
                from:   "posts".to_string(),
                to:     "comments".to_string(),
                entity: "Post".to_string(),
            },
        ],
    };

    // Export in requested format
    let output = match format {
        GraphFormat::Json => serde_json::to_value(&graph)?,
        GraphFormat::Dot => serde_json::Value::String(to_dot(&graph)),
        GraphFormat::Mermaid => serde_json::Value::String(to_mermaid(&graph)),
    };

    Ok(CommandResult::success("federation/graph", output))
}

/// Convert federation graph to DOT format (Graphviz)
fn to_dot(graph: &FederationGraph) -> String {
    let mut dot = String::from("digraph federation {\n");

    // Add subgraph nodes
    for subgraph in &graph.subgraphs {
        let entities = subgraph.entities.join(", ");
        dot.push_str(&format!(
            "    {} [label=\"{}\\n[{}]\"];\n",
            subgraph.name, subgraph.name, entities
        ));
    }

    // Add edges
    for edge in &graph.edges {
        dot.push_str(&format!("    {} -> {} [label=\"{}\"];\n", edge.from, edge.to, edge.entity));
    }

    dot.push_str("}\n");
    dot
}

/// Convert federation graph to Mermaid format
fn to_mermaid(graph: &FederationGraph) -> String {
    let mut mermaid = String::from("graph LR\n");

    // Add nodes
    for subgraph in &graph.subgraphs {
        let entities = subgraph.entities.join("<br/>");
        mermaid.push_str(&format!(
            "    {}[\"{}\\n[{}\\n]\"]\n",
            subgraph.name, subgraph.name, entities
        ));
    }

    // Add edges
    for edge in &graph.edges {
        mermaid.push_str(&format!("    {} -->|{}| {}\n", edge.from, edge.entity, edge.to));
    }

    mermaid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_format_from_str() {
        assert_eq!("json".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("dot".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("mermaid".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
    }

    #[test]
    fn test_graph_format_case_insensitive() {
        assert_eq!("JSON".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("DOT".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
    }

    #[test]
    fn test_graph_format_invalid() {
        assert!("invalid".parse::<GraphFormat>().is_err());
    }

    #[test]
    fn test_to_dot_format() {
        let graph = FederationGraph {
            subgraphs: vec![Subgraph {
                name:     "a".to_string(),
                url:      "http://a".to_string(),
                entities: vec!["A".to_string()],
            }],
            edges:     vec![],
        };

        let dot = to_dot(&graph);
        assert!(dot.contains("digraph"));
        assert!(dot.contains('a'));
    }

    #[test]
    fn test_to_mermaid_format() {
        let graph = FederationGraph {
            subgraphs: vec![Subgraph {
                name:     "a".to_string(),
                url:      "http://a".to_string(),
                entities: vec!["A".to_string()],
            }],
            edges:     vec![],
        };

        let mermaid = to_mermaid(&graph);
        assert!(mermaid.contains("graph"));
        assert!(mermaid.contains('a'));
    }
}
