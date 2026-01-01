//! Unified GraphQL execution pipeline (Phase 9).

use anyhow::Result;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

use crate::cache::{CachedQueryPlan, QueryPlanCache};
use crate::db::pool::DatabasePool;
use crate::graphql::{
    complexity::{ComplexityAnalyzer, ComplexityConfig},
    fragments::FragmentGraph,
    types::ParsedQuery,
    variables::VariableProcessor,
};
use crate::query::composer::SQLComposer;
use crate::query::schema::SchemaMetadata;

/// User context for authorization and personalization.
#[derive(Debug, Clone)]
pub struct UserContext {
    /// User identifier
    pub user_id: Option<String>,
    /// User permissions
    pub permissions: Vec<String>,
    /// User roles
    pub roles: Vec<String>,
    /// Expiration timestamp for cache management
    pub exp: u64,
}

/// Complete unified GraphQL pipeline.
#[derive(Debug, Clone)]
pub struct GraphQLPipeline {
    schema: SchemaMetadata,
    cache: Arc<QueryPlanCache>,
    pool: Arc<DatabasePool>,
}

impl GraphQLPipeline {
    /// Create a new unified GraphQL pipeline with schema, cache, and database pool
    #[must_use]
    pub fn new(schema: SchemaMetadata, cache: Arc<QueryPlanCache>, pool: Arc<DatabasePool>) -> Self {
        Self { schema, cache, pool }
    }

    /// Execute complete GraphQL query end-to-end (async version for production).
    ///
    /// # Errors
    ///
    /// Returns an error if query parsing, SQL building, or execution fails.
    #[allow(clippy::unused_async)]
    pub async fn execute(
        &self,
        query_string: &str,
        variables: HashMap<String, JsonValue>,
        user_context: UserContext,
    ) -> Result<Vec<u8>> {
        // For Phase 9, delegate to sync version
        self.execute_sync(query_string, &variables, user_context)
    }

    /// Execute complete GraphQL query end-to-end (sync version for Phase 9 demo).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - GraphQL query parsing fails
    /// - Advanced feature validation fails
    /// - SQL building or composition fails
    /// - JSON transformation fails
    ///
    /// # Panics
    ///
    /// Panics if the system time is before the UNIX epoch (January 1, 1970).
    /// This should never happen on any modern system.
    pub fn execute_sync(
        &self,
        query_string: &str,
        variables: &HashMap<String, JsonValue>,
        _user_context: UserContext,
    ) -> Result<Vec<u8>> {
        // Phase 6: Parse GraphQL query
        let parsed_query = crate::graphql::parser::parse_query(query_string)?;

        // Phase 13: Advanced GraphQL Features Validation
        Self::validate_advanced_graphql_features(&parsed_query, variables)?;

        // Phase 7 + 8: Build SQL (with caching)
        let signature = crate::cache::signature::generate_signature(&parsed_query);
        let sql = if let Ok(Some(cached_plan)) = self.cache.get(&signature) {
            // Cache hit - use cached SQL
            cached_plan.sql_template
        } else {
            // Cache miss - build SQL
            let composer = SQLComposer::new(self.schema.clone());
            let sql_query = composer.compose(&parsed_query)?;

            // Store in cache
            let cached_plan = CachedQueryPlan {
                signature: signature.clone(),
                sql_template: sql_query.sql.clone(),
                parameters: vec![], // Simplified for Phase 9
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system time before UNIX epoch")
                    .as_secs(),
                hit_count: 0,
            };

            if let Err(e) = self.cache.put(signature, cached_plan) {
                eprintln!("Cache put error: {e}"); // Log but don't fail
            }

            sql_query.sql
        };

        // Phase 1 + 2 + 3: Database execution (real production database)
        let db_results = self.execute_database_query(&sql)?;

        // Phase 3 + 4: Transform to GraphQL response
        let response = Self::build_graphql_response(&parsed_query, db_results)?;

        // Return JSON bytes
        Ok(serde_json::to_vec(&response)?)
    }

