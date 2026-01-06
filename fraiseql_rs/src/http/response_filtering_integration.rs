//! Integration module for response filtering in the HTTP layer
//!
//! This module provides utilities to integrate GraphQL response filtering
//! into the HTTP request/response pipeline, particularly for cached responses.
//!
//! **SECURITY CRITICAL**: This prevents unauthorized field exposure from
//! APQ cached responses when field selections differ between requests.

use crate::http::response_filter::{extract_selections, filter_response_by_selection};
use serde_json::Value;

/// Configuration for response filtering behavior
#[derive(Debug, Clone)]
pub struct ResponseFilteringConfig {
    /// Enable response filtering for all responses
    pub enabled: bool,

    /// Only filter cached responses (recommended for performance)
    pub filter_cached_only: bool,

    /// Enable selection parsing for APQ queries
    pub filter_apq_responses: bool,
}

impl Default for ResponseFilteringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            filter_cached_only: false, // For Phase 2: filter all responses for defense-in-depth
            filter_apq_responses: true,
        }
    }
}

/// Filter a GraphQL response based on the query's field selection
///
/// # Arguments
///
/// * `response_data` - The GraphQL response data to filter
/// * `query` - The GraphQL query string
/// * `operation_name` - Optional operation name (for multi-operation documents)
///
/// # Returns
///
/// Filtered response data, or original response if filtering fails or is disabled
pub fn filter_graphql_response(
    response_data: &Value,
    query: &str,
    operation_name: Option<&str>,
) -> Value {
    // Extract selections from query
    let selections = extract_selections(query, operation_name);

    if selections.is_empty() {
        // Parsing failed or no selections - return response as-is
        return response_data.clone();
    }

    // Filter response by selection
    filter_response_by_selection(response_data, &selections)
}

/// Filter a response object containing "data" and optional "errors"
///
/// **SECURITY CRITICAL**: Use this for full GraphQL responses (with errors field)
///
/// # Arguments
///
/// * `response` - Full GraphQL response with "data" and optional "errors" keys
/// * `query` - The GraphQL query string
/// * `operation_name` - Optional operation name
///
/// # Returns
///
/// Filtered response with only requested fields in the data section
pub fn filter_complete_graphql_response(
    response: &Value,
    query: &str,
    operation_name: Option<&str>,
) -> Value {
    // Extract data field
    let data = match response.get("data") {
        Some(d) => d,
        None => return response.clone(),
    };

    // Filter the data field
    let filtered_data = filter_graphql_response(data, query, operation_name);

    // Reconstruct response with filtered data and preserve other fields
    let mut result = response.clone();
    result["data"] = filtered_data;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_graphql_response() {
        let response = json!({
            "id": 1,
            "name": "John",
            "email": "john@example.com",
            "secret": "should-be-hidden"
        });

        let query = "{ id name }";
        let filtered = filter_graphql_response(&response, query, None);

        let obj = filtered.as_object().unwrap();
        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("name"));
        assert!(!obj.contains_key("email"));
        assert!(!obj.contains_key("secret"));
    }

    #[test]
    fn test_filter_complete_response() {
        let response = json!({
            "data": {
                "user": {
                    "id": 1,
                    "name": "John",
                    "salary": 100_000
                }
            },
            "errors": null
        });

        let query = "{ user { id name } }";
        let filtered = filter_complete_graphql_response(&response, query, None);

        let data = filtered["data"].as_object().unwrap();
        let user = data["user"].as_object().unwrap();

        assert!(user.contains_key("id"));
        assert!(user.contains_key("name"));
        assert!(!user.contains_key("salary"));

        // Preserve other response fields
        assert!(filtered.get("errors").is_some());
    }

    #[test]
    fn test_filter_preserves_errors_field() {
        let response = json!({
            "data": null,
            "errors": [
                {
                    "message": "Field error",
                    "path": ["user"]
                }
            ]
        });

        let query = "{ user { id } }";
        let filtered = filter_complete_graphql_response(&response, query, None);

        assert!(filtered["data"].is_null());
        assert!(filtered["errors"].is_array());
    }

    #[test]
    fn test_empty_selections_returns_original() {
        let response = json!({
            "id": 1,
            "name": "John"
        });

        // Invalid query that won't parse
        let query = "invalid query";
        let filtered = filter_graphql_response(&response, query, None);

        assert_eq!(filtered, response);
    }

    #[test]
    fn test_configuration_defaults() {
        let config = ResponseFilteringConfig::default();
        assert!(config.enabled);
        assert!(!config.filter_cached_only);
        assert!(config.filter_apq_responses);
    }
}
