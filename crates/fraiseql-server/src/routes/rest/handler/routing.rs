//! Route matching and resolution for REST handlers.

use fraiseql_core::{runtime::QueryMatch, schema::CompiledSchema};

use crate::routes::rest::{
    embedding,
    handler::RestError,
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
    pub query_name:            String,
    /// Pre-built query match with field selection and arguments.
    pub query_match:           QueryMatch,
    /// Variables for relay pagination.
    pub variables:             serde_json::Value,
    /// Extracted request parameters (pagination, embeddings, etc.).
    pub params:                ExtractedParams,
    /// Keys added to the projection for the **server's** use, which the response
    /// must not carry.
    ///
    /// Populated only by [`Self::with_embed_join_keys`], and empty otherwise — a
    /// representation that does not execute embeddings never asks for them, so it
    /// has nothing to take back out. Whoever populates this is responsible for
    /// passing it to [`embedding::strip_projected_keys`] before serialising.
    ///
    /// The export representations are the "otherwise": since #1268 they *refuse* a
    /// request naming an embed rather than dropping it, so a `ResolvedGetQuery` they
    /// produce carries no `embeddings` and no `embedding_counts` — nothing to widen for
    /// and nothing to strip. That is a stronger guarantee than the one this field was
    /// written under, where the widening had to be withheld from a path that would
    /// otherwise have emitted the extra column as a CSV header.
    pub server_projected_keys: Vec<String>,
}

impl ResolvedGetQuery {
    /// Add the parent-row keys this request's embeds join on to the projection.
    ///
    /// Applied by the JSON representation only, because it is the only one that
    /// executes embeddings. Keeping this a step the caller applies — rather than folding
    /// it into [`super::RestHandler::resolve_get_query`] — is what lets
    /// [`super::RestHandler::resolve_streaming_get_query`] keep resolving through the
    /// same function (#958) without inheriting a projection it cannot undo, and keeps
    /// the widening adjacent to the [`embedding::strip_projected_keys`] call that is its
    /// other half.
    ///
    /// When this was written the exports *dropped* `?select=` embeds, so widening for
    /// them would have emitted a column the client never named — as a CSV header, no
    /// less. Since #1268 they refuse such a request outright, so there is no longer a
    /// path that could inherit the widening at all: `required_join_keys` over the empty
    /// selections an export is allowed to carry returns nothing, and this method is an
    /// identity.
    ///
    /// See [`embedding::required_join_keys`] for why the server projects a key the
    /// client did not ask for (#1230).
    ///
    /// # Errors
    ///
    /// Returns `RestError` if the widened projection cannot be rebuilt into a
    /// `QueryMatch` — the same failure `resolve_get_query` reports for the original.
    pub fn with_embed_join_keys(mut self, schema: &CompiledSchema) -> Result<Self, RestError> {
        let return_type = self.query_match.query_def.return_type.clone();
        let required = embedding::required_join_keys(
            schema,
            &return_type,
            &self.params.embeddings,
            &self.params.embedding_counts,
        );

        let mut fields = self.query_match.fields.clone();
        let added = embedding::project_missing_join_keys(&mut fields, &required);
        if added.is_empty() {
            return Ok(self);
        }

        self.query_match = QueryMatch::from_operation(
            self.query_match.query_def.clone(),
            fields,
            self.query_match.arguments.clone(),
            schema.find_type(&return_type),
        )?;
        self.server_projected_keys = added;
        Ok(self)
    }
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
