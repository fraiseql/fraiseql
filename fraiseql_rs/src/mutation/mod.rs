//! Mutation result transformation module
//!
//! Transforms PostgreSQL mutation_result_v2 JSON into GraphQL responses.

#[cfg(test)]
mod tests;

use serde_json::{json, Map, Value};
use crate::camel_case::to_camel_case;

/// Build complete GraphQL mutation response
///
/// This is the main entry point. It takes PostgreSQL JSON and returns
/// HTTP-ready bytes with proper GraphQL structure.
///
/// Supports TWO formats:
/// 1. **Simple format**: Just entity JSONB (no status field) - auto-detected
/// 2. **Full v2 format**: Complete mutation_result_v2 with status, message, etc.
///
/// # Arguments
/// * `mutation_json` - Raw JSON from PostgreSQL (simple or v2 format)
/// * `field_name` - GraphQL field name (e.g., "createUser")
/// * `success_type` - Success type name (e.g., "CreateUserSuccess")
/// * `error_type` - Error type name (e.g., "CreateUserError")
/// * `entity_field_name` - Field name for entity (e.g., "user")
/// * `entity_type` - Entity type for __typename (e.g., "User") - REQUIRED for simple format
/// * `cascade_selections` - Optional cascade field selections (not implemented yet)
/// * `auto_camel_case` - Whether to convert field names and JSON keys to camelCase
pub fn build_mutation_response(
    mutation_json: &str,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    entity_type: Option<&str>,
    _cascade_selections: Option<&str>,
    auto_camel_case: bool,
) -> Result<Vec<u8>, String> {
    // Step 1: Parse the mutation result with entity_type for simple format
    let result = MutationResult::from_json(mutation_json, entity_type)?;

    // Step 2: Build response object based on status
    let response_obj = if result.status.is_success() {
        build_success_object(&result, success_type, entity_field_name, auto_camel_case)?
    } else {
        build_error_object(&result, error_type, auto_camel_case)?
    };

    // Step 3: Wrap in GraphQL response structure
    let graphql_response = json!({
        "data": {
            field_name: response_obj
        }
    });

    // Step 4: Serialize to bytes
    serde_json::to_vec(&graphql_response)
        .map_err(|e| format!("Failed to serialize: {}", e))
}

#[cfg(test)]
mod test_stub {
    use super::*;

    #[test]
    fn test_stub_function() {
        let result = build_mutation_response("", "", "", "", None, None, None, true);
        assert!(result.is_ok());
    }
}

/// Mutation result status classification
#[derive(Debug, Clone, PartialEq)]
pub enum MutationStatus {
    Success(String),      // "success", "new", "updated", "deleted"
    Noop(String),         // "noop:reason" - no changes made
    Error(String),        // "failed:reason" - actual error
}

impl MutationStatus {
    /// Parse status string into enum with minimal taxonomy
    ///
    /// # Status Categories
    ///
    /// ## Success (no colon)
    /// - "success", "created", "updated", "deleted"
    ///
    /// ## Error (colon-separated)
    /// - "failed:", "unauthorized:", "forbidden:", "not_found:", "conflict:", "timeout:"
    ///
    /// ## Noop (colon-separated, success with no changes)
    /// - "noop:"
    ///
    /// # Case Insensitivity
    /// All status strings are matched case-insensitively.
    ///
    /// # Examples
    /// ```
    /// assert!(MutationStatus::from_str("success").is_success());
    /// assert!(MutationStatus::from_str("failed:validation").is_error());
    /// assert!(MutationStatus::from_str("noop:unchanged").is_noop());
    /// assert!(MutationStatus::from_str("CONFLICT:duplicate").is_error());
    /// ```
    pub fn from_str(status: &str) -> Self {
        let status_lower = status.to_lowercase();

        // ERROR PREFIXES - Return Error type
        if status_lower.starts_with("failed:")
            || status_lower.starts_with("unauthorized:")
            || status_lower.starts_with("forbidden:")
            || status_lower.starts_with("not_found:")
            || status_lower.starts_with("conflict:")
            || status_lower.starts_with("timeout:")
        {
            // Extract reason after first colon
            let colon_pos = status.find(':').unwrap_or(status.len());
            let reason = if colon_pos < status.len() - 1 {
                &status[colon_pos + 1..]
            } else {
                ""
            };
            MutationStatus::Error(reason.to_string())
        }
        // NOOP PREFIX - Return Noop (success with no changes)
        else if status_lower.starts_with("noop:") {
            let colon_pos = status.find(':').unwrap_or(status.len());
            let reason = if colon_pos < status.len() - 1 {
                &status[colon_pos + 1..]
            } else {
                ""
            };
            MutationStatus::Noop(reason.to_string())
        }
        // SUCCESS KEYWORDS - Return Success
        else if matches!(
            status_lower.as_str(),
            "success" | "created" | "updated" | "deleted"
        ) {
            MutationStatus::Success(status.to_string())
        }
        // DEFAULT - Unknown statuses become Success (backward compatibility)
        else {
            // Note: In production, this should log a warning
            MutationStatus::Success(status.to_string())
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, MutationStatus::Success(_))
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, MutationStatus::Noop(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, MutationStatus::Error(_))
    }

