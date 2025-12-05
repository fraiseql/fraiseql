//! GraphQL Response Builder
//!
//! Builds GraphQL-compliant Success and Error responses from mutation results.

use serde_json::{json, Map, Value};
use crate::camel_case::to_camel_case;
use super::{MutationResult, MutationStatus};

/// Build GraphQL response from mutation result
///
/// This is the main entry point that dispatches to success or error builders
/// based on the mutation status.
pub fn build_graphql_response(
    result: &MutationResult,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    entity_type: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
) -> Result<Value, String> {
    let response_obj = if result.status.is_success() || result.status.is_noop() {
        build_success_response(result, success_type, entity_field_name, auto_camel_case, success_type_fields)?
    } else {
        build_error_response(result, error_type, auto_camel_case)?
    };

    // Wrap in GraphQL response structure
    Ok(json!({
        "data": {
            field_name: response_obj
        }
    }))
}

/// Build success response object
///
/// Key behaviors:
/// - CASCADE at success level (sibling to entity, NOT nested inside entity)
/// - Entity field name derived from entity_type or explicit parameter
/// - Wrapper fields promoted to success level
/// - __typename added to response and entity
/// - camelCase applied if requested
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Add __typename
    obj.insert("__typename".to_string(), json!(success_type));

    // Add id from entity_id if present
    if let Some(ref entity_id) = result.entity_id {
        obj.insert("id".to_string(), json!(entity_id));
    }

    // Add message
    obj.insert("message".to_string(), json!(result.message));

    // Add entity with __typename and camelCase keys
    if let Some(entity) = &result.entity {
        let entity_type = result.entity_type.as_deref().unwrap_or("Entity");

        // Determine the field name for the entity in the response
        let field_name = entity_field_name
            .map(|name| {
                // Convert entity_field_name based on auto_camel_case flag
                if auto_camel_case {
                    to_camel_case(name)
                } else {
                    name.to_string()
                }
            })
            .unwrap_or_else(|| {
                // No entity_field_name provided, derive from type
                if auto_camel_case {
                    to_camel_case(&entity_type.to_lowercase())
                } else {
                    entity_type.to_lowercase()
                }
            });

        // Check if entity is a wrapper object containing entity_field_name
        // This happens when Python entity_flattener skips flattening (CASCADE case)
        // The entity looks like: {"post": {...}, "message": "..."}
        let actual_entity = if let Value::Object(entity_map) = entity {
            // Check if the entity wrapper contains a field matching entity_field_name
            if let Some(entity_field_name_raw) = entity_field_name {
                if let Some(nested_entity) = entity_map.get(entity_field_name_raw) {
                    // Found nested entity - extract it
                    nested_entity
                } else {
                    // No nested field, use entire entity
                    &entity
                }
            } else {
                // No entity_field_name hint, use entire entity
                entity
            }
        } else {
            // Entity is not an object (array or primitive), use as-is
            entity
        };

        let transformed = transform_entity(actual_entity, entity_type, auto_camel_case);
        obj.insert(field_name, transformed);

        // If entity was a wrapper, copy other fields from it (like "message")
        if let Value::Object(entity_map) = entity {
            if let Some(entity_field_name_raw) = entity_field_name {
                if entity_map.contains_key(entity_field_name_raw) {
                    // Entity was a wrapper - copy other fields
                    for (key, value) in entity_map {
                        if key != entity_field_name_raw && key != "entity" {
                            // Don't copy the entity field itself or nested "entity"
                            let field_key = if auto_camel_case {
                                to_camel_case(&key)
                            } else {
                                key.clone()
                            };
                            // Only add if not already present (message might be at top level)
                            if !obj.contains_key(&field_key) {
                                obj.insert(field_key, transform_value(&value, auto_camel_case));
                            }
                        }
                    }
                }
            }
        }
    }

    // Add updatedFields (convert to camelCase)
    if let Some(fields) = &result.updated_fields {
        let transformed_fields: Vec<Value> = fields.iter()
            .map(|f| json!(if auto_camel_case { to_camel_case(f) } else { f.to_string() }))
            .collect();
        obj.insert("updatedFields".to_string(), json!(transformed_fields));
    }

    // Add cascade if present (add __typename for GraphQL)
    if let Some(cascade) = &result.cascade {
        let cascade_with_typename = transform_cascade(cascade, auto_camel_case);
        obj.insert("cascade".to_string(), cascade_with_typename);
    }

    // Phase 3: Schema validation - check that all expected fields are present
    if let Some(expected_fields) = success_type_fields {
        let mut missing_fields = Vec::new();
        let mut extra_fields = Vec::new();

        // Check for missing expected fields
        for field in expected_fields {
            if !obj.contains_key(field) {
                missing_fields.push(field.clone());
            }
        }

        // Check for unexpected fields (warn about them)
        for key in obj.keys() {
            if !expected_fields.contains(key) && !key.starts_with("__") {
                // Allow special fields like __typename
                extra_fields.push(key.clone());
            }
        }

        // Report validation results
        if !missing_fields.is_empty() {
            eprintln!(
                "Schema validation warning: Missing expected fields in {}: {:?}",
                success_type, missing_fields
            );
        }

        if !extra_fields.is_empty() {
            eprintln!(
                "Schema validation warning: Extra fields in {} not in schema: {:?}",
                success_type, extra_fields
            );
        }
    }

    Ok(Value::Object(obj))
}

