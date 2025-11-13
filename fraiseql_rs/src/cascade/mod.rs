//! GraphQL Cascade field selection and filtering
//!
//! This module provides high-performance filtering of cascade data based on
//! GraphQL field selections. It operates on raw JSONB from PostgreSQL and
//! applies filtering before Python serialization.

use serde_json::{Value, Map};
use std::collections::HashSet;

#[cfg(test)]
mod tests;

/// Cascade field selection metadata from GraphQL query
#[derive(Debug, Clone)]
pub struct CascadeSelections {
    /// Selected fields at cascade root level
    /// e.g., ["updated", "deleted", "invalidations"]
    pub fields: HashSet<String>,

    /// Type filters for 'updated' field
    /// e.g., ["Post", "User"] for include: ["Post", "User"]
    pub updated_include: Option<Vec<String>>,
    pub updated_exclude: Option<Vec<String>>,

    /// Field selections for 'updated' array items
    pub updated_fields: Option<HashSet<String>>,

    /// Field selections for 'updated.entity' objects
    /// Keyed by __typename, contains field names
    pub entity_selections: Map<String, Value>,

    /// Field selections for other cascade fields
    pub deleted_fields: Option<HashSet<String>>,
    pub invalidations_fields: Option<HashSet<String>>,
    pub metadata_fields: Option<HashSet<String>>,
}

impl CascadeSelections {
    /// Parse cascade selections from GraphQL field selection JSON
    ///
    /// Expected JSON format from Python:
    /// ```json
    /// {
    ///   "fields": ["updated", "deleted", "invalidations"],
    ///   "updated": {
    ///     "include": ["Post", "User"],
    ///     "exclude": null,
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
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Invalid cascade selections JSON: {}", e))?;

        // Parse root fields
        let fields = v.get("fields")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Parse updated filters
        let updated_obj = v.get("updated");
        let updated_include = updated_obj
            .and_then(|u| u.get("include"))
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let updated_exclude = updated_obj
            .and_then(|u| u.get("exclude"))
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let updated_fields = updated_obj
            .and_then(|u| u.get("fields"))
            .and_then(|f| Self::parse_field_set_from_value(f));

        // Parse entity selections
        let entity_selections = updated_obj
            .and_then(|u| u.get("entity_selections"))
            .and_then(|e| e.as_object())
            .cloned()
            .unwrap_or_default();

        // Parse deleted, invalidations, metadata fields
        let deleted_fields = Self::parse_field_set(&v, "deleted");
        let invalidations_fields = Self::parse_field_set(&v, "invalidations");
        let metadata_fields = Self::parse_field_set(&v, "metadata");

        Ok(CascadeSelections {
            fields,
            updated_include,
            updated_exclude,
            updated_fields,
            entity_selections,
            deleted_fields,
            invalidations_fields,
            metadata_fields,
        })
    }

    fn parse_field_set(v: &Value, field_name: &str) -> Option<HashSet<String>> {
        v.get(field_name)
            .and_then(|f| f.get("fields"))
            .and_then(Self::parse_field_set_from_value)
    }

    fn parse_field_set_from_value(v: &Value) -> Option<HashSet<String>> {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
    }
}

/// Filter cascade data based on GraphQL field selections
///
/// This is the main entry point called from Python.
///
/// # Arguments
/// * `cascade_json` - Raw JSONB cascade data from PostgreSQL (JSON string)
/// * `selections_json` - Parsed GraphQL field selections (JSON string)
///
/// # Returns
/// Filtered cascade data as JSON string
///
/// # Performance
/// - Zero-copy JSON manipulation where possible
/// - Operates on serde_json::Value for efficiency
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
    let mut cascade: Value = serde_json::from_str(cascade_json)
        .map_err(|e| format!("Invalid cascade JSON: {}", e))?;

    // Parse selections
    let selections = CascadeSelections::from_json(sel_json)?;

    // Filter cascade object
    if let Some(obj) = cascade.as_object_mut() {
        filter_cascade_object(obj, &selections)?;
    }

    // Serialize back to JSON
    serde_json::to_string(&cascade)
        .map_err(|e| format!("Failed to serialize filtered cascade: {}", e))
}

