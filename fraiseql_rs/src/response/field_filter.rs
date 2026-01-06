#![allow(clippy::excessive_nesting)]
//! Unified field filtering across all response types
//!
//! This module provides a centralized interface for filtering GraphQL responses
//! to include only the fields requested by the client. This prevents unauthorized
//! field exposure from cached responses and ensures data consistency across
//! all response paths (regular GraphQL, APQ, subscriptions).
//!
//! **SECURITY CRITICAL**: Response filtering prevents unauthorized field exposure
//! when APQ caching is enabled. A cached response for a query might contain more fields
//! than the client requested if the cache key doesn't include the selection set.
//!
//! # Example Vulnerability Without Filtering
//! ```text
//! Authorized Query:   { user { id name } }      → cached with full response
//! Attacker Query:     { user { id name salary } } → receives cached response with salary!
//! ```
//!
//! # Solution: Unified Field Filtering
//! Filter all responses (cached or fresh) to only include fields in the query's selection set.

use serde_json::{json, Map, Value};
use std::fmt;

/// Error type for field filtering operations
#[derive(Debug, Clone)]
pub enum FilterError {
    /// Query parsing failed
    QueryParseError(String),
    /// Invalid filter configuration
    InvalidConfiguration(String),
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueryParseError(msg) => write!(f, "Query parse error: {msg}"),
            Self::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {msg}"),
        }
    }
}

impl std::error::Error for FilterError {}

/// Represents a GraphQL field selection for filtering
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSelection {
    /// Field name
    pub name: String,
    /// Alias if present (for response key)
    pub alias: Option<String>,
    /// Nested selections (for object fields)
    pub selections: Vec<FieldSelection>,
}

impl FieldSelection {
    /// Get the response key (alias or field name)
    #[must_use]
    pub fn response_key(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    /// Get the field name
    #[must_use]
    pub fn field_name(&self) -> &str {
        &self.name
    }
}

/// Unified field filtering interface
#[derive(Debug)]
pub struct FieldFilter;

impl FieldFilter {
    /// Filter response to only include requested fields
    ///
    /// # Arguments
    ///
    /// * `response_data` - The response object to filter
    /// * `query` - The GraphQL query string
    ///
    /// # Returns
    ///
    /// Filtered response object with only requested fields, or error if parsing fails
    ///
    /// # Errors
    ///
    /// Returns `FilterError::QueryParseError` if the query cannot be parsed.
    pub fn filter_response(response_data: &Value, query: &str) -> Result<Value, FilterError> {
        let selections = extract_selections(query)?;
        Ok(Self::filter_by_selections(response_data, &selections))
    }

    /// Filter response using pre-parsed selections
    ///
    /// # Arguments
    ///
    /// * `response_data` - The response object to filter
    /// * `selections` - Pre-parsed field selections
    ///
    /// # Returns
    ///
    /// Filtered response object with only requested fields
    #[must_use]
    pub fn filter_by_selections(response_data: &Value, selections: &[FieldSelection]) -> Value {
        if selections.is_empty() {
            // No selections specified, return empty object (safest approach)
            return json!({});
        }

        match response_data {
            Value::Object(obj) => {
                let mut filtered = Map::new();

                for selection in selections {
                    if let Some(field_value) = obj
                        .get(selection.field_name())
                        .or_else(|| obj.get(selection.response_key()))
                    {
                        let response_key = selection.response_key().to_string();

                        if selection.selections.is_empty() {
                            // Scalar field - copy directly
                            filtered.insert(response_key, field_value.clone());
                        } else {
                            // Object field with nested selections - recurse
                            let filtered_value =
                                Self::filter_by_selections(field_value, &selection.selections);
                            filtered.insert(response_key, filtered_value);
                        }
                    }
                }

                Value::Object(filtered)
            }
            Value::Array(arr) => {
                // Filter each item in the array
                Value::Array(
                    arr.iter()
                        .map(|item| Self::filter_by_selections(item, selections))
                        .collect(),
                )
            }
            // Scalar values pass through unchanged
            other => other.clone(),
        }
    }

