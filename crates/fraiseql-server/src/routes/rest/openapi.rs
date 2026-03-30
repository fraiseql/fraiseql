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

use fraiseql_core::schema::{
    Cardinality, CompiledSchema, DeleteResponse, FieldType, MutationDefinition, MutationOperation,
    QueryDefinition, RestConfig, TypeDefinition,
};
use serde_json::{Map, Value, json};

use super::resource::{HttpMethod, RestResource, RestRoute, RestRouteTable, RouteSource};

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
    schema:      &'a CompiledSchema,
    route_table: &'a RestRouteTable,
    config:      &'a RestConfig,
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

        // Add server entry with the base path.
        spec["servers"] = json!([{
            "url": self.config.path,
            "description": "REST API base path"
        }]);

        spec
    }

    // -- Info ----------------------------------------------------------------

    fn build_info(&self) -> Value {
        json!({
            "title": "FraiseQL REST API",
            "version": "1.0.0",
            "description": "Auto-generated REST API from compiled schema",
        })
    }

    // -- Paths ---------------------------------------------------------------

    fn build_paths(&self) -> Value {
        let mut paths = Map::new();

        // Add the openapi.json self-reference endpoint.
        let openapi_path = "/openapi.json";
        paths.insert(
            openapi_path.to_string(),
            json!({
                "get": {
                    "summary": "OpenAPI specification",
                    "description": "Returns this OpenAPI 3.0.3 specification as JSON.",
                    "tags": ["Meta"],
                    "responses": {
                        "200": {
                            "description": "OpenAPI specification",
                            "content": {
                                "application/json": {
                                    "schema": { "type": "object" }
                                }
                            }
                        }
                    }
                }
            }),
        );

        for resource in &self.route_table.resources {
            for route in &resource.routes {
                let path_key = &route.path;
                let method_key = method_to_string(route.method);

                let operation = self.build_operation(resource, route);

                // Ensure path object exists.
                let path_obj =
                    paths.entry(path_key.clone()).or_insert_with(|| Value::Object(Map::new()));

                if let Value::Object(ref mut map) = path_obj {
                    map.insert(method_key.to_string(), operation);
                }
            }

            // Add bulk operation endpoints (collection-level PATCH/DELETE).
            self.add_bulk_operations(&mut paths, resource);

            // Add SSE stream endpoint: /{resource}/stream
            self.add_stream_endpoint(&mut paths, resource);
        }

        Value::Object(paths)
    }

    fn build_operation(&self, resource: &RestResource, route: &RestRoute) -> Value {
        let mut op = Map::new();

        // Tags.
        op.insert("tags".to_string(), json!([capitalize(&resource.name)]));

        // Summary and operation ID.
        let (summary, operation_id) = self.operation_summary(resource, route);
        op.insert("summary".to_string(), json!(summary));
        op.insert("operationId".to_string(), json!(operation_id));

        // Deprecation.
        if self.is_deprecated(route) {
            op.insert("deprecated".to_string(), json!(true));
        }

        // Parameters.
        let params = self.build_parameters(resource, route);
        if !params.is_empty() {
            op.insert("parameters".to_string(), Value::Array(params));
        }

        // Request body (for POST/PUT/PATCH).
        if let Some(body) = self.build_request_body(resource, route) {
            op.insert("requestBody".to_string(), body);
        }

        // Responses.
        op.insert("responses".to_string(), self.build_responses(resource, route));

        // Security.
        if let Some(security) = self.build_security(route) {
            op.insert("security".to_string(), security);
        }

        Value::Object(op)
    }

    /// Add collection-level PATCH and DELETE operations for bulk update/delete.
    fn add_bulk_operations(&self, paths: &mut Map<String, Value>, resource: &RestResource) {
        let collection_path = format!("/{}", resource.name);
        let type_ref = format!("#/components/schemas/{}", resource.type_name);

        let has_update = resource.routes.iter().any(|r| {
            matches!(&r.source, RouteSource::Mutation { name }
                if self.schema.find_mutation(name)
                    .is_some_and(|m| matches!(m.operation,
                        MutationOperation::Update { .. })))
        });

        let has_delete = resource.routes.iter().any(|r| {
            matches!(&r.source, RouteSource::Mutation { name }
                if self.schema.find_mutation(name)
                    .is_some_and(|m| matches!(m.operation,
                        MutationOperation::Delete { .. })))
        });

        let path_obj = paths.entry(collection_path).or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(ref mut map) = path_obj else {
            return;
        };

        let bulk_prefer_params = json!([
            {
                "name": "Prefer",
                "in": "header",
                "required": false,
                "description": "Bulk operation preferences: return=representation, return=minimal, max-affected=N, tx=rollback.",
                "schema": { "type": "string" },
                "examples": {
                    "max-affected": {
                        "summary": "Limit affected rows",
                        "value": "max-affected=100"
                    },
                    "dry-run": {
                        "summary": "Preview changes without committing",
                        "value": "tx=rollback"
                    }
                }
            }
        ]);

        if has_update && !map.contains_key("patch") {
            let mut params = bulk_prefer_params.as_array().cloned().unwrap_or_default();
            params.push(json!({
                "name": "filter",
                "in": "query",
                "required": true,
                "description": "At least one filter parameter is required for bulk update. Use bracket operators (e.g., status[eq]=inactive) or JSON filter DSL.",
                "schema": { "type": "string" },
            }));

            map.insert("patch".to_string(), json!({
                "tags": [capitalize(&resource.name)],
                "summary": format!("Bulk update {}", resource.name),
                "operationId": format!("bulk_update_{}", resource.name),
                "description": format!(
                    "Update all {} matching the filter. CQRS: queries the read view for matching IDs, then calls the update mutation per row.",
                    resource.name
                ),
                "parameters": params,
                "requestBody": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "description": "Fields to update on each matching entity"
                            }
                        }
                    }
                },
                "responses": {
                    "200": {
                        "description": format!("Updated {}", resource.name),
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": type_ref }
                                }
                            }
                        },
                        "headers": {
                            "X-Rows-Affected": {
                                "description": "Number of rows affected",
                                "schema": { "type": "integer" }
                            }
                        }
                    },
                    "204": { "description": "No content (return=minimal)" },
                    "400": { "description": "Bad request (missing filter or max-affected exceeded)" }
                }
            }));
        }

        if has_delete && !map.contains_key("delete") {
            let mut params = bulk_prefer_params.as_array().cloned().unwrap_or_default();
            params.push(json!({
                "name": "filter",
                "in": "query",
                "required": true,
                "description": "At least one filter parameter is required for bulk delete. Use bracket operators (e.g., status[eq]=archived) or JSON filter DSL.",
                "schema": { "type": "string" },
            }));

            map.insert("delete".to_string(), json!({
                "tags": [capitalize(&resource.name)],
                "summary": format!("Bulk delete {}", resource.name),
                "operationId": format!("bulk_delete_{}", resource.name),
                "description": format!(
                    "Delete all {} matching the filter. CQRS: queries the read view for matching IDs, then calls the delete mutation per row.",
                    resource.name
                ),
                "parameters": params,
                "responses": {
                    "200": {
                        "description": format!("Deleted {}", resource.name),
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": type_ref }
                                }
                            }
                        },
                        "headers": {
                            "X-Rows-Affected": {
                                "description": "Number of rows affected",
                                "schema": { "type": "integer" }
                            }
                        }
                    },
                    "204": { "description": "No content (return=minimal or no matches)" },
                    "400": { "description": "Bad request (missing filter or max-affected exceeded)" }
                }
            }));
        }
    }

    /// Add an SSE stream endpoint for a resource: `/{resource}/stream`.
    fn add_stream_endpoint(&self, paths: &mut Map<String, Value>, resource: &RestResource) {
        let stream_path = format!("/{}/stream", resource.name);

        paths.insert(
            stream_path,
            json!({
                "get": {
                    "tags": [capitalize(&resource.name)],
                    "summary": format!("Stream {} changes (SSE)", resource.name),
                    "operationId": format!("stream_{}", resource.name),
                    "description": format!(
                        "Subscribe to real-time changes on {} via Server-Sent Events. \
                         Requires the `observers` feature. Events: `insert`, `update`, `delete`, `ping` (heartbeat).",
                        resource.name
                    ),
                    "parameters": [
                        {
                            "name": "Accept",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "enum": ["text/event-stream"] },
                            "description": "Must be text/event-stream for SSE."
                        },
                        {
                            "name": "Last-Event-ID",
                            "in": "header",
                            "required": false,
                            "schema": { "type": "string" },
                            "description": "Resume from a specific event ID on reconnection."
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "SSE event stream",
                            "content": {
                                "text/event-stream": {
                                    "schema": { "type": "string" }
                                }
                            }
                        },
                        "501": {
                            "description": "Not Implemented (observers feature disabled)"
                        }
                    }
                }
            }),
        );
    }

    fn operation_summary(&self, resource: &RestResource, route: &RestRoute) -> (String, String) {
        let res_name = &resource.name;
        let type_name = &resource.type_name;

        match (&route.source, route.method) {
            (RouteSource::Query { name }, HttpMethod::Get) => {
                // Find the query to check if it's a list or single.
                let is_list = self
                    .schema
                    .queries
                    .iter()
                    .find(|q| q.name == *name)
                    .is_some_and(|q| q.returns_list);

                if is_list {
                    (format!("List {res_name}"), format!("list_{res_name}"))
                } else {
                    (format!("Get {type_name} by ID"), format!("get_{}", to_snake(type_name)))
                }
            },
            (RouteSource::Mutation { name }, HttpMethod::Post) => {
                let mutation = self.schema.mutations.iter().find(|m| m.name == *name);
                if let Some(MutationOperation::Insert { .. }) = mutation.map(|m| &m.operation) {
                    (format!("Create {type_name}"), format!("create_{}", to_snake(type_name)))
                } else {
                    // Custom action.
                    let action = extract_action(name, type_name);
                    (format!("{} {type_name}", capitalize(&action)), name.clone())
                }
            },
            (RouteSource::Mutation { name: _ }, HttpMethod::Put) => {
                (format!("Replace {type_name}"), format!("replace_{}", to_snake(type_name)))
            },
            (RouteSource::Mutation { name }, HttpMethod::Patch) => {
                // Check if it's a partial action or full patch.
                if route.path.contains('/') && route.path.matches('/').count() > 1 {
                    let action = extract_action(name, type_name);
                    (format!("{} {type_name}", capitalize(&action)), name.clone())
                } else {
                    (format!("Update {type_name}"), format!("update_{}", to_snake(type_name)))
                }
            },
            (RouteSource::Mutation { .. }, HttpMethod::Delete) => {
                (format!("Delete {type_name}"), format!("delete_{}", to_snake(type_name)))
            },
            _ => ("Operation".to_string(), "operation".to_string()),
        }
    }

    fn is_deprecated(&self, route: &RestRoute) -> bool {
        match &route.source {
            RouteSource::Query { name } => self
                .schema
                .queries
                .iter()
                .find(|q| q.name == *name)
                .is_some_and(|q| q.deprecation.is_some()),
            RouteSource::Mutation { name } => self
                .schema
                .mutations
                .iter()
                .find(|m| m.name == *name)
                .is_some_and(|m| m.deprecation.is_some()),
        }
    }

    // -- Parameters ----------------------------------------------------------

    fn build_parameters(&self, resource: &RestResource, route: &RestRoute) -> Vec<Value> {
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

        // Cache-Control response header documentation (via response headers).
        // Not a request parameter — documented via spec description.

        params
    }

    fn add_collection_params(
        &self,
        params: &mut Vec<Value>,
        query_def: &QueryDefinition,
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

    fn detect_id_type(&self, resource: &RestResource) -> Value {
        let type_def = self.schema.find_type(&resource.type_name);
        if let Some(td) = type_def {
            // Look for `id` field first.
            if let Some(f) = td.fields.iter().find(|f| f.name.as_str() == "id") {
                return field_type_to_json_schema(&f.field_type);
            }
            // Fall back to pk_* field.
            if let Some(f) = td.fields.iter().find(|f| f.name.as_str().starts_with("pk_")) {
                return field_type_to_json_schema(&f.field_type);
            }
        }
        json!({ "type": "string" })
    }

    // -- Request body --------------------------------------------------------

    fn build_request_body(&self, resource: &RestResource, route: &RestRoute) -> Option<Value> {
        match route.method {
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {},
            _ => return None,
        }

        let RouteSource::Mutation { name } = &route.source else {
            return None;
        };

        let mutation = self.schema.mutations.iter().find(|m| m.name == *name)?;
        let type_def = self.schema.find_type(&resource.type_name);

        let schema = match route.method {
            HttpMethod::Post => {
                // Insert: single object or array for bulk insert.
                let single = self.mutation_args_schema(mutation);
                json!({
                    "oneOf": [
                        single,
                        {
                            "type": "array",
                            "items": single,
                            "description": "Array body triggers bulk insert mode"
                        }
                    ]
                })
            },
            HttpMethod::Put => {
                // Full update: all writable fields required.
                if let Some(td) = type_def {
                    self.writable_fields_schema(td, true)
                } else {
                    self.mutation_args_schema(mutation)
                }
            },
            HttpMethod::Patch => {
                // Partial update: writable fields, none required.
                if let Some(td) = type_def {
                    self.writable_fields_schema(td, false)
                } else {
                    self.mutation_args_schema(mutation)
                }
            },
            _ => return None,
        };

        Some(json!({
            "required": true,
            "content": {
                "application/json": {
                    "schema": schema,
                }
            }
        }))
    }

    fn mutation_args_schema(&self, mutation: &MutationDefinition) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();

        for arg in &mutation.arguments {
            // Skip the ID argument.
            if arg.name == "id" || arg.name.starts_with("pk_") {
                continue;
            }
            properties.insert(arg.name.clone(), field_type_to_json_schema(&arg.arg_type));
            if !arg.nullable {
                required.push(json!(arg.name));
            }
        }

        let mut schema = json!({
            "type": "object",
            "properties": properties,
        });
        if !required.is_empty() {
            schema["required"] = Value::Array(required);
        }
        schema
    }

    fn writable_fields_schema(&self, type_def: &TypeDefinition, all_required: bool) -> Value {
        let writable = type_def.writable_fields();
        let mut properties = Map::new();
        let mut required = Vec::new();

        for field in &writable {
            properties.insert(field.name.to_string(), field_type_to_json_schema(&field.field_type));
            if all_required && !field.nullable {
                required.push(json!(field.name.to_string()));
            }
        }

        let mut schema = json!({
            "type": "object",
            "properties": properties,
        });
        if !required.is_empty() {
            schema["required"] = Value::Array(required);
        }
        schema
    }

    // -- Responses -----------------------------------------------------------

    #[allow(clippy::too_many_lines)] // Reason: response building requires handling each HTTP method
    fn build_responses(&self, resource: &RestResource, route: &RestRoute) -> Value {
        let mut responses = Map::new();
        let type_ref = format!("#/components/schemas/{}", resource.type_name);

        match route.method {
            HttpMethod::Get => {
                let is_list = matches!(&route.source, RouteSource::Query { name }
                    if self.schema.queries.iter()
                        .find(|q| q.name == *name)
                        .is_some_and(|q| q.returns_list));

                if is_list {
                    responses.insert(
                        "200".to_string(),
                        json!({
                            "description": format!("List of {}", resource.name),
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "data": {
                                                "type": "array",
                                                "items": { "$ref": type_ref }
                                            },
                                            "meta": {
                                                "type": "object",
                                                "properties": {
                                                    "total": { "type": "integer" },
                                                    "limit": { "type": "integer" },
                                                    "offset": { "type": "integer" }
                                                }
                                            },
                                            "links": {
                                                "type": "object",
                                                "properties": {
                                                    "self": { "type": "string" },
                                                    "next": { "type": "string" },
                                                    "prev": { "type": "string" }
                                                }
                                            }
                                        }
                                    }
                                },
                                "application/x-ndjson": {
                                    "description": "Newline-delimited JSON stream (one object per line, no envelope)",
                                    "schema": { "$ref": type_ref }
                                }
                            }
                        }),
                    );
                } else {
                    responses.insert(
                        "200".to_string(),
                        json!({
                            "description": resource.type_name,
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": type_ref }
                                }
                            }
                        }),
                    );
                    responses.insert(
                        "304".to_string(),
                        json!({ "description": "Not Modified (ETag match)" }),
                    );
                    responses.insert("404".to_string(), json!({ "description": "Not found" }));
                }
            },
            HttpMethod::Post => {
                let status = route.success_status.to_string();
                responses.insert(
                    status,
                    json!({
                        "description": format!("{} created/executed", resource.type_name),
                        "content": {
                            "application/json": {
                                "schema": { "$ref": type_ref }
                            }
                        },
                        "headers": {
                            "Location": {
                                "description": "URL of the created resource",
                                "schema": { "type": "string" }
                            }
                        }
                    }),
                );
            },
            HttpMethod::Put | HttpMethod::Patch => {
                responses.insert(
                    "200".to_string(),
                    json!({
                        "description": format!("Updated {}", resource.type_name),
                        "content": {
                            "application/json": {
                                "schema": { "$ref": type_ref }
                            }
                        }
                    }),
                );
                if route.method == HttpMethod::Put {
                    responses.insert(
                        "422".to_string(),
                        json!({ "description": "Unprocessable entity (missing required fields)" }),
                    );
                }
                responses.insert("404".to_string(), json!({ "description": "Not found" }));
            },
            HttpMethod::Delete => {
                match self.config.delete_response {
                    DeleteResponse::NoContent => {
                        responses.insert(
                            "204".to_string(),
                            json!({ "description": "Deleted (no content)" }),
                        );
                    },
                    DeleteResponse::Entity | _ => {
                        responses.insert(
                            "200".to_string(),
                            json!({
                                "description": format!("Deleted {}", resource.type_name),
                                "content": {
                                    "application/json": {
                                        "schema": { "$ref": type_ref }
                                    }
                                }
                            }),
                        );
                    },
                }
                responses.insert("404".to_string(), json!({ "description": "Not found" }));
            },
        }

        // Common error responses.
        if !responses.contains_key("400") {
            responses.insert("400".to_string(), json!({ "description": "Bad request" }));
        }

        if self.config.require_auth {
            responses.insert("401".to_string(), json!({ "description": "Unauthorized" }));
            responses.insert("403".to_string(), json!({ "description": "Forbidden" }));
        }

        Value::Object(responses)
    }

    // -- Security ------------------------------------------------------------

    fn build_security(&self, _route: &RestRoute) -> Option<Value> {
        if !self.config.require_auth {
            return None;
        }

        Some(json!([{ "BearerAuth": [] }]))
    }

    // -- Components ----------------------------------------------------------

    fn build_components(&self) -> Value {
        let mut schemas = Map::new();

        // Build type schemas for all types referenced by routes.
        let referenced_types: Vec<&str> =
            self.route_table.resources.iter().map(|r| r.type_name.as_str()).collect();

        for type_name in &referenced_types {
            if let Some(td) = self.schema.find_type(type_name) {
                schemas.insert((*type_name).to_string(), self.type_to_schema(td));
            }
        }

        // Walk nested object references.
        let mut to_process: Vec<String> = Vec::new();
        for td in referenced_types.iter().filter_map(|tn| self.schema.find_type(tn)) {
            for field in &td.fields {
                if let FieldType::Object(ref name) = field.field_type {
                    if !schemas.contains_key(name.as_str()) {
                        to_process.push(name.clone());
                    }
                }
            }
        }

        while let Some(name) = to_process.pop() {
            if schemas.contains_key(&name) {
                continue;
            }
            if let Some(td) = self.schema.find_type(&name) {
                schemas.insert(name.clone(), self.type_to_schema(td));
                for field in &td.fields {
                    if let FieldType::Object(ref nested) = field.field_type {
                        if !schemas.contains_key(nested.as_str()) {
                            to_process.push(nested.clone());
                        }
                    }
                }
            }
        }

        // Enum schemas.
        for enum_def in &self.schema.enums {
            let values: Vec<Value> = enum_def.values.iter().map(|v| json!(v.name)).collect();
            schemas.insert(
                enum_def.name.clone(),
                json!({
                    "type": "string",
                    "enum": values,
                }),
            );
        }

        // Error schema.
        schemas.insert(
            "Error".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "error": {
                        "type": "object",
                        "properties": {
                            "code": { "type": "string" },
                            "message": { "type": "string" },
                            "details": {
                                "type": "array",
                                "items": { "type": "object" }
                            }
                        },
                        "required": ["code", "message"]
                    }
                },
                "required": ["error"]
            }),
        );

        let mut components = json!({
            "schemas": schemas,
        });

        // Security schemes.
        if self.config.require_auth {
            components["securitySchemes"] = json!({
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT",
                    "description": "JWT authentication token"
                }
            });
        }

        components
    }

    fn type_to_schema(&self, type_def: &TypeDefinition) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();

        for field in &type_def.fields {
            let mut field_schema = field_type_to_json_schema(&field.field_type);

            if let Some(ref desc) = field.description {
                if let Value::Object(ref mut map) = field_schema {
                    map.insert("description".to_string(), json!(desc));
                }
            }

            if field.deprecation.is_some() {
                if let Value::Object(ref mut map) = field_schema {
                    map.insert("deprecated".to_string(), json!(true));
                }
            }

            properties.insert(field.name.to_string(), field_schema);

            if !field.nullable {
                required.push(json!(field.name.to_string()));
            }
        }

        // Add relationship properties for embedded resources.
        for rel in &type_def.relationships {
            let ref_schema = json!({ "$ref": format!("#/components/schemas/{}", rel.target_type) });
            let rel_schema = match rel.cardinality {
                Cardinality::OneToMany => {
                    json!({
                        "type": "array",
                        "items": ref_schema,
                        "description": format!("Embedded {} (use ?select={}(fields) to include)", rel.target_type, rel.name),
                    })
                },
                Cardinality::ManyToOne | Cardinality::OneToOne => {
                    let mut s = ref_schema;
                    if let Some(obj) = s.as_object_mut() {
                        obj.insert(
                            "description".to_string(),
                            json!(format!(
                                "Embedded {} (use ?select={}(fields) to include)",
                                rel.target_type, rel.name
                            )),
                        );
                        obj.insert("nullable".to_string(), json!(true));
                    }
                    s
                },
                _ => ref_schema,
            };
            properties.insert(rel.name.clone(), rel_schema);
        }

        let mut schema = json!({
            "type": "object",
            "properties": properties,
        });
        if !required.is_empty() {
            schema["required"] = Value::Array(required);
        }
        schema
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Bracket operators documented in filter parameter descriptions.
const BRACKET_OPERATORS_DESC: &str = "eq, ne, gt, gte, lt, lte, in, nin, like, ilike, is_null, contains, icontains, startswith, endswith";

