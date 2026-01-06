#![allow(clippy::excessive_nesting)]
//! GraphQL response filtering based on field selection
//!
//! This module provides utilities to parse GraphQL queries and filter response data
//! based on the query's field selection. This ensures that responses only contain
//! the fields that were actually requested by the client.
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
//! # Solution: Response Filtering
//! Filter cached responses to only include fields in the current query's selection set.

use serde_json::{json, Map, Value};

/// Represents a GraphQL field selection for filtering
#[derive(Debug, Clone)]
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
    pub fn response_key(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    /// Get the field name
    pub fn field_name(&self) -> &str {
        &self.name
    }
}

/// Extract field selections from a GraphQL query string
///
/// **SECURITY CRITICAL**: This parser must handle:
/// - Simple fields: `{ user { id } }`
/// - Field aliases: `{ user: myself { id } }`
/// - Nested selections: `{ user { profile { avatar } } }`
/// - Fragments: `{ ...userFields }`
/// - Inline fragments: `{ ... on User { id } }`
///
/// # Arguments
///
/// * `query` - GraphQL query string
/// * `operation_name` - Optional operation name (for multi-operation documents)
///
/// # Returns
///
/// Vector of top-level field selections, or empty vec if parsing fails
pub fn extract_selections(query: &str, _operation_name: Option<&str>) -> Vec<FieldSelection> {
    // Simple recursive descent parser for GraphQL selection sets
    // This is a lightweight parser - for complex queries with fragments,
    // consider using graphql-core-js bindings or graphql-parser crate

    parse_query(query).unwrap_or_default()
}

/// Filter response data to only include fields in the selection set
///
/// **SECURITY CRITICAL**: This prevents unauthorized field exposure from cached responses.
///
/// # Arguments
///
/// * `response_data` - The response object to filter
/// * `selections` - The field selections to apply
///
/// # Returns
///
/// Filtered response object with only requested fields
pub fn filter_response_by_selection(response_data: &Value, selections: &[FieldSelection]) -> Value {
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
                            filter_response_by_selection(field_value, &selection.selections);
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
                    .map(|item| filter_response_by_selection(item, selections))
                    .collect(),
            )
        }
        // Scalar values pass through unchanged
        other => other.clone(),
    }
}

/// Simple GraphQL query parser
///
/// Parses GraphQL selection sets using recursive descent parsing.
/// Supports:
/// - Field names
/// - Field aliases
/// - Nested selections
/// - Fragment spreads (limited)
/// - Inline fragments (limited)
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
    let mut current_pos = 0;

    while let Some(&ch) = chars.peek() {
        match ch {
            '}' => {
                // End of selection set
                break;
            }
            '{' | '(' | '@' | '!' | ':' | ',' | ' ' | '\n' | '\t' | '\r' => {
                // Skip whitespace and special chars
                chars.next();
                current_pos += 1;
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
                let field = parse_field(&mut chars, &mut current_pos)?;
                selections.push(field);
            }
            _ => {
                // Unknown character, skip it
                chars.next();
                current_pos += 1;
            }
        }
    }

    Ok(selections)
}

/// Parse a single field from the selection set
fn parse_field(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    _pos: &mut usize,
) -> Result<FieldSelection, String> {
    let mut name = String::new();

    // Read field name
    while chars
        .peek()
        .is_some_and(|&c| c.is_alphanumeric() || c == '_')
    {
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
                let field = parse_field(chars, &mut 0)?;
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

    #[test]
    fn test_extract_simple_fields() {
        let query = "{ user { id name } }";
        let selections = extract_selections(query, None);

        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].name, "user");
        assert_eq!(selections[0].selections.len(), 2);
        assert_eq!(selections[0].selections[0].name, "id");
        assert_eq!(selections[0].selections[1].name, "name");
    }

    #[test]
    fn test_extract_with_aliases() {
        let query = "{ user: myself { id } }";
        let selections = extract_selections(query, None);

        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].name, "user");
        assert_eq!(selections[0].alias, Some("myself".to_string()));
        assert_eq!(selections[0].response_key(), "myself");
    }

    #[test]
    fn test_extract_multiple_fields() {
        let query = "{ user { id } post { title } }";
        let selections = extract_selections(query, None);

        assert_eq!(selections.len(), 2);
        assert_eq!(selections[0].name, "user");
        assert_eq!(selections[1].name, "post");
    }

    #[test]
    fn test_filter_response_simple() {
        let response = json!({
            "id": 1,
            "name": "John",
            "email": "john@example.com",
            "salary": 100000
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

        let filtered = filter_response_by_selection(&response, &selections);
        let obj = filtered.as_object().unwrap();

        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("name"));
        assert!(!obj.contains_key("email"));
        assert!(!obj.contains_key("salary"));
    }

    #[test]
    fn test_filter_response_with_alias() {
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

        let filtered = filter_response_by_selection(&response, &selections);
        let obj = filtered.as_object().unwrap();

        // Response key should be the alias
        assert!(obj.contains_key("myself"));
        assert!(!obj.contains_key("user"));
        assert!(!obj.get("myself").unwrap()["name"].is_null());
    }

    #[test]
    fn test_filter_response_nested() {
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

        let filtered = filter_response_by_selection(&response, &selections);
        let user = filtered["user"].as_object().unwrap();

        assert!(user.contains_key("id"));
        assert!(!user.contains_key("name"));
        assert!(!user.contains_key("email"));

        let profile = user["profile"].as_object().unwrap();
        assert!(profile.contains_key("avatar"));
        assert!(!profile.contains_key("bio"));
    }

    #[test]
    fn test_filter_response_array() {
        let response = json!([
            {"id": 1, "name": "John", "secret": "s1"},
            {"id": 2, "name": "Jane", "secret": "s2"}
        ]);

        let selections = vec![FieldSelection {
            name: "id".to_string(),
            alias: None,
            selections: Vec::new(),
        }];

        let filtered = filter_response_by_selection(&response, &selections);
        let arr = filtered.as_array().unwrap();

        assert_eq!(arr.len(), 2);
        for item in arr {
            let obj = item.as_object().unwrap();
            assert!(obj.contains_key("id"));
            assert!(!obj.contains_key("name"));
            assert!(!obj.contains_key("secret"));
        }
    }

    #[test]
    fn test_security_prevents_field_exposure() {
        // Simulates the vulnerability: cached response with more fields than requested
        let cached_response = json!({
            "user": {
                "id": 123,
                "name": "Alice",
                "email": "alice@example.com",
                "salary": 150000,
                "ssn": "123-45-6789"
            }
        });

        // Client only requested: { user { id name } }
        let client_selections = vec![FieldSelection {
            name: "user".to_string(),
            alias: None,
            selections: vec![
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
            ],
        }];

        let filtered = filter_response_by_selection(&cached_response, &client_selections);
        let user = filtered["user"].as_object().unwrap();

        // CRITICAL: Only requested fields present
        assert!(user.contains_key("id"));
        assert!(user.contains_key("name"));
        assert!(!user.contains_key("email"));
        assert!(!user.contains_key("salary"));
        assert!(!user.contains_key("ssn")); // Secret field NOT exposed
    }
}
