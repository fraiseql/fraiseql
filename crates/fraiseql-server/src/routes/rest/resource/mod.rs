//! REST resource derivation engine.
//!
//! Derives REST resources and routes from a [`CompiledSchema`] by grouping
//! operations by return type and mapping them to HTTP methods and paths.

pub mod derivation;
pub mod naming;
pub mod validation;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeSet, HashMap},
    fmt,
};

use derivation::derive_resource;
use fraiseql_core::schema::{CompiledSchema, MutationDefinition, QueryDefinition};
use tracing::debug;
use validation::{detect_conflicts, is_filtered_out, should_skip_query};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The `(route-relative path, method)` operations a REST router serves.
///
/// This is the **single** answer to "what does this transport expose". The router drives
/// its axum registration from it, and the served `OpenAPI` document is filtered through
/// it, so the two cannot describe different surfaces.
///
/// They used to be derived independently from the [`RestRouteTable`], and they disagreed
/// in both directions at once. A read-only deployment mounted `rest_query_router` while
/// the document was generated from the full table, so it published the entire write API
/// the server answered with `405` (#865). And an item-level `PATCH /items/{id}/rename`
/// marked the collection as already having `PATCH`, suppressing the bulk fallback in the
/// router while `add_bulk_operations` advertised it from the mutation list anyway
/// (#918). Both are the same defect: two loops answering one question.
///
/// Paths are stored in route-table form (`/items/{id}`), not axum form, because that is
/// what the `OpenAPI` document uses as its path keys; the router converts on the way to
/// `Router::route`.
#[derive(Debug, Default, Clone)]
pub struct MountedRoutes(BTreeSet<(String, HttpMethod)>);

impl MountedRoutes {
    /// The operations a **read-only** router serves: every derived `GET`, plus one SSE
    /// stream endpoint per resource.
    ///
    /// This is the posture of an adapter that cannot execute mutations at all
    /// (`SqliteAdapter`, `FraiseWireAdapter`).
    #[must_use]
    pub fn read_surface(route_table: &RestRouteTable) -> Self {
        let mut mounted = Self::default();
        for resource in &route_table.resources {
            for route in &resource.routes {
                if route.method == HttpMethod::Get {
                    mounted.insert(route.path.clone(), HttpMethod::Get);
                }
            }
            mounted.insert(stream_route_path(resource), HttpMethod::Get);
        }
        mounted
    }

    /// The operations a **full** router serves: every derived route, the collection-level
    /// bulk `PATCH`/`DELETE` fallbacks, and one SSE stream endpoint per resource.
    ///
    /// A bulk fallback is added only when the resource has a matching mutation *and* the
    /// derived routes did not already claim that method on the collection path — the
    /// condition #918 got wrong by keying on the resource name rather than on the route
    /// actually being the collection route.
    #[must_use]
    pub fn write_surface(schema: &CompiledSchema, route_table: &RestRouteTable) -> Self {
        use fraiseql_core::schema::MutationOperation;

        let mut mounted = Self::default();
        for resource in &route_table.resources {
            for route in &resource.routes {
                mounted.insert(route.path.clone(), route.method);
            }

            let collection = collection_path(resource);
            let has = |pred: fn(&MutationOperation) -> bool| {
                resource.routes.iter().any(|r| {
                    matches!(&r.source, RouteSource::Mutation { name }
                        if schema.find_mutation(name).is_some_and(|m| pred(&m.operation)))
                })
            };

            if has(|op| matches!(op, MutationOperation::Update { .. }))
                && !mounted.contains(&collection, HttpMethod::Patch)
            {
                mounted.insert(collection.clone(), HttpMethod::Patch);
            }
            if has(|op| matches!(op, MutationOperation::Delete { .. }))
                && !mounted.contains(&collection, HttpMethod::Delete)
            {
                mounted.insert(collection, HttpMethod::Delete);
            }

            mounted.insert(stream_route_path(resource), HttpMethod::Get);
        }
        mounted
    }

    /// Record that `method path` is served.
    pub fn insert(&mut self, path: impl Into<String>, method: HttpMethod) {
        self.0.insert((path.into(), method));
    }

    /// Whether `method path` is served.
    #[must_use]
    pub fn contains(&self, path: &str, method: HttpMethod) -> bool {
        self.0.iter().any(|(p, m)| p == path && *m == method)
    }

    /// Every served operation, in a deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, HttpMethod)> {
        self.0.iter().map(|(p, m)| (p.as_str(), *m))
    }

    /// Number of served operations. Used by the router's startup log.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no operation is served at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The collection route path for a resource (`/users`).
#[must_use]
pub fn collection_path(resource: &RestResource) -> String {
    format!("/{}", resource.name)
}

/// The SSE stream route path for a resource (`/users/stream`).
#[must_use]
pub fn stream_route_path(resource: &RestResource) -> String {
    format!("/{}/stream", resource.name)
}

/// HTTP method for a REST route.
///
/// `Ord` is derived so [`MountedRoutes`] can hold its operations in a deterministic
/// order: the router's registration order and the served `OpenAPI` document's path order
/// are both driven from that set, and neither should vary between processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
