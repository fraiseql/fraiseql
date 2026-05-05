//! `OpenAPI` component schema building: `JSON Schema` generation for types.

use fraiseql_core::schema::{Cardinality, FieldType, TypeDefinition};
use serde_json::{Map, Value, json};

use super::{OpenApiGenerator, helpers::field_type_to_json_schema};

impl OpenApiGenerator<'_> {
    /// Build the `components` object (schemas + security schemes).
    pub(super) fn build_components(&self) -> Value {
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

    /// Build a JSON Schema object for a type definition.
    pub(super) fn type_to_schema(&self, type_def: &TypeDefinition) -> Value {
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
            let ref_schema =
                json!({ "$ref": format!("#/components/schemas/{}", rel.target_type) });
            let rel_schema = match rel.cardinality {
                Cardinality::OneToMany => {
                    json!({
                        "type": "array",
                        "items": ref_schema,
                        "description": format!(
                            "Embedded {} (use ?select={}(fields) to include)",
                            rel.target_type, rel.name
                        ),
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
