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
        //
        // It carries the same security posture as the data routes — the document is
        // served by a handler behind the same `require_auth` gate and the same mount
        // auth layer — so it must be *described* that way too. Emitting it as an
        // unsecured operation was the last place the document disagreed with what the
        // server does (#810).
        let openapi_path = "/openapi.json";
        let mut meta_get = json!({
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
        });
        self.apply_security(&mut meta_get);
        paths.insert(openapi_path.to_string(), json!({ "get": meta_get }));

        for resource in &self.route_table.resources {
            for route in &resource.routes {
                // Describe only what the router registered. A read-only mount derives
                // the same route table as a write mount and registers a subset of it,
                // so walking the table here is what made the read-only deployment
                // publish a complete write API it answered with 405 (#865).
                if !self.mounted.contains(&route.path, route.method) {
                    continue;
                }

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

        // Security — via the one helper every operation builder must use.
        let mut operation = Value::Object(op);
        self.apply_security(&mut operation);
        operation
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
        if !self.mounted.contains(&stream_path, HttpMethod::Get) {
            return;
        }

        // #1113: the document used to promise "Resume from a specific event ID on
        // reconnection". Nothing implemented that — the header was read into a discarded
        // binding — so the published contract advertised a resumption the code could
        // never perform, the same class of lie #873.4 removed from the 200 itself. What
        // the header actually gets is stated below, and the refusals the handler can
        // return are listed rather than left to be discovered.
        let mut responses = json!({
            "200": {
                "description": "SSE event stream. Each event's `id:` is the Change-Spine \
                                sequence (`seq`) of the change, and is absent for a change \
                                whose source row carried no sequence — per the SSE \
                                specification an absent `id:` leaves the client's \
                                last-event-id unchanged.",
                "content": {
                    "text/event-stream": {
                        "schema": { "type": "string" }
                    }
                }
            },
            "501": {
                "description": "Not Implemented — the `observers` feature is disabled, no \
                                event transport is configured, or the request carried a \
                                `Last-Event-ID` (resumption is not implemented)."
            }
        });

        // Only a multi-tenant deployment can produce this refusal: in single-tenant mode
        // the subscription is unscoped and there is no tenant to be missing.
        if self.schema.is_multi_tenant() {
            responses["403"] = json!({
                "description": "Forbidden — this deployment is multi-tenant and the request \
                                carries no tenant, so the stream cannot be scoped to one."
            });
        }

        let mut stream_get = json!({
                    "tags": [capitalize(&resource.name)],
                    "summary": format!("Stream {} changes (SSE)", resource.name),
                    "operationId": format!("stream_{}", resource.name),
                    "description": format!(
                        "Subscribe to real-time changes on {} via Server-Sent Events. \
                         Requires the `observers` feature. Events: `insert`, `update`, `delete`, `ping` (heartbeat). \
                         In a multi-tenant deployment the subscription is scoped to the caller's tenant.",
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
                            "description": "Sent automatically by a browser EventSource on reconnect. \
                                            Resumption is NOT implemented: a request carrying this header \
                                            is refused with 501, rather than answered with a stream that \
                                            silently skips everything since the given id. Reconnect \
                                            without it to receive events from now on."
                        }
                    ],
                    "responses": responses
        });
        self.apply_security(&mut stream_get);
        paths.insert(stream_path, json!({ "get": stream_get }));
    }
}