/// Map a `FieldType` to a JSON Schema type object.
fn field_type_to_json_schema(ft: &FieldType) -> Value {
    match ft {
        FieldType::Int => json!({ "type": "integer" }),
        FieldType::Float => json!({ "type": "number" }),
        FieldType::Boolean => json!({ "type": "boolean" }),
        FieldType::Id | FieldType::Uuid => json!({ "type": "string", "format": "uuid" }),
        FieldType::DateTime => json!({ "type": "string", "format": "date-time" }),
        FieldType::Date => json!({ "type": "string", "format": "date" }),
        FieldType::Time => json!({ "type": "string", "format": "time" }),
        FieldType::Json => json!({ "type": "object" }),
        FieldType::Decimal => json!({ "type": "string", "format": "decimal" }),
        FieldType::Vector => json!({ "type": "array", "items": { "type": "number" } }),
        FieldType::Scalar(name) => scalar_to_json_schema(name),
        FieldType::List(inner) => {
            json!({ "type": "array", "items": field_type_to_json_schema(inner) })
        },
        FieldType::Object(name) | FieldType::Enum(name) | FieldType::Input(name) => {
            json!({ "$ref": format!("#/components/schemas/{name}") })
        },
        FieldType::Interface(name) | FieldType::Union(name) => {
            json!({ "type": "object", "description": format!("See {name}") })
        },
        // Reason: FieldType is #[non_exhaustive]; default to string for unknown variants.
        _ => json!({ "type": "string" }),
    }
}