    /// Map status to HTTP code
    pub fn http_code(&self) -> i32 {
        match self {
            MutationStatus::Success(_) => 200,
            MutationStatus::Noop(_) => 200,  // Noop is success (no change made)
            MutationStatus::Error(reason) => {
                // Map error reasons to HTTP status codes
                let reason_lower = reason.to_lowercase();
                if reason_lower.contains("not_found") || reason_lower.contains("missing") {
                    404
                } else if reason_lower.contains("unauthorized") || reason_lower.contains("unauthenticated") {
                    401
                } else if reason_lower.contains("forbidden") || reason_lower.contains("permission") {
                    403
                } else if reason_lower.contains("conflict") || reason_lower.contains("duplicate") {
                    409
                } else if reason_lower.contains("validation") || reason_lower.contains("invalid") {
                    422
                } else if reason_lower.contains("timeout") {
                    408
                } else {
                    500  // Generic internal error
                }
            }
        }
    }
}

/// Parsed mutation result from PostgreSQL
///
/// Supports TWO formats:
/// 1. Simple: Just entity JSONB (detected by absence of "status" field)
/// 2. Full v2: Complete mutation_result_v2 with status, message, entity, etc.
#[derive(Debug, Clone)]
pub struct MutationResult {
    pub status: MutationStatus,
    pub message: String,
    pub entity_id: Option<String>,
    pub entity_type: Option<String>,
    pub entity: Option<Value>,
    pub updated_fields: Option<Vec<String>>,
    pub cascade: Option<Value>,
    pub metadata: Option<Value>,
    /// True if this was parsed from simple JSONB format (no status field)
    pub is_simple_format: bool,
}

/// Valid mutation status prefixes/values for format detection
const VALID_STATUS_PREFIXES: &[&str] = &[
    // Success keywords (no colon)
    "success", "created", "updated", "deleted",
    // Error prefixes
    "failed:", "unauthorized:", "forbidden:", "not_found:", "conflict:", "timeout:",
    // Noop prefix
    "noop:",
];

impl MutationResult {
    /// Check if a status string is a valid mutation status
    /// (vs. a data field that happens to be named "status")
    fn is_valid_mutation_status(status: &str) -> bool {
        VALID_STATUS_PREFIXES.iter().any(|prefix| {
            status == *prefix || status.starts_with(prefix)
        })
    }

    /// Check if JSON is simple format (entity only, no mutation status)
    pub fn is_simple_format_json(json_str: &str) -> bool {
        let v: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => return false,
        };

        // Arrays are always simple format
        if v.is_array() {
            return true;
        }

        // Check if has a valid mutation status field
        match v.get("status").and_then(|s| s.as_str()) {
            Some(status) => !Self::is_valid_mutation_status(status),
            None => true, // No status field = simple format
        }
    }

    /// Parse from PostgreSQL JSON string with smart format detection
    ///
    /// # Arguments
    /// * `json_str` - Raw JSON from PostgreSQL
    /// * `default_entity_type` - Entity type to use for simple format (e.g., "User")
    pub fn from_json(json_str: &str, default_entity_type: Option<&str>) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Invalid JSON: {}", e))?;

        Self::from_value(&v, default_entity_type)
    }

    /// Parse from serde_json Value with smart format detection
    pub fn from_value(v: &Value, default_entity_type: Option<&str>) -> Result<Self, String> {
        // SMART DETECTION: Check if this is simple format
        let is_simple = match v.get("status").and_then(|s| s.as_str()) {
            Some(status) => !Self::is_valid_mutation_status(status),
            None => true, // No status = simple format
        };

        if is_simple || v.is_array() {
            // SIMPLE FORMAT: Treat entire JSON as entity, assume success
            // Extract '_cascade' field from simple format (note underscore prefix)
            let cascade = v.get("_cascade").filter(|c| !c.is_null()).cloned();

            return Ok(MutationResult {
                status: MutationStatus::Success("success".to_string()),
                message: "Success".to_string(),
                entity_id: v.get("id").and_then(|id| id.as_str()).map(String::from),
                entity_type: default_entity_type.map(String::from),
                entity: Some(v.clone()),
                updated_fields: None,
                cascade,
                metadata: None,
                is_simple_format: true,
            });
        }

        // FULL V2 FORMAT: Parse all fields
        let status_str = v.get("status")
            .and_then(|s| s.as_str())
            .ok_or("Missing 'status' field")?;

        let message = v.get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        let entity_id = v.get("entity_id")
            .and_then(|id| id.as_str())
            .map(String::from);

        // Use entity_type from JSON, fall back to default
        let entity_type = v.get("entity_type")
            .and_then(|t| t.as_str())
            .map(String::from)
            .or_else(|| default_entity_type.map(String::from));

        let entity = v.get("entity")
            .filter(|e| !e.is_null())
            .cloned();

        let updated_fields = v.get("updated_fields")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let cascade = v.get("cascade").filter(|c| !c.is_null()).cloned();
        let metadata = v.get("metadata").filter(|m| !m.is_null()).cloned();

        Ok(MutationResult {
            status: MutationStatus::from_str(status_str),
            message,
            entity_id,
            entity_type,
            entity,
            updated_fields,
            cascade,
            metadata,
            is_simple_format: false,
        })
    }

    /// Get errors array from metadata
    pub fn errors(&self) -> Option<&Vec<Value>> {
        self.metadata.as_ref()
            .and_then(|m| m.get("errors"))
            .and_then(|e| e.as_array())
    }
}

