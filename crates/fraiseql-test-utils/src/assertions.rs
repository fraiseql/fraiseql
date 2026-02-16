//! Common test assertions for FraiseQL testing
//!
//! Provides helper macros and functions for asserting common test conditions.

/// Assert that a JSON value has a specific key at the given path
///
/// # Example
///
/// ```ignore
/// let json = json!({"user": {"id": 123}});
/// assert_json_key!(&json, "user.id", 123);
/// ```
#[macro_export]
macro_rules! assert_json_key {
    ($value:expr, $key:expr, $expected:expr) => {
        let parts: Vec<&str> = $key.split('.').collect();
        let mut current = $value;

        for part in parts {
            current = &current[part];
        }

        assert_eq!(current, $expected);
    };
}

/// Assert that a response has no errors
///
/// # Example
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
/// # Example
///
/// ```ignore
/// let data = assert_has_data(&response);
/// ```
pub fn assert_has_data(response: &serde_json::Value) -> &serde_json::Value {
    response
        .get("data")
        .expect("Response should have 'data' field")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_no_graphql_errors_success() {
        let response = json!({"data": {"user": {"id": 1}}});
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
}
