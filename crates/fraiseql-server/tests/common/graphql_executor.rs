//! Test GraphQL executor - minimal implementation for testing
//!
//! This is a simplified GraphQL executor for testing purposes.
//! It provides basic query execution against in-memory test data.

use serde_json::{json, Value};
use std::collections::HashMap;

/// Simple GraphQL executor for test purposes
#[derive(Debug, Clone)]
pub struct TestGraphQLExecutor {
    /// In-memory test data
    data: HashMap<String, Value>,
}

impl TestGraphQLExecutor {
    /// Create a new test executor
    pub fn new() -> Self {
        let mut data = HashMap::new();

        // Add test users
        data.insert(
            "users".to_string(),
            json!([
                {
                    "id": "user-1",
                    "name": "Alice Smith",
                    "email": "alice@example.com",
                    "active": true,
                    "posts": [
                        {
                            "id": "post-1",
                            "title": "Introduction to GraphQL",
                            "published": true
                        }
                    ]
                },
                {
                    "id": "user-2",
                    "name": "Bob Jones",
                    "email": "bob@example.com",
                    "active": true,
                    "posts": []
                },
                {
                    "id": "user-3",
                    "name": "Charlie Brown",
                    "email": "charlie@example.com",
                    "active": false,
                    "posts": [
                        {
                            "id": "post-2",
                            "title": "Database Design",
                            "published": true
                        }
                    ]
                }
            ]),
        );

        // Add test posts
        data.insert(
            "posts".to_string(),
            json!([
                {
                    "id": "post-1",
                    "title": "Introduction to GraphQL",
                    "content": "GraphQL is a query language...",
                    "published": true,
                    "author": {
                        "id": "user-1",
                        "name": "Alice Smith"
                    }
                },
                {
                    "id": "post-2",
                    "title": "Database Design",
                    "content": "JSONB is powerful...",
                    "published": true,
                    "author": {
                        "id": "user-3",
                        "name": "Charlie Brown"
                    }
                },
                {
                    "id": "post-3",
                    "title": "Draft Post",
                    "content": "This is a draft...",
                    "published": false,
                    "author": {
                        "id": "user-2",
                        "name": "Bob Jones"
                    }
                }
            ]),
        );

        Self { data }
    }

    /// Execute a simple GraphQL query against test data
    ///
    /// Supports basic queries like:
    /// - `{ users { id name } }`
    /// - `{ posts { id title published } }`
    /// - `{ users { id name posts { id title } } }`
    pub fn execute(&self, query: &str) -> Result<Value, String> {
        // Simple parsing for common test patterns
        if query.contains("{ users") {
            return self.execute_users_query(query);
        }

        if query.contains("{ posts") {
            return self.execute_posts_query(query);
        }

        // For any other query, return a generic error
        Err(format!("Unsupported query pattern: {}", query))
    }

    /// Execute a users query
    fn execute_users_query(&self, query: &str) -> Result<Value, String> {
        let users = self.data.get("users").ok_or("Users data not found")?;

        // Extract requested fields
        let fields = self.extract_fields(query);

        if fields.is_empty() {
            return Err("No fields requested".to_string());
        }

        // Filter users data to only requested fields
        if let Value::Array(users_arr) = users {
            let filtered_users: Vec<Value> = users_arr
                .iter()
                .map(|user| self.filter_fields(user, &fields))
                .collect();

            Ok(json!({ "users": filtered_users }))
        } else {
            Err("Users data is not an array".to_string())
        }
    }

    /// Execute a posts query
    fn execute_posts_query(&self, query: &str) -> Result<Value, String> {
        let posts = self.data.get("posts").ok_or("Posts data not found")?;

        // Extract requested fields
        let fields = self.extract_fields(query);

        if fields.is_empty() {
            return Err("No fields requested".to_string());
        }

        // Filter posts data to only requested fields
        if let Value::Array(posts_arr) = posts {
            let filtered_posts: Vec<Value> = posts_arr
                .iter()
                .map(|post| self.filter_fields(post, &fields))
                .collect();

            Ok(json!({ "posts": filtered_posts }))
        } else {
            Err("Posts data is not an array".to_string())
        }
    }

    /// Extract field names from a GraphQL query
    fn extract_fields(&self, query: &str) -> Vec<String> {
        let mut fields = Vec::new();

        // Simple extraction: look for field names between the first and last braces
        let start = query.find('{').unwrap_or(0);
        let end = query.rfind('}').unwrap_or(query.len());
        let content = &query[start + 1..end];

        // Parse field names, handling nested structures
        let mut i = 0;
        let chars: Vec<char> = content.chars().collect();

        while i < chars.len() {
            let c = chars[i];

            // Skip whitespace
            if c.is_whitespace() {
                i += 1;
                continue;
            }

            // Skip nested braces
            if c == '{' || c == '}' {
                i += 1;
                continue;
            }

            // Extract field name
            let mut field = String::new();
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '{' && chars[i] != '}' {
                field.push(chars[i]);
                i += 1;
            }

            // Add field if it's not a root type name or id
            if !field.is_empty()
                && field != "users"
                && field != "posts"
                && field != "id"
                && field != "comments"
            {
                if !fields.contains(&field) {
                    fields.push(field);
                }
            }
        }

        fields
    }

    /// Filter a value to only include specified fields
    fn filter_fields(&self, value: &Value, fields: &[String]) -> Value {
        if let Value::Object(map) = value {
            let mut result = serde_json::Map::new();

            // Always include id if it exists
            if let Some(id) = map.get("id") {
                result.insert("id".to_string(), id.clone());
            }

            // Include all requested fields
            for field in fields {
                if let Some(field_value) = map.get(field) {
                    result.insert(field.clone(), field_value.clone());
                }
            }

            // If no fields specified except id, include all fields from original
            if fields.is_empty() {
                for (key, val) in map.iter() {
                    result.insert(key.clone(), val.clone());
                }
            }

            Value::Object(result)
        } else {
            value.clone()
        }
    }
}

impl Default for TestGraphQLExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = TestGraphQLExecutor::new();
        assert!(!executor.data.is_empty());
    }

    #[test]
    fn test_users_query() {
        let executor = TestGraphQLExecutor::new();
        let query = "{ users { id name } }";
        let result = executor.execute(query);
        assert!(result.is_ok());
    }

    #[test]
    fn test_posts_query() {
        let executor = TestGraphQLExecutor::new();
        let query = "{ posts { id title } }";
        let result = executor.execute(query);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unsupported_query() {
        let executor = TestGraphQLExecutor::new();
        let query = "{ comments { id } }";
        let result = executor.execute(query);
        assert!(result.is_err());
    }
}
