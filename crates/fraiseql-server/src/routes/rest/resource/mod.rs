//! REST resource derivation engine.
//!
//! Derives REST resources and routes from a [`CompiledSchema`] by grouping
//! operations by return type and mapping them to HTTP methods and paths.

pub mod derivation;
pub mod naming;
pub mod validation;

#[cfg(test)]
mod tests;

use std::{collections::HashMap, fmt};

use derivation::derive_resource;
use fraiseql_core::schema::{CompiledSchema, MutationDefinition, QueryDefinition};
use tracing::debug;
use validation::{detect_conflicts, is_filtered_out, should_skip_query};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// HTTP method for a REST route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HttpMethod {
    /// HTTP GET.
    Get,
    /// HTTP POST.
    Post,
    /// HTTP PUT.
    Put,
    /// HTTP PATCH.
    Patch,
    /// HTTP DELETE.
    Delete,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Patch => write!(f, "PATCH"),
            Self::Delete => write!(f, "DELETE"),
        }
    }
}

/// Classification of an Update mutation's field coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpdateCoverage {
    /// Mutation covers all writable fields — generates both PUT and PATCH.
    Full,
    /// Mutation covers only a subset — generates PATCH as a sub-resource action.
    Partial,
}

/// The kind of operation backing a REST route.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RouteSource {
    /// Backed by a compiled query.
    Query {
        /// Query operation name.
        name: String,
    },
    /// Backed by a compiled mutation.
    Mutation {
        /// Mutation operation name.
        name: String,
    },
}

/// A single REST route derived from the compiled schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestRoute {
    /// HTTP method.
    pub method:          HttpMethod,
    /// Path relative to the REST base (e.g., `/users` or `/users/{id}`).
    pub path:            String,
    /// The operation backing this route.
    pub source:          RouteSource,
    /// For Update mutations, the coverage classification.
    pub update_coverage: Option<UpdateCoverage>,
    /// Expected successful HTTP status code.
    pub success_status:  u16,
}

/// A REST resource groups routes under a common base path derived from a
/// return type.
#[derive(Debug, Clone)]
pub struct RestResource {
    /// Resource base name (e.g., `users`).
    pub name:      String,
    /// GraphQL return type name (e.g., `User`).
    pub type_name: String,
    /// Name of the ID argument for single-resource routes (e.g., `id`).
    pub id_arg:    Option<String>,
    /// Routes for this resource.
    pub routes:    Vec<RestRoute>,
}

/// Complete route table derived from a compiled schema.
#[derive(Debug, Clone)]
pub struct RestRouteTable {
    /// REST base path (e.g., `/rest/v1`).
    pub base_path:   String,
    /// Resources keyed by resource name.
    pub resources:   Vec<RestResource>,
    /// Diagnostics emitted during derivation.
    pub diagnostics: Vec<Diagnostic>,
}

/// A diagnostic message from the derivation engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity level.
    pub level:   DiagnosticLevel,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticLevel {
    /// Informational (e.g., fallback resource name derived from type).
    Info,
    /// Warning (e.g., CQRS naming violation).
    Warning,
    /// Error (e.g., route conflict).
    Error,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl RestRouteTable {
    /// Derive a route table from a compiled schema.
    ///
    /// Returns `Err` if route conflicts are detected that cannot be resolved.
    ///
    /// # Errors
    ///
    /// Returns an error string if two operations produce the same method+path
    /// combination and neither has a `rest_path` override.
    pub fn from_compiled_schema(schema: &CompiledSchema) -> Result<Self, String> {
        let config = schema.rest_config.clone().unwrap_or_default();
        let base_path = config.path.clone();

        // Group operations by return type.
        let mut query_groups: HashMap<&str, Vec<&QueryDefinition>> = HashMap::new();
        let mut mutation_groups: HashMap<&str, Vec<&MutationDefinition>> = HashMap::new();

        for q in &schema.queries {
            if should_skip_query(q) {
                debug!(query = %q.name, "skipping query (aggregate/window/scalar)");
                continue;
            }
            if is_filtered_out(&q.name, &config) {
                debug!(query = %q.name, "skipping query (include/exclude filter)");
                continue;
            }
            // Check return type has a TypeDefinition.
            if schema.find_type(&q.return_type).is_none() {
                debug!(query = %q.name, return_type = %q.return_type, "skipping query (no TypeDefinition)");
                continue;
            }
            query_groups.entry(q.return_type.as_str()).or_default().push(q);
        }

        for m in &schema.mutations {
            if is_filtered_out(&m.name, &config) {
                debug!(mutation = %m.name, "skipping mutation (include/exclude filter)");
                continue;
            }
            if schema.find_type(&m.return_type).is_none() {
                debug!(mutation = %m.name, return_type = %m.return_type, "skipping mutation (no TypeDefinition)");
                continue;
            }
            mutation_groups.entry(m.return_type.as_str()).or_default().push(m);
        }

        // Collect all return types.
        let mut all_types: Vec<&str> = query_groups.keys().copied().collect();
        for t in mutation_groups.keys() {
            if !all_types.contains(t) {
                all_types.push(t);
            }
        }
        all_types.sort_unstable();

        let mut resources = Vec::new();
        let mut diagnostics = Vec::new();

        for type_name in all_types {
            let Some(type_def) = schema.find_type(type_name) else {
                continue;
            };
            let queries = query_groups.get(type_name).map_or(&[][..], |v| v.as_slice());
            let mutations = mutation_groups.get(type_name).map_or(&[][..], |v| v.as_slice());

            let resource =
                derive_resource(type_name, type_def, queries, mutations, &config, &mut diagnostics);

            if let Some(r) = resource {
                resources.push(r);
            }
        }

        // Detect route conflicts.
        detect_conflicts(&resources, &mut diagnostics)?;

        let table = Self {
            base_path,
            resources,
            diagnostics,
        };

        Ok(table)
    }
}

impl fmt::Display for RestRouteTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "REST Route Table (base: {})", self.base_path)?;
        for resource in &self.resources {
            writeln!(f, "  Resource: {} (type: {})", resource.name, resource.type_name)?;
            for route in &resource.routes {
                writeln!(f, "    {} {}{}", route.method, self.base_path, route.path)?;
            }
        }
        Ok(())
    }
}
