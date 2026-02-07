//! Schema dependency graph command
//!
//! Analyzes and exports schema type dependencies in multiple formats.
//!
//! Usage: fraiseql dependency-graph <schema.compiled.json> [--format=json|dot|mermaid|d2|console]

use std::{fmt::Display, fs, str::FromStr};

use anyhow::Result;
use fraiseql_core::schema::{CompiledSchema, CyclePath, SchemaDependencyGraph};
use serde::Serialize;
use serde_json::Value;

use crate::output::CommandResult;

/// Export format for dependency graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphFormat {
    /// JSON format (machine-readable, default)
    #[default]
    Json,
    /// DOT format (Graphviz)
    Dot,
    /// Mermaid format (documentation/markdown)
    Mermaid,
    /// D2 format (modern diagram language)
    D2,
    /// Console format (human-readable text)
    Console,
}

impl FromStr for GraphFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(GraphFormat::Json),
            "dot" | "graphviz" => Ok(GraphFormat::Dot),
            "mermaid" | "md" => Ok(GraphFormat::Mermaid),
            "d2" => Ok(GraphFormat::D2),
            "console" | "text" | "txt" => Ok(GraphFormat::Console),
            other => Err(format!(
                "Unknown format: '{other}'. Valid formats: json, dot, mermaid, d2, console"
            )),
        }
    }
}

impl Display for GraphFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphFormat::Json => write!(f, "json"),
            GraphFormat::Dot => write!(f, "dot"),
            GraphFormat::Mermaid => write!(f, "mermaid"),
            GraphFormat::D2 => write!(f, "d2"),
            GraphFormat::Console => write!(f, "console"),
        }
    }
}

/// Serializable representation of the dependency graph
#[derive(Debug, Serialize)]
pub struct DependencyGraphOutput {
    /// Total number of types in the schema
    pub type_count: usize,

    /// All nodes (types) in the graph
    pub nodes: Vec<GraphNode>,

    /// All edges (dependencies) in the graph
    pub edges: Vec<GraphEdge>,

    /// Circular dependencies detected (empty if none)
    pub cycles: Vec<CycleInfo>,

    /// Types with no incoming references (orphaned)
    pub unused_types: Vec<String>,

    /// Summary statistics
    pub stats: GraphStats,
}

/// A node in the dependency graph
#[derive(Debug, Serialize)]
pub struct GraphNode {
    /// Type name
    pub name: String,

    /// Number of types this type depends on
    pub dependency_count: usize,

    /// Number of types that depend on this type
    pub dependent_count: usize,

    /// Whether this is a root type (Query, Mutation, Subscription)
    pub is_root: bool,
}

/// An edge in the dependency graph
#[derive(Debug, Serialize)]
pub struct GraphEdge {
    /// Source type (the type that has the dependency)
    pub from: String,

    /// Target type (the type being depended on)
    pub to: String,
}

/// Information about a detected cycle
#[derive(Debug, Serialize)]
pub struct CycleInfo {
    /// Types involved in the cycle
    pub types: Vec<String>,

    /// Human-readable path string
    pub path: String,

    /// Whether this is a self-reference
    pub is_self_reference: bool,
}

impl From<&CyclePath> for CycleInfo {
    fn from(cycle: &CyclePath) -> Self {
        Self {
            types:             cycle.nodes.clone(),
            path:              cycle.path_string(),
            is_self_reference: cycle.is_self_reference(),
        }
    }
}

/// Statistics about the dependency graph
#[derive(Debug, Serialize)]
pub struct GraphStats {
    /// Total number of types
    pub total_types: usize,

    /// Total number of edges (dependencies)
    pub total_edges: usize,

    /// Number of circular dependencies
    pub cycle_count: usize,

    /// Number of unused types
    pub unused_count: usize,

    /// Average dependencies per type
    pub avg_dependencies: f64,

    /// Maximum dependency depth from any root
    pub max_depth: usize,

