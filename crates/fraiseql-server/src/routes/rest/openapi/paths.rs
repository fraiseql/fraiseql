//! `OpenAPI` path item and operation building.

use fraiseql_core::schema::MutationOperation;
use serde_json::{Map, Value, json};

use super::{
    OpenApiGenerator,
    format::{capitalize, extract_action, method_to_string, to_snake},
};
use crate::routes::rest::resource::{HttpMethod, RestResource, RestRoute, RouteSource};

impl OpenApiGenerator<'_> {
    /// Build the `paths` object for all routes.
    pub(super) fn build_paths(&self) -> Value {
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

    /// Build a single `OpenAPI` operation object for a route.
    pub(super) fn build_operation(&self, resource: &RestResource, route: &RestRoute) -> Value {
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

    /// Derive a human-readable summary and operation ID for a route.
    pub(super) fn operation_summary(
        &self,
        resource: &RestResource,
        route: &RestRoute,
    ) -> (String, String) {
        let res_name = &resource.name;
        let type_name = &resource.type_name;

        match (&route.source, route.method) {
            (RouteSource::Query { name }, HttpMethod::Get) => {
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

    /// Check whether a route's backing operation is deprecated.
    pub(super) fn is_deprecated(&self, route: &RestRoute) -> bool {
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

    /// Add an SSE stream endpoint for a resource: `/{resource}/stream`.
    pub(super) fn add_stream_endpoint(
        &self,
        paths: &mut Map<String, Value>,
        resource: &RestResource,
    ) {
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
}
