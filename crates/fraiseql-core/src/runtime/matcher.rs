//! Query pattern matching - matches incoming GraphQL queries to compiled templates.

use std::collections::HashMap;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{DirectiveEvaluator, FieldSelection, FragmentResolver, ParsedQuery, parse_query},
    schema::{CompiledSchema, QueryDefinition},
};

/// A matched query with extracted information.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// The matched query definition from compiled schema.
    pub query_def: QueryDefinition,

    /// Requested fields (selection set) - now includes full field info.
    pub fields: Vec<String>,

    /// Parsed and processed field selections (after fragment/directive resolution).
    pub selections: Vec<FieldSelection>,

    /// Query arguments/variables.
    pub arguments: HashMap<String, serde_json::Value>,

    /// Query operation name (if provided).
    pub operation_name: Option<String>,

    /// The parsed query (for access to fragments, variables, etc.).
    pub parsed_query: ParsedQuery,
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
    /// - Fragment resolution fails
    /// - Directive evaluation fails
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
        // 1. Parse GraphQL query using proper parser
        let parsed = parse_query(query).map_err(|e| FraiseQLError::Parse {
            message:  e.to_string(),
            location: "query".to_string(),
        })?;

        // 2. Build variables map for directive evaluation
        let variables_map = self.build_variables_map(variables);

        // 3. Resolve fragment spreads
        let resolver = FragmentResolver::new(&parsed.fragments);
        let resolved_selections = resolver.resolve_spreads(&parsed.selections).map_err(|e| {
            FraiseQLError::Validation {
                message: e.to_string(),
                path:    Some("fragments".to_string()),
            }
        })?;

        // 4. Evaluate directives (@skip, @include) and filter selections
        let final_selections =
            DirectiveEvaluator::filter_selections(&resolved_selections, &variables_map).map_err(
                |e| FraiseQLError::Validation {
                    message: e.to_string(),
                    path:    Some("directives".to_string()),
                },
            )?;

        // 5. Find matching query definition using root field
        let query_def = self
            .schema
            .find_query(&parsed.root_field)
            .ok_or_else(|| FraiseQLError::Validation {
                message: format!("Query '{}' not found in schema", parsed.root_field),
                path:    None,
            })?
            .clone();

        // 6. Extract field names for backward compatibility
        let fields = self.extract_field_names(&final_selections);

        // 7. Extract arguments from variables
        let arguments = self.extract_arguments(variables);

        Ok(QueryMatch {
            query_def,
            fields,
            selections: final_selections,
            arguments,
            operation_name: parsed.operation_name.clone(),
            parsed_query: parsed,
        })
    }

    /// Build a variables map from JSON value for directive evaluation.
    fn build_variables_map(
        &self,
        variables: Option<&serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        if let Some(serde_json::Value::Object(map)) = variables {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            HashMap::new()
        }
    }

    /// Extract field names from selections (for backward compatibility).
    fn extract_field_names(&self, selections: &[FieldSelection]) -> Vec<String> {
        selections.iter().map(|s| s.name.clone()).collect()
    }

    /// Extract arguments from variables.
    fn extract_arguments(
        &self,
        variables: Option<&serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        if let Some(serde_json::Value::Object(map)) = variables {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
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
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    Vec::new(),
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  crate::schema::AutoParams::default(),
            deprecation:  None,
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
    fn test_match_simple_query() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ users { id name } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        assert_eq!(result.fields.len(), 1); // "users" is the root field
        assert!(result.selections[0].nested_fields.len() >= 2); // id, name
    }

    #[test]
    fn test_match_query_with_operation_name() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "query GetUsers { users { id name } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        assert_eq!(result.operation_name, Some("GetUsers".to_string()));
    }

    #[test]
    fn test_match_query_with_fragment() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = r"
            fragment UserFields on User {
                id
                name
            }
            query { users { ...UserFields } }
        ";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        // Fragment should be resolved - nested fields should contain id, name
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_match_query_with_skip_directive() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = r"{ users { id name @skip(if: true) } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        // "name" should be skipped due to @skip(if: true)
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(!root_selection.nested_fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_match_query_with_include_directive_variable() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query =
            r"query($includeEmail: Boolean!) { users { id email @include(if: $includeEmail) } }";
        let variables = serde_json::json!({ "includeEmail": false });
        let result = matcher.match_query(query, Some(&variables)).unwrap();

        assert_eq!(result.query_def.name, "users");
        // "email" should be excluded because $includeEmail is false
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(!root_selection.nested_fields.iter().any(|f| f.name == "email"));
    }

    #[test]
    fn test_match_query_unknown_query() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ unknown { id } }";
        let result = matcher.match_query(query, None);

        assert!(result.is_err());
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