    /// Types with the most dependents (most "important")
    pub most_depended_on: Vec<String>,
}

/// Run the dependency graph command
pub fn run(schema_path: &str, format: GraphFormat) -> Result<CommandResult> {
    // Load and parse schema
    let schema_content = fs::read_to_string(schema_path)?;
    let schema: CompiledSchema = serde_json::from_str(&schema_content)?;

    // Build dependency graph
    let graph = SchemaDependencyGraph::build(&schema);

    // Analyze the graph
    let cycles = graph.find_cycles();
    let unused = graph.find_unused();

    // Build output structure
    let output = build_output(&graph, &cycles, &unused);

    // Check for cycles (these are errors)
    let warnings: Vec<String> = unused
        .iter()
        .map(|t| format!("Unused type: '{t}' has no incoming references"))
        .collect();

    // Format output based on requested format
    let data = match format {
        GraphFormat::Json => serde_json::to_value(&output)?,
        GraphFormat::Dot => Value::String(to_dot(&output)),
        GraphFormat::Mermaid => Value::String(to_mermaid(&output)),
        GraphFormat::D2 => Value::String(to_d2(&output)),
        GraphFormat::Console => Value::String(to_console(&output)),
    };

    // If cycles exist, return validation failure
    if !cycles.is_empty() {
        let errors: Vec<String> = cycles
            .iter()
            .map(|c| format!("Circular dependency: {}", c.path_string()))
            .collect();

        // Include the graph data in the error response
        return Ok(CommandResult {
            status: "validation-failed".to_string(),
            command: "dependency-graph".to_string(),
            data: Some(data),
            message: Some(format!("Schema has {} circular dependencies", cycles.len())),
            code: Some("CIRCULAR_DEPENDENCY".to_string()),
            errors,
            warnings,
            exit_code: 2,
        });
    }

    // Success - return graph with any warnings
    if warnings.is_empty() {
        Ok(CommandResult::success("dependency-graph", data))
    } else {
        Ok(CommandResult::success_with_warnings("dependency-graph", data, warnings))
    }
}

/// Build the output structure from the dependency graph
fn build_output(
    graph: &SchemaDependencyGraph,
    cycles: &[CyclePath],
    unused: &[String],
) -> DependencyGraphOutput {
    let all_types = graph.all_types();
    let root_types = ["Query", "Mutation", "Subscription"];

    // Build nodes
    let mut nodes: Vec<GraphNode> = all_types
        .iter()
        .map(|name| GraphNode {
            name:             name.clone(),
            dependency_count: graph.dependencies_of(name).len(),
            dependent_count:  graph.dependents_of(name).len(),
            is_root:          root_types.contains(&name.as_str()),
        })
        .collect();

    // Sort by dependent count (most depended on first)
    nodes.sort_by_key(|n| std::cmp::Reverse(n.dependent_count));

    // Build edges
    let mut edges: Vec<GraphEdge> = Vec::new();
    for type_name in &all_types {
        for dep in graph.dependencies_of(type_name) {
            edges.push(GraphEdge {
                from: type_name.clone(),
                to:   dep,
            });
        }
    }

    // Sort edges for consistent output
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

    // Build cycle info
    let cycle_info: Vec<CycleInfo> = cycles.iter().map(CycleInfo::from).collect();

    // Calculate stats
    let total_deps: usize = nodes.iter().map(|n| n.dependency_count).sum();
    #[allow(clippy::cast_precision_loss)] // Schema type counts won't exceed f64 precision
    let avg_deps = if nodes.is_empty() {
        0.0
    } else {
        total_deps as f64 / nodes.len() as f64
    };

    // Find most depended on types (top 5)
    let most_depended: Vec<String> = nodes
        .iter()
        .filter(|n| n.dependent_count > 0 && !n.is_root)
        .take(5)
        .map(|n| n.name.clone())
        .collect();

    // Calculate max depth (BFS from roots)
    let max_depth = calculate_max_depth(graph, &root_types);

    let stats = GraphStats {
        total_types: nodes.len(),
        total_edges: edges.len(),
        cycle_count: cycles.len(),
        unused_count: unused.len(),
        avg_dependencies: (avg_deps * 100.0).round() / 100.0,
        max_depth,
        most_depended_on: most_depended,
    };

    DependencyGraphOutput {
        type_count: nodes.len(),
        nodes,
        edges,
        cycles: cycle_info,
        unused_types: unused.to_vec(),
        stats,
    }
}

