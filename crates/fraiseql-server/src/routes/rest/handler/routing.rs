//! Route matching and resolution for REST handlers.

use fraiseql_core::runtime::QueryMatch;

use crate::routes::rest::{
    params::ExtractedParams,
    resource::{HttpMethod, RestResource, RestRoute, RestRouteTable},
};

/// Resolved route from a request path and method.
#[derive(Debug)]
pub struct ResolvedRoute<'a> {
    /// The matched REST resource.
    pub resource:    &'a RestResource,
    /// The matched REST route.
    pub route:       &'a RestRoute,
    /// Path parameters extracted from the URL (e.g., `[("id", "123")]`).
    pub path_params: Vec<(String, String)>,
}

/// Pre-resolved GET query context, ready for execution.
///
/// Produced by [`super::RestHandler::resolve_get_query`] and consumed by both
/// `handle_get` (JSON envelope) and NDJSON streaming.
pub struct ResolvedGetQuery {
    /// Name of the matched query.
    pub query_name:  String,
    /// Pre-built query match with field selection and arguments.
    pub query_match: QueryMatch,
    /// Variables for relay pagination.
    pub variables:   serde_json::Value,
    /// Extracted request parameters (pagination, embeddings, etc.).
    pub params:      ExtractedParams,
}

impl RestRouteTable {
    /// Resolve a request path and HTTP method to a route.
    ///
    /// `relative_path` should be the path relative to the REST base path,
    /// e.g., `/users/123` when base is `/rest/v1`.
    ///
    /// # Errors
    ///
    /// Returns `None` if no route matches the path+method combination.
    #[must_use]
    pub fn resolve(&self, relative_path: &str, method: HttpMethod) -> Option<ResolvedRoute<'_>> {
        let segments: Vec<&str> = relative_path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for resource in &self.resources {
            for route in &resource.routes {
                if route.method != method {
                    continue;
                }

                if let Some(path_params) = match_route_path(&route.path, &segments) {
                    return Some(ResolvedRoute {
                        resource,
                        route,
                        path_params,
                    });
                }
            }
        }

        None
    }
}

/// Match a route path pattern against URL segments.
///
/// Route paths use `{param}` syntax for path parameters.
/// Returns extracted path params on match, or `None`.
pub(super) fn match_route_path(
    route_path: &str,
    segments: &[&str],
) -> Option<Vec<(String, String)>> {
    let pattern_segments: Vec<&str> = route_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if pattern_segments.len() != segments.len() {
        return None;
    }

    let mut path_params = Vec::new();
    for (pattern, actual) in pattern_segments.iter().zip(segments.iter()) {
        if pattern.starts_with('{') && pattern.ends_with('}') {
            let param_name = &pattern[1..pattern.len() - 1];
            path_params.push((param_name.to_string(), (*actual).to_string()));
        } else if *pattern != *actual {
            return None;
        }
    }

    Some(path_params)
}
