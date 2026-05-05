//! Schema dependency graph analysis.
//!
//! This module provides tools for analyzing type dependencies in a compiled schema,
//! including cycle detection, unused type detection, and impact analysis.
//!
//! # Example
//!
//! ```
//! use fraiseql_core::schema::{CompiledSchema, SchemaDependencyGraph};
//!
//! let schema = CompiledSchema::default();
//! let graph = SchemaDependencyGraph::build(&schema);
//!
//! // Check for circular dependencies
//! let cycles = graph.find_cycles();
//! if !cycles.is_empty() {
//!     for cycle in &cycles {
//!         println!("Cycle detected: {}", cycle.path_string());
//!     }
//! }
//!
//! // Find unused types
//! let unused = graph.find_unused();
//! for type_name in &unused {
//!     println!("Unused type: {}", type_name);
//! }
//! ```

mod analysis;
mod builder;
mod graph;
mod types;

pub use graph::SchemaDependencyGraph;
pub use types::{ChangeImpact, CyclePath};

#[cfg(test)]
mod tests;