/// Map well-known scalar names to JSON Schema.
fn scalar_to_json_schema(name: &str) -> Value {
    match name {
        "Email" => json!({ "type": "string", "format": "email" }),
        "URL" | "Uri" => json!({ "type": "string", "format": "uri" }),
        "PhoneNumber" => json!({ "type": "string", "format": "phone" }),
        _ => json!({ "type": "string" }),
    }
}

const fn method_to_string(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
    }
}

fn should_have_prefer_header(route: &RestRoute) -> bool {
    match route.method {
        HttpMethod::Get => {
            // Collection GET endpoints (no path parameter).
            !route.path.contains('{')
        },
        HttpMethod::Post | HttpMethod::Patch | HttpMethod::Delete => true,
        HttpMethod::Put => false,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn to_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Extract an action name from a mutation name by stripping the type prefix.
///
/// Example: `archiveUser` on type `User` → `archive`
fn extract_action(mutation_name: &str, type_name: &str) -> String {
    // Try stripping type name suffix (e.g., `archiveUser` → `archive`).
    let lower_type = type_name.to_lowercase();
    let lower_name = mutation_name.to_lowercase();

    if let Some(prefix) = lower_name.strip_suffix(&lower_type) {
        if !prefix.is_empty() {
            return prefix.trim_end_matches('_').replace('_', "-");
        }
    }

    // Try stripping type name prefix (e.g., `userArchive` → `archive`).
    if let Some(suffix) = lower_name.strip_prefix(&lower_type) {
        let trimmed = suffix.trim_start_matches('_');
        if !trimmed.is_empty() {
            return trimmed.replace('_', "-");
        }
    }

    // Fallback: use the full mutation name kebab-cased.
    to_snake(mutation_name).replace('_', "-")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use fraiseql_core::schema::{
        DeprecationInfo, FieldType, MutationDefinition, MutationOperation, RestConfig,
    };
    use fraiseql_test_utils::schema_builder::{
        TestFieldBuilder, TestSchemaBuilder, TestTypeBuilder,
    };

    use super::*;

    // -- Helpers -------------------------------------------------------------

    fn mutation(name: &str, op: MutationOperation) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, "User");
        m.operation = op;
        m.sql_source = Some(format!("fn_{name}"));
        // Add `id` argument for single-resource mutations.
        if name != "create_user" {
            m.arguments
                .push(fraiseql_core::schema::ArgumentDefinition::new("id", FieldType::Int));
        }
        // Add writable field arguments for update mutations.
        if name.starts_with("update") {
            m.arguments
                .push(fraiseql_core::schema::ArgumentDefinition::new("name", FieldType::String));
            m.arguments
                .push(fraiseql_core::schema::ArgumentDefinition::new("email", FieldType::String));
        }
        m
    }

    fn rest_schema() -> CompiledSchema {
        let table = "users".to_string();
        let mut users_query = fraiseql_core::schema::QueryDefinition::new("users", "User");
        users_query.returns_list = true;
        users_query.auto_params = fraiseql_core::schema::AutoParams::all();
        users_query.sql_source = Some("v_user".to_string());

        let mut schema = TestSchemaBuilder::new()
            .with_query(users_query)
            .with_simple_query("user", "User", false)
            .with_mutation(mutation(
                "create_user",
                MutationOperation::Insert {
                    table: table.clone(),
                },
            ))
            .with_mutation(mutation(
                "update_user",
                MutationOperation::Update {
                    table: table.clone(),
                },
            ))
            .with_mutation(mutation("delete_user", MutationOperation::Delete { table }))
            .with_type(
                TestTypeBuilder::new("User", "v_user")
                    .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                    .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                    .with_field(TestFieldBuilder::nullable("email", FieldType::String).build())
                    .build(),
            )
            .build();

        schema.rest_config = Some(RestConfig {
            enabled:      true,
            require_auth: true,
            ..RestConfig::default()
        });

        schema
    }

    fn generate(schema: &CompiledSchema) -> Value {
        let route_table = RestRouteTable::from_compiled_schema(schema).unwrap();
        generate_openapi(schema, &route_table).unwrap()
    }

    // -- Structural tests ----------------------------------------------------

    #[test]
    fn spec_is_valid_openapi_303() {
        let spec = generate(&rest_schema());
        assert_eq!(spec["openapi"], "3.0.3");
    }

    #[test]
    fn spec_has_info_title_and_version() {
        let spec = generate(&rest_schema());
        assert!(spec["info"]["title"].is_string());
        assert!(spec["info"]["version"].is_string());
    }

    #[test]
    fn spec_has_paths_and_components() {
        let spec = generate(&rest_schema());
        assert!(spec["paths"].is_object());
        assert!(spec["components"].is_object());
        assert!(spec["components"]["schemas"].is_object());
    }

    #[test]
    fn spec_has_server_entry() {
        let spec = generate(&rest_schema());
        assert!(spec["servers"].is_array());
        assert_eq!(spec["servers"][0]["url"], "/rest/v1");
    }

    // -- Type schemas --------------------------------------------------------

    #[test]
    fn type_definition_produces_component_schema() {
        let spec = generate(&rest_schema());
        let user_schema = &spec["components"]["schemas"]["User"];
        assert_eq!(user_schema["type"], "object");
        assert!(user_schema["properties"]["name"].is_object());
        assert!(user_schema["properties"]["email"].is_object());
    }

    #[test]
    fn scalar_fields_map_to_json_schema_types() {
        let spec = generate(&rest_schema());
        let props = &spec["components"]["schemas"]["User"]["properties"];
        assert_eq!(props["name"]["type"], "string");
        assert_eq!(props["pk_user_id"]["type"], "integer");
    }

    #[test]
    fn nested_object_produces_ref() {
        let mut schema = rest_schema();
        schema.types.push(
            TestTypeBuilder::new("Address", "v_address")
                .with_field(TestFieldBuilder::new("city", FieldType::String).build())
                .build(),
        );
        // Add an Object field referencing Address.
        for td in &mut schema.types {
            if td.name == "User" {
                td.fields.push(
                    TestFieldBuilder::new("address", FieldType::Object("Address".to_string()))
                        .build(),
                );
            }
        }

        let spec = generate(&schema);
        let addr_prop = &spec["components"]["schemas"]["User"]["properties"]["address"];
        assert_eq!(addr_prop["$ref"], "#/components/schemas/Address");
        // Address schema should exist.
        assert!(spec["components"]["schemas"]["Address"].is_object());
    }

    #[test]
    fn enum_field_produces_ref() {
        let mut schema = rest_schema();
        schema.enums.push(fraiseql_core::schema::EnumDefinition {
            name:        "Status".to_string(),
            values:      vec![
                fraiseql_core::schema::EnumValueDefinition {
                    name:        "ACTIVE".to_string(),
                    description: None,
                    deprecation: None,
                },
                fraiseql_core::schema::EnumValueDefinition {
                    name:        "INACTIVE".to_string(),
                    description: None,
                    deprecation: None,
                },
            ],
            description: None,
        });
        for td in &mut schema.types {
            if td.name == "User" {
                td.fields.push(
                    TestFieldBuilder::new("status", FieldType::Enum("Status".to_string())).build(),
                );
            }
        }

        let spec = generate(&schema);
        let status_prop = &spec["components"]["schemas"]["User"]["properties"]["status"];
        assert_eq!(status_prop["$ref"], "#/components/schemas/Status");
        let enum_schema = &spec["components"]["schemas"]["Status"];
        assert_eq!(enum_schema["type"], "string");
        let enum_vals = enum_schema["enum"].as_array().unwrap();
        assert_eq!(enum_vals.len(), 2);
    }

    // -- Query paths ---------------------------------------------------------

    #[test]
    fn list_query_produces_get_collection_path() {
        let spec = generate(&rest_schema());
        let paths = spec["paths"].as_object().unwrap();
        // Should have a /users path with GET.
        let users_path = paths.keys().find(|k| *k == "/users");
        assert!(users_path.is_some(), "Expected /users path");
        assert!(paths["/users"]["get"].is_object());
    }

    #[test]
    fn single_query_produces_get_by_id_path() {
        let spec = generate(&rest_schema());
        let paths = spec["paths"].as_object().unwrap();
        // The test schema uses pk_user_id, so the path is /users/{pk_user_id}.
        let user_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users"));
        assert!(
            user_path.is_some(),
            "Expected /users/{{pk_user_id}} path, found: {:?}",
            paths.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn collection_get_has_pagination_params() {
        let spec = generate(&rest_schema());
        let params = &spec["paths"]["/users"]["get"]["parameters"];
        let param_names: Vec<&str> =
            params.as_array().unwrap().iter().filter_map(|p| p["name"].as_str()).collect();
        assert!(param_names.contains(&"limit"));
        assert!(param_names.contains(&"offset"));
        assert!(param_names.contains(&"select"));
        assert!(param_names.contains(&"sort"));
    }

    #[test]
    fn relay_query_has_cursor_params() {
        let mut schema = rest_schema();
        for q in &mut schema.queries {
            if q.name == "users" {
                q.relay = true;
            }
        }

        let spec = generate(&schema);
        let params = &spec["paths"]["/users"]["get"]["parameters"];
        let param_names: Vec<&str> =
            params.as_array().unwrap().iter().filter_map(|p| p["name"].as_str()).collect();
        assert!(param_names.contains(&"first"));
        assert!(param_names.contains(&"after"));
        assert!(param_names.contains(&"last"));
        assert!(param_names.contains(&"before"));
        assert!(!param_names.contains(&"limit"));
    }

    // -- Mutation paths -------------------------------------------------------

    #[test]
    fn insert_mutation_produces_post_path() {
        let spec = generate(&rest_schema());
        assert!(spec["paths"]["/users"]["post"].is_object());
    }

    #[test]
    fn update_mutation_produces_put_and_patch() {
        let spec = generate(&rest_schema());
        let paths = spec["paths"].as_object().unwrap();
        // Look for PUT and PATCH on user ID path.
        let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
        assert!(paths[id_path]["put"].is_object() || paths[id_path]["patch"].is_object());
    }

    #[test]
    fn delete_mutation_produces_delete_path() {
        let spec = generate(&rest_schema());
        let paths = spec["paths"].as_object().unwrap();
        let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
        assert!(paths[id_path]["delete"].is_object());
    }

    #[test]
    fn post_has_request_body() {
        let spec = generate(&rest_schema());
        let post_op = &spec["paths"]["/users"]["post"];
        assert!(post_op["requestBody"].is_object());
        assert!(post_op["requestBody"]["content"]["application/json"].is_object());
    }

    #[test]
    fn put_has_422_response() {
        let spec = generate(&rest_schema());
        let paths = spec["paths"].as_object().unwrap();
        let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
        if let Some(put_op) = paths[id_path].get("put") {
            assert!(put_op["responses"]["422"].is_object());
        }
    }

    #[test]
    fn custom_mutation_produces_post_action() {
        let mut schema = rest_schema();
        schema.mutations.push({
            let mut m = MutationDefinition::new("archiveUser", "User");
            m.operation = MutationOperation::Custom;
            m.sql_source = Some("fn_archive_user".to_string());
            m
        });

        let spec = generate(&schema);
        let paths = spec["paths"].as_object().unwrap();
        let action_path = paths
            .keys()
            .find(|k| k.contains("archive"))
            .expect("Expected an archive action path");
        assert!(paths[action_path]["post"].is_object());
    }

    // -- Deprecated operations -----------------------------------------------

    #[test]
    fn deprecated_operation_has_deprecated_flag() {
        let mut schema = rest_schema();
        for q in &mut schema.queries {
            if q.name == "user" {
                q.deprecation = Some(DeprecationInfo {
                    reason: Some("Use v2".to_string()),
                });
            }
        }

        let spec = generate(&schema);
        let paths = spec["paths"].as_object().unwrap();
        let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
        let get_op = &paths[id_path]["get"];
        assert_eq!(get_op["deprecated"], true);
    }

    // -- Auth ----------------------------------------------------------------

    #[test]
    fn auth_required_produces_security_schemes() {
        let schema = rest_schema();
        let spec = generate(&schema);
        assert!(spec["components"]["securitySchemes"]["BearerAuth"].is_object());
    }

    #[test]
    fn auth_required_adds_401_403_to_responses() {
        let schema = rest_schema();
        let spec = generate(&schema);
        let get_op = &spec["paths"]["/users"]["get"];
        assert!(get_op["responses"]["401"].is_object());
        assert!(get_op["responses"]["403"].is_object());
    }

    #[test]
    fn no_auth_omits_security_schemes() {
        let mut schema = rest_schema();
        schema.rest_config = Some(RestConfig {
            enabled: true,
            require_auth: false,
            ..RestConfig::default()
        });

        let spec = generate(&schema);
        assert!(spec["components"]["securitySchemes"].is_null());
    }

    // -- Prefer header -------------------------------------------------------

    #[test]
    fn collection_get_has_prefer_header() {
        let spec = generate(&rest_schema());
        let params = &spec["paths"]["/users"]["get"]["parameters"];
        let has_prefer = params.as_array().unwrap().iter().any(|p| p["name"] == "Prefer");
        assert!(has_prefer);
    }

    #[test]
    fn delete_has_prefer_header() {
        let spec = generate(&rest_schema());
        let paths = spec["paths"].as_object().unwrap();
        let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
        if let Some(delete_op) = paths[id_path].get("delete") {
            let has_prefer = delete_op["parameters"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|p| p["name"] == "Prefer"));
            assert!(has_prefer);
        }
    }

    // -- Bracket operators ---------------------------------------------------

    #[test]
    fn filter_params_document_bracket_operators() {
        let spec = generate(&rest_schema());
        let params = spec["paths"]["/users"]["get"]["parameters"].as_array().unwrap();
        let filter_param = params
            .iter()
            .find(|p| p["name"].as_str().is_some_and(|n| n.contains("[operator]")));
        assert!(filter_param.is_some(), "Expected bracket operator param");
        let desc = filter_param.unwrap()["description"].as_str().unwrap();
        assert!(desc.contains("eq"));
        assert!(desc.contains("like"));
    }

    // -- OpenAPI self-reference endpoint -------------------------------------

    #[test]
    fn openapi_json_endpoint_present() {
        let spec = generate(&rest_schema());
        assert!(spec["paths"]["/openapi.json"]["get"].is_object());
    }

    // -- Delete response modes -----------------------------------------------

    #[test]
    fn delete_no_content_mode() {
        let spec = generate(&rest_schema());
        let paths = spec["paths"].as_object().unwrap();
        let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
        if let Some(delete_op) = paths[id_path].get("delete") {
            assert!(delete_op["responses"]["204"].is_object());
        }
    }

    #[test]
    fn delete_entity_mode() {
        let mut schema = rest_schema();
        schema.rest_config = Some(RestConfig {
            enabled: true,
            delete_response: DeleteResponse::Entity,
            ..RestConfig::default()
        });

        let spec = generate(&schema);
        let paths = spec["paths"].as_object().unwrap();
        let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
        if let Some(delete_op) = paths[id_path].get("delete") {
            assert!(delete_op["responses"]["200"].is_object());
        }
    }

    // -- Error schema --------------------------------------------------------

    #[test]
    fn error_schema_present() {
        let spec = generate(&rest_schema());
        let error_schema = &spec["components"]["schemas"]["Error"];
        assert_eq!(error_schema["type"], "object");
        assert!(error_schema["properties"]["error"].is_object());
    }

    // -- Edge cases ----------------------------------------------------------

    #[test]
    fn missing_rest_config_returns_error() {
        let schema = TestSchemaBuilder::new().build();
        let route_table = RestRouteTable {
            base_path:   "/rest/v1".to_string(),
            resources:   vec![],
            diagnostics: vec![],
        };
        let result = generate_openapi(&schema, &route_table);
        assert!(result.is_err());
    }

    #[test]
    fn empty_route_table_produces_minimal_spec() {
        let mut schema = TestSchemaBuilder::new().build();
        schema.rest_config = Some(RestConfig {
            enabled: true,
            ..RestConfig::default()
        });
        let route_table = RestRouteTable {
            base_path:   "/rest/v1".to_string(),
            resources:   vec![],
            diagnostics: vec![],
        };
        let spec = generate_openapi(&schema, &route_table).unwrap();
        assert_eq!(spec["openapi"], "3.0.3");
        // Only the openapi.json self-reference endpoint.
        let paths = spec["paths"].as_object().unwrap();
        assert_eq!(paths.len(), 1);
    }

    // -- Bulk operations ----------------------------------------------------

    #[test]
    fn bulk_update_produces_collection_patch() {
        let spec = generate(&rest_schema());
        let patch_op = &spec["paths"]["/users"]["patch"];
        assert!(patch_op.is_object(), "Expected PATCH on /users");
        assert_eq!(patch_op["operationId"], "bulk_update_users");
        assert!(patch_op["responses"]["200"].is_object());
        assert!(patch_op["responses"]["400"].is_object());
    }

    #[test]
    fn bulk_delete_produces_collection_delete() {
        let spec = generate(&rest_schema());
        let delete_op = &spec["paths"]["/users"]["delete"];
        assert!(delete_op.is_object(), "Expected DELETE on /users");
        assert_eq!(delete_op["operationId"], "bulk_delete_users");
        assert!(delete_op["responses"]["200"].is_object());
        assert!(delete_op["responses"]["400"].is_object());
    }

    #[test]
    fn post_body_supports_array_for_bulk_insert() {
        let spec = generate(&rest_schema());
        let post_body = &spec["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"];
        // Should have oneOf with single object and array variant.
        assert!(post_body["oneOf"].is_array(), "Expected oneOf schema for bulk insert support");
        let variants = post_body["oneOf"].as_array().unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[1]["type"], "array");
    }

    #[test]
    fn post_has_prefer_header_for_upsert() {
        let spec = generate(&rest_schema());
        let params = &spec["paths"]["/users"]["post"]["parameters"];
        let has_prefer = params.as_array().unwrap().iter().any(|p| p["name"] == "Prefer");
        assert!(has_prefer, "POST should have Prefer header for upsert/bulk preferences");
    }

    #[test]
    fn info_has_default_title() {
        let spec = generate(&rest_schema());
        assert_eq!(spec["info"]["title"], "FraiseQL REST API");
    }

    #[test]
    fn info_has_default_version() {
        let spec = generate(&rest_schema());
        assert_eq!(spec["info"]["version"], "1.0.0");
    }

    // -- Logical operators ---------------------------------------------------

    #[test]
    fn collection_get_has_logical_operator_params() {
        let spec = generate(&rest_schema());
        let params = spec["paths"]["/users"]["get"]["parameters"].as_array().unwrap();
        let param_names: Vec<&str> = params.iter().filter_map(|p| p["name"].as_str()).collect();
        assert!(param_names.contains(&"or"), "Expected `or` logical param");
        assert!(param_names.contains(&"and"), "Expected `and` logical param");
        assert!(param_names.contains(&"not"), "Expected `not` logical param");
    }

    // -- Full-text search ----------------------------------------------------

    #[test]
    fn fts_enabled_resource_has_search_param() {
        let schema = rest_schema();
        // All String fields are searchable by default (via searchable_fields()).

        let spec = generate(&schema);
        let params = spec["paths"]["/users"]["get"]["parameters"].as_array().unwrap();
        let search_param = params.iter().find(|p| p["name"] == "search");
        assert!(search_param.is_some(), "Expected `search` param on FTS-enabled resource");
        let desc = search_param.unwrap()["description"].as_str().unwrap();
        assert!(desc.contains("name"), "Expected field name in search description: {desc}");
    }

    #[test]
    fn non_fts_resource_has_no_search_param() {
        // Build a schema whose type has no String fields so searchable_fields() is empty.
        let mut users_query = fraiseql_core::schema::QueryDefinition::new("counters", "Counter");
        users_query.returns_list = true;
        users_query.auto_params = fraiseql_core::schema::AutoParams::all();
        users_query.sql_source = Some("v_counter".to_string());

        let mut schema = TestSchemaBuilder::new()
            .with_query(users_query)
            .with_type(
                TestTypeBuilder::new("Counter", "v_counter")
                    .with_field(TestFieldBuilder::new("pk_id", FieldType::Int).build())
                    .with_field(TestFieldBuilder::new("value", FieldType::Int).build())
                    .build(),
            )
            .build();
        schema.rest_config = Some(RestConfig::default());

        let spec = generate(&schema);
        let params = spec["paths"]["/counters"]["get"]["parameters"].as_array().unwrap();
        let search_param = params.iter().find(|p| p["name"] == "search");
        assert!(search_param.is_none(), "Non-FTS resource should not have search param");
    }
}
