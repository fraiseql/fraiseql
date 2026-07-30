//! `OpenAPI` 3.0.3 specification generator for the REST transport.
//!
//! Generates a complete `OpenAPI` spec from a [`CompiledSchema`] and its
//! [`RestRouteTable`].  The spec is built using `serde_json::Value` directly —
//! no runtime dependency on `openapiv3`.
//!
//! The generated spec includes:
//! - Type schemas in `components/schemas`
//! - Path items derived from the route table
//! - Security schemes from REST config
//! - Bracket operator documentation in filter parameters
//! - `Prefer` header documentation on collection/delete endpoints

pub mod bulk;
pub mod format;
pub mod parameters;
pub mod paths;
pub mod responses;
pub mod schemas;
pub mod security;

#[cfg(test)]
mod tests;

use fraiseql_core::schema::{CompiledSchema, RestConfig};
use serde_json::{Value, json};

use super::resource::{MountedRoutes, RestRouteTable};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate an `OpenAPI` 3.0.3 specification from a compiled schema and its
/// derived REST route table.
///
/// `auth_layer_attached` reports whether the deployment puts an authentication layer in
/// front of the REST router — which the compiled schema alone cannot know, since it is a
/// property of the *server* config (`[auth]` / `[auth_hs256]`), not of `[rest]`.
///
/// The document's security advertisement is derived from this **and** `require_auth`
/// together, so it cannot disagree with what the server enforces in either direction.
/// Before #810 it was derived from `require_auth` alone while `require_auth` was read at
/// exactly one route, so the spec promised `BearerAuth` and a 401 on operations that
/// accepted anonymous callers — an operator reading the served contract to confirm the
/// surface was closed got a document that said yes and a server that said no.
///
/// `mounted` is the set of operations the router actually registered. The document
/// describes exactly those and nothing else, so it cannot advertise a path the server
/// answers with `405` (#865, #918). Passing a set derived from the route table instead
/// of from the registration loop would reintroduce the drift this argument exists to
/// remove.
///
/// # Errors
///
/// Returns `Err` if the schema is missing REST configuration.
pub fn generate_openapi(
    schema: &CompiledSchema,
    route_table: &RestRouteTable,
    auth_layer_attached: bool,
    mounted: &MountedRoutes,
) -> Result<Value, String> {
    let config = schema
        .rest_config
        .as_ref()
        .ok_or_else(|| "REST config not found in compiled schema".to_string())?;

    let generator =
        OpenApiGenerator::new(schema, route_table, config, auth_layer_attached, mounted);
    Ok(generator.generate())
}

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

/// Generates an `OpenAPI` 3.0.3 spec from schema metadata.
struct OpenApiGenerator<'a> {
    schema:            &'a CompiledSchema,
    route_table:       &'a RestRouteTable,
    config:            &'a RestConfig,
    /// Whether a caller must present a credential to reach *any* REST route.
    ///
    /// True when `[rest] require_auth = true` (enforced for every route by the
    /// `RestSecurityContext` extractor) or when the deployment attaches an auth
    /// middleware to the REST router (enforced by that middleware). Either way the
    /// answer to "does this operation need a bearer token" is the same for all
    /// operations, because both mechanisms are transport-wide.
    security_required: bool,
    /// The operations the router registered. Every path item is filtered through this.
    mounted:           &'a MountedRoutes,
}

impl<'a> OpenApiGenerator<'a> {
    const fn new(
        schema: &'a CompiledSchema,
        route_table: &'a RestRouteTable,
        config: &'a RestConfig,
        auth_layer_attached: bool,
        mounted: &'a MountedRoutes,
    ) -> Self {
        Self {
            schema,
            route_table,
            config,
            security_required: config.require_auth || auth_layer_attached,
            mounted,
        }
    }

    fn generate(&self) -> Value {
        let mut spec = json!({
            "openapi": "3.0.3",
            "info": self.build_info(),
            "paths": self.build_paths(),
            "components": self.build_components(),
        });

        spec["servers"] = json!([{
            "url": self.config.path,
            "description": "REST API base path"
        }]);

        spec
    }

    fn build_info(&self) -> Value {
        json!({
            "title": "FraiseQL REST API",
            "version": "1.0.0",
            "description": "Auto-generated REST API from compiled schema",
        })
    }
}
