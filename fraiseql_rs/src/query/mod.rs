//! Query building module.

pub mod schema;
pub mod where_builder;
pub mod composer;

use pyo3::prelude::*;
use crate::graphql::types::ParsedQuery;
use crate::query::composer::SQLComposer;
use crate::query::schema::SchemaMetadata;

/// Build complete SQL query from parsed GraphQL.
#[pyfunction]
pub fn build_sql_query(
    _py: Python,
    parsed_query: ParsedQuery,
    schema_json: String,
) -> PyResult<GeneratedQuery> {
    // Deserialize schema
    let schema: SchemaMetadata = serde_json::from_str(&schema_json)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid schema JSON: {}", e)))?;

    // Compose SQL
    let composer = SQLComposer::new(schema);
    let composed = composer.compose(&parsed_query)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Query composition failed: {}", e)))?;

    // Return GeneratedQuery
    Ok(GeneratedQuery {
        sql: composed.sql,
        parameters: composed.parameters.into_iter()
            .map(|(name, value)| {
                let value_str = match value {
                    where_builder::ParameterValue::String(s) => s,
                    where_builder::ParameterValue::Integer(i) => i.to_string(),
                    where_builder::ParameterValue::Float(f) => f.to_string(),
                    where_builder::ParameterValue::Boolean(b) => b.to_string(),
                    where_builder::ParameterValue::JsonObject(s) => s,
                    where_builder::ParameterValue::Array(_) => "[]".to_string(),
                };
                (name, value_str)
            })
            .collect(),
    })
}

#[pyclass]
pub struct GeneratedQuery {
    #[pyo3(get)]
    pub sql: String,

    #[pyo3(get)]
    pub parameters: Vec<(String, String)>,
}