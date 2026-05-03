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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // has_filter_params tests
    // -----------------------------------------------------------------------

    #[test]
    fn no_filter_params_empty() {
        assert!(!has_filter_params(&[]));
    }

    #[test]
    fn no_filter_only_reserved() {
        let params = vec![
            ("select", "id,name"),
            ("sort", "-name"),
            ("limit", "10"),
            ("offset", "0"),
        ];
        assert!(!has_filter_params(&params));
    }

    #[test]
    fn filter_bracket_operator() {
        let params = vec![("status[eq]", "inactive")];
        assert!(has_filter_params(&params));
    }

    #[test]
    fn filter_json_dsl() {
        let params = vec![("filter", r#"{"status":{"eq":"inactive"}}"#)];
        assert!(has_filter_params(&params));
    }

    #[test]
    fn filter_simple_value() {
        // Simple value param that isn't reserved → implicit eq
        let params = vec![("status", "inactive")];
        assert!(has_filter_params(&params));
    }

    #[test]
    fn filter_mixed_with_reserved() {
        let params = vec![("limit", "10"), ("status[eq]", "inactive")];
        assert!(has_filter_params(&params));
    }

    // -----------------------------------------------------------------------
    // extract_entity_from_result tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_entity_nested_format() {
        let result: serde_json::Value =
            serde_json::from_str(r#"{"data":{"createUser":{"entity":{"id":1,"name":"Alice"}}}}"#)
                .unwrap();
        let entity = extract_entity_from_result(&result).unwrap();
        assert_eq!(entity["id"], 1);
        assert_eq!(entity["name"], "Alice");
    }

    #[test]
    fn extract_entity_executor_format() {
        let result: serde_json::Value = serde_json::from_str(
            r#"{"data":{"createUser":{"pk_user_id":1,"name":"Alice","__typename":"User"}}}"#,
        )
        .unwrap();
        let entity = extract_entity_from_result(&result).unwrap();
        assert_eq!(entity["pk_user_id"], 1);
        assert!(entity.get("__typename").is_none());
    }

    #[test]
    fn extract_entity_null() {
        let result: serde_json::Value =
            serde_json::from_str(r#"{"data":{"createUser":{"entity":null}}}"#).unwrap();
        assert!(extract_entity_from_result(&result).is_none());
    }

    #[test]
    fn extract_entity_null_value() {
        assert!(extract_entity_from_result(&serde_json::Value::Null).is_none());
    }

    // -----------------------------------------------------------------------
    // X-Rows-Affected header tests
    // -----------------------------------------------------------------------

    #[test]
    fn rows_affected_header() {
        let mut headers = HeaderMap::new();
        set_rows_affected(&mut headers, 42);
        assert_eq!(headers.get("x-rows-affected").unwrap().to_str().unwrap(), "42");
    }

    #[test]
    fn rows_affected_zero() {
        let mut headers = HeaderMap::new();
        set_rows_affected(&mut headers, 0);
        assert_eq!(headers.get("x-rows-affected").unwrap().to_str().unwrap(), "0");
    }
}