    /// Check if response would be empty after filtering
    ///
    /// # Arguments
    ///
    /// * `response_data` - The response object to check
    /// * `selections` - Field selections to apply
    ///
    /// # Returns
    ///
    /// True if filtering would result in empty response
    #[must_use]
    pub fn is_empty_after_filtering(response_data: &Value, selections: &[FieldSelection]) -> bool {
        let filtered = Self::filter_by_selections(response_data, selections);
        match filtered {
            Value::Object(obj) => obj.is_empty(),
            Value::Array(arr) => arr.is_empty(),
            _ => false,
        }
    }
}

/// Extract field selections from a GraphQL query string
///
/// # Arguments
///
/// * `query` - GraphQL query string
///
/// # Returns
///
/// Vector of top-level field selections
fn extract_selections(query: &str) -> Result<Vec<FieldSelection>, FilterError> {
    parse_query(query).map_err(FilterError::QueryParseError)
}

/// Simple GraphQL query parser
fn parse_query(query: &str) -> Result<Vec<FieldSelection>, String> {
    let trimmed = query.trim();

    // Find the first opening brace
    let start = trimmed.find('{').ok_or("No opening brace found")?;
    let content = &trimmed[start + 1..];

    // Find matching closing brace
    let selections = parse_selection_set(content)?;
    Ok(selections)
}

/// Parse a GraphQL selection set
fn parse_selection_set(input: &str) -> Result<Vec<FieldSelection>, String> {
    let mut selections = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            '}' => {
                // End of selection set
                break;
            }
            '{' | '(' | '@' | '!' | ':' | ',' | ' ' | '\n' | '\t' | '\r' => {
                // Skip whitespace and special chars
                chars.next();
            }
            '#' => {
                // Comment - skip to end of line
                chars.next();
                while chars.peek().is_some_and(|&c| c != '\n') {
                    chars.next();
                }
            }
            '$' => {
                // Variable - skip it
                chars.next();
                while chars
                    .peek()
                    .is_some_and(|&c| c.is_alphanumeric() || c == '_')
                {
                    chars.next();
                }
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                // Field name or fragment
                let field = parse_field(&mut chars)?;
                selections.push(field);
            }
            _ => {
                // Unknown character, skip it
                chars.next();
            }
        }
    }

    Ok(selections)
}

