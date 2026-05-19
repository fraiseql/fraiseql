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

use super::resource::RestRouteTable;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate an `OpenAPI` 3.0.3 specification from a compiled schema and its
/// derived REST route table.
///
/// # Errors
///
/// Returns `Err` if the schema is missing REST configuration.
pub fn generate_openapi(
    schema: &CompiledSchema,
    route_table: &RestRouteTable,
) -> Result<Value, String> {
    let config = schema
        .rest_config
        .as_ref()
        .ok_or_else(|| "REST config not found in compiled schema".to_string())?;

    let generator = OpenApiGenerator::new(schema, route_table, config);
    Ok(generator.generate())
}

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

/// Generates an `OpenAPI` 3.0.3 spec from schema metadata.
struct OpenApiGenerator<'a> {
    schema: &'a CompiledSchema,
    route_table: &'a RestRouteTable,
    config: &'a RestConfig,
}

impl<'a> OpenApiGenerator<'a> {
    const fn new(
        schema: &'a CompiledSchema,
        route_table: &'a RestRouteTable,
        config: &'a RestConfig,
    ) -> Self {
        Self {
            schema,
            route_table,
            config,
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
