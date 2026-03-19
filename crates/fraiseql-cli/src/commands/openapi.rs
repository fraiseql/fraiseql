//! `fraiseql openapi` — generate an OpenAPI 3.0.3 specification from a compiled schema.

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;

/// Run the `openapi` command.
///
/// Reads a compiled schema, derives the REST route table, generates an OpenAPI
/// 3.0.3 spec, and writes it to the output path.
///
/// # Errors
///
/// Returns an error if the schema cannot be read, is missing REST configuration,
/// or route derivation fails.
pub fn run(schema_path: &str, output: &str) -> Result<()> {
    let schema_json = std::fs::read_to_string(schema_path)
        .with_context(|| format!("Failed to read schema file: {schema_path}"))?;

    let schema: CompiledSchema = serde_json::from_str(&schema_json)
        .with_context(|| format!("Failed to parse compiled schema: {schema_path}"))?;

    let config = schema
        .rest_config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(
            "No REST configuration found in schema. Add [rest] section to fraiseql.toml."
        ))?;

    if !config.enabled {
        anyhow::bail!("REST transport is disabled (rest.enabled = false)");
    }

    let spec = generate_spec(&schema)?;

    let pretty = serde_json::to_string_pretty(&spec)
        .context("Failed to serialize OpenAPI spec")?;

    if output == "-" {
        println!("{pretty}");
    } else {
        std::fs::write(output, &pretty)
            .with_context(|| format!("Failed to write OpenAPI spec to: {output}"))?;
        eprintln!("OpenAPI spec written to {output}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Inline route derivation (avoids fraiseql-server dependency)
// ---------------------------------------------------------------------------

fn generate_spec(schema: &CompiledSchema) -> Result<serde_json::Value> {
    // For the CLI, we generate a simplified spec without full route derivation.
    // The full spec is generated at runtime by the server.
    // This CLI command provides a preview based on schema metadata.
    let config = schema
        .rest_config
        .as_ref()
        .expect("rest_config already validated");
    let base_path = &config.path;

    let mut paths = serde_json::Map::new();

    // Generate paths from queries.
    for query in &schema.queries {
        if query.name.ends_with("_aggregate") || query.name.ends_with("_window") {
            continue;
        }
        if schema.find_type(&query.return_type).is_none() {
            continue;
        }

        let resource_name = if query.returns_list {
            query.name.clone()
        } else {
            continue; // Single queries need ID paths — skip for the simple listing.
        };

        let path = format!("/{resource_name}");
        paths.insert(
            path,
            serde_json::json!({
                "get": {
                    "summary": format!("List {resource_name}"),
                    "tags": [capitalize(&resource_name)],
                    "responses": {
                        "200": {
                            "description": format!("List of {resource_name}"),
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "data": {
                                                "type": "array",
                                                "items": {
                                                    "$ref": format!("#/components/schemas/{}", query.return_type)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );
    }

    // Build component schemas from types.
    let mut schemas = serde_json::Map::new();
    for type_def in &schema.types {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for field in &type_def.fields {
            properties.insert(
                field.name.to_string(),
                field_type_to_json_schema(&field.field_type),
            );
            if !field.nullable {
                required.push(serde_json::json!(field.name.to_string()));
            }
        }

        let mut type_schema = serde_json::json!({
            "type": "object",
            "properties": properties,
        });
        if !required.is_empty() {
            type_schema["required"] = serde_json::Value::Array(required);
        }
        schemas.insert(type_def.name.to_string(), type_schema);
    }

    Ok(serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "FraiseQL REST API",
            "version": "1.0.0",
            "description": "Auto-generated REST API from compiled schema",
        },
        "servers": [{
            "url": base_path,
            "description": "REST API base path"
        }],
        "paths": paths,
        "components": {
            "schemas": schemas,
        }
    }))
}

use fraiseql_core::schema::FieldType;

fn field_type_to_json_schema(ft: &FieldType) -> serde_json::Value {
    match ft {
        FieldType::String => serde_json::json!({ "type": "string" }),
        FieldType::Int => serde_json::json!({ "type": "integer" }),
        FieldType::Float => serde_json::json!({ "type": "number" }),
        FieldType::Boolean => serde_json::json!({ "type": "boolean" }),
        FieldType::Id | FieldType::Uuid => serde_json::json!({ "type": "string", "format": "uuid" }),
        FieldType::DateTime => serde_json::json!({ "type": "string", "format": "date-time" }),
        FieldType::Date => serde_json::json!({ "type": "string", "format": "date" }),
        FieldType::Time => serde_json::json!({ "type": "string", "format": "time" }),
        FieldType::Json => serde_json::json!({ "type": "object" }),
        FieldType::Decimal => serde_json::json!({ "type": "string", "format": "decimal" }),
        FieldType::Vector => serde_json::json!({ "type": "array", "items": { "type": "number" } }),
        FieldType::Scalar(_) => serde_json::json!({ "type": "string" }),
        FieldType::List(inner) => serde_json::json!({ "type": "array", "items": field_type_to_json_schema(inner) }),
        FieldType::Object(name) => serde_json::json!({ "$ref": format!("#/components/schemas/{name}") }),
        FieldType::Enum(name) => serde_json::json!({ "$ref": format!("#/components/schemas/{name}") }),
        FieldType::Input(name) => serde_json::json!({ "$ref": format!("#/components/schemas/{name}") }),
        FieldType::Interface(name) | FieldType::Union(name) => {
            serde_json::json!({ "type": "object", "description": format!("See {name}") })
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn minimal_schema_json() -> String {
        serde_json::json!({
            "types": [{
                "name": "User",
                "sql_source": "v_user",
                "fields": [
                    { "name": "id", "field_type": "UUID" },
                    { "name": "name", "field_type": "String" },
                ]
            }],
            "queries": [{
                "name": "users",
                "return_type": "User",
                "returns_list": true,
            }],
            "mutations": [],
            "rest_config": {
                "enabled": true,
                "path": "/rest/v1"
            }
        }).to_string()
    }

    #[test]
    fn run_writes_openapi_spec() {
        let mut schema_file = NamedTempFile::new().unwrap();
        write!(schema_file, "{}", minimal_schema_json()).unwrap();

        let output_file = NamedTempFile::new().unwrap();
        let output_path = output_file.path().to_str().unwrap().to_string();

        run(schema_file.path().to_str().unwrap(), &output_path).unwrap();

        let content = std::fs::read_to_string(&output_path).unwrap();
        let spec: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(spec["openapi"], "3.0.3");
        assert!(spec["paths"]["/users"]["get"].is_object());
    }

    #[test]
    fn run_fails_without_rest_config() {
        let schema = serde_json::json!({
            "types": [],
            "queries": [],
            "mutations": [],
        });
        let mut schema_file = NamedTempFile::new().unwrap();
        write!(schema_file, "{schema}").unwrap();

        let result = run(schema_file.path().to_str().unwrap(), "/dev/null");
        assert!(result.is_err());
    }

    #[test]
    fn run_fails_when_disabled() {
        let schema = serde_json::json!({
            "types": [],
            "queries": [],
            "mutations": [],
            "rest_config": { "enabled": false }
        });
        let mut schema_file = NamedTempFile::new().unwrap();
        write!(schema_file, "{schema}").unwrap();

        let result = run(schema_file.path().to_str().unwrap(), "/dev/null");
        assert!(result.is_err());
    }
}
