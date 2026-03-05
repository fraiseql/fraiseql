//! Common test assertions for FraiseQL testing
//!
//! Provides helper macros and functions for asserting common test conditions.

/// Assert that a JSON value has a specific key at the given path and matches the expected value.
///
/// Fails with a descriptive message if any segment of the path is missing
/// (i.e. returns `null` for the key), rather than silently comparing against `null`.
///
/// # Example
///
/// ```ignore
/// let json = json!({"user": {"id": 123}});
/// assert_json_key!(&json, "user.id", 123);
/// ```
#[macro_export]
macro_rules! assert_json_key {
    ($value:expr, $key:expr, $expected:expr) => {{
        let parts: Vec<&str> = $key.split('.').collect();
        let mut current: &serde_json::Value = $value;

        for part in &parts {
            match current.get(part) {
                Some(v) => current = v,
                None => panic!(
                    "assert_json_key!: key '{}' not found in path '{}'\nJSON was: {}",
                    part,
                    $key,
                    current
                ),
            }
        }

        assert_eq!(current, &serde_json::json!($expected), "at path '{}'", $key);
    }};
}

/// Assert that a response has no errors
///
/// # Example
///
/// # Panics
///
/// Panics if the response contains a non-empty `errors` array.
///
/// ```ignore
/// assert_no_graphql_errors(&response);
/// ```
pub fn assert_no_graphql_errors(response: &serde_json::Value) {
    if let Some(errors) = response.get("errors") {
        let is_valid_empty =
            errors.is_array() && errors.as_array().is_some_and(|arr| arr.is_empty());
        assert!(is_valid_empty, "Expected no GraphQL errors, but got: {}", errors);
    }
}

/// Assert that a response has data
///
/// # Panics
///
/// Panics if the response does not contain a `data` field.
///
/// # Example
///
/// ```ignore
/// let data = assert_has_data(&response);
/// ```
pub fn assert_has_data(response: &serde_json::Value) -> &serde_json::Value {
    response.get("data").expect("Response should have 'data' field")
}

/// Assert that a GraphQL response has no errors (alias for `assert_no_graphql_errors`).
///
/// # Panics
///
/// Panics if the response contains a non-empty `errors` array.
///
/// # Example
///
/// ```ignore
/// assert_graphql_success(&response);
/// ```
pub fn assert_graphql_success(response: &serde_json::Value) {
    assert_no_graphql_errors(response);
}

/// Assert that a GraphQL response contains an error with the given message substring.
///
/// # Panics
///
/// Panics if the response has no errors array or none of the error messages
/// contain the expected substring.
///
/// # Example
///
/// ```ignore
/// assert_graphql_error_contains(&response, "not found");
/// ```
pub fn assert_graphql_error_contains(response: &serde_json::Value, expected: &str) {
    let errors = response
        .get("errors")
        .and_then(|e| e.as_array())
        .expect("Response should have an 'errors' array");

    assert!(
        !errors.is_empty(),
        "Expected at least one GraphQL error containing '{}', but errors array is empty",
        expected
    );

    let found = errors.iter().any(|e| {
        e.get("message")
            .and_then(|m| m.as_str())
            .is_some_and(|m| m.contains(expected))
    });

    assert!(
        found,
        "Expected an error containing '{}', but got: {:?}",
        expected,
        errors.iter().map(|e| e.get("message").and_then(|m| m.as_str()).unwrap_or("")).collect::<Vec<_>>()
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_no_graphql_errors_success() {
        let response = json!({"data": {"user": {"id": 1}}});
        assert_no_graphql_errors(&response);
    }

    #[test]
    fn test_no_graphql_errors_empty_errors_array() {
        let response = json!({"data": {}, "errors": []});
        assert_no_graphql_errors(&response);
    }

    #[test]
    #[should_panic(expected = "Expected no GraphQL errors")]
    fn test_no_graphql_errors_fails() {
        let response = json!({"errors": [{"message": "error"}]});
        assert_no_graphql_errors(&response);
    }

    #[test]
    fn test_has_data_success() {
        let response = json!({"data": {"user": {"id": 1}}});
        let data = assert_has_data(&response);
        assert_eq!(data["user"]["id"], 1);
    }

    #[test]
    #[should_panic(expected = "should have 'data' field")]
    fn test_has_data_fails() {
        let response = json!({"errors": [{"message": "error"}]});
        assert_has_data(&response);
    }

    #[test]
    fn test_graphql_success_passes_on_no_errors() {
        let response = json!({"data": {"users": [{"id": 1}]}});
        assert_graphql_success(&response);
    }

    #[test]
    #[should_panic(expected = "Expected no GraphQL errors")]
    fn test_graphql_success_fails_on_errors() {
        let response = json!({"errors": [{"message": "field not found"}]});
        assert_graphql_success(&response);
    }

    #[test]
    fn test_graphql_error_contains_match() {
        let response = json!({"errors": [{"message": "Field 'id' not found"}]});
        assert_graphql_error_contains(&response, "not found");
    }

    #[test]
    fn test_graphql_error_contains_partial_match() {
        let response = json!({"errors": [
            {"message": "Rate limit exceeded"},
            {"message": "Retry after 60s"}
        ]});
        assert_graphql_error_contains(&response, "Rate limit");
    }

    #[test]
    #[should_panic(expected = "Expected an error containing")]
    fn test_graphql_error_contains_no_match() {
        let response = json!({"errors": [{"message": "Database error"}]});
        assert_graphql_error_contains(&response, "not found");
    }

    #[test]
    #[should_panic(expected = "errors' array")]
    fn test_graphql_error_contains_no_errors_key() {
        let response = json!({"data": {}});
        assert_graphql_error_contains(&response, "error");
    }

    #[test]
    #[should_panic(expected = "errors array is empty")]
    fn test_graphql_error_contains_empty_errors() {
        let response = json!({"errors": []});
        assert_graphql_error_contains(&response, "error");
    }
}
