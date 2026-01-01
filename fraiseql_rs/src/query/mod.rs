//! Query building module.

pub mod composer;
pub mod schema;
pub mod where_builder;

// Phase 7.2: WHERE normalization in Rust
pub mod casing;
pub mod field_analyzer;
pub mod operators;
pub mod prepared_statement;
pub mod where_normalization;

use crate::cache::QueryPlanCache;
use crate::graphql::types::ParsedQuery;
use crate::query::composer::SQLComposer;
use crate::query::schema::SchemaMetadata;
use lazy_static::lazy_static;
use pyo3::prelude::*;

lazy_static! {
    static ref QUERY_PLAN_CACHE: QueryPlanCache = QueryPlanCache::new(5000);
}

/// Convert a `ParameterValue` to its string representation
fn parameter_value_to_string(value: where_builder::ParameterValue) -> String {
    match value {
        where_builder::ParameterValue::String(s) | where_builder::ParameterValue::JsonObject(s) => {
            s
        }
        where_builder::ParameterValue::Integer(i) => i.to_string(),
        where_builder::ParameterValue::Float(f) => f.to_string(),
        where_builder::ParameterValue::Boolean(b) => b.to_string(),
        where_builder::ParameterValue::Array(_) => "[]".to_string(),
    }
}

/// Build complete SQL query from parsed GraphQL.
///
/// # Errors
///
/// Returns a Python error if:
/// - Schema JSON is invalid or malformed
/// - Query composition fails
#[pyfunction]
pub fn build_sql_query(
    _py: Python,
    parsed_query: &ParsedQuery,
    schema_json: &str,
) -> PyResult<GeneratedQuery> {
    // Deserialize schema
    let schema: SchemaMetadata = serde_json::from_str(schema_json).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid schema JSON: {e}"))
    })?;

    // Compose SQL
    let composer = SQLComposer::new(schema);
    let sql_query = composer.compose(parsed_query).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Query composition failed: {e}"))
    })?;

    // Return GeneratedQuery
    Ok(GeneratedQuery {
        sql: sql_query.sql,
        parameters: sql_query
            .parameters
            .into_iter()
            .map(|(name, value)| (name, parameter_value_to_string(value)))
            .collect(),
    })
}

/// Build complete SQL query with caching.
///
/// # Errors
///
/// Returns a Python error if:
/// - Schema JSON is invalid or malformed
/// - Query composition fails
///
/// # Panics
///
/// Panics if the system time is before the UNIX epoch (January 1, 1970).
/// This should never happen on any modern system.
#[pyfunction]
pub fn build_sql_query_cached(
    _py: Python,
    parsed_query: &ParsedQuery,
    schema_json: &str,
) -> PyResult<GeneratedQuery> {
    // Generate query signature
    let signature = crate::cache::signature::generate_signature(parsed_query);

    // Check cache
    if let Ok(Some(cached_plan)) = QUERY_PLAN_CACHE.get(&signature) {
        // Cache hit - return cached plan
        return Ok(GeneratedQuery {
            sql: cached_plan.sql_template,
            parameters: vec![], // Parameters already bound
        });
    }

    // Cache miss - build query normally
    let schema: SchemaMetadata = serde_json::from_str(schema_json).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid schema JSON: {e}"))
    })?;

    let composer = SQLComposer::new(schema);
    let sql_query = composer.compose(parsed_query).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Query composition failed: {e}"))
    })?;

    let result = GeneratedQuery {
        sql: sql_query.sql.clone(),
        parameters: sql_query
            .parameters
            .into_iter()
            .map(|(name, value)| (name, parameter_value_to_string(value)))
            .collect(),
    };

    // Store in cache
    let _ = QUERY_PLAN_CACHE.put(
        signature.clone(),
        crate::cache::CachedQueryPlan {
            signature,
            sql_template: sql_query.sql,
            parameters: vec![],
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before UNIX epoch")
                .as_secs(),
            hit_count: 0,
        },
    );

    Ok(result)
}

/// Get cache statistics.
///
/// # Errors
///
/// Returns a Python error if:
/// - Cache statistics retrieval fails
/// - Python dictionary creation fails
#[pyfunction]
pub fn get_cache_stats(py: Python) -> PyResult<PyObject> {
    let stats = QUERY_PLAN_CACHE.stats().map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Cache stats error: {e}"))
    })?;

    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("hits", stats.hits)?;
    dict.set_item("misses", stats.misses)?;
    dict.set_item("hit_rate", stats.hit_rate)?;
    dict.set_item("cached_plans", stats.size)?;
    dict.set_item("max_cached_plans", stats.max_size)?;

    Ok(dict.into())
}

/// Clear cache (for schema changes).
///
/// # Errors
///
/// Returns a Python error if cache clearing fails.
#[pyfunction]
pub fn clear_cache() -> PyResult<()> {
    QUERY_PLAN_CACHE.clear().map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Cache clear error: {e}"))
    })
}

/// Normalize a WHERE clause dictionary to SQL (Phase 7.2).
///
/// # Arguments
///
/// * `where_dict` - JSON string representing the WHERE clause
/// * `table_columns` - List of SQL column names
/// * `fk_mappings` - JSON string mapping FK field names to SQL columns
/// * `jsonb_column` - Name of the JSONB column (default: "data")
///
/// # Returns
///
/// A tuple of (`sql_string`, `parameters_list`) for prepared statement
///
/// # Errors
///
/// Returns a Python error if:
/// - JSON parsing fails
/// - WHERE clause normalization fails
#[pyfunction]
#[pyo3(signature = (where_dict, table_columns, fk_mappings = "{}", jsonb_column = "data"))]
pub fn normalize_where_to_sql(
    _py: Python,
    where_dict: &str,
    table_columns: Vec<String>,
    fk_mappings: &str,
    jsonb_column: &str,
) -> PyResult<(String, Vec<String>)> {
    // Parse WHERE dictionary from JSON
    let where_obj: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(where_dict).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid WHERE dict JSON: {e}"))
        })?;

    // Parse FK mappings from JSON
    let fk_map: std::collections::HashMap<String, String> = serde_json::from_str(fk_mappings)
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid FK mappings JSON: {e}"
            ))
        })?;

    // Convert table_columns to HashSet
    let columns: std::collections::HashSet<String> = table_columns.into_iter().collect();

    // Call Rust WHERE normalization
    let result =
        where_normalization::normalize_dict_where(&where_obj, &columns, &fk_map, jsonb_column);

    // Convert parameters to strings for Python
    let params: Vec<String> = result.params.into_iter().map(|v| v.to_string()).collect();

    Ok((result.sql, params))
}

/// Generated SQL query with parameters for Python binding
#[derive(Debug)]
#[pyclass]
pub struct GeneratedQuery {
    /// SQL query string
    #[pyo3(get)]
    pub sql: String,

    /// Query parameters as (name, value) tuples
    #[pyo3(get)]
    pub parameters: Vec<(String, String)>,
}
