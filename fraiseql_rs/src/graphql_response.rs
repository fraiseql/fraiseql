// src/graphql_response.rs

use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use serde_json::{Value, Map};

/// Escape a string for safe inclusion in JSON
fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str(r#"\""#),
            '\\' => result.push_str(r"\\"),
            '\n' => result.push_str(r"\n"),
            '\r' => result.push_str(r"\r"),
            '\t' => result.push_str(r"\t"),
            '\x08' => result.push_str(r"\b"),
            '\x0C' => result.push_str(r"\f"),
            _ => result.push(c),
        }
    }
    result
}

/// Estimate buffer capacity needed for GraphQL response
fn estimate_capacity(json_strings: &[String], field_name: &str) -> usize {
    let rows_size: usize = json_strings.iter().map(|s| s.len()).sum();
    let commas = json_strings.len().saturating_sub(1);
    let wrapper_overhead = 50 + field_name.len() * 2; // {"data":{"fieldName":[]}}
    rows_size + commas + wrapper_overhead
}

/// Transform snake_case keys to camelCase in JSON
fn transform_to_camel_case(json_str: &str, type_name: Option<&str>) -> Result<String, String> {
    // Parse JSON
    let mut value: Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    // Transform recursively
    transform_value(&mut value, type_name);

    // Serialize back
    serde_json::to_string(&value)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))
}

/// Recursively transform JSON value
fn transform_value(value: &mut Value, type_name: Option<&str>) {
    match value {
        Value::Object(map) => {
            let mut new_map = Map::new();

            // Add __typename if provided
            if let Some(tn) = type_name {
                new_map.insert("__typename".to_string(), Value::String(tn.to_string()));
            }

            // Transform each key
            for (key, val) in map.iter_mut() {
                let camel_key = snake_to_camel(key);
                transform_value(val, None); // Don't add typename to nested objects
                new_map.insert(camel_key, val.clone());
            }

            *map = new_map;
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                transform_value(item, type_name);
            }
        }
        _ => {}
    }
}

/// Convert snake_case to camelCase
fn snake_to_camel(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;

    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Project only requested fields from JSON object
fn project_fields(obj: &Value, field_paths: &[Vec<String>]) -> Value {
    let mut result = Map::new();

    for path in field_paths {
        if let Some(value) = extract_value_at_path(obj, path) {
            // Build nested structure
            set_value_at_path(&mut result, path, value);
        }
    }

    Value::Object(result)
}

/// Extract value at a JSON path
fn extract_value_at_path(obj: &Value, path: &[String]) -> Option<Value> {
    let mut current = obj;
    for segment in path {
        current = current.get(segment)?;
    }
    Some(current.clone())
}

/// Set value at a JSON path, creating intermediate objects
fn set_value_at_path(obj: &mut Map<String, Value>, path: &[String], value: Value) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        obj.insert(path[0].clone(), value);
        return;
    }

    // Create intermediate objects
    let key = &path[0];
    let nested = obj
        .entry(key.clone())
        .or_insert_with(|| Value::Object(Map::new()));

    if let Value::Object(ref mut nested_map) = nested {
        set_value_at_path(nested_map, &path[1..], value);
    }
}

