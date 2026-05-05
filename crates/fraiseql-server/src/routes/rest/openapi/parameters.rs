//! `OpenAPI` parameter building: path, query, header parameters.

use serde_json::{Value, json};

use super::{
    OpenApiGenerator,
    helpers::{BRACKET_OPERATORS_DESC, field_type_to_json_schema, should_have_prefer_header},
};
use crate::routes::rest::resource::{HttpMethod, RestResource, RestRoute, RouteSource};

impl OpenApiGenerator<'_> {
    /// Build the parameter array for a route.
    pub(super) fn build_parameters(
        &self,
        resource: &RestResource,
        route: &RestRoute,
    ) -> Vec<Value> {
        let mut params = Vec::new();

        // ID path parameter for single-resource routes.
        if let Some(ref id_arg) = resource.id_arg {
            if route.path.contains('{') {
                let id_type = self.detect_id_type(resource);
                params.push(json!({
                    "name": id_arg,
                    "in": "path",
                    "required": true,
                    "description": format!("{} identifier", resource.type_name),
                    "schema": id_type,
                }));
            }
        }

        // Query parameters for GET collection endpoints.
        if route.method == HttpMethod::Get {
            if let RouteSource::Query { name } = &route.source {
                let query_def = self.schema.queries.iter().find(|q| q.name == *name);

                if let Some(q) = query_def {
                    if q.returns_list {
                        self.add_collection_params(&mut params, q, resource);
                    }
                }
            }
        }

        // Prefer header for collection GET and DELETE.
        if should_have_prefer_header(route) {
            params.push(json!({
                "name": "Prefer",
                "in": "header",
                "required": false,
                "description": "Request preferences (RFC 7240). Supported: count=exact|planned|estimated, return=representation|minimal, resolution=merge-duplicates|ignore-duplicates, handling=strict|lenient, tx=rollback|commit, max-affected=N.",
                "schema": { "type": "string" },
                "examples": {
                    "count_exact": {
                        "summary": "Include exact total count",
                        "value": "count=exact"
                    },
                    "count_planned": {
                        "summary": "Include estimated count from EXPLAIN (PostgreSQL)",
                        "value": "count=planned"
                    },
                    "return_representation": {
                        "summary": "Return entity body on mutating operations",
                        "value": "return=representation"
                    },
                    "handling_lenient": {
                        "summary": "Ignore unknown parameters",
                        "value": "handling=lenient"
                    },
                    "combined": {
                        "summary": "Multiple preferences",
                        "value": "return=representation, count=exact, handling=strict"
                    }
                }
            }));
        }

        // Idempotency-Key header for POST (create) endpoints.
        if route.method == HttpMethod::Post {
            params.push(json!({
                "name": "Idempotency-Key",
                "in": "header",
                "required": false,
                "description": "Client-generated unique key for idempotent POST requests. If a previous request with the same key and body was executed, the stored response is replayed. Reuse with a different body returns 422 IDEMPOTENCY_CONFLICT. Ignored on GET, PUT, and DELETE (inherently idempotent).",
                "schema": { "type": "string" }
            }));
        }

        params
    }

    /// Add collection-level query parameters (select, sort, pagination, filters).
    pub(super) fn add_collection_params(
        &self,
        params: &mut Vec<Value>,
        query_def: &fraiseql_core::schema::QueryDefinition,
        _resource: &RestResource,
    ) {
        // Select parameter.
        let type_def = self.schema.find_type(&query_def.return_type);
        let field_names: Vec<String> = type_def
            .map(|td| td.fields.iter().map(|f| f.name.to_string()).collect())
            .unwrap_or_default();
        let fields_desc = if field_names.is_empty() {
            String::new()
        } else {
            format!(" Available: {}", field_names.join(", "))
        };

        params.push(json!({
            "name": "select",
            "in": "query",
            "required": false,
            "description": format!("Comma-separated list of fields to include.{fields_desc}"),
            "schema": { "type": "string" },
        }));

        // Sort parameter.
        params.push(json!({
            "name": "sort",
            "in": "query",
            "required": false,
            "description": "Sort order. Prefix with - for descending. Example: -created_at,name",
            "schema": { "type": "string" },
        }));

        // Pagination parameters (relay vs offset).
        if query_def.relay {
            params.push(json!({
                "name": "first",
                "in": "query",
                "required": false,
                "description": "Number of items to return (forward pagination).",
                "schema": { "type": "integer", "minimum": 1 },
            }));
            params.push(json!({
                "name": "after",
                "in": "query",
                "required": false,
                "description": "Cursor for forward pagination.",
                "schema": { "type": "string" },
            }));
            params.push(json!({
                "name": "last",
                "in": "query",
                "required": false,
                "description": "Number of items to return (backward pagination).",
                "schema": { "type": "integer", "minimum": 1 },
            }));
            params.push(json!({
                "name": "before",
                "in": "query",
                "required": false,
                "description": "Cursor for backward pagination.",
                "schema": { "type": "string" },
            }));
        } else {
            params.push(json!({
                "name": "limit",
                "in": "query",
                "required": false,
                "description": format!(
                    "Maximum number of items to return. Default: {}, max: {}.",
                    self.config.default_page_size, self.config.max_page_size
                ),
                "schema": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": self.config.max_page_size,
                    "default": self.config.default_page_size,
                },
            }));
            params.push(json!({
                "name": "offset",
                "in": "query",
                "required": false,
                "description": "Number of items to skip.",
                "schema": { "type": "integer", "minimum": 0, "default": 0 },
            }));
        }

        // Filter parameters — document bracket operators per field.
        if query_def.auto_params.has_where {
            if let Some(td) = type_def {
                for field in &td.fields {
                    let desc = format!(
                        "Filter by {}. Bracket operators: {}",
                        field.name, BRACKET_OPERATORS_DESC
                    );
                    params.push(json!({
                        "name": format!("{}[operator]", field.name),
                        "in": "query",
                        "required": false,
                        "description": desc,
                        "schema": field_type_to_json_schema(&field.field_type),
                    }));
                }
            }

            // JSON filter escape hatch.
            params.push(json!({
                "name": "filter",
                "in": "query",
                "required": false,
                "description": "Full filter expression as JSON. Overrides bracket-style filters.",
                "schema": { "type": "string" },
            }));

            // Logical operators.
            for (op, desc) in &[
                (
                    "or",
                    "OR group: `or=(field[op]=val,field[op]=val)`. Conditions within are OR'd together.",
                ),
                (
                    "and",
                    "AND group: `and=(field[op]=val,field[op]=val)`. Explicit AND (equivalent to multiple filters).",
                ),
                ("not", "NOT group: `not=(field[op]=val)`. Negates the enclosed conditions."),
            ] {
                params.push(json!({
                    "name": op,
                    "in": "query",
                    "required": false,
                    "description": desc,
                    "schema": { "type": "string" },
                }));
            }
        }

        // Full-text search parameter (only when type has searchable fields).
        if let Some(td) = type_def {
            if !td.searchable_fields().is_empty() {
                let searchable_names: Vec<&str> =
                    td.searchable_fields().iter().map(|f| f.name.as_str()).collect();
                params.push(json!({
                    "name": "search",
                    "in": "query",
                    "required": false,
                    "description": format!(
                        "Full-text search query. Searches across: {}. \
                         Supports phrases (\"exact phrase\") and exclusions (-term). \
                         Results are ranked by relevance unless `sort` is specified.",
                        searchable_names.join(", ")
                    ),
                    "schema": { "type": "string" },
                }));
            }
        }
    }

    /// Detect the JSON Schema type for a resource's ID field.
    pub(super) fn detect_id_type(&self, resource: &RestResource) -> Value {
        let type_def = self.schema.find_type(&resource.type_name);
        if let Some(td) = type_def {
            if let Some(f) = td.fields.iter().find(|f| f.name.as_str() == "id") {
                return field_type_to_json_schema(&f.field_type);
            }
            if let Some(f) = td.fields.iter().find(|f| f.name.as_str().starts_with("pk_")) {
                return field_type_to_json_schema(&f.field_type);
            }
        }
        json!({ "type": "string" })
    }
}