/// Parse a single field from the selection set
fn parse_field(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<FieldSelection, String> {
    let mut name = String::new();

    // Read field name
    while chars
        .peek()
        .is_some_and(|&c| c.is_alphanumeric() || c == '_')
    {
        #[allow(clippy::unwrap_used)]
        name.push(chars.next().unwrap());
    }

    // Skip whitespace
    skip_whitespace(chars);

    // Check for alias (colon after field name)
    let alias = if chars.peek() == Some(&':') {
        chars.next(); // consume ':'
        skip_whitespace(chars);
        let mut alias_name = String::new();
        while chars
            .peek()
            .is_some_and(|&c| c.is_alphanumeric() || c == '_')
        {
            #[allow(clippy::unwrap_used)]
            alias_name.push(chars.next().unwrap());
        }
        skip_whitespace(chars);
        Some(alias_name)
    } else {
        None
    };

    // Check for arguments
    if chars.peek() == Some(&'(') {
        skip_to_next_significant(chars, ')');
        skip_whitespace(chars);
    }

    // Check for directives (@skip, @include, etc.)
    while chars.peek() == Some(&'@') {
        chars.next(); // consume '@'
                      // Skip directive name
        while chars
            .peek()
            .is_some_and(|&c| c.is_alphanumeric() || c == '_')
        {
            chars.next();
        }
        // Skip directive arguments if present
        if chars.peek() == Some(&'(') {
            skip_to_next_significant(chars, ')');
        }
        skip_whitespace(chars);
    }

    // Check for nested selections
    let selections = if chars.peek() == Some(&'{') {
        chars.next(); // consume '{'
        parse_selection_set_until_close(chars)?
    } else {
        Vec::new()
    };

    Ok(FieldSelection {
        name,
        alias,
        selections,
    })
}

/// Parse selections until we find the closing brace
fn parse_selection_set_until_close(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<Vec<FieldSelection>, String> {
    let mut selections = Vec::new();

    loop {
        skip_whitespace(chars);

        match chars.peek() {
            None => return Err("Unexpected end of input".to_string()),
            Some(&'}') => {
                chars.next(); // consume '}'
                break;
            }
            Some(&'#') => {
                // Comment
                while chars.peek().is_some_and(|&c| c != '\n') {
                    chars.next();
                }
            }
            Some(&'$') => {
                // Variable fragment spread - skip
                chars.next();
                while chars
                    .peek()
                    .is_some_and(|&c| c.is_alphanumeric() || c == '_')
                {
                    chars.next();
                }
            }
            Some(&c) if c.is_alphabetic() || c == '_' => {
                let field = parse_field(chars)?;
                selections.push(field);
            }
            Some(&'.') => {
                // Fragment spread or inline fragment
                chars.next();
                chars.next(); // consume second dot
                chars.next(); // consume third dot
                skip_whitespace(chars);

                // Check if it's an inline fragment (... on Type)
                if chars.peek().is_some_and(|&c| c.is_alphabetic()) {
                    let mut word = String::new();
                    while chars
                        .peek()
                        .is_some_and(|&c| c.is_alphanumeric() || c == '_')
                    {
                        #[allow(clippy::unwrap_used)]
                        word.push(chars.next().unwrap());
                    }

                    // If it's "on", parse inline fragment
                    if word == "on" {
                        skip_whitespace(chars);
                        skip_type_name(chars);
                        skip_whitespace(chars);
                        // Parse nested selections for inline fragment
                        if chars.peek() == Some(&'{') {
                            chars.next(); // consume '{'
                            let nested = parse_selection_set_until_close(chars)?;
                            // Merge nested selections into parent
                            selections.extend(nested);
                        }
                    }
                }
            }
            _ => {
                chars.next(); // skip unknown character
            }
        }
    }

    Ok(selections)
}

/// Skip whitespace and comments
fn skip_whitespace(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek().is_some_and(|&c| c.is_whitespace()) {
        chars.next();
    }
}

/// Skip a GraphQL type name (alphanumeric and underscore characters)
fn skip_type_name(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars
        .peek()
        .is_some_and(|&c| c.is_alphanumeric() || c == '_')
    {
        chars.next();
    }
}

/// Skip to a target character (for arguments)
fn skip_to_next_significant(chars: &mut std::iter::Peekable<std::str::Chars>, _target: char) {
    let mut depth = 1;
    while depth > 0 && chars.peek().is_some() {
        match chars.next() {
            Some('(' | '{' | '[') => depth += 1,
            Some(')' | '}' | ']') if depth > 0 => depth -= 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test Suite 1: Field Selection Parsing
    // ========================================================================

    #[test]
    fn test_extract_simple_fields() {
        let query = "{ user { id name } }";
        let selections = extract_selections(query).unwrap();

        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].name, "user");
        assert_eq!(selections[0].selections.len(), 2);
        assert_eq!(selections[0].selections[0].name, "id");
        assert_eq!(selections[0].selections[1].name, "name");
    }

    #[test]
    fn test_extract_multiple_root_fields() {
        let query = "{ user { id } post { title } }";
        let selections = extract_selections(query).unwrap();

        assert_eq!(selections.len(), 2);
        assert_eq!(selections[0].name, "user");
        assert_eq!(selections[1].name, "post");
    }

    #[test]
    fn test_extract_with_aliases() {
        let query = "{ user: myself { id } }";
        let selections = extract_selections(query).unwrap();

        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].name, "user");
        assert_eq!(selections[0].alias, Some("myself".to_string()));
        assert_eq!(selections[0].response_key(), "myself");
    }

    #[test]
    fn test_extract_deeply_nested() {
        let query = "{ user { profile { avatar { url size } } } }";
        let selections = extract_selections(query).unwrap();

        assert_eq!(selections.len(), 1);
        assert_eq!(
            selections[0].selections[0].selections[0].selections.len(),
            2
        );
    }

    // ========================================================================
    // Test Suite 2: Response Filtering - Simple Cases
    // ========================================================================

    #[test]
    fn test_filter_simple_object() {
        let response = json!({
            "id": 1,
            "name": "John",
            "email": "john@example.com",
            "salary": 100_000
        });

        let selections = vec![
            FieldSelection {
                name: "id".to_string(),
                alias: None,
                selections: Vec::new(),
            },
            FieldSelection {
                name: "name".to_string(),
                alias: None,
                selections: Vec::new(),
            },
        ];

        let filtered = FieldFilter::filter_by_selections(&response, &selections);
        let obj = filtered.as_object().unwrap();

        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("name"));
        assert!(!obj.contains_key("email"));
        assert!(!obj.contains_key("salary"));
    }

    #[test]
    fn test_filter_with_alias() {
        let response = json!({
            "user": {
                "id": 1,
                "name": "John"
            }
        });

        let selections = vec![FieldSelection {
            name: "user".to_string(),
            alias: Some("myself".to_string()),
            selections: vec![FieldSelection {
                name: "id".to_string(),
                alias: None,
                selections: Vec::new(),
            }],
        }];

        let filtered = FieldFilter::filter_by_selections(&response, &selections);
        let obj = filtered.as_object().unwrap();

        // Response key should be the alias
        assert!(obj.contains_key("myself"));
        assert!(!obj.contains_key("user"));
    }

    // ========================================================================
    // Test Suite 3: Nested Field Filtering
    // ========================================================================

    #[test]
    fn test_filter_nested_objects() {
        let response = json!({
            "user": {
                "id": 1,
                "name": "John",
                "email": "john@example.com",
                "profile": {
                    "avatar": "https://example.com/avatar.jpg",
                    "bio": "Secret bio"
                }
            }
        });

        let selections = vec![FieldSelection {
            name: "user".to_string(),
            alias: None,
            selections: vec![
                FieldSelection {
                    name: "id".to_string(),
                    alias: None,
                    selections: Vec::new(),
                },
                FieldSelection {
                    name: "profile".to_string(),
                    alias: None,
                    selections: vec![FieldSelection {
                        name: "avatar".to_string(),
                        alias: None,
                        selections: Vec::new(),
                    }],
                },
            ],
        }];

        let filtered = FieldFilter::filter_by_selections(&response, &selections);
        let user = filtered["user"].as_object().unwrap();

        assert!(user.contains_key("id"));
        assert!(!user.contains_key("name"));
        assert!(!user.contains_key("email"));

        let profile = user["profile"].as_object().unwrap();
        assert!(profile.contains_key("avatar"));
        assert!(!profile.contains_key("bio"));
    }

    // ========================================================================
    // Test Suite 4: Array Filtering
    // ========================================================================

    #[test]
    fn test_filter_array_of_objects() {
        let response = json!([
            {"id": 1, "name": "John", "secret": "s1"},
            {"id": 2, "name": "Jane", "secret": "s2"}
        ]);

        let selections = vec![FieldSelection {
            name: "id".to_string(),
            alias: None,
            selections: Vec::new(),
        }];

        let filtered = FieldFilter::filter_by_selections(&response, &selections);
        let arr = filtered.as_array().unwrap();

        assert_eq!(arr.len(), 2);
        for item in arr {
            let obj = item.as_object().unwrap();
            assert!(obj.contains_key("id"));
            assert!(!obj.contains_key("name"));
            assert!(!obj.contains_key("secret"));
        }
    }

    // ========================================================================
    // Test Suite 5: Full Integration - Query String to Filtered Response
    // ========================================================================

    #[test]
    fn test_filter_response_from_query_string() {
        let response = json!({
            "user": {
                "id": 1,
                "name": "John",
                "email": "john@example.com"
            }
        });

        let query = "{ user { id name } }";
        let filtered = FieldFilter::filter_response(&response, query).unwrap();
        let user = filtered["user"].as_object().unwrap();

        assert!(user.contains_key("id"));
        assert!(user.contains_key("name"));
        assert!(!user.contains_key("email"));
    }

    #[test]
    fn test_filter_apq_response() {
        let cached_response = json!({
            "user": {
                "id": 123,
                "name": "Alice",
                "email": "alice@example.com"
            }
        });

        let query = "{ user { id } }";
        let filtered = FieldFilter::filter_response(&cached_response, query).unwrap();
        let user = filtered["user"].as_object().unwrap();

        assert!(user.contains_key("id"));
        assert!(!user.contains_key("name"));
        assert!(!user.contains_key("email"));
    }

    #[test]
    fn test_filter_subscription_response() {
        let response = json!({
            "subscription": {
                "postAdded": {
                    "id": 1,
                    "title": "New Post",
                    "content": "Secret content",
                    "author": "John"
                }
            }
        });

        let query = "{ postAdded { id title } }";
        let filtered = FieldFilter::filter_response(&response, query).unwrap();
        let post = filtered["postAdded"].as_object().unwrap();

        assert!(post.contains_key("id"));
        assert!(post.contains_key("title"));
        assert!(!post.contains_key("content"));
        assert!(!post.contains_key("author"));
    }

    // ========================================================================
    // Test Suite 6: Security - Preventing Field Exposure
    // ========================================================================

    #[test]
    fn test_security_prevents_unauthorized_field_exposure() {
        // Simulates APQ cache vulnerability
        let cached_response = json!({
            "user": {
                "id": 123,
                "name": "Alice",
                "email": "alice@example.com",
                "salary": 150_000,
                "ssn": "123-45-6789"
            }
        });

        // Client only requested: { user { id name } }
        let client_query = "{ user { id name } }";
        let filtered = FieldFilter::filter_response(&cached_response, client_query).unwrap();
        let user = filtered["user"].as_object().unwrap();

        // CRITICAL: Only requested fields present
        assert!(user.contains_key("id"));
        assert!(user.contains_key("name"));
        assert!(!user.contains_key("email"));
        assert!(!user.contains_key("salary"));
        assert!(!user.contains_key("ssn")); // Secret field NOT exposed
    }

    // ========================================================================
    // Test Suite 7: Empty Response Handling
    // ========================================================================

    #[test]
    fn test_empty_selections_returns_empty_object() {
        let response = json!({"id": 1, "name": "John"});
        let filtered = FieldFilter::filter_by_selections(&response, &[]);

        assert_eq!(filtered, json!({}));
    }

    #[test]
    fn test_is_empty_after_filtering() {
        let response = json!({"id": 1});
        let selections = vec![FieldSelection {
            name: "name".to_string(),
            alias: None,
            selections: Vec::new(),
        }];

        assert!(FieldFilter::is_empty_after_filtering(
            &response,
            &selections
        ));
    }

    // ========================================================================
    // Test Suite 8: Edge Cases
    // ========================================================================

    #[test]
    fn test_filter_non_existent_field() {
        let response = json!({"id": 1, "name": "John"});
        let selections = vec![FieldSelection {
            name: "nonexistent".to_string(),
            alias: None,
            selections: Vec::new(),
        }];

        let filtered = FieldFilter::filter_by_selections(&response, &selections);
        assert_eq!(filtered, json!({}));
    }

    #[test]
    fn test_filter_null_value() {
        let response = json!({"id": 1, "name": null});
        let selections = vec![
            FieldSelection {
                name: "id".to_string(),
                alias: None,
                selections: Vec::new(),
            },
            FieldSelection {
                name: "name".to_string(),
                alias: None,
                selections: Vec::new(),
            },
        ];

        let filtered = FieldFilter::filter_by_selections(&response, &selections);
        let obj = filtered.as_object().unwrap();

        assert_eq!(obj["id"], 1);
        assert!(obj["name"].is_null());
    }

    #[test]
    fn test_filter_complex_nested_structure() {
        let response = json!({
            "user": {
                "id": 1,
                "profile": {
                    "contacts": [
                        {"type": "email", "value": "alice@example.com", "verified": true},
                        {"type": "phone", "value": "555-1234", "verified": false}
                    ]
                }
            }
        });

        let query = "{ user { profile { contacts { type value } } } }";
        let filtered = FieldFilter::filter_response(&response, query).unwrap();
        let contacts = filtered["user"]["profile"]["contacts"].as_array().unwrap();

        for contact in contacts {
            let obj = contact.as_object().unwrap();
            assert!(obj.contains_key("type"));
            assert!(obj.contains_key("value"));
            assert!(!obj.contains_key("verified"));
        }
    }

    // ========================================================================
    // Test Suite 9: Error Handling
    // ========================================================================

    #[test]
    fn test_parse_error_invalid_query() {
        let query = "invalid query without braces";
        let result = extract_selections(query);

        assert!(result.is_err());
        match result {
            Err(FilterError::QueryParseError(_)) => {}
            _ => panic!("Expected QueryParseError"),
        }
    }

    #[test]
    fn test_filter_response_with_invalid_query() {
        let response = json!({"id": 1});
        let query = "no braces here";
        let result = FieldFilter::filter_response(&response, query);

        assert!(result.is_err());
    }
}