/// Build error response object
///
/// Key behaviors:
/// - Extract error code from status string (part after ':')
/// - Auto-generate errors array if not in metadata
/// - Map status to HTTP code
pub fn build_error_response(
    result: &MutationResult,
    error_type: &str,
    auto_camel_case: bool,
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Add __typename
    obj.insert("__typename".to_string(), json!(error_type));

    // Add message
    obj.insert("message".to_string(), json!(result.message));

    // Add status string
    let status_str = match &result.status {
        MutationStatus::Noop(full_status) => full_status.clone(),
        MutationStatus::Error(full_status) => full_status.clone(),
        MutationStatus::Success(full_status) => full_status.clone(),
    };
    obj.insert("status".to_string(), json!(status_str));

    // Add HTTP code
    obj.insert("code".to_string(), json!(result.status.http_code()));

    // Add errors array
    if let Some(errors) = result.errors() {
        let transformed: Vec<Value> = errors.iter()
            .map(|e| transform_error(e, auto_camel_case))
            .collect();
        obj.insert("errors".to_string(), json!(transformed));
    } else {
        // Auto-generate error from status/message
        let code = match &result.status {
            MutationStatus::Noop(full_status) => {
                // Extract reason after "noop:"
                full_status.strip_prefix("noop:").unwrap_or(full_status).to_string()
            },
            MutationStatus::Error(full_status) => {
                // Extract reason after first colon
                if let Some(colon_pos) = full_status.find(':') {
                    if colon_pos < full_status.len() - 1 {
                        full_status[colon_pos + 1..].to_string()
                    } else {
                        "".to_string()
                    }
                } else {
                    full_status.clone()
                }
            },
            MutationStatus::Success(s) => s.clone(),
        };
        let auto_error = json!({
            "field": null,
            "code": code,
            "message": result.message
        });
        obj.insert("errors".to_string(), json!([auto_error]));
    }

    Ok(Value::Object(obj))
}

/// Transform entity: add __typename and convert keys to camelCase
fn transform_entity(entity: &Value, entity_type: &str, auto_camel_case: bool) -> Value {
    match entity {
        Value::Object(map) => {
            let mut result = Map::with_capacity(map.len() + 1);

            // Add __typename first
            result.insert("__typename".to_string(), json!(entity_type));

            // Transform each field to camelCase
            for (key, val) in map {
                let transformed_key = if auto_camel_case { to_camel_case(key) } else { key.clone() };
                result.insert(transformed_key, transform_value(val, auto_camel_case));
            }

            Value::Object(result)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| transform_entity(v, entity_type, auto_camel_case)).collect())
        }
        other => other.clone(),
    }
}

/// Transform value: convert keys to camelCase (no __typename)
fn transform_value(value: &Value, auto_camel_case: bool) -> Value {
    match value {
        Value::Object(map) => {
            let mut result = Map::new();
            for (key, val) in map {
                let transformed_key = if auto_camel_case { to_camel_case(key) } else { key.clone() };
                result.insert(transformed_key, transform_value(val, auto_camel_case));
            }
            Value::Object(result)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| transform_value(v, auto_camel_case)).collect())
        }
        other => other.clone(),
    }
}

/// Transform error object to camelCase
fn transform_error(error: &Value, auto_camel_case: bool) -> Value {
    transform_value(error, auto_camel_case)
}

