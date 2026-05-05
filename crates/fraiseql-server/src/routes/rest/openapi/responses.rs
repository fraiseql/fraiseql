//! `OpenAPI` response and request body building.

use fraiseql_core::schema::{DeleteResponse, MutationDefinition, TypeDefinition};
use serde_json::{Map, Value, json};

use super::{OpenApiGenerator, format::field_type_to_json_schema};
use crate::routes::rest::resource::{HttpMethod, RestResource, RestRoute, RouteSource};

impl OpenApiGenerator<'_> {
    /// Build the `responses` object for a route.
    #[allow(clippy::too_many_lines)] // Reason: response building requires handling each HTTP method
    pub(super) fn build_responses(&self, resource: &RestResource, route: &RestRoute) -> Value {
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

    /// Build the `requestBody` for a mutating route, or `None` for GET/DELETE.
    pub(super) fn build_request_body(
        &self,
        resource: &RestResource,
        route: &RestRoute,
    ) -> Option<Value> {
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
                if let Some(td) = type_def {
                    self.writable_fields_schema(td, true)
                } else {
                    self.mutation_args_schema(mutation)
                }
            },
            HttpMethod::Patch => {
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

    /// Build a JSON Schema object for mutation arguments.
    pub(super) fn mutation_args_schema(&self, mutation: &MutationDefinition) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();

        for arg in &mutation.arguments {
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

    /// Build a JSON Schema object for writable fields (PUT = all required, PATCH = none required).
    pub(super) fn writable_fields_schema(
        &self,
        type_def: &TypeDefinition,
        all_required: bool,
    ) -> Value {
        let writable = type_def.writable_fields();
        let mut properties = Map::new();
        let mut required = Vec::new();

        for field in &writable {
            properties
                .insert(field.name.to_string(), field_type_to_json_schema(&field.field_type));
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
}
