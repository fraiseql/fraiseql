//! Query plan selection - chooses optimal execution strategy.

use super::matcher::QueryMatch;
use crate::{error::Result, graphql::FieldSelection};

/// Execution plan for a query.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// SQL query to execute.
    pub sql: String,

    /// Parameter bindings (parameter name → value).
    pub parameters: Vec<(String, serde_json::Value)>,

    /// Whether this plan uses a cached result.
    pub is_cached: bool,

    /// Estimated cost (for optimization).
    pub estimated_cost: usize,

    /// Fields to project from JSONB result.
    pub projection_fields: Vec<String>,
}

/// Query planner - selects optimal execution strategy.
pub struct QueryPlanner {
    /// Enable query plan caching.
    cache_enabled: bool,
}

impl QueryPlanner {
    /// Create new query planner.
    #[must_use]
    pub fn new(cache_enabled: bool) -> Self {
        Self { cache_enabled }
    }

    /// Create an execution plan for a matched query.
    ///
    /// # Arguments
    ///
    /// * `query_match` - Matched query with extracted information
    ///
    /// # Returns
    ///
    /// Execution plan with SQL, parameters, and optimization hints
    ///
    /// # Errors
    ///
    /// Returns error if plan generation fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let planner = QueryPlanner::new(true);
    /// let plan = planner.plan(&query_match)?;
    /// assert!(!plan.sql.is_empty());
    /// ```
    pub fn plan(&self, query_match: &QueryMatch) -> Result<ExecutionPlan> {
        // Note: FraiseQL uses compiled SQL templates, so "query planning" means
        // extracting the pre-compiled SQL from the matched query definition.
        // No dynamic query optimization is needed - templates are pre-optimized.

        let sql = self.generate_sql(query_match);
        let parameters = self.extract_parameters(query_match);

        // Extract nested field names from the first selection's nested_fields
        // The first selection is typically the root query field (e.g., "users")
        let projection_fields = self.extract_projection_fields(&query_match.selections);

        Ok(ExecutionPlan {
            sql,
            parameters,
            is_cached: false,
            estimated_cost: self.estimate_cost(query_match),
            projection_fields,
        })
    }

    /// Extract field names for projection from parsed selections.
    ///
    /// For a query like `{ users { id name } }`, this extracts `["id", "name"]`.
    fn extract_projection_fields(&self, selections: &[FieldSelection]) -> Vec<String> {
        // Get the first (root) selection and extract its nested fields
        if let Some(root_selection) = selections.first() {
            root_selection
                .nested_fields
                .iter()
                .map(|f| f.response_key().to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Generate SQL from query match.
    fn generate_sql(&self, query_match: &QueryMatch) -> String {
        // Get SQL source from query definition
        let table = query_match.query_def.sql_source.as_ref().map_or("unknown", String::as_str);

        // Build basic SELECT query
        let fields_sql = if query_match.fields.is_empty() {
            "data".to_string()
        } else {
            // For now, select all data (projection happens later)
            "data".to_string()
        };

        format!("SELECT {fields_sql} FROM {table}")
    }

    /// Extract parameters from query match.
    fn extract_parameters(&self, query_match: &QueryMatch) -> Vec<(String, serde_json::Value)> {
        query_match.arguments.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Estimate query cost (for optimization).
    fn estimate_cost(&self, query_match: &QueryMatch) -> usize {
        // Simple heuristic: base cost + field cost
        let base_cost = 100;
        let field_cost = query_match.fields.len() * 10;
        let arg_cost = query_match.arguments.len() * 5;

        base_cost + field_cost + arg_cost
    }

    /// Check if caching is enabled.
    #[must_use]
    pub const fn cache_enabled(&self) -> bool {
        self.cache_enabled
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        graphql::{FieldSelection, ParsedQuery},
        schema::{AutoParams, QueryDefinition},
    };

    fn test_query_match() -> QueryMatch {
        QueryMatch {
            query_def:      QueryDefinition {
                name:         "users".to_string(),
                return_type:  "User".to_string(),
                returns_list: true,
                nullable:     false,
                arguments:    Vec::new(),
                sql_source:   Some("v_user".to_string()),
                description:  None,
                auto_params:  AutoParams::default(),
                deprecation:  None,
                jsonb_column: "data".to_string(),
            },
            fields:         vec!["id".to_string(), "name".to_string()],
            selections:     vec![FieldSelection {
                name:          "users".to_string(),
                alias:         None,
                arguments:     vec![],
                nested_fields: vec![
                    FieldSelection {
                        name:          "id".to_string(),
                        alias:         None,
                        arguments:     vec![],
                        nested_fields: vec![],
                        directives:    vec![],
                    },
                    FieldSelection {
                        name:          "name".to_string(),
                        alias:         None,
                        arguments:     vec![],
                        nested_fields: vec![],
                        directives:    vec![],
                    },
                ],
                directives:    vec![],
            }],
            arguments:      HashMap::new(),
            operation_name: Some("users".to_string()),
            parsed_query:   ParsedQuery {
                operation_type: "query".to_string(),
                operation_name: Some("users".to_string()),
                root_field:     "users".to_string(),
                selections:     vec![],
                variables:      vec![],
                fragments:      vec![],
                source:         "{ users { id name } }".to_string(),
            },
        }
    }

    #[test]
    fn test_planner_new() {
        let planner = QueryPlanner::new(true);
        assert!(planner.cache_enabled());

        let planner = QueryPlanner::new(false);
        assert!(!planner.cache_enabled());
    }

    #[test]
    fn test_generate_sql() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let sql = planner.generate_sql(&query_match);
        assert_eq!(sql, "SELECT data FROM v_user");
    }

    #[test]
    fn test_extract_parameters() {
        let planner = QueryPlanner::new(true);
        let mut query_match = test_query_match();
        query_match.arguments.insert("id".to_string(), serde_json::json!("123"));
        query_match.arguments.insert("limit".to_string(), serde_json::json!(10));

        let params = planner.extract_parameters(&query_match);
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_estimate_cost() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let cost = planner.estimate_cost(&query_match);
        // base (100) + 2 fields (20) + 0 args (0) = 120
        assert_eq!(cost, 120);
    }

    #[test]
    fn test_plan() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let plan = planner.plan(&query_match).unwrap();
        assert!(!plan.sql.is_empty());
        assert_eq!(plan.projection_fields.len(), 2);
        assert!(!plan.is_cached);
        assert_eq!(plan.estimated_cost, 120);
    }
}
