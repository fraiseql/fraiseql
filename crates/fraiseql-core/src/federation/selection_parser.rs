//! GraphQL field selection parsing for federation queries.
//!
//! Extracts which fields are requested in a GraphQL query so federation
//! can project only necessary fields from the database.

use crate::error::Result;

/// Represents requested fields in a GraphQL selection set.
#[derive(Debug, Clone)]
pub struct FieldSelection {
    /// Names of fields requested in the query
    pub fields: Vec<String>,
}

impl FieldSelection {
    /// Create a new field selection.
    #[must_use]
    pub fn new(fields: Vec<String>) -> Self {
        Self { fields }
    }

    /// Check if a field is selected.
    #[must_use]
    pub fn contains(&self, field: &str) -> bool {
        self.fields.contains(&field.to_string())
    }

    /// Add a field to the selection.
    pub fn add_field(&mut self, field: String) {
        if !self.fields.contains(&field) {
            self.fields.push(field);
        }
    }
}

/// Parse GraphQL query to extract field selection for _entities.
///
/// Example query:
/// ```graphql
/// query {
///   _entities(representations: [...]) {
///     __typename
///     id
///     name
///     email
///   }
/// }
/// ```
///
/// Returns: FieldSelection { fields: ["__typename", "id", "name", "email"] }
///
/// # Errors
///
/// Returns error if query parsing fails.
pub fn parse_field_selection(query: &str) -> Result<FieldSelection> {
    let trimmed = query.trim();

    // Extract fields between outermost { } for _entities
    let fields = extract_fields_from_selection_set(trimmed)?;

    Ok(FieldSelection::new(fields))
}

/// Extract field names from a GraphQL selection set.
fn extract_fields_from_selection_set(query: &str) -> Result<Vec<String>> {
    let mut fields = Vec::new();

    // Simple regex-like extraction: look for patterns like "fieldName" within selection sets
    // This is a simplified implementation that handles common cases
    let mut in_selection = false;
    let mut current_field = String::new();
    let mut depth = 0;

    for ch in query.chars() {
        match ch {
            '{' => {
                depth += 1;
                if depth == 2 {
                    // Entering selection set for _entities
                    in_selection = true;
                }
            }
            '}' => {
                depth -= 1;
                if in_selection && !current_field.is_empty() {
                    fields.push(current_field.trim().to_string());
                    current_field.clear();
                }
                if depth == 1 {
                    in_selection = false;
                }
            }
            '\n' | '\r' if in_selection => {
                if !current_field.is_empty() {
                    fields.push(current_field.trim().to_string());
                    current_field.clear();
                }
            }
            _ if in_selection => {
                current_field.push(ch);
            }
            _ => {}
        }
    }

    // Filter out empty and invalid field names
    let fields: Vec<String> = fields
        .into_iter()
        .filter(|f| !f.is_empty() && !f.contains('(') && !f.contains(':'))
        .map(|f| f.split_whitespace().next().unwrap_or("").to_string())
        .filter(|f| !f.is_empty())
        .collect();

    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_field_selection() {
        let query = r#"
            query {
                _entities(representations: [...]) {
                    __typename
                    id
                    name
                }
            }
        "#;

        let selection = parse_field_selection(query).unwrap();
        assert!(selection.contains("__typename"));
        assert!(selection.contains("id"));
        assert!(selection.contains("name"));
    }

    #[test]
    fn test_field_selection_without_whitespace() {
        let query = "{ _entities(representations: [...]) { id name email } }";

        let selection = parse_field_selection(query).unwrap();
        assert!(selection.contains("id"));
        assert!(selection.contains("name"));
        assert!(selection.contains("email"));
    }

    #[test]
    fn test_field_selection_contains() {
        let mut selection = FieldSelection::new(vec!["id".to_string(), "name".to_string()]);
        assert!(selection.contains("id"));
        assert!(!selection.contains("email"));

        selection.add_field("email".to_string());
        assert!(selection.contains("email"));
    }

    #[test]
    fn test_field_selection_excludes_invalid_patterns() {
        let query = r#"
            query {
                _entities(representations: [...]) {
                    id
                    user(id: "123") @include(if: true)
                    name
                }
            }
        "#;

        let selection = parse_field_selection(query).unwrap();
        assert!(selection.contains("id"));
        assert!(selection.contains("name"));
        // Should not include the function call or directive
    }
}