/// Filter the cascade object in place
fn filter_cascade_object(
    obj: &mut Map<String, Value>,
    selections: &CascadeSelections,
) -> Result<(), String> {
    // Remove fields not in selections
    obj.retain(|key, _| selections.fields.contains(key));

    // Filter 'updated' array
    if let Some(updated) = obj.get_mut("updated") {
        filter_updated_array(updated, selections)?;
    }

    // Filter 'deleted' array
    if let Some(deleted) = obj.get_mut("deleted") {
        filter_simple_array(deleted, &selections.deleted_fields)?;
    }

    // Filter 'invalidations' array
    if let Some(invalidations) = obj.get_mut("invalidations") {
        filter_simple_array(invalidations, &selections.invalidations_fields)?;
    }

    // Filter 'metadata' object
    if let Some(metadata) = obj.get_mut("metadata") {
        filter_metadata_object(metadata, &selections.metadata_fields)?;
    }

    Ok(())
}

/// Filter the 'updated' array based on type filters and field selections
fn filter_updated_array(
    updated: &mut Value,
    selections: &CascadeSelections,
) -> Result<(), String> {
    let Some(arr) = updated.as_array_mut() else {
        return Ok(());
    };

    // Apply type filtering (include/exclude)
    if let Some(ref include_types) = selections.updated_include {
        arr.retain(|item| {
            item.get("__typename")
                .and_then(|t| t.as_str())
                .map(|typename| include_types.iter().any(|t| t == typename))
                .unwrap_or(false)
        });
    } else if let Some(ref exclude_types) = selections.updated_exclude {
        arr.retain(|item| {
            item.get("__typename")
                .and_then(|t| t.as_str())
                .map(|typename| !exclude_types.iter().any(|t| t == typename))
                .unwrap_or(true)
        });
    }

    // Apply field selections to each item
    for item in arr.iter_mut() {
        if let Some(obj) = item.as_object_mut() {
            filter_updated_item(obj, selections)?;
        }
    }

    Ok(())
}

/// Filter a single updated item
fn filter_updated_item(
    item: &mut Map<String, Value>,
    selections: &CascadeSelections,
) -> Result<(), String> {
    // Filter item fields if specified
    if let Some(ref fields) = selections.updated_fields {
        item.retain(|key, _| fields.contains(key));
    }

    // Get typename for entity field selection (owned String to avoid borrow conflicts)
    let typename: String = item.get("__typename")
        .and_then(|t| t.as_str())
        .map(String::from)
        .unwrap_or_default();

    // Filter entity object
    if let Some(entity) = item.get_mut("entity") {
        if let Some(entity_obj) = entity.as_object_mut() {
            // Get field selection for this typename
            if let Some(field_selection) = selections.entity_selections.get(&typename) {
                if let Some(fields_arr) = field_selection.as_array() {
                    let fields: HashSet<String> = fields_arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    entity_obj.retain(|key, _| fields.contains(key));
                }
            }
        }
    }

    Ok(())
}

/// Filter a simple array (deleted, invalidations)
fn filter_simple_array(
    arr: &mut Value,
    field_selection: &Option<HashSet<String>>,
) -> Result<(), String> {
    let Some(arr_val) = arr.as_array_mut() else {
        return Ok(());
    };

    let Some(fields) = field_selection else {
        return Ok(());
    };

    for item in arr_val.iter_mut() {
        if let Some(obj) = item.as_object_mut() {
            obj.retain(|key, _| fields.contains(key));
        }
    }

    Ok(())
}

/// Filter metadata object
fn filter_metadata_object(
    metadata: &mut Value,
    field_selection: &Option<HashSet<String>>,
) -> Result<(), String> {
    let Some(obj) = metadata.as_object_mut() else {
        return Ok(());
    };

    let Some(fields) = field_selection else {
        return Ok(());
    };

    obj.retain(|key, _| fields.contains(key));
    Ok(())
}
