//! GraphQL Cascade field selection and filtering
//!
//! This module provides high-performance filtering of cascade data based on
//! GraphQL field selections. It operates on raw JSONB from `PostgreSQL` and
//! applies filtering before Python serialization.

use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashSet;

fn deserialize_fields_as_hashset<'de, D>(deserializer: D) -> Result<HashSet<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let vec: Vec<String> = Deserialize::deserialize(deserializer)?;
    Ok(vec.into_iter().collect())
}

#[cfg(test)]
mod tests;

/// Cascade field selection metadata from GraphQL query
#[derive(Debug, Deserialize)]
pub struct CascadeSelections {
    /// Top-level fields selected in cascade
    #[serde(deserialize_with = "deserialize_fields_as_hashset")]
    pub fields: HashSet<String>,
    /// Selections for updated entities
    #[serde(default)]
    pub updated: Option<FieldSelections>,
    /// Selections for deleted entities
    #[serde(default)]
    pub deleted: Option<FieldSelections>,
    /// Selections for invalidations
    #[serde(default)]
    pub invalidations: Option<FieldSelections>,
    /// Selections for metadata
    #[serde(default)]
    pub metadata: Option<FieldSelections>,
}

/// Field selections for cascade operations
#[derive(Debug, Deserialize)]
pub struct FieldSelections {
    /// List of fields to select
    pub fields: Vec<String>,
    /// Entity-specific field selections
    #[serde(default)]
    pub entity_selections: Option<EntitySelections>,
}

/// Entity-specific field selections by type
#[derive(Debug, Deserialize)]
pub struct EntitySelections {
    /// Map from entity type name to list of selected fields
    #[serde(flatten)]
    pub type_selections: std::collections::HashMap<String, Vec<String>>,
}

impl CascadeSelections {
    /// Parse cascade selections from GraphQL field selection JSON
    ///
    /// Expected JSON format from Python:
    /// ```json
    /// {
    ///   "fields": ["updated", "deleted", "invalidations"],
    ///   "updated": {
    ///     "fields": ["__typename", "id", "operation", "entity"],
    ///     "entity_selections": {
    ///       "Post": ["id", "title", "content"],
    ///       "User": ["id", "name", "postCount"]
    ///     }
    ///   },
    ///   "deleted": {
    ///     "fields": ["__typename", "id"]
    ///   }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - JSON string is not valid JSON syntax
    /// - JSON structure doesn't match expected cascade selections format
    /// - Required fields are missing or have wrong types
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        serde_json::from_str(json_str).map_err(|e| format!("Invalid cascade selections JSON: {e}"))
    }
}

/// Filter cascade value based on GraphQL field selections
///
/// This function operates on `serde_json::Value` for cases where
/// you already have parsed JSON and want to avoid serialize/deserialize overhead.
///
/// # Errors
///
/// Returns an error if:
/// - Cascade value is not a JSON object
/// - Field filtering fails for updated/deleted/invalidations/metadata fields
pub fn filter_cascade_by_selections(
    cascade: &Value,
    selections: &CascadeSelections,
    auto_camel_case: bool,
) -> Result<Value, String> {
    if selections.fields.is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    let Value::Object(cascade_obj) = cascade else {
        return Err("CASCADE must be an object".to_string());
    };

    let mut filtered = Map::with_capacity(selections.fields.len());

    for field_name in &selections.fields {
        let key = convert_field_name(field_name, auto_camel_case);

        if let Some(value) = cascade_obj.get(&key) {
            let filtered_value = match field_name.as_str() {
                "updated" => filter_updated_field(value, selections.updated.as_ref()),
                "deleted" => filter_simple_field(value, selections.deleted.as_ref()),
                "invalidations" => filter_simple_field(value, selections.invalidations.as_ref()),
                "metadata" => filter_simple_field(value, selections.metadata.as_ref()),
                _ => value.clone(),
            };

            filtered.insert(key, filtered_value);
        }
    }

    Ok(Value::Object(filtered))
}

fn convert_field_name(field_name: &str, auto_camel_case: bool) -> String {
    if !auto_camel_case {
        return field_name.to_string();
    }

    let mut result = String::new();
    let mut capitalize_next = false;

    for (i, ch) in field_name.chars().enumerate() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else if i == 0 {
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }

    result
}

