//! Entity Field Filtering
//!
//! Filters entity objects based on GraphQL field selections to reduce payload size
//! and respect GraphQL query semantics for nested entity fields.
//!
//! Related to GitHub issue #525.

use serde_json::{Map, Value};

/// Filter entity fields based on GraphQL selections
///
/// Recursively filters entity objects to include only fields that were selected
/// in the GraphQL query. This reduces response payload size and ensures mutations
/// behave consistently with queries.
///
/// # Arguments
///
/// * `entity` - The entity value to filter (can be object, array, primitive, or null)
/// * `selections` - JSON object describing field selections with structure:
///   ```json
///   {
///     "fields": ["id", "name", "address"],
///     "address": {
///       "fields": ["id", "city"]
///     }
///   }
///   ```
///
/// # Returns
///
/// Filtered entity with only selected fields. Returns original entity for:
/// - Null selections (backward compatibility)
/// - Empty field arrays (GraphQL default behavior)
/// - Non-object entities (arrays, primitives, null)
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use fraiseql_rs::mutation::filter_entity_fields;
///
/// let entity = json!({
///     "id": "loc-123",
///     "name": "Warehouse A",
///     "level": "floor-1",
///     "has_elevator": true,
/// });
///
/// let selections = json!({
///     "fields": ["id", "name"]
/// });
///
/// let filtered = filter_entity_fields(&entity, &selections);
/// assert_eq!(filtered["id"], "loc-123");
/// assert_eq!(filtered["name"], "Warehouse A");
/// assert!(filtered.get("level").is_none());
/// ```
pub fn filter_entity_fields(entity: &Value, selections: &Value) -> Value {
    // Handle null or missing selections - return entity unchanged (backward compat)
    if selections.is_null() {
        return entity.clone();
    }

    // Only filter object entities
    let Value::Object(entity_map) = entity else {
        // Arrays, primitives, null - return unchanged
        return entity.clone();
    };

    // Extract fields array from selections
    let Some(selections_obj) = selections.as_object() else {
        // Invalid selections format - return entity unchanged
        return entity.clone();
    };

    let Some(fields_value) = selections_obj.get("fields") else {
        // No fields specified - return all
        return entity.clone();
    };

    let Some(fields) = fields_value.as_array() else {
        // Invalid fields format - return all
        return entity.clone();
    };

    // Empty fields array - GraphQL default behavior is to return all fields
    if fields.is_empty() {
        return entity.clone();
    }

    // Build filtered object with only selected fields
    let mut filtered = Map::new();

    for field_value in fields {
        let Some(field_name) = field_value.as_str() else {
            continue; // Skip non-string field names
        };

        // Get field value from entity
        let Some(field_val) = entity_map.get(field_name) else {
            continue; // Skip fields not in entity (silently ignore)
        };

        // Check if this field has nested selections
        if let Some(nested_selections) = selections_obj.get(field_name) {
            // Recursively filter nested object
            let filtered_nested = filter_entity_fields(field_val, nested_selections);
            filtered.insert(field_name.to_string(), filtered_nested);
        } else {
            // Leaf field - include as-is
            filtered.insert(field_name.to_string(), field_val.clone());
        }
    }

    Value::Object(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_simple_fields() {
        let entity = json!({
            "id": "123",
            "name": "Test",
            "extra": "Should be filtered",
        });

        let selections = json!({
            "fields": ["id", "name"]
        });

        let result = filter_entity_fields(&entity, &selections);

        assert_eq!(result["id"], "123");
        assert_eq!(result["name"], "Test");
        assert!(result.get("extra").is_none());
    }

    #[test]
    fn test_filter_nested_fields() {
        let entity = json!({
            "id": "123",
            "address": {
                "id": "addr-1",
                "city": "Paris",
                "postal_code": "75001",
            }
        });

        let selections = json!({
            "fields": ["id", "address"],
            "address": {
                "fields": ["id", "city"]
            }
        });

        let result = filter_entity_fields(&entity, &selections);

        assert_eq!(result["id"], "123");
        assert_eq!(result["address"]["id"], "addr-1");
        assert_eq!(result["address"]["city"], "Paris");
        assert!(result["address"].get("postal_code").is_none());
    }

    #[test]
    fn test_null_selections_returns_all() {
        let entity = json!({"id": "123", "name": "Test"});
        let selections = Value::Null;

        let result = filter_entity_fields(&entity, &selections);

        assert_eq!(result["id"], "123");
        assert_eq!(result["name"], "Test");
    }

    #[test]
    fn test_empty_fields_returns_all() {
        let entity = json!({"id": "123", "name": "Test"});
        let selections = json!({"fields": []});

        let result = filter_entity_fields(&entity, &selections);

        assert_eq!(result["id"], "123");
        assert_eq!(result["name"], "Test");
    }

    #[test]
    fn test_non_object_entity_unchanged() {
        // Array
        let entity = json!([1, 2, 3]);
        let selections = json!({"fields": ["id"]});
        let result = filter_entity_fields(&entity, &selections);
        assert!(result.is_array());

        // Primitive
        let entity = json!("string");
        let result = filter_entity_fields(&entity, &selections);
        assert_eq!(result, "string");

        // Null
        let entity = Value::Null;
        let result = filter_entity_fields(&entity, &selections);
        assert!(result.is_null());
    }

    #[test]
    fn test_deeply_nested() {
        let entity = json!({
            "id": "1",
            "a": {
                "id": "2",
                "b": {
                    "id": "3",
                    "c": "value",
                }
            }
        });

        let selections = json!({
            "fields": ["a"],
            "a": {
                "fields": ["b"],
                "b": {
                    "fields": ["c"]
                }
            }
        });

        let result = filter_entity_fields(&entity, &selections);

        assert!(result.get("id").is_none());
        assert_eq!(result["a"]["b"]["c"], "value");
        assert!(result["a"].get("id").is_none());
        assert!(result["a"]["b"].get("id").is_none());
    }
}