/// Build GraphQL list response from JSON strings
///
/// This function performs ALL post-database operations:
/// 1. Concatenate JSON rows into array
/// 2. Wrap in GraphQL response structure
/// 3. Transform snake_case → camelCase
/// 4. Inject __typename
/// 5. Encode to UTF-8 bytes
///
/// # Arguments
/// * `json_strings` - Vec of JSON strings from PostgreSQL
/// * `field_name` - GraphQL field name (e.g., "users")
/// * `type_name` - Optional GraphQL type for transformation (e.g., "User")
///
/// # Returns
/// UTF-8 encoded bytes ready for HTTP response
#[pyfunction]
pub fn build_list_response(
    json_strings: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
) -> PyResult<Vec<u8>> {
    // Step 1: Pre-allocate buffer
    let capacity = estimate_capacity(&json_strings, field_name);
    let mut buffer = String::with_capacity(capacity);

    // Step 2: Build GraphQL response structure opening
    buffer.push_str(r#"{"data":{"#);
    buffer.push('"');
    buffer.push_str(&escape_json_string(field_name));
    buffer.push_str("\":[");

    // Step 3: Concatenate rows
    for (i, row) in json_strings.iter().enumerate() {
        if i > 0 {
            buffer.push(',');
        }
        buffer.push_str(row);
    }

    // Step 4: Close GraphQL structure
    buffer.push_str("]}}");

    // Step 5: Transform if type_name provided
    let final_json = if type_name.is_some() {
        transform_to_camel_case(&buffer, type_name)
            .map_err(|e| PyRuntimeError::new_err(e))?
    } else {
        buffer
    };

    // Step 6: Return as UTF-8 bytes
    Ok(final_json.into_bytes())
}

/// Build GraphQL single object response
#[pyfunction]
pub fn build_single_response(
    json_string: String,
    field_name: &str,
    type_name: Option<&str>,
) -> PyResult<Vec<u8>> {
    // Pre-allocate buffer
    let capacity = json_string.len() + 100 + field_name.len() * 2;
    let mut buffer = String::with_capacity(capacity);

    // Build GraphQL response
    buffer.push_str(r#"{"data":{"#);
    buffer.push('"');
    buffer.push_str(&escape_json_string(field_name));
    buffer.push_str("\":");
    buffer.push_str(&json_string);
    buffer.push_str("}}");

    // Transform if needed
    let final_json = if type_name.is_some() {
        transform_to_camel_case(&buffer, type_name)
            .map_err(|e| PyRuntimeError::new_err(e))?
    } else {
        buffer
    };

    Ok(final_json.into_bytes())
}

/// Build empty array response: {"data":{"fieldName":[]}}
#[pyfunction]
pub fn build_empty_array_response(field_name: &str) -> PyResult<Vec<u8>> {
    let json = format!(
        r#"{{"data":{{"{}":[]}}}}"#,
        escape_json_string(field_name)
    );
    Ok(json.into_bytes())
}

/// Build null response: {"data":{"fieldName":null}}
#[pyfunction]
pub fn build_null_response(field_name: &str) -> PyResult<Vec<u8>> {
    let json = format!(
        r#"{{"data":{{"{}":null}}}}"#,
        escape_json_string(field_name)
    );
    Ok(json.into_bytes())
}

/// Build GraphQL list response with field projection
///
/// This function performs field projection in Rust for maximum performance:
/// 1. Parse each JSON string
/// 2. Project only requested fields
/// 3. Concatenate filtered objects into array
/// 4. Wrap in GraphQL response structure
/// 5. Transform snake_case → camelCase
/// 6. Inject __typename
/// 7. Encode to UTF-8 bytes
///
/// # Arguments
/// * `json_strings` - Vec of JSON strings from PostgreSQL
/// * `field_name` - GraphQL field name (e.g., "users")
/// * `type_name` - Optional GraphQL type for transformation (e.g., "User")
/// * `field_paths` - Optional field paths for projection (e.g., [["id"], ["firstName"]])
///
/// # Returns
/// UTF-8 encoded bytes ready for HTTP response
#[pyfunction]
pub fn build_list_response_with_projection(
    json_strings: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
) -> PyResult<Vec<u8>> {
    // Step 1: Parse and project fields if requested
    let processed_strings: Vec<String> = if let Some(paths) = field_paths {
        json_strings
            .iter()
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .map(|obj| {
                let projected = project_fields(&obj, &paths);
                serde_json::to_string(&projected).unwrap_or_else(|_| "{}".to_string())
            })
            .collect()
    } else {
        json_strings
    };

    // Step 2: Use existing build_list_response for the rest
    build_list_response(processed_strings, field_name, type_name)
}

/// Build GraphQL single object response with field projection
#[pyfunction]
pub fn build_single_response_with_projection(
    json_string: String,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
) -> PyResult<Vec<u8>> {
    // Step 1: Parse and project fields if requested
    let processed_string: String = if let Some(paths) = field_paths {
        if let Ok(obj) = serde_json::from_str::<Value>(&json_string) {
            let projected = project_fields(&obj, &paths);
            serde_json::to_string(&projected).unwrap_or_else(|_| "{}".to_string())
        } else {
            json_string
        }
    } else {
        json_string
    };

    // Step 2: Use existing build_single_response for the rest
    build_single_response(processed_string, field_name, type_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_json_string() {
        assert_eq!(escape_json_string("hello"), "hello");
        assert_eq!(escape_json_string("hello\"world"), r#"hello\"world"#);
        assert_eq!(escape_json_string("line\nbreak"), r"line\nbreak");
    }

    #[test]
    fn test_snake_to_camel() {
        assert_eq!(snake_to_camel("first_name"), "firstName");
        assert_eq!(snake_to_camel("user_id"), "userId");
        assert_eq!(snake_to_camel("id"), "id");
        assert_eq!(snake_to_camel("is_active"), "isActive");
    }

    #[test]
    fn test_build_empty_array_response() {
        let result = build_empty_array_response("users").unwrap();
        let json = String::from_utf8(result).unwrap();
        assert_eq!(json, r#"{"data":{"users":[]}}"#);
    }

    #[test]
    fn test_build_null_response() {
        let result = build_null_response("user").unwrap();
        let json = String::from_utf8(result).unwrap();
        assert_eq!(json, r#"{"data":{"user":null}}"#);
    }

    #[test]
    fn test_build_list_response_no_transform() {
        let json_strings = vec![
            r#"{"id":"1","name":"Alice"}"#.to_string(),
            r#"{"id":"2","name":"Bob"}"#.to_string(),
        ];

        let result = build_list_response(json_strings, "users", None).unwrap();
        let json = String::from_utf8(result).unwrap();

        assert_eq!(
            json,
            r#"{"data":{"users":[{"id":"1","name":"Alice"},{"id":"2","name":"Bob"}]}}"#
        );
    }

    #[test]
    fn test_build_single_response_no_transform() {
        let json_string = r#"{"id":"1","name":"Alice"}"#.to_string();

        let result = build_single_response(json_string, "user", None).unwrap();
        let json = String::from_utf8(result).unwrap();

        assert_eq!(
            json,
            r#"{"data":{"user":{"id":"1","name":"Alice"}}}"#
        );
    }

    #[test]
    fn test_project_fields_simple() {
        let obj: Value = serde_json::from_str(r#"{"id":"1","first_name":"Alice","last_name":"Smith","email":"alice@example.com"}"#).unwrap();
        let field_paths = vec![
            vec!["id".to_string()],
            vec!["first_name".to_string()],
        ];

        let result = project_fields(&obj, &field_paths);

        assert_eq!(result["id"], "1");
        assert_eq!(result["first_name"], "Alice");
        assert!(result.get("last_name").is_none()); // Filtered out!
        assert!(result.get("email").is_none()); // Filtered out!
    }

    #[test]
    fn test_project_fields_nested() {
        let obj: Value = serde_json::from_str(r#"{"id":"1","address":{"street":"123 Main","city":"NYC","zip":"10001"}}"#).unwrap();
        let field_paths = vec![
            vec!["id".to_string()],
            vec!["address".to_string(), "city".to_string()],
        ];

        let result = project_fields(&obj, &field_paths);

        assert_eq!(result["id"], "1");
        assert_eq!(result["address"]["city"], "NYC");
        assert!(result["address"].get("street").is_none()); // Filtered out!
        assert!(result["address"].get("zip").is_none()); // Filtered out!
    }

    #[test]
    fn test_build_list_response_with_projection() {
        let json_strings = vec![
            r#"{"id":"1","first_name":"Alice","last_name":"Smith","email":"alice@example.com"}"#.to_string(),
            r#"{"id":"2","first_name":"Bob","last_name":"Jones","email":"bob@example.com"}"#.to_string(),
        ];

        let field_paths = Some(vec![
            vec!["id".to_string()],
            vec!["first_name".to_string()],
        ]);

        let result = build_list_response_with_projection(json_strings, "users", Some("User"), field_paths).unwrap();
        let json = String::from_utf8(result).unwrap();

        // Parse and verify structure
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let users = &parsed["data"]["users"];

        assert_eq!(users[0]["id"], "1");
        assert_eq!(users[0]["firstName"], "Alice");
        assert_eq!(users[0]["__typename"], "User");
        assert!(users[0].get("lastName").is_none()); // Filtered out!
        assert!(users[0].get("email").is_none()); // Filtered out!

        assert_eq!(users[1]["id"], "2");
        assert_eq!(users[1]["firstName"], "Bob");
        assert_eq!(users[1]["__typename"], "User");
    }

    #[test]
    fn test_build_single_response_with_projection() {
        let json_string = r#"{"id":"1","first_name":"Alice","last_name":"Smith","email":"alice@example.com"}"#.to_string();

        let field_paths = Some(vec![
            vec!["id".to_string()],
            vec!["first_name".to_string()],
        ]);

        let result = build_single_response_with_projection(json_string, "user", Some("User"), field_paths).unwrap();
        let json = String::from_utf8(result).unwrap();

        // Parse and verify structure
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let user = &parsed["data"]["user"];

        assert_eq!(user["id"], "1");
        assert_eq!(user["firstName"], "Alice");
        assert_eq!(user["__typename"], "User");
        assert!(user.get("lastName").is_none()); // Filtered out!
        assert!(user.get("email").is_none()); // Filtered out!
    }
}