/// Calculate maximum depth from root types using BFS
fn calculate_max_depth(graph: &SchemaDependencyGraph, root_types: &[&str]) -> usize {
    use std::collections::{HashSet, VecDeque};

    let mut max_depth = 0;
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Start from each root that exists
    for &root in root_types {
        if graph.has_type(root) {
            queue.push_back((root.to_string(), 0));
            visited.insert(root.to_string());
        }
    }

    while let Some((type_name, depth)) = queue.pop_front() {
        max_depth = max_depth.max(depth);

        for dep in graph.dependencies_of(&type_name) {
            if !visited.contains(&dep) {
                visited.insert(dep.clone());
                queue.push_back((dep, depth + 1));
            }
        }
    }

    max_depth
}

/// Convert dependency graph to DOT format (Graphviz)
fn to_dot(output: &DependencyGraphOutput) -> String {
    use std::fmt::Write;

    let mut dot = String::from("digraph schema_dependencies {\n");
    dot.push_str("    rankdir=LR;\n");
    dot.push_str("    node [shape=box, style=rounded];\n\n");

    // Add legend comment
    dot.push_str("    // Root types (Query, Mutation, Subscription)\n");

    // Add nodes with styling
    for node in &output.nodes {
        let style = if node.is_root {
            "style=\"rounded,bold\", color=blue"
        } else if output.unused_types.contains(&node.name) {
            "style=\"rounded,dashed\", color=gray"
        } else {
            "style=rounded"
        };

        let name = &node.name;
        let deps = node.dependency_count;
        let refs = node.dependent_count;
        let _ = writeln!(
            dot,
            "    \"{name}\" [label=\"{name}\\n(deps: {deps}, refs: {refs})\", {style}];"
        );
    }

    dot.push_str("\n    // Dependencies\n");

    // Add edges
    for edge in &output.edges {
        let from = &edge.from;
        let to = &edge.to;
        let _ = writeln!(dot, "    \"{from}\" -> \"{to}\";");
    }

    // Highlight cycles
    if !output.cycles.is_empty() {
        dot.push_str("\n    // Cycles (highlighted in red)\n");
        for cycle in &output.cycles {
            for i in 0..cycle.types.len() {
                let from = &cycle.types[i];
                let to = &cycle.types[(i + 1) % cycle.types.len()];
                let _ = writeln!(dot, "    \"{from}\" -> \"{to}\" [color=red, penwidth=2];");
            }
        }
    }

    dot.push_str("}\n");
    dot
}

/// Convert dependency graph to Mermaid format
fn to_mermaid(output: &DependencyGraphOutput) -> String {
    use std::fmt::Write;

    let mut mermaid = String::from("```mermaid\ngraph LR\n");

    // Add subgraph for root types
    mermaid.push_str("    subgraph Roots\n");
    for node in &output.nodes {
        if node.is_root {
            let name = &node.name;
            let _ = writeln!(mermaid, "        {name}[\"{name}\"]");
        }
    }
    mermaid.push_str("    end\n\n");

    // Add other nodes
    for node in &output.nodes {
        if !node.is_root {
            let style = if output.unused_types.contains(&node.name) {
                ":::unused"
            } else {
                ""
            };
            let name = &node.name;
            let _ = writeln!(mermaid, "    {name}[\"{name}\"]{style}");
        }
    }

    mermaid.push('\n');

    // Add edges
    for edge in &output.edges {
        // Check if this edge is part of a cycle
        let is_cycle_edge = output.cycles.iter().any(|c| {
            let types = &c.types;
            for i in 0..types.len() {
                let from = &types[i];
                let to = &types[(i + 1) % types.len()];
                if from == &edge.from && to == &edge.to {
                    return true;
                }
            }
            false
        });

        let from = &edge.from;
        let to = &edge.to;
        if is_cycle_edge {
            let _ = writeln!(mermaid, "    {from} -->|CYCLE| {to}");
        } else {
            let _ = writeln!(mermaid, "    {from} --> {to}");
        }
    }

    // Add styling
    mermaid.push_str("\n    classDef unused fill:#f9f,stroke:#333,stroke-dasharray: 5 5\n");

    mermaid.push_str("```\n");
    mermaid
}