/// Build success response object
fn build_success_object(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
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
        let transformed = transform_entity(entity, entity_type, auto_camel_case);

        // Use provided field name or derive from type
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

        obj.insert(field_name, transformed);
    }

    // Add updatedFields (convert to camelCase)
    if let Some(fields) = &result.updated_fields {
        let transformed_fields: Vec<Value> = fields.iter()
            .map(|f| json!(if auto_camel_case { to_camel_case(f) } else { f.clone() }))
            .collect();
        obj.insert("updatedFields".to_string(), json!(transformed_fields));
    }

    // Add cascade if present
    if let Some(cascade) = &result.cascade {
        obj.insert("cascade".to_string(), cascade.clone());
    }

    Ok(Value::Object(obj))
}

/// Build error response object
fn build_error_object(
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
        MutationStatus::Noop(reason) => format!("noop:{}", reason),
        MutationStatus::Error(reason) => format!("failed:{}", reason),
        MutationStatus::Success(s) => s.clone(),
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
        let auto_error = json!({
            "field": null,
            "code": match &result.status {
                MutationStatus::Noop(r) => r.clone(),
                MutationStatus::Error(r) => r.clone(),
                _ => "unknown".to_string(),
            },
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

// ============================================================================
// Tests for STATUS TAXONOMY (Phase 2: GREEN)
// ============================================================================

#[cfg(test)]
mod test_status_taxonomy {
    use super::*;

    // SUCCESS KEYWORDS (no colon)
    #[test]
    fn test_success_keywords() {
        assert!(MutationStatus::from_str("success").is_success());
        assert!(MutationStatus::from_str("created").is_success());
        assert!(MutationStatus::from_str("updated").is_success());
        assert!(MutationStatus::from_str("deleted").is_success());
    }

    // ERROR PREFIXES (colon-separated)
    #[test]
    fn test_failed_prefix() {
        let status = MutationStatus::from_str("failed:validation");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(reason) => assert_eq!(reason, "validation"),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_unauthorized_prefix() {
        let status = MutationStatus::from_str("unauthorized:token_expired");
        assert!(status.is_error());
    }

    #[test]
    fn test_forbidden_prefix() {
        let status = MutationStatus::from_str("forbidden:insufficient_permissions");
        assert!(status.is_error());
    }

    #[test]
    fn test_not_found_prefix() {
        let status = MutationStatus::from_str("not_found:user_missing");
        assert!(status.is_error());
    }

    #[test]
    fn test_conflict_prefix() {
        let status = MutationStatus::from_str("conflict:duplicate_email");
        assert!(status.is_error());
    }

    #[test]
    fn test_timeout_prefix() {
        let status = MutationStatus::from_str("timeout:database_query");
        assert!(status.is_error());
    }

    // NOOP PREFIX (success with no changes)
    #[test]
    fn test_noop_prefix() {
        let status = MutationStatus::from_str("noop:unchanged");
        assert!(status.is_noop());
        match status {
            MutationStatus::Noop(reason) => assert_eq!(reason, "unchanged"),
            _ => panic!("Expected Noop variant"),
        }
    }

    #[test]
    fn test_noop_duplicate() {
        let status = MutationStatus::from_str("noop:duplicate");
        assert!(status.is_noop());
    }

    // CASE INSENSITIVITY
    #[test]
    fn test_case_insensitive_error_prefix() {
        assert!(MutationStatus::from_str("FAILED:validation").is_error());
        assert!(MutationStatus::from_str("Unauthorized:token").is_error());
        assert!(MutationStatus::from_str("Conflict:DUPLICATE").is_error());
    }

    #[test]
    fn test_case_insensitive_success() {
        assert!(MutationStatus::from_str("SUCCESS").is_success());
        assert!(MutationStatus::from_str("Created").is_success());
    }

    // EDGE CASES
    #[test]
    fn test_status_with_multiple_colons() {
        let status = MutationStatus::from_str("failed:validation:email_invalid");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(reason) => assert_eq!(reason, "validation:email_invalid"),
            _ => panic!("Expected Error with full reason"),
        }
    }

    #[test]
    fn test_error_prefix_without_reason() {
        let status = MutationStatus::from_str("failed:");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(reason) => assert_eq!(reason, ""),
            _ => panic!("Expected Error with empty reason"),
        }
    }

    #[test]
    fn test_unknown_status_becomes_success() {
        // Unknown statuses default to success for backward compatibility
        let status = MutationStatus::from_str("unknown_status");
        assert!(status.is_success());
    }

    #[test]
    fn test_empty_status() {
        let status = MutationStatus::from_str("");
        assert!(status.is_success());
    }
}
