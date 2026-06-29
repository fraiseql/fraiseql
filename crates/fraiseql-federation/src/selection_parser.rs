//! GraphQL field selection parsing for federation queries.
//!
//! Extracts which fields are requested in a GraphQL query so federation
//! can project only necessary fields from the database.

use fraiseql_error::Result;

/// Represents requested fields in a GraphQL selection set.
#[derive(Debug, Clone, Default)]
pub struct FieldSelection {
    /// Names of fields requested in the query
    pub fields: Vec<String>,
}

impl FieldSelection {
    /// Create a new field selection.
    #[must_use]
    pub const fn new(fields: Vec<String>) -> Self {
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
/// Returns: `FieldSelection { fields: ["__typename", "id", "name", "email"] }`
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
                // A `{` opens a (possibly nested) selection set — e.g. the body of an
                // inline fragment. Flush any pending token first, exactly as whitespace
                // does, so a type condition butted directly against the body brace
                // (minified `...on User{name email}`) does not fuse with the first field
                // and yield `Username`. A space before `{` previously masked this. (#512)
                if in_selection && !current_field.is_empty() {
                    fields.push(current_field.trim().to_string());
                    current_field.clear();
                }
                depth += 1;
                if depth == 2 {
                    // Entering selection set for _entities
                    in_selection = true;
                }
            },
            '}' => {
                depth -= 1;
                if in_selection && !current_field.is_empty() {
                    fields.push(current_field.trim().to_string());
                    current_field.clear();
                }
                if depth == 1 {
                    in_selection = false;
                }
            },
            ' ' | '\n' | '\r' | '\t' if in_selection => {
                // Whitespace is a field separator
                if !current_field.is_empty() {
                    fields.push(current_field.trim().to_string());
                    current_field.clear();
                }
            },
            _ if in_selection => {
                current_field.push(ch);
            },
            _ => {},
        }
    }

    // Filter out empty/invalid names and inline-fragment syntax. For
    // `_entities` the selection is `... on TypeName { field … }`; the char scanner
    // emits the spread (`...`), the `on` keyword, and the type condition as separate
    // tokens, so drop them and keep only the leaf field names.
    let mut result = Vec::new();
    let mut skip_type_after_on = false;
    for field in fields {
        let field = field.trim();
        if skip_type_after_on {
            // The previous token introduced an inline-fragment type condition
            // (`on` or the minified `...on`); this token is the bare type name. Skip it.
            skip_type_after_on = false;
            continue;
        }
        if field.is_empty() || field.contains('(') || field.contains(':') {
            continue;
        }
        if field == "on" || field == "...on" {
            // Inline-fragment type condition. Pretty-printed input emits `on` as its own
            // token (`... on User`); a minifier fuses the spread and keyword into a single
            // `...on` token (`...on User`). Either way the *type name* follows next, so
            // skip it. `on` is a reserved word, so `...on` is never a named-fragment spread. (#512)
            skip_type_after_on = true;
            continue;
        }
        if field.starts_with("...") {
            // Bare spread (`...`) or named-fragment spread (`...Fragment`) — not a field.
            continue;
        }
        result.push(field.to_string());
    }

    Ok(result)
}

#[cfg(test)]
mod tests;
