//! Helper functions for bulk operations.
//!
//! Contains utility functions for filter detection, entity extraction, and response headers.

use axum::http::{HeaderMap, HeaderValue};

/// Primary-key values from a bulk filter query's result envelope.
///
/// The envelope is `{"data": {"<key>": [ {..}, .. ]}}`; rows carry only the projected
/// primary key. Rows without the id field are skipped rather than defaulted — a row we
/// cannot identify is a row we must not mutate.
pub(super) fn extract_ids(result: &serde_json::Value, id_field: &str) -> Vec<serde_json::Value> {
    result
        .get("data")
        .and_then(serde_json::Value::as_object)
        .and_then(|o| o.values().next())
        .and_then(serde_json::Value::as_array)
        .map(|rows| rows.iter().filter_map(|r| r.get(id_field).cloned()).collect())
        .unwrap_or_default()
}

/// Extract entity data from a mutation result value.
pub(super) fn extract_entity_from_result(result: &serde_json::Value) -> Option<serde_json::Value> {
    let data = result.get("data")?;

    // Get the first field in the data object (mutation name)
    let mutation_result = data.as_object()?.values().next()?;

    // Try nested entity format first
    if let Some(entity) = mutation_result.get("entity") {
        if entity.is_null() {
            return None;
        }
        let mut cleaned = entity.clone();
        if let Some(obj) = cleaned.as_object_mut() {
            obj.remove("__typename");
        }
        return Some(cleaned);
    }

    // Executor format: fields + __typename at top level
    if mutation_result.is_object() && !mutation_result.as_object()?.is_empty() {
        let mut cleaned = mutation_result.clone();
        if let Some(obj) = cleaned.as_object_mut() {
            obj.remove("__typename");
        }
        if cleaned.as_object().is_some_and(serde_json::Map::is_empty) {
            return None;
        }
        return Some(cleaned);
    }

    None
}

/// Set `X-Rows-Affected` header.
pub(super) fn set_rows_affected(headers: &mut HeaderMap, count: u64) {
    if let Ok(val) = HeaderValue::from_str(&count.to_string()) {
        headers.insert("x-rows-affected", val);
    }
}