    /// Validate advanced GraphQL features (Phase 13).
    fn validate_advanced_graphql_features(
        query: &ParsedQuery,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<()> {
        // 1. Fragment cycle detection
        let fragment_graph = FragmentGraph::new(query);
        fragment_graph
            .validate_fragments()
            .map_err(|e| anyhow::anyhow!("Fragment validation error: {e}"))?;

        // 2. Variable processing and validation
        let var_processor = VariableProcessor::new(query);
        let processed_vars = var_processor.process_variables(variables);
        if !processed_vars.errors.is_empty() {
            return Err(anyhow::anyhow!(
                "Variable processing errors: {}",
                processed_vars.errors.join(", ")
            ));
        }

        // 3. Query complexity analysis
        let complexity_config = ComplexityConfig {
            max_complexity: 1000, // Configurable limit
            field_cost: 1,
            depth_multiplier: 1.5,
            field_overrides: HashMap::new(),
            type_multipliers: HashMap::new(),
        };
        let analyzer = ComplexityAnalyzer::with_config(complexity_config);
        analyzer
            .validate_complexity(query)
            .map_err(|e| anyhow::anyhow!("Complexity validation error: {e}"))?;

        Ok(())
    }

    /// Execute database query using production pool.
    ///
    /// This function bridges the sync execution context with the async database pool.
    /// It uses the Tokio runtime that was initialized at module load time.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection fails
    /// - Query execution fails
    /// - JSON serialization fails
    fn execute_database_query(&self, sql: &str) -> Result<Vec<String>> {
        // Use the global Tokio runtime to execute async database query
        // The runtime was initialized in lib.rs during module import

        // Get the underlying pool from DatabasePool
        let underlying_pool = self.pool.get_pool()
            .ok_or_else(|| anyhow::anyhow!("Database pool not available"))?;

        // Execute query asynchronously and block on result
        let db_results = tokio::runtime::Handle::current()
            .block_on(async {
                // Execute raw SQL query
                let client = underlying_pool.get().await
                    .map_err(|e| anyhow::anyhow!("Failed to get connection: {}", e))?;

                let rows = client.query(sql, &[]).await
                    .map_err(|e| anyhow::anyhow!("Query execution failed: {}", e))?;

                // Convert rows to JSON values (FraiseQL CQRS pattern)
                let results: Vec<serde_json::Value> = rows.iter()
                    .filter_map(|row| {
                        // Extract JSONB column (FraiseQL uses `data` column)
                        row.try_get::<_, serde_json::Value>(0).ok()
                    })
                    .collect();

                Ok::<Vec<serde_json::Value>, anyhow::Error>(results)
            })?;

        // Convert serde_json::Value results to JSON strings
        db_results
            .iter()
            .map(|value| serde_json::to_string(value).map_err(Into::into))
            .collect()
    }

    /// Build GraphQL response from database results.
    fn build_graphql_response(
        parsed_query: &ParsedQuery,
        db_results: Vec<String>,
    ) -> Result<serde_json::Value> {
        let root_field = &parsed_query.selections[0];

        // Build data array from results
        let data_array: Vec<serde_json::Value> = db_results
            .into_iter()
            .map(|row| serde_json::from_str(&row))
            .collect::<Result<Vec<_>, _>>()?;

        // Create GraphQL response
        let response = serde_json::json!({
            "data": {
                root_field.name.clone(): data_array
            }
        });

        Ok(response)
    }
}

/// Python wrapper for the unified pipeline.
#[derive(Debug)]
#[pyclass]
pub struct PyGraphQLPipeline {
    pipeline: Arc<GraphQLPipeline>,
}

#[pymethods]
impl PyGraphQLPipeline {
    /// # Errors
    ///
    /// Returns a Python error if schema JSON is invalid or cannot be parsed.
    #[new]
    pub fn new(schema_json: &str, pool: &DatabasePool) -> PyResult<Self> {
        let schema: SchemaMetadata = serde_json::from_str(schema_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let cache = Arc::new(QueryPlanCache::new(5000));

        let pipeline = Arc::new(GraphQLPipeline::new(schema, cache, Arc::new(pool.clone())));

        Ok(Self { pipeline })
    }

    /// Execute GraphQL query (Python interface).
    ///
    /// # Errors
    ///
    /// Returns a Python error if:
    /// - Variable or user context conversion fails
    /// - Query execution fails
    /// - Response conversion to Python fails
    #[pyo3(name = "execute")]
    pub fn execute_py(
        &self,
        py: Python,
        query_string: &str,
        variables: &Bound<'_, PyDict>,
        user_context: &Bound<'_, PyDict>,
    ) -> PyResult<PyObject> {
        let vars = dict_to_hashmap(variables)?;
        let user = dict_to_user_context(user_context)?;

        // For Phase 9 demo, execute synchronously with mock data
        let result_bytes = self
            .pipeline
            .execute_sync(query_string, &vars, user)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(PyBytes::new(py, &result_bytes).into())
    }
}

/// Convert `PyDict` to `HashMap` for variables.
fn dict_to_hashmap(dict: &Bound<'_, PyDict>) -> PyResult<HashMap<String, JsonValue>> {
    let mut result = HashMap::new();
    for (key, value) in dict.iter() {
        let key_str = key.extract::<String>()?;
        let value_json = py_to_json(&value);
        result.insert(key_str, value_json);
    }
    Ok(result)
}

/// Convert Python object to JSON value.
fn py_to_json(obj: &Bound<'_, PyAny>) -> JsonValue {
    if obj.is_none() {
        JsonValue::Null
    } else if let Ok(s) = obj.extract::<String>() {
        JsonValue::String(s)
    } else if let Ok(i) = obj.extract::<i64>() {
        JsonValue::Number(i.into())
    } else if let Ok(f) = obj.extract::<f64>() {
        JsonValue::Number(serde_json::Number::from_f64(f).expect("finite f64"))
    } else if let Ok(b) = obj.extract::<bool>() {
        JsonValue::Bool(b)
    } else {
        JsonValue::Null // Simplified fallback
    }
}

/// Convert `PyDict` to `UserContext`.
fn dict_to_user_context(dict: &Bound<'_, PyDict>) -> PyResult<UserContext> {
    let user_id = dict.get_item("user_id")?.and_then(|v| {
        if v.is_none() {
            None
        } else {
            v.extract::<String>().ok()
        }
    });

    let permissions = dict
        .get_item("permissions")?
        .and_then(|v| v.extract::<Vec<String>>().ok())
        .unwrap_or_default();

    let roles = dict
        .get_item("roles")?
        .and_then(|v| v.extract::<Vec<String>>().ok())
        .unwrap_or_default();

    Ok(UserContext {
        user_id,
        permissions,
        roles,
        exp: 0, // Default for mock contexts
    })
}
