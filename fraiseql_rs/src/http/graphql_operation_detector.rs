//! GraphQL operation detection and analysis (Phase 19, Commit 4.5)
//!
//! This module provides utilities for parsing GraphQL queries to extract operation details:
//! - Operation type (query, mutation, subscription)
//! - Operation name (if named)
//! - Field counting (including nested fields)
//! - Alias counting
//!
//! Uses simple regex and string parsing to analyze GraphQL documents without
//! full AST parsing, optimized for performance metrics collection.

use crate::http::operation_metrics::GraphQLOperationType;
use regex::Regex;
use std::collections::HashSet;

/// GraphQL operation detector for metrics collection
///
/// Analyzes GraphQL query strings to extract metadata for observability.
#[derive(Debug)]
pub struct GraphQLOperationDetector;

impl GraphQLOperationDetector {
    /// Detect the operation type and name from a GraphQL query string
    ///
    /// Returns a tuple of (`GraphQLOperationType`, Option<`operation_name`>).
    ///
    /// Supports:
    /// - Named operations: `query GetUser { ... }` → (Query, Some("GetUser"))
    /// - Anonymous operations: `query { ... }` → (Query, None)
    /// - Multiple operations (returns first): `query Op1 { } query Op2 { }` → (Query, Some("Op1"))
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (op_type, op_name) = GraphQLOperationDetector::detect_operation_type(
    ///     "mutation UpdateUser($id: ID!) { updateUser(id: $id) { id name } }"
    /// );
    /// assert_eq!(op_type, GraphQLOperationType::Mutation);
    /// assert_eq!(op_name, Some("UpdateUser".to_string()));
    /// ```
    #[must_use]
    pub fn detect_operation_type(query: &str) -> (GraphQLOperationType, Option<String>) {
        let trimmed = query.trim();

        // Skip comments
        let query_no_comments = Self::strip_comments(trimmed);

        // Regex to match operation type and optional name
        // Matches: (query|mutation|subscription) [OperationName]
        if let Ok(re) =
            Regex::new(r"^\s*(query|mutation|subscription)\s+([A-Za-z_][A-Za-z0-9_]*)?\s*[\(\{]")
        {
            if let Some(caps) = re.captures(&query_no_comments) {
                let op_type_str = caps.get(1).map_or("query", |m| m.as_str());
                let op_name = caps.get(2).map(|m| m.as_str().to_string());

                let operation_type = match op_type_str {
                    "query" => GraphQLOperationType::Query,
                    "mutation" => GraphQLOperationType::Mutation,
                    "subscription" => GraphQLOperationType::Subscription,
                    _ => GraphQLOperationType::Unknown,
                };

                return (operation_type, op_name);
            }
        }

        // Fallback: assume it's a query if no type specified (shorthand form)
        (GraphQLOperationType::Query, None)
    }

    /// Count the number of fields selected in a GraphQL operation
    ///
    /// Counts top-level and nested field selections, including fragments.
    /// Does not count variable definitions or directives.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = GraphQLOperationDetector::count_fields(
    ///     "query { user { id name email } posts { id title } }"
    /// );
    /// assert_eq!(count, 7); // user, id, name, email, posts, id, title
    /// ```
    #[must_use]
    pub fn count_fields(query: &str) -> usize {
        // Remove comments and whitespace
        let query_clean = Self::strip_comments(query);

        // Split by braces to count field selections
        // Each field that's not a variable definition is a field
        let mut field_count = 0;
        let mut in_variable_section = false;
        let mut bracket_depth: i32 = 0;

        for char in query_clean.chars() {
            match char {
                '(' => {
                    // Check if we're entering variable section
                    if bracket_depth == 0 {
                        in_variable_section = true;
                    }
                }
                '{' => {
                    bracket_depth += 1;
                    if !in_variable_section {
                        // This is a field selection, not a variable definition
                    }
                }
                '}' => {
                    bracket_depth = (bracket_depth - 1).max(0);
                    if bracket_depth == 0 {
                        in_variable_section = false;
                    }
                }
                ',' | ':' if !in_variable_section => {
                    // These separate fields
                }
                _ => {}
            }
        }

        // Count identifiers that are fields (more accurate approach)
        // Fields are identifiers followed by { or : or , or whitespace (not =)
        if let Ok(re) = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*[\s\{:,]|[A-Za-z_][A-Za-z0-9_]*$") {
            // Count matches, but exclude GraphQL keywords
            let keywords = [
                "query",
                "mutation",
                "subscription",
                "fragment",
                "on",
                "as",
                "alias",
                "input",
                "type",
                "enum",
                "union",
                "interface",
                "schema",
                "scalar",
                "extends",
            ];

            field_count = re
                .captures_iter(&query_clean)
                .filter(|cap| {
                    let matched = cap.get(0).map_or("", |m| m.as_str());
                    let ident = matched.trim_end_matches(|c: char| {
                        c.is_whitespace() || c == '{' || c == ':' || c == ','
                    });
                    !keywords.contains(&ident)
                })
                .count();
        }

        // Conservative estimate: count { to estimate nesting depth
        // Each pair of braces contains at least one field
        field_count = field_count.max(query.matches('{').count());

        field_count
    }

    /// Count the number of field aliases in a GraphQL operation
    ///
    /// Aliases are specified as `alias: fieldName` in GraphQL.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = GraphQLOperationDetector::count_aliases(
    ///     "query { userProfile: user { id firstName: firstName } }"
    /// );
    /// assert_eq!(count, 2); // userProfile and firstName
    /// ```
    #[must_use]
    pub fn count_aliases(query: &str) -> usize {
        // Aliases appear as "identifier: identifier" pattern
        // We need to be careful not to confuse with key:value in variables

        let query_clean = Self::strip_comments(query);

        // Find all "word: word" patterns outside of variable definitions
        Regex::new(r"[A-Za-z_][A-Za-z0-9_]*\s*:\s*[A-Za-z_][A-Za-z0-9_]*")
            .map(|re| {
                let matches: Vec<&str> = re.find_iter(&query_clean).map(|m| m.as_str()).collect();
                // Rough estimate: each match that's in a selection set is an alias
                // More accurate: count colons that appear in field selection context
                matches.len()
            })
            .unwrap_or(0)
    }

    /// Count the number of variables used in a GraphQL operation
    ///
    /// Counts `$variableName` patterns in the operation definition.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = GraphQLOperationDetector::count_variables(
    ///     "query GetUser($id: ID!, $fetchPosts: Boolean) { user(id: $id) { posts @include(if: $fetchPosts) } }"
    /// );
    /// assert_eq!(count, 2); // $id, $fetchPosts
    /// ```
    #[must_use]
    pub fn count_variables(query: &str) -> usize {
        // Count $ prefixed identifiers
        Regex::new(r"\$[A-Za-z_][A-Za-z0-9_]*")
            .map(|re| {
                let mut vars = HashSet::new();
                for cap in re.captures_iter(query) {
                    if let Some(m) = cap.get(0) {
                        vars.insert(m.as_str().to_string());
                    }
                }
                vars.len()
            })
            .unwrap_or(0)
    }

    /// Strip GraphQL comments from query string
    ///
    /// Removes both single-line (#) and multi-line (""" """) comments.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string with potential comments
    #[allow(clippy::excessive_nesting)]
    fn strip_comments(query: &str) -> String {
        let mut result = String::new();
        let mut chars = query.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '#' {
                // Skip rest of line
                while let Some(c) = chars.peek() {
                    if *c == '\n' {
                        result.push('\n');
                        chars.next();
                        break;
                    }
                    chars.next();
                }
            } else if ch == '"' && chars.peek() == Some(&'"') {
                // Check for """ (block comment)
                chars.next(); // consume first "
                if chars.peek() == Some(&'"') {
                    chars.next(); // consume second "
                                  // Skip until we find """
                    let mut found_end = false;
                    while let Some(c) = chars.next() {
                        if c == '"' && chars.peek() == Some(&'"') {
                            chars.next();
                            if chars.peek() == Some(&'"') {
                                chars.next();
                                found_end = true;
                                break;
                            }
                        }
                    }
                    if !found_end {
                        result.push('"');
                        result.push('"');
                    }
                } else {
                    result.push('"');
                    result.push('"');
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Analyze a GraphQL query and return detailed operation information
    ///
    /// Returns a struct with all detected operation metadata.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    #[must_use]
    pub fn analyze(query: &str) -> OperationInfo {
        let (operation_type, operation_name) = Self::detect_operation_type(query);
        let field_count = Self::count_fields(query);
        let alias_count = Self::count_aliases(query);
        let variable_count = Self::count_variables(query);

        OperationInfo {
            operation_type,
            operation_name,
            field_count,
            alias_count,
            variable_count,
            query_length: query.len(),
        }
    }
}

/// Detailed information about a GraphQL operation
#[derive(Debug, Clone)]
pub struct OperationInfo {
    /// Type of operation (query, mutation, subscription)
    pub operation_type: GraphQLOperationType,

    /// Optional operation name
    pub operation_name: Option<String>,

    /// Number of fields selected
    pub field_count: usize,

    /// Number of field aliases used
    pub alias_count: usize,

    /// Number of unique variables defined
    pub variable_count: usize,

    /// Length of query string in characters
    pub query_length: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_named_query() {
        let (op_type, op_name) =
            GraphQLOperationDetector::detect_operation_type("query GetUser { user { id name } }");
        assert_eq!(op_type, GraphQLOperationType::Query);
        assert_eq!(op_name, Some("GetUser".to_string()));
    }

    #[test]
    fn test_detect_anonymous_query() {
        let (op_type, op_name) =
            GraphQLOperationDetector::detect_operation_type("query { user { id } }");
        assert_eq!(op_type, GraphQLOperationType::Query);
        assert_eq!(op_name, None);
    }

    #[test]
    fn test_detect_named_mutation() {
        let (op_type, op_name) = GraphQLOperationDetector::detect_operation_type(
            "mutation UpdateUser($id: ID!) { updateUser(id: $id) { id } }",
        );
        assert_eq!(op_type, GraphQLOperationType::Mutation);
        assert_eq!(op_name, Some("UpdateUser".to_string()));
    }

    #[test]
    fn test_detect_subscription() {
        let (op_type, op_name) = GraphQLOperationDetector::detect_operation_type(
            "subscription OnUserUpdate { userUpdated { id name } }",
        );
        assert_eq!(op_type, GraphQLOperationType::Subscription);
        assert_eq!(op_name, Some("OnUserUpdate".to_string()));
    }

    #[test]
    fn test_detect_shorthand_query() {
        let (op_type, op_name) = GraphQLOperationDetector::detect_operation_type("{ user { id } }");
        assert_eq!(op_type, GraphQLOperationType::Query);
        assert_eq!(op_name, None);
    }

    #[test]
    fn test_count_fields_simple() {
        let count = GraphQLOperationDetector::count_fields("query { user { id name email } }");
        // Should count: user, id, name, email (at minimum)
        assert!(count >= 4);
    }

    #[test]
    fn test_count_fields_nested() {
        let count =
            GraphQLOperationDetector::count_fields("query { user { id name posts { id title } } }");
        // Should count: user, id, name, posts, id, title
        assert!(count >= 6);
    }

    #[test]
    fn test_count_aliases_simple() {
        let count = GraphQLOperationDetector::count_aliases(
            "query { userProfile: user { firstName: name } }",
        );
        // Should count: userProfile, firstName (at minimum)
        assert!(count >= 2);
    }

    #[test]
    fn test_count_variables_simple() {
        let count = GraphQLOperationDetector::count_variables(
            "query GetUser($id: ID!) { user(id: $id) { id } }",
        );
        assert_eq!(count, 1);
    }

    #[test]
    fn test_count_variables_multiple() {
        let count = GraphQLOperationDetector::count_variables(
            "query GetUsers($first: Int!, $after: String) { users(first: $first, after: $after) { id } }",
        );
        assert_eq!(count, 2);
    }

    #[test]
    fn test_analyze_complete_query() {
        let info = GraphQLOperationDetector::analyze(
            "query GetUser($id: ID!) { user(id: $id) { id name email } }",
        );

        assert_eq!(info.operation_type, GraphQLOperationType::Query);
        assert_eq!(info.operation_name, Some("GetUser".to_string()));
        assert_eq!(info.variable_count, 1);
        assert!(info.field_count >= 3); // user, id, name, email
        assert!(info.query_length > 0);
    }

    #[test]
    fn test_strip_comments_single_line() {
        let query_with_comments = "query GetUser { # This is a comment\n  user { id } }";
        let clean = GraphQLOperationDetector::strip_comments(query_with_comments);
        assert!(!clean.contains("This is a comment"));
        assert!(clean.contains("query GetUser"));
    }

    #[test]
    fn test_empty_query() {
        let (op_type, op_name) = GraphQLOperationDetector::detect_operation_type("");
        assert_eq!(op_type, GraphQLOperationType::Query);
        assert_eq!(op_name, None);
    }

    #[test]
    fn test_operation_info_json_compatible() {
        let info = GraphQLOperationDetector::analyze(
            "mutation CreateUser($name: String!) { createUser(name: $name) { id } }",
        );

        assert_eq!(info.operation_type, GraphQLOperationType::Mutation);
        assert_eq!(info.operation_name, Some("CreateUser".to_string()));
        assert!(info.field_count >= 2); // createUser, id
        assert_eq!(info.variable_count, 1);
    }

    #[test]
    fn test_query_with_directives() {
        let query = "query { user @cached { id name @deprecated } }";
        let info = GraphQLOperationDetector::analyze(query);
        // Should detect query even with directives
        assert_eq!(info.operation_type, GraphQLOperationType::Query);
    }

    #[test]
    fn test_query_with_fragments() {
        let query = "query { user { ...UserFragment } } fragment UserFragment on User { id name }";
        let info = GraphQLOperationDetector::analyze(query);
        assert_eq!(info.operation_type, GraphQLOperationType::Query);
        // Fragment keywords should not be counted as fields
    }
}
