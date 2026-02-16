//! Test fixtures and sample data
//!
//! Provides pre-built test data structures for common testing scenarios.

use serde_json::json;

/// Create a sample user fixture
#[must_use]
pub fn sample_user() -> serde_json::Value {
    json!({
        "id": "user_123",
        "name": "Test User",
        "email": "test@example.com",
        "created_at": "2024-01-01T00:00:00Z"
    })
}

/// Create a sample GraphQL query response
#[must_use]
pub fn sample_query_response() -> serde_json::Value {
    json!({
        "data": {
            "user": {
                "id": "123",
                "name": "John Doe",
                "email": "john@example.com"
            }
        }
    })
}

/// Create a sample error response
#[must_use]
pub fn sample_error_response(message: &str) -> serde_json::Value {
    json!({
        "errors": [
            {
                "message": message,
                "extensions": {
                    "code": "INTERNAL_ERROR"
                }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_user() {
        let user = sample_user();
        assert_eq!(user["name"], "Test User");
        assert_eq!(user["email"], "test@example.com");
    }

    #[test]
    fn test_sample_query_response() {
        let response = sample_query_response();
        assert_eq!(response["data"]["user"]["name"], "John Doe");
    }

    #[test]
    fn test_sample_error_response() {
        let error = sample_error_response("Something went wrong");
        assert_eq!(error["errors"][0]["message"], "Something went wrong");
    }
}
