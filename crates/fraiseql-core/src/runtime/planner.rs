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
    pub(crate) const fn choose_jsonb_strategy(
        &self,
        projection_fields: &[String],
    ) -> JsonbStrategy {
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
    pub(crate) fn generate_sql(&self, query_match: &QueryMatch) -> String {
        // Get SQL source from query definition
        let table = query_match.query_def.sql_source.as_ref().map_or("unknown", String::as_str);

        // Build basic SELECT query
        // Select all data — projection happens later in the execution pipeline
        let fields_sql = "data".to_string();

        format!("SELECT {fields_sql} FROM {table}")
    }

    /// Extract parameters from query match.
    pub(crate) fn extract_parameters(
        &self,
        query_match: &QueryMatch,
    ) -> Vec<(String, serde_json::Value)> {
        query_match.arguments.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Estimate query cost (for optimization).
    pub(crate) fn estimate_cost(&self, query_match: &QueryMatch) -> usize {
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
