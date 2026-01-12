//! Query pattern matching - matches incoming GraphQL queries to compiled templates.

use crate::error::{FraiseQLError, Result};
use crate::schema::{CompiledSchema, QueryDefinition};
use std::collections::HashMap;

/// A matched query with extracted information.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// The matched query definition from compiled schema.
    pub query_def: QueryDefinition,

    /// Requested fields (selection set).
    pub fields: Vec<String>,

    /// Query arguments/variables.
    pub arguments: HashMap<String, serde_json::Value>,

    /// Query operation name (if provided).
    pub operation_name: Option<String>,
}

/// Query pattern matcher.
///
/// Matches incoming GraphQL queries against the compiled schema to determine
/// which pre-compiled SQL template to execute.
pub struct QueryMatcher {
    schema: CompiledSchema,
}

impl QueryMatcher {
    /// Create new query matcher.
    #[must_use]
    pub fn new(schema: CompiledSchema) -> Self {
        Self { schema }
    }

    /// Match a GraphQL query to a compiled template.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    ///
    /// # Returns
    ///
    /// QueryMatch with query definition and extracted information
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query syntax is invalid
    /// - Query references undefined operation
    /// - Query structure doesn't match schema
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let matcher = QueryMatcher::new(schema);
    /// let query = "query { users { id name } }";
    /// let matched = matcher.match_query(query, None)?;
    /// assert_eq!(matched.query_def.name, "users");
    /// ```
    pub fn match_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<QueryMatch> {
        // TODO: Parse GraphQL query (use graphql-parser or similar)
        // For now, implement basic pattern matching

        // Extract operation name from query
        let operation_name = self.extract_operation_name(query);

        // Find matching query definition
        let query_def = if let Some(op_name) = &operation_name {
            self.schema
                .find_query(op_name)
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!("Query '{}' not found in schema", op_name),
                    path: None,
                })?
                .clone()
        } else {
            // If no operation name, try to infer from query structure
            self.infer_query_from_structure(query)?
        };

        // Extract requested fields
        let fields = self.extract_fields(query)?;

        // Extract arguments
        let arguments = self.extract_arguments(variables);

        Ok(QueryMatch {
            query_def,
            fields,
            arguments,
            operation_name,
        })
    }

    /// Extract operation name from query string.
    fn extract_operation_name(&self, query: &str) -> Option<String> {
        // Simple regex-based extraction (TODO: use proper GraphQL parser)
        // Pattern: "query operationName" or "{ operationName"

        let query_trimmed = query.trim();

        // Try to match "query operationName {"
        if let Some(start) = query_trimmed.find("query") {
            let after_query = &query_trimmed[start + 5..].trim_start();
            if let Some(brace_pos) = after_query.find('{') {
                let op_name = after_query[..brace_pos].trim();
                if !op_name.is_empty() {
                    return Some(op_name.to_string());
                }
            }
        }

        // Try to match "{ operationName"
        if let Some(brace_pos) = query_trimmed.find('{') {
            let after_brace = &query_trimmed[brace_pos + 1..].trim_start();
            if let Some(space_or_brace) = after_brace.find(|c: char| c.is_whitespace() || c == '{') {
                let op_name = after_brace[..space_or_brace].trim();
                if !op_name.is_empty() {
                    return Some(op_name.to_string());
                }
            }
        }

        None
    }

    /// Infer query from structure when no explicit operation name.
    fn infer_query_from_structure(&self, query: &str) -> Result<QueryDefinition> {
        // Extract first field name from query
        if let Some(field_name) = self.extract_first_field(query) {
            Ok(self.schema
                .find_query(&field_name)
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!("Query '{}' not found in schema", field_name),
                    path: None,
                })?
                .clone())
        } else {
            Err(FraiseQLError::Parse {
                message: "Could not extract operation name from query".to_string(),
                location: "query".to_string(),
            })
        }
    }

    /// Extract first field name from query.
    fn extract_first_field(&self, query: &str) -> Option<String> {
        // Find first field after opening brace
        if let Some(brace_pos) = query.find('{') {
            let after_brace = query[brace_pos + 1..].trim_start();
            if let Some(end_pos) = after_brace.find(|c: char| c.is_whitespace() || c == '{' || c == '(') {
                return Some(after_brace[..end_pos].trim().to_string());
            }
        }
        None
    }

    /// Extract requested fields from query.
    fn extract_fields(&self, query: &str) -> Result<Vec<String>> {
        let mut fields = Vec::new();

        // TODO: Use proper GraphQL parser
        // For now, extract fields between inner braces
        if let Some(first_brace) = query.find('{') {
            if let Some(second_brace) = query[first_brace + 1..].find('{') {
                let start = first_brace + 1 + second_brace + 1;
                if let Some(closing_brace) = query[start..].find('}') {
                    let fields_str = &query[start..start + closing_brace];
                    for field in fields_str.split_whitespace() {
                        let field_name = field.trim();
                        if !field_name.is_empty() {
                            fields.push(field_name.to_string());
                        }
                    }
                }
            }
        }

        if fields.is_empty() {
            return Err(FraiseQLError::Parse {
                message: "No fields found in query".to_string(),
                location: "query".to_string(),
            });
        }

        Ok(fields)
    }

    /// Extract arguments from variables.
    fn extract_arguments(&self, variables: Option<&serde_json::Value>) -> HashMap<String, serde_json::Value> {
        if let Some(serde_json::Value::Object(map)) = variables {
            map.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        } else {
            HashMap::new()
        }
    }

    /// Get the compiled schema.
    #[must_use]
    pub const fn schema(&self) -> &CompiledSchema {
        &self.schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{CompiledSchema, QueryDefinition};

    fn test_schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_user".to_string()),
            description: None,
            auto_params: crate::schema::AutoParams::default(),
        });
        schema
    }

    #[test]
    fn test_matcher_new() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema.clone());
        assert_eq!(matcher.schema().queries.len(), 1);
    }

    #[test]
    fn test_extract_operation_name_explicit() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "query users { users { id } }";
        let op_name = matcher.extract_operation_name(query);
        assert_eq!(op_name, Some("users".to_string()));
    }

    #[test]
    fn test_extract_operation_name_shorthand() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ users { id } }";
        let op_name = matcher.extract_operation_name(query);
        assert_eq!(op_name, Some("users".to_string()));
    }

    #[test]
    fn test_extract_first_field() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ users { id name } }";
        let field = matcher.extract_first_field(query);
        assert_eq!(field, Some("users".to_string()));
    }

    #[test]
    fn test_extract_fields() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ users { id name email } }";
        let fields = matcher.extract_fields(query).unwrap();
        assert_eq!(fields, vec!["id", "name", "email"]);
    }

    #[test]
    fn test_extract_arguments_none() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let args = matcher.extract_arguments(None);
        assert!(args.is_empty());
    }

    #[test]
    fn test_extract_arguments_some() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let variables = serde_json::json!({
            "id": "123",
            "limit": 10
        });

        let args = matcher.extract_arguments(Some(&variables));
        assert_eq!(args.len(), 2);
        assert_eq!(args.get("id"), Some(&serde_json::json!("123")));
        assert_eq!(args.get("limit"), Some(&serde_json::json!(10)));
    }
}