/// Transform cascade object: add __typename and convert keys to camelCase
fn transform_cascade(cascade: &Value, auto_camel_case: bool) -> Value {
    match cascade {
        Value::Object(map) => {
            let mut result = Map::with_capacity(map.len() + 1);

            // Add __typename for GraphQL
            result.insert("__typename".to_string(), json!("Cascade"));

            // Transform each field to camelCase and recursively transform nested values
            for (key, val) in map {
                let transformed_key = if auto_camel_case { to_camel_case(key) } else { key.clone() };
                result.insert(transformed_key, transform_value(val, auto_camel_case));
            }

            Value::Object(result)
        }
        // If cascade is not an object, return as-is (shouldn't happen)
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutation::MutationResult;

    #[test]
    fn test_build_success_simple() {
        let result = MutationResult {
            status: MutationStatus::Success("success".to_string()),
            message: "Success".to_string(),
            entity_id: Some("123".to_string()),
            entity_type: Some("User".to_string()),
            entity: Some(json!({"id": "123", "name": "John"})),
            updated_fields: None,
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        let response = build_success_response(&result, "CreateUserSuccess", Some("user"), true, None).unwrap();
        let obj = response.as_object().unwrap();

        assert_eq!(obj["__typename"], "CreateUserSuccess");
        assert_eq!(obj["id"], "123");
        assert_eq!(obj["message"], "Success");
        assert!(obj.contains_key("user"));
        assert_eq!(obj["user"]["__typename"], "User");
        assert_eq!(obj["user"]["id"], "123");
        assert_eq!(obj["user"]["name"], "John");
    }

    #[test]
    fn test_build_success_with_cascade() {
        let result = MutationResult {
            status: MutationStatus::Success("created".to_string()),
            message: "User created".to_string(),
            entity_id: None,
            entity_type: Some("User".to_string()),
            entity: Some(json!({"id": "123", "name": "John"})),
            updated_fields: None,
            cascade: Some(json!({"updated": []})),
            metadata: None,
            is_simple_format: false,
        };

        let response = build_success_response(&result, "CreateUserSuccess", Some("user"), true, None).unwrap();
        let obj = response.as_object().unwrap();

        // CASCADE at success level
        assert!(obj.contains_key("cascade"));
        assert_eq!(obj["cascade"]["__typename"], "Cascade");
        assert!(obj["cascade"]["updated"].is_array());

        // NOT in entity
        assert!(!obj["user"].as_object().unwrap().contains_key("cascade"));
    }

    #[test]
    fn test_wrapper_fields_promoted() {
        let result = MutationResult {
            status: MutationStatus::Success("success".to_string()),
            message: "Success".to_string(),
            entity_id: None,
            entity_type: Some("Post".to_string()),
            entity: Some(json!({"post": {"id": "456", "title": "Hello"}, "message": "Created"})),
            updated_fields: None,
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        let response = build_success_response(&result, "CreatePostSuccess", Some("post"), true, None).unwrap();
        let obj = response.as_object().unwrap();

        // Entity extracted and has __typename
        assert_eq!(obj["post"]["__typename"], "Post");
        assert_eq!(obj["post"]["id"], "456");
        assert_eq!(obj["post"]["title"], "Hello");

        // Wrapper field promoted to success level
        assert_eq!(obj["message"], "Created");
    }

    #[test]
    fn test_build_error() {
        let result = MutationResult {
            status: MutationStatus::Error("failed:validation".to_string()),
            message: "Validation failed".to_string(),
            entity_id: None,
            entity_type: None,
            entity: None,
            updated_fields: None,
            cascade: None,
            metadata: Some(json!({"errors": [{"field": "email", "code": "invalid", "message": "Invalid email"}]})),
            is_simple_format: false,
        };

        let response = build_error_response(&result, "CreateUserError", true).unwrap();
        let obj = response.as_object().unwrap();

        assert_eq!(obj["__typename"], "CreateUserError");
        assert_eq!(obj["message"], "Validation failed");
        assert_eq!(obj["status"], "failed:validation");
        assert_eq!(obj["code"], 422); // HTTP code for validation error

        let errors = obj["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["field"], "email");
        assert_eq!(errors[0]["code"], "invalid");
    }

    #[test]
    fn test_error_code_extraction() {
        let result = MutationResult {
            status: MutationStatus::Error("failed:validation".to_string()),
            message: "Validation failed".to_string(),
            entity_id: None,
            entity_type: None,
            entity: None,
            updated_fields: None,
            cascade: None,
            metadata: None, // No errors in metadata
            is_simple_format: false,
        };

        let response = build_error_response(&result, "CreateUserError", true).unwrap();
        let obj = response.as_object().unwrap();

        // Auto-generated error with extracted code
        let errors = obj["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["code"], "validation");
        assert_eq!(errors[0]["message"], "Validation failed");
    }

    #[test]
    fn test_http_code_mapping() {
        // Test various error types map to correct HTTP codes
        let test_cases = vec![
            ("failed:not_found", 404),
            ("unauthorized:token", 401),
            ("forbidden:access", 403),
            ("conflict:duplicate", 409),
            ("failed:validation", 422),
            ("timeout:database", 408),
            ("failed:unknown", 500),
        ];

        for (status_str, expected_code) in test_cases {
            let result = MutationResult {
                status: MutationStatus::Error(status_str.to_string()),
                message: "Error".to_string(),
                entity_id: None,
                entity_type: None,
                entity: None,
                updated_fields: None,
                cascade: None,
                metadata: None,
                is_simple_format: false,
            };

            let response = build_error_response(&result, "TestError", true).unwrap();
            let obj = response.as_object().unwrap();
            assert_eq!(obj["code"], expected_code, "Status '{}' should map to HTTP code {}", status_str, expected_code);
        }
    }
}