/// Filter cascade data based on GraphQL field selections
///
/// This is the main entry point called from Python.
///
/// # Arguments
/// * `cascade_json` - Raw JSONB cascade data from `PostgreSQL` (JSON string)
/// * `selections_json` - Parsed GraphQL field selections (JSON string)
///
/// # Returns
/// Filtered cascade data as JSON string
///
/// # Errors
///
/// Returns an error if:
/// - Cascade JSON is not valid JSON syntax
/// - Selections JSON is invalid or malformed
/// - Cascade object filtering fails
/// - Filtered cascade cannot be serialized back to JSON
///
/// # Performance
/// - Zero-copy JSON manipulation where possible
/// - Operates on `serde_json::Value` for efficiency
/// - Target: < 0.5ms for typical cascade payloads
pub fn filter_cascade_data(
    cascade_json: &str,
    selections_json: Option<&str>,
) -> Result<String, String> {
    // If no selections provided, return original cascade
    let Some(sel_json) = selections_json else {
        return Ok(cascade_json.to_string());
    };

    // Parse cascade data
    let mut cascade: Value =
        serde_json::from_str(cascade_json).map_err(|e| format!("Invalid cascade JSON: {e}"))?;

    // Parse selections
    let selections = CascadeSelections::from_json(sel_json)?;

    // Filter cascade object
    if let Some(obj) = cascade.as_object_mut() {
        filter_cascade_object(obj, &selections);
    }

    // Serialize back to JSON
    serde_json::to_string(&cascade)
        .map_err(|e| format!("Failed to serialize filtered cascade: {e}"))
}

/// Filter the cascade object in place
fn filter_cascade_object(obj: &mut Map<String, Value>, selections: &CascadeSelections) {
    // Remove fields not in selections
    obj.retain(|key, _| selections.fields.contains(key));

    // Filter each selected field
    for field_name in &selections.fields {
        if let Some(value) = obj.get_mut(field_name) {
            let filtered_value = match field_name.as_str() {
                "updated" => filter_updated_field(value, selections.updated.as_ref()),
                "deleted" => filter_simple_field(value, selections.deleted.as_ref()),
                "invalidations" => filter_simple_field(value, selections.invalidations.as_ref()),
                "metadata" => filter_simple_field(value, selections.metadata.as_ref()),
                _ => continue, // No filtering needed for unknown fields
            };
            *value = filtered_value;
        }
    }
}

fn filter_updated_field(value: &Value, field_selections: Option<&FieldSelections>) -> Value {
    let Some(selections) = field_selections else {
        return value.clone();
    };

    if let Value::Array(entities) = value {
        let filtered_entities: Vec<Value> = entities
            .iter()
            .map(|entity| filter_entity_fields(entity, &selections.fields))
            .collect();

        Value::Array(filtered_entities)
    } else {
        value.clone()
    }
}

fn filter_simple_field(value: &Value, field_selections: Option<&FieldSelections>) -> Value {
    let Some(selections) = field_selections else {
        return value.clone();
    };

    if let Value::Array(items) = value {
        let filtered_items: Vec<Value> = items
            .iter()
            .map(|item| filter_object_fields(item, &selections.fields))
            .collect();

        Value::Array(filtered_items)
    } else if let Value::Object(_) = value {
        filter_object_fields(value, &selections.fields)
    } else {
        value.clone()
    }
}

fn filter_entity_fields(entity: &Value, fields: &[String]) -> Value {
    let Value::Object(entity_obj) = entity else {
        return entity.clone();
    };

    let mut filtered = Map::new();

    for field in fields {
        if let Some(value) = entity_obj.get(field) {
            filtered.insert(field.clone(), value.clone());
        }
    }

    if !filtered.contains_key("__typename") {
        if let Some(typename) = entity_obj.get("__typename") {
            filtered.insert("__typename".to_string(), typename.clone());
        }
    }

    Value::Object(filtered)
}

fn filter_object_fields(obj: &Value, fields: &[String]) -> Value {
    let Value::Object(obj_map) = obj else {
        return obj.clone();
    };

    let mut filtered = Map::new();

    for field in fields {
        if let Some(value) = obj_map.get(field) {
            filtered.insert(field.clone(), value.clone());
        }
    }

    Value::Object(filtered)
}
