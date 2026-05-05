//! `OpenAPI` bulk operation documentation (collection-level PATCH and DELETE).

use fraiseql_core::schema::MutationOperation;
use serde_json::{Map, Value, json};

use super::{OpenApiGenerator, format::capitalize};
use crate::routes::rest::resource::{RestResource, RouteSource};

impl OpenApiGenerator<'_> {
    /// Add collection-level PATCH and DELETE operations for bulk update/delete.
    pub(super) fn add_bulk_operations(
        &self,
        paths: &mut Map<String, Value>,
        resource: &RestResource,
    ) {
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
}
