//! Federation graph export command.
//!
//! Usage: fraiseql federation graph <schema.compiled.json>... [--format=json|dot|mermaid]
//!
//! One compiled schema describes one subgraph: its `service_name`, the entities it owns
//! and the ones it extends. The graph is drawn from the schemas given: one node per
//! subgraph, and an edge `A → B` labelled `E` when `A` extends entity `E` and `B` owns
//! it. An entity a subgraph extends that no input owns is listed under `unresolved`, so
//! a missing input reads as a gap rather than as a smaller federation.
//!
//! This command used to read its input, discard it and print the same three-subgraph
//! graph for anything, `{}` included (#1404).

use std::{fmt::Display, str::FromStr};

use anyhow::Result;
use serde::Serialize;

use super::check::load;
use crate::output::CommandResult;

const COMMAND: &str = "federation graph";

/// Export format for federation graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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

/// The federation the given subgraphs form.
#[derive(Debug, Serialize)]
pub struct FederationGraph {
    /// One node per input, in input order.
    pub subgraphs:  Vec<Subgraph>,
    /// `from` extends `entity`, which `to` owns.
    pub edges:      Vec<Edge>,
    /// Entities a subgraph extends that no input owns.
    pub unresolved: Vec<Unresolved>,
}

/// One subgraph: a compiled schema's `federation` block.
#[derive(Debug, Serialize)]
pub struct Subgraph {
    /// The subgraph's `service_name`.
    pub name:    String,
    /// Entities this subgraph declares and resolves.
    pub owns:    Vec<String>,
    /// Entities this subgraph extends from another subgraph.
    pub extends: Vec<String>,
}

/// `from` extends `entity`, which `to` owns.
#[derive(Debug, Serialize)]
pub struct Edge {
    /// The extending subgraph.
    pub from:   String,
    /// The owning subgraph.
    pub to:     String,
    /// The entity linking them.
    pub entity: String,
}

/// An entity a subgraph extends that none of the inputs owns.
#[derive(Debug, Serialize)]
pub struct Unresolved {
    /// The extending subgraph.
    pub subgraph: String,
    /// The entity with no owner among the inputs.
    pub entity:   String,
}

/// Run federation graph command.
///
/// # Errors
///
/// Returns an error if a schema file cannot be read, or the graph cannot be serialized.
/// An input that is not a compiled schema, or has no federation block or `service_name`,
/// is a `validation-failed` result naming it.
pub fn run(schema_paths: &[String], format: GraphFormat) -> Result<CommandResult> {
    let mut subgraphs = Vec::with_capacity(schema_paths.len());
    let mut problems = Vec::new();
    for path in schema_paths {
        let schema = match load(path)? {
            Ok(schema) => schema,
            Err(problem) => {
                problems.push(problem);
                continue;
            },
        };
        let Some(federation) = schema.federation.filter(|f| f.enabled) else {
            problems
                .push(format!("{path} declares no federation; compile it with federation enabled"));
            continue;
        };
        let Some(name) = federation.service_name else {
            problems.push(format!("{path} has no federation service_name"));
            continue;
        };
        let (extended, owned): (Vec<_>, Vec<_>) =
            federation.entities.into_iter().partition(|e| e.extends);
        subgraphs.push(Subgraph {
            name,
            owns: owned.into_iter().map(|e| e.name).collect(),
            extends: extended.into_iter().map(|e| e.name).collect(),
        });
    }
    if !problems.is_empty() {
        return Ok(CommandResult::validation_failed(COMMAND, problems, "INVALID_SCHEMA"));
    }

    let graph = build(subgraphs);
    let output = match format {
        GraphFormat::Json => serde_json::to_value(&graph)?,
        GraphFormat::Dot => serde_json::Value::String(to_dot(&graph)),
        GraphFormat::Mermaid => serde_json::Value::String(to_mermaid(&graph)),
    };
    Ok(CommandResult::success("federation/graph", output))
}

/// Connect each extension to every input that owns the entity.
fn build(subgraphs: Vec<Subgraph>) -> FederationGraph {
    let mut edges = Vec::new();
    let mut unresolved = Vec::new();
    for from in &subgraphs {
        for entity in &from.extends {
            let owners: Vec<&Subgraph> =
                subgraphs.iter().filter(|s| s.owns.contains(entity)).collect();
            if owners.is_empty() {
                unresolved.push(Unresolved {
                    subgraph: from.name.clone(),
                    entity:   entity.clone(),
                });
            }
            for to in owners {
                edges.push(Edge {
                    from:   from.name.clone(),
                    to:     to.name.clone(),
                    entity: entity.clone(),
                });
            }
        }
    }
    FederationGraph {
        subgraphs,
        edges,
        unresolved,
    }
}

/// A DOT double-quoted string.
///
/// A JSON string literal is one: DOT reads `\"`, `\\` and `\n` inside quotes exactly as
/// JSON writes them, so the escaping is serde_json's rather than a hand-rolled copy (#719).
fn dot_quoted(s: &str) -> String {
    serde_json::Value::String(s.to_owned()).to_string()
}

/// Mermaid label text: its quote and markup characters as HTML entities.
fn mermaid_text(s: &str) -> String {
    s.replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
        .replace('\n', " ")
}

/// Convert federation graph to DOT format (Graphviz).
///
/// Node ids are positional (`n0`, `n1`, …) and names appear only as quoted labels, so a
/// name cannot inject DOT syntax.
pub(crate) fn to_dot(graph: &FederationGraph) -> String {
    let mut dot = String::from("digraph federation {\n");
    let id = |name: &str| graph.subgraphs.iter().position(|s| s.name == name).unwrap_or(0);
    for (i, subgraph) in graph.subgraphs.iter().enumerate() {
        let label = format!("{}\n[{}]", subgraph.name, subgraph.owns.join(", "));
        dot.push_str(&format!("    n{i} [label={}];\n", dot_quoted(&label)));
    }
    for edge in &graph.edges {
        dot.push_str(&format!(
            "    n{} -> n{} [label={}];\n",
            id(&edge.from),
            id(&edge.to),
            dot_quoted(&edge.entity)
        ));
    }
    dot.push_str("}\n");
    dot
}

/// Convert federation graph to Mermaid format, with positional node ids and quoted labels.
pub(crate) fn to_mermaid(graph: &FederationGraph) -> String {
    let mut mermaid = String::from("graph LR\n");
    let id = |name: &str| graph.subgraphs.iter().position(|s| s.name == name).unwrap_or(0);
    for (i, subgraph) in graph.subgraphs.iter().enumerate() {
        let owns: Vec<String> = subgraph.owns.iter().map(|e| mermaid_text(e)).collect();
        mermaid.push_str(&format!(
            "    n{i}[\"{}<br/>[{}]\"]\n",
            mermaid_text(&subgraph.name),
            owns.join("<br/>")
        ));
    }
    for edge in &graph.edges {
        mermaid.push_str(&format!(
            "    n{} -->|\"{}\"| n{}\n",
            id(&edge.from),
            mermaid_text(&edge.entity),
            id(&edge.to)
        ));
    }
    mermaid
}