/// Convert dependency graph to D2 format (modern diagram language)
///
/// D2 is a modern diagram scripting language that compiles to SVG.
/// See: https://d2lang.com/
fn to_d2(output: &DependencyGraphOutput) -> String {
    use std::fmt::Write;

    let mut d2 = String::new();

    // Header comment
    d2.push_str("# Schema Dependency Graph\n");
    d2.push_str("# Generated by FraiseQL CLI\n");
    d2.push_str("# Render with: d2 schema.d2 schema.svg\n\n");

    // Global styling
    d2.push_str("direction: right\n\n");

    // Root types container
    let has_roots = output.nodes.iter().any(|n| n.is_root);
    if has_roots {
        d2.push_str("roots: {\n");
        d2.push_str("  label: \"Root Types\"\n");
        d2.push_str("  style.fill: \"#e3f2fd\"\n");
        d2.push_str("  style.stroke: \"#1976d2\"\n\n");
        for node in &output.nodes {
            if node.is_root {
                let name = &node.name;
                let deps = node.dependency_count;
                let refs = node.dependent_count;
                let _ = writeln!(d2, "  {name}: \"{name}\\n(deps: {deps}, refs: {refs})\" {{");
                d2.push_str("    style.bold: true\n");
                d2.push_str("    style.fill: \"#bbdefb\"\n");
                d2.push_str("  }\n");
            }
        }
        d2.push_str("}\n\n");
    }

    // Unused types container (if any)
    if !output.unused_types.is_empty() {
        d2.push_str("unused: {\n");
        d2.push_str("  label: \"Unused Types\"\n");
        d2.push_str("  style.fill: \"#fff3e0\"\n");
        d2.push_str("  style.stroke: \"#ff9800\"\n");
        d2.push_str("  style.stroke-dash: 3\n\n");
        for node in &output.nodes {
            if output.unused_types.contains(&node.name) {
                let name = &node.name;
                let _ = writeln!(d2, "  {name}: \"{name}\" {{");
                d2.push_str("    style.fill: \"#ffe0b2\"\n");
                d2.push_str("    style.stroke-dash: 3\n");
                d2.push_str("  }\n");
            }
        }
        d2.push_str("}\n\n");
    }

    // Regular types (not root, not unused)
    for node in &output.nodes {
        if !node.is_root && !output.unused_types.contains(&node.name) {
            let name = &node.name;
            let deps = node.dependency_count;
            let refs = node.dependent_count;
            let _ = writeln!(d2, "{name}: \"{name}\\n(deps: {deps}, refs: {refs})\"");
        }
    }

    d2.push('\n');

    // Edges
    d2.push_str("# Dependencies\n");
    for edge in &output.edges {
        // Check if this edge is part of a cycle
        let is_cycle_edge = output.cycles.iter().any(|c| {
            let types = &c.types;
            for i in 0..types.len() {
                let from = &types[i];
                let to = &types[(i + 1) % types.len()];
                if from == &edge.from && to == &edge.to {
                    return true;
                }
            }
            false
        });

        let from = &edge.from;
        let to = &edge.to;

        // Handle edges from root types (need to reference inside container)
        let from_ref = if output.nodes.iter().any(|n| n.is_root && &n.name == from) {
            format!("roots.{from}")
        } else if output.unused_types.contains(from) {
            format!("unused.{from}")
        } else {
            from.clone()
        };

        let to_ref = if output.nodes.iter().any(|n| n.is_root && &n.name == to) {
            format!("roots.{to}")
        } else if output.unused_types.contains(to) {
            format!("unused.{to}")
        } else {
            to.clone()
        };

        if is_cycle_edge {
            let _ = writeln!(d2, "{from_ref} -> {to_ref}: \"CYCLE\" {{");
            d2.push_str("  style.stroke: \"#d32f2f\"\n");
            d2.push_str("  style.stroke-width: 2\n");
            d2.push_str("}\n");
        } else {
            let _ = writeln!(d2, "{from_ref} -> {to_ref}");
        }
    }

    // Cycle warning comment
    if !output.cycles.is_empty() {
        d2.push_str("\n# WARNING: Circular dependencies detected!\n");
        for cycle in &output.cycles {
            let _ = writeln!(d2, "# Cycle: {}", cycle.path);
        }
    }

    d2
}

