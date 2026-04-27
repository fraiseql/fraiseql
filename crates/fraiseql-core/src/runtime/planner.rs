//! Query plan selection - chooses optimal execution strategy.

use super::matcher::QueryMatch;
use crate::{
    error::Result,
    graphql::FieldSelection,
    runtime::{JsonbOptimizationOptions, JsonbStrategy},
};

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

    /// JSONB handling strategy for this query
    pub jsonb_strategy: JsonbStrategy,
}

/// Query planner - selects optimal execution strategy.
pub struct QueryPlanner {
    /// Enable query plan caching.
    cache_enabled: bool,

    /// JSONB optimization options for strategy selection
    jsonb_options: JsonbOptimizationOptions,
}

impl QueryPlanner {
    /// Create new query planner with default JSONB optimization options.
    #[must_use]
    pub fn new(cache_enabled: bool) -> Self {
        Self::with_jsonb_options(cache_enabled, JsonbOptimizationOptions::default())
    }

    /// Create query planner with custom JSONB optimization options.
    #[must_use]
    pub const fn with_jsonb_options(
        cache_enabled: bool,
        jsonb_options: JsonbOptimizationOptions,
    ) -> Self {
        Self {
            cache_enabled,
            jsonb_options,
        }
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
    /// ```no_run
    /// // Requires: a QueryMatch from compiled schema matching.
    /// # use fraiseql_core::runtime::{QueryMatcher, QueryPlanner};
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_error::Result;
    /// # fn example() -> Result<()> {
    /// # let schema: CompiledSchema = panic!("example");
    /// # let query_match = QueryMatcher::new(schema).match_query("query{users{id}}", None)?;
    /// let planner = QueryPlanner::new(true);
    /// let plan = planner.plan(&query_match)?;
    /// assert!(!plan.sql.is_empty());
    /// # Ok(())
    /// # }
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

        // Determine JSONB optimization strategy based on field count
        let jsonb_strategy = self.choose_jsonb_strategy(&projection_fields);

        Ok(ExecutionPlan {
            sql,
            parameters,
            is_cached: false,
            estimated_cost: self.estimate_cost(query_match),
            projection_fields,
            jsonb_strategy,
        })
    }

    /// Choose JSONB handling strategy based on requested fields.
    ///
    /// When a selection set is available (non-empty `projection_fields`), we
    /// always use `Project` so that the response keys are emitted in camelCase
    /// by `jsonb_build_object`.  The `Stream` strategy returns raw JSONB with
    /// `snake_case` keys, which violates client expectations.
    ///
    /// `Stream` is only used as a fallback when no specific fields are requested.
    const fn choose_jsonb_strategy(&self, projection_fields: &[String]) -> JsonbStrategy {
        if projection_fields.is_empty() {
            self.jsonb_options.default_strategy
        } else {
            JsonbStrategy::Project
        }
    }

    /// Extract field names for projection from parsed selections.
    ///
    /// For a query like `{ users { id name } }`, this extracts `["id", "name"]`.
    ///
    /// Filter `__typename` from SQL projection fields.
    /// `__typename` is a GraphQL meta-field not stored in JSONB.
    /// The `ResultProjector` handles injection — see `projection.rs`.
    /// Removing this filter causes `data->>'__typename'` (NULL) to overwrite
    /// the value injected by `with_typename()`, depending on field iteration order.
    fn extract_projection_fields(&self, selections: &[FieldSelection]) -> Vec<String> {
        // Get the first (root) selection and extract its nested fields.
        // Skip `__typename` — it is a GraphQL meta-field handled by the projector
        // at the Rust level; including it in the field list causes the SQL projection
        // to emit `data->>'__typename'` which returns NULL and then overwrites the
        // correctly-computed typename injected by `ResultProjector::with_typename`.
        if let Some(root_selection) = selections.first() {
            root_selection
                .nested_fields
                .iter()
                .filter(|f| f.name != "__typename")
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
        // Select all data — projection happens later in the execution pipeline
        let fields_sql = "data".to_string();

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
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::collections::HashMap;

    use indexmap::IndexMap;

    use super::*;
    use crate::{
        graphql::{FieldSelection, ParsedQuery},
        schema::{AutoParams, CursorType, QueryDefinition},
    };

    fn test_query_match() -> QueryMatch {
        QueryMatch {
            query_def:      QueryDefinition {
                name:                "users".to_string(),
                return_type:         "User".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           Vec::new(),
                sql_source:          Some("v_user".to_string()),
                sql_source_dispatch: None,
                description:         None,
                auto_params:         AutoParams::default(),
                deprecation:         None,
                jsonb_column:        "data".to_string(),
                relay:               false,
                relay_cursor_column: None,
                relay_cursor_type:   CursorType::default(),
                inject_params:       IndexMap::default(),
                cache_ttl_seconds:   None,
                additional_views:    vec![],
                requires_role:       None,
                rest_path:           None,
                rest_method:         None,
                native_columns:      HashMap::new(),
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
        assert_eq!(plan.jsonb_strategy, JsonbStrategy::Project);
    }

    // ========================================================================

    // ========================================================================

    #[test]
    fn test_projection_fields_exclude_typename() {
        let planner = QueryPlanner::new(true);
        let mut query_match = test_query_match();

        // Add __typename to the nested fields of the root selection
        query_match.selections[0].nested_fields.push(FieldSelection {
            name:          "__typename".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        });

        let plan = planner.plan(&query_match).unwrap();

        // __typename must NOT appear in projection fields (it's a GraphQL meta-field)
        assert!(!plan.projection_fields.contains(&"__typename".to_string()));
        assert_eq!(plan.projection_fields, vec!["id".to_string(), "name".to_string()]);
    }

    #[test]
    fn test_plan_includes_jsonb_strategy() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let plan = planner.plan(&query_match).unwrap();
        // Should include strategy in execution plan
        assert_eq!(plan.jsonb_strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_planner_always_projects_when_fields_present() {
        let custom_options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 50,
        };
        let planner = QueryPlanner::with_jsonb_options(true, custom_options);
        let query_match = test_query_match();

        let plan = planner.plan(&query_match).unwrap();
        // Even with Stream default, must use Project when selections exist
        // to ensure camelCase response keys
        assert_eq!(plan.jsonb_strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_jsonb_strategy_forces_project_with_fields() {
        let options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 80,
        };
        let planner = QueryPlanner::with_jsonb_options(true, options);

        // Any non-empty selection set must use Project for camelCase keys
        let strategy = planner.choose_jsonb_strategy(&["id".to_string(), "name".to_string()]);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_jsonb_strategy_forces_project_with_many_fields() {
        let options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };
        let planner = QueryPlanner::with_jsonb_options(true, options);

        // Even with many fields (above old threshold), must use Project
        let many_fields = (0..9).map(|i| format!("field_{}", i)).collect::<Vec<_>>();
        let strategy = planner.choose_jsonb_strategy(&many_fields);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_jsonb_strategy_empty_fields_uses_default() {
        let options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 80,
        };
        let planner = QueryPlanner::with_jsonb_options(true, options);

        // Empty selection set falls back to default strategy
        let strategy = planner.choose_jsonb_strategy(&[]);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }
}
