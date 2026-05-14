//! Helper functions for bulk operations.
//!
//! Contains utility functions for filter detection, entity extraction, and response headers.

use axum::http::{HeaderMap, HeaderValue};

/// Check if query parameters contain at least one filter.
pub(super) fn has_filter_params(query_params: &[(&str, &str)]) -> bool {
    // Reserved non-filter params
    const NON_FILTER: &[&str] = &[
        "select", "sort", "limit", "offset", "first", "after", "last", "before", "filter",
    ];

    query_params.iter().any(|(key, _)| {
        let base_key = key.split('[').next().unwrap_or(key);
        // "filter" IS a filter param (JSON DSL), others with brackets are bracket operators
        if *key == "filter" {
            return true;
        }
        // Bracket operators like name[eq]=foo are filters
        if key.contains('[') {
            return true;
        }
        // Simple value params that aren't reserved are implicit eq filters
        !NON_FILTER.contains(&base_key)
    })
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