/// Convert dependency graph to console (human-readable) format
fn to_console(output: &DependencyGraphOutput) -> String {
    use std::fmt::Write;

    let mut console = String::new();

    // Header
    console.push_str("Schema Dependency Graph Analysis\n");
    console.push_str("================================\n\n");

    // Summary stats
    let _ = writeln!(console, "Total types: {}", output.stats.total_types);
    let _ = writeln!(console, "Total dependencies: {}", output.stats.total_edges);
    let _ =
        writeln!(console, "Average dependencies per type: {:.2}", output.stats.avg_dependencies);
    let _ = writeln!(console, "Maximum depth from roots: {}", output.stats.max_depth);
    console.push('\n');

    // Cycles (errors)
    if !output.cycles.is_empty() {
        let _ = writeln!(console, "CIRCULAR DEPENDENCIES ({}):", output.cycles.len());
        for cycle in &output.cycles {
            let _ = writeln!(console, "  - {}", cycle.path);
        }
        console.push('\n');
    }

    // Unused types (warnings)
    if !output.unused_types.is_empty() {
        let _ = writeln!(console, "UNUSED TYPES ({}):", output.unused_types.len());
        for unused in &output.unused_types {
            let _ = writeln!(console, "  - {unused}");
        }
        console.push('\n');
    }

    // Most depended on types
    if !output.stats.most_depended_on.is_empty() {
        console.push_str("Most referenced types:\n");
        for (i, type_name) in output.stats.most_depended_on.iter().enumerate() {
            let node = output.nodes.iter().find(|n| &n.name == type_name);
            if let Some(node) = node {
                let _ = writeln!(
                    console,
                    "  {}. {type_name} ({} references)",
                    i + 1,
                    node.dependent_count
                );
            }
        }
        console.push('\n');
    }

    // Type details
    console.push_str("Type Details:\n");
    console.push_str("-------------\n");

    for node in &output.nodes {
        let prefix = if node.is_root {
            "[ROOT] "
        } else if output.unused_types.contains(&node.name) {
            "[UNUSED] "
        } else {
            ""
        };

        let _ = writeln!(
            console,
            "{prefix}{}: {} deps, {} refs",
            node.name, node.dependency_count, node.dependent_count
        );
    }

    console
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_format_from_str() {
        assert_eq!("json".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("dot".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("graphviz".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("mermaid".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
        assert_eq!("md".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
        assert_eq!("d2".parse::<GraphFormat>().unwrap(), GraphFormat::D2);
        assert_eq!("console".parse::<GraphFormat>().unwrap(), GraphFormat::Console);
        assert_eq!("text".parse::<GraphFormat>().unwrap(), GraphFormat::Console);
    }

    #[test]
    fn test_graph_format_case_insensitive() {
        assert_eq!("JSON".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("DOT".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("MERMAID".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
        assert_eq!("D2".parse::<GraphFormat>().unwrap(), GraphFormat::D2);
    }

    #[test]
    fn test_graph_format_invalid() {
        let result = "invalid".parse::<GraphFormat>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown format"));
    }

    #[test]
    fn test_graph_format_display() {
        assert_eq!(GraphFormat::Json.to_string(), "json");
        assert_eq!(GraphFormat::Dot.to_string(), "dot");
        assert_eq!(GraphFormat::Mermaid.to_string(), "mermaid");
        assert_eq!(GraphFormat::D2.to_string(), "d2");
        assert_eq!(GraphFormat::Console.to_string(), "console");
    }

    #[test]
    fn test_to_dot_contains_expected_elements() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "Query".to_string(),
                    dependency_count: 1,
                    dependent_count:  0,
                    is_root:          true,
                },
                GraphNode {
                    name:             "User".to_string(),
                    dependency_count: 0,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![GraphEdge {
                from: "Query".to_string(),
                to:   "User".to_string(),
            }],
            cycles:       vec![],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      1,
                cycle_count:      0,
                unused_count:     0,
                avg_dependencies: 0.5,
                max_depth:        1,
                most_depended_on: vec!["User".to_string()],
            },
        };

        let dot = to_dot(&output);
        assert!(dot.contains("digraph schema_dependencies"));
        assert!(dot.contains("Query"));
        assert!(dot.contains("User"));
        assert!(dot.contains("\"Query\" -> \"User\""));
    }

    #[test]
    fn test_to_mermaid_contains_expected_elements() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "Query".to_string(),
                    dependency_count: 1,
                    dependent_count:  0,
                    is_root:          true,
                },
                GraphNode {
                    name:             "User".to_string(),
                    dependency_count: 0,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![GraphEdge {
                from: "Query".to_string(),
                to:   "User".to_string(),
            }],
            cycles:       vec![],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      1,
                cycle_count:      0,
                unused_count:     0,
                avg_dependencies: 0.5,
                max_depth:        1,
                most_depended_on: vec!["User".to_string()],
            },
        };

        let mermaid = to_mermaid(&output);
        assert!(mermaid.contains("```mermaid"));
        assert!(mermaid.contains("graph LR"));
        assert!(mermaid.contains("Query"));
        assert!(mermaid.contains("User"));
        assert!(mermaid.contains("Query --> User"));
    }

    #[test]
    fn test_to_d2_contains_expected_elements() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "Query".to_string(),
                    dependency_count: 1,
                    dependent_count:  0,
                    is_root:          true,
                },
                GraphNode {
                    name:             "User".to_string(),
                    dependency_count: 0,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![GraphEdge {
                from: "Query".to_string(),
                to:   "User".to_string(),
            }],
            cycles:       vec![],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      1,
                cycle_count:      0,
                unused_count:     0,
                avg_dependencies: 0.5,
                max_depth:        1,
                most_depended_on: vec!["User".to_string()],
            },
        };

        let d2 = to_d2(&output);
        assert!(d2.contains("# Schema Dependency Graph"));
        assert!(d2.contains("direction: right"));
        assert!(d2.contains("roots:"));
        assert!(d2.contains("Query"));
        assert!(d2.contains("User"));
        assert!(d2.contains("roots.Query -> User"));
    }

    #[test]
    fn test_to_d2_shows_unused() {
        let output = DependencyGraphOutput {
            type_count:   1,
            nodes:        vec![GraphNode {
                name:             "Orphan".to_string(),
                dependency_count: 0,
                dependent_count:  0,
                is_root:          false,
            }],
            edges:        vec![],
            cycles:       vec![],
            unused_types: vec!["Orphan".to_string()],
            stats:        GraphStats {
                total_types:      1,
                total_edges:      0,
                cycle_count:      0,
                unused_count:     1,
                avg_dependencies: 0.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let d2 = to_d2(&output);
        assert!(d2.contains("unused:"));
        assert!(d2.contains("Unused Types"));
        assert!(d2.contains("Orphan"));
        assert!(d2.contains("stroke-dash"));
    }

    #[test]
    fn test_to_d2_shows_cycles() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "A".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
                GraphNode {
                    name:             "B".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![
                GraphEdge {
                    from: "A".to_string(),
                    to:   "B".to_string(),
                },
                GraphEdge {
                    from: "B".to_string(),
                    to:   "A".to_string(),
                },
            ],
            cycles:       vec![CycleInfo {
                types:             vec!["A".to_string(), "B".to_string()],
                path:              "A -> B -> A".to_string(),
                is_self_reference: false,
            }],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      2,
                cycle_count:      1,
                unused_count:     0,
                avg_dependencies: 1.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let d2 = to_d2(&output);
        assert!(d2.contains("CYCLE"));
        assert!(d2.contains("stroke: \"#d32f2f\""));
        assert!(d2.contains("# WARNING: Circular dependencies detected!"));
    }

    #[test]
    fn test_to_console_contains_expected_elements() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "Query".to_string(),
                    dependency_count: 1,
                    dependent_count:  0,
                    is_root:          true,
                },
                GraphNode {
                    name:             "User".to_string(),
                    dependency_count: 0,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![GraphEdge {
                from: "Query".to_string(),
                to:   "User".to_string(),
            }],
            cycles:       vec![],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      1,
                cycle_count:      0,
                unused_count:     0,
                avg_dependencies: 0.5,
                max_depth:        1,
                most_depended_on: vec!["User".to_string()],
            },
        };

        let console = to_console(&output);
        assert!(console.contains("Schema Dependency Graph Analysis"));
        assert!(console.contains("Total types: 2"));
        assert!(console.contains("[ROOT] Query"));
        assert!(console.contains("User"));
    }

    #[test]
    fn test_to_console_shows_cycles() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "A".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
                GraphNode {
                    name:             "B".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![
                GraphEdge {
                    from: "A".to_string(),
                    to:   "B".to_string(),
                },
                GraphEdge {
                    from: "B".to_string(),
                    to:   "A".to_string(),
                },
            ],
            cycles:       vec![CycleInfo {
                types:             vec!["A".to_string(), "B".to_string()],
                path:              "A -> B -> A".to_string(),
                is_self_reference: false,
            }],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      2,
                cycle_count:      1,
                unused_count:     0,
                avg_dependencies: 1.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let console = to_console(&output);
        assert!(console.contains("CIRCULAR DEPENDENCIES"));
        assert!(console.contains("A -> B -> A"));
    }

    #[test]
    fn test_to_console_shows_unused() {
        let output = DependencyGraphOutput {
            type_count:   1,
            nodes:        vec![GraphNode {
                name:             "Orphan".to_string(),
                dependency_count: 0,
                dependent_count:  0,
                is_root:          false,
            }],
            edges:        vec![],
            cycles:       vec![],
            unused_types: vec!["Orphan".to_string()],
            stats:        GraphStats {
                total_types:      1,
                total_edges:      0,
                cycle_count:      0,
                unused_count:     1,
                avg_dependencies: 0.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let console = to_console(&output);
        assert!(console.contains("UNUSED TYPES"));
        assert!(console.contains("Orphan"));
        assert!(console.contains("[UNUSED]"));
    }

    #[test]
    fn test_cycle_info_from_cycle_path() {
        use fraiseql_core::schema::CyclePath;

        let cycle = CyclePath::new(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        let info = CycleInfo::from(&cycle);

        assert_eq!(info.types, vec!["A", "B", "C"]);
        assert_eq!(info.path, "A → B → C → A");
        assert!(!info.is_self_reference);
    }

    #[test]
    fn test_cycle_info_self_reference() {
        use fraiseql_core::schema::CyclePath;

        let cycle = CyclePath::new(vec!["Node".to_string()]);
        let info = CycleInfo::from(&cycle);

        assert!(info.is_self_reference);
        assert_eq!(info.path, "Node → Node");
    }
}
