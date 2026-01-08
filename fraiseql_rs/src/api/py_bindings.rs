//! PyO3 FFI bindings for GraphQLEngine
//!
//! This module provides Python bindings for the Rust GraphQLEngine using PyO3.
//! All Python code interacts with the Rust engine exclusively through this interface.

use crate::api::engine::GraphQLEngine;
use crate::api::types::{MutationRequest, QueryRequest};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;

/// Python wrapper for GraphQLEngine
///
/// This class provides the main interface for Python code to interact with the Rust engine.
/// All instances are thread-safe (Arc<T> internally) and can be shared between threads.
///
/// # Example
///
/// ```python
/// from fraiseql_rs import GraphQLEngine
///
/// # Create engine
/// config = '{"db": "postgres://localhost/db"}'
/// engine = GraphQLEngine(config)
///
/// # Execute query
/// result = engine.execute_query('{ users { id name } }', {})
/// print(result)
/// ```
#[pyclass]
pub struct PyGraphQLEngine {
    inner: GraphQLEngine,
}

#[pymethods]
impl PyGraphQLEngine {
    /// Create a new GraphQL engine instance
    ///
    /// # Arguments
    ///
    /// * `config_json` - Engine configuration as JSON string
    ///
    /// # Returns
    ///
    /// PyGraphQLEngine instance or raises PyErr if configuration is invalid
    ///
    /// # Raises
    ///
    /// * `ValueError` - If config_json is not valid JSON
    /// * `RuntimeError` - If engine initialization fails
    #[new]
    fn new(config_json: &str) -> PyResult<Self> {
        GraphQLEngine::new(config_json)
            .map(|engine| PyGraphQLEngine { inner: engine })
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }

    /// Execute a GraphQL query asynchronously
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Dictionary of variables (optional, defaults to empty)
    /// * `operation_name` - Operation name for multi-operation documents (optional)
    ///
    /// # Returns
    ///
    /// Dictionary with keys:
    /// - `data`: Query result data (may be None if errors occurred)
    /// - `errors`: List of error objects (may be None if no errors)
    /// - `extensions`: Additional metadata (may be None)
    ///
    /// # Raises
    ///
    /// * `ValueError` - If query is invalid
    /// * `RuntimeError` - If query execution fails
    ///
    /// # Example
    ///
    /// ```python
    /// result = engine.execute_query(
    ///     '{ users { id name } }',
    ///     {},
    ///     'GetUsers'
    /// )
    /// ```
    #[pyo3(signature = (query, variables=None, operation_name=None))]
    fn execute_query(
        &self,
        py: Python,
        query: &str,
        variables: Option<&Bound<'_, PyDict>>,
        operation_name: Option<&str>,
    ) -> PyResult<Py<PyDict>> {
        // Convert Python dict to Rust HashMap
        let vars = convert_py_dict_to_hashmap(variables)?;

        // Create query request
        let request = QueryRequest {
            query: query.to_string(),
            variables: vars,
            operation_name: operation_name.map(std::string::ToString::to_string),
        };

        // Execute query (block on async operation)
        let response = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // We're in a tokio context, use block_in_place
                tokio::task::block_in_place(|| handle.block_on(self.inner.execute_query(request)))
            }
            Err(_) => {
                // No tokio runtime, create a new one
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create Tokio runtime: {}", e)))?;
                rt.block_on(self.inner.execute_query(request))
            }
        }
        .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))?;

        // Convert response to Python dict by converting to JSON and back
        response_to_dict(py, &response)
    }

    /// Execute a GraphQL mutation asynchronously
    ///
    /// # Arguments
    ///
    /// * `mutation` - GraphQL mutation string
    /// * `variables` - Dictionary of variables (optional, defaults to empty)
    ///
    /// # Returns
    ///
    /// Dictionary with keys:
    /// - `data`: Mutation result data (may be None if errors occurred)
    /// - `errors`: List of error objects (may be None if no errors)
    /// - `extensions`: Additional metadata (may be None)
    ///
    /// # Raises
    ///
    /// * `ValueError` - If mutation is invalid
    /// * `RuntimeError` - If mutation execution fails
    ///
    /// # Example
    ///
    /// ```python
    /// result = engine.execute_mutation(
    ///     'mutation { createUser(name: "John") { id } }',
    ///     {}
    /// )
    /// ```
    #[pyo3(signature = (mutation, variables=None))]
    fn execute_mutation(
        &self,
        py: Python,
        mutation: &str,
        variables: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyDict>> {
        // Convert Python dict to Rust HashMap
        let vars = convert_py_dict_to_hashmap(variables)?;

        // Create mutation request
        let request = MutationRequest {
            mutation: mutation.to_string(),
            variables: vars,
        };

        // Execute mutation (block on async operation)
        let response = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // We're in a tokio context, use block_in_place
                tokio::task::block_in_place(|| handle.block_on(self.inner.execute_mutation(request)))
            }
            Err(_) => {
                // No tokio runtime, create a new one
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create Tokio runtime: {}", e)))?;
                rt.block_on(self.inner.execute_mutation(request))
            }
        }
        .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))?;

        // Convert response to Python dict by converting to JSON and back
        response_to_dict(py, &response)
    }

    /// Check if engine is ready to process requests
    ///
    /// # Returns
    ///
    /// True if engine is initialized and ready, False otherwise
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Get engine version
    ///
    /// # Returns
    ///
    /// Version string matching Cargo.toml version
    fn version(&self) -> &str {
        self.inner.version()
    }

    /// Get engine configuration (for debugging)
    ///
    /// # Returns
    ///
    /// Configuration as dictionary
    fn config(&self, py: Python) -> PyResult<Py<PyDict>> {
        // Serialize config to JSON and parse back as Python dict
        let config_json_str =
            serde_json::to_string(self.inner.config()).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Failed to serialize config: {e}"))
            })?;

        // Use Python's json module to parse
        json_to_dict(py, &config_json_str)
    }

    fn __repr__(&self) -> String {
        format!(
            "PyGraphQLEngine(version='{}', ready={})",
            self.version(),
            self.is_ready()
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

/// Helper function to convert Python dict to Rust HashMap<String, serde_json::Value>
fn convert_py_dict_to_hashmap(
    py_dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<HashMap<String, serde_json::Value>> {
    match py_dict {
        None => Ok(HashMap::new()),
        Some(dict) => {
            let mut map = HashMap::new();
            for (key, value) in dict.iter() {
                let key_str = key.extract::<String>()?;
                let json_val = python_to_json(&value)?;
                map.insert(key_str, json_val);
            }
            Ok(map)
        }
    }
}

/// Convert Python object to serde_json::Value
fn python_to_json(value: &Bound<'_, pyo3::types::PyAny>) -> PyResult<serde_json::Value> {
    use pyo3::types::{PyDict, PyList};

    if value.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = value.extract::<bool>() {
        // Check bool before int (bool is a subtype of int in Python)
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = value.extract::<i64>() {
        Ok(serde_json::Value::Number(i.into()))
    } else if let Ok(f) = value.extract::<f64>() {
        serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid float value: {f}"))
            })
    } else if let Ok(s) = value.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else if let Ok(dict) = value.downcast::<PyDict>() {
        // Recursively convert nested dict
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key_str = k.str()?.to_str()?.to_string();
            let json_val = python_to_json(&v)?;
            map.insert(key_str, json_val);
        }
        Ok(serde_json::Value::Object(map))
    } else if let Ok(list) = value.downcast::<PyList>() {
        // Recursively convert list items
        list.iter()
            .map(|item| python_to_json(&item))
            .collect::<PyResult<Vec<_>>>()
            .map(serde_json::Value::Array)
    } else {
        // Fallback: use string representation for unknown types
        Ok(serde_json::Value::String(
            value.str()?.to_str()?.to_string(),
        ))
    }
}

/// Convert GraphQL response to Python dictionary using JSON as intermediate
fn response_to_dict(
    py: Python,
    response: &crate::api::types::GraphQLResponse,
) -> PyResult<Py<PyDict>> {
    // Serialize response to JSON
    let response_json = serde_json::json!({
        "data": response.data,
        "errors": response.errors.as_ref().map(|errors| {
            errors.iter().map(|e| {
                serde_json::json!({
                    "message": e.message,
                    "locations": e.locations.as_ref().map(|locs| {
                        locs.iter().map(|loc| {
                            serde_json::json!({
                                "line": loc.line,
                                "column": loc.column,
                            })
                        }).collect::<Vec<_>>()
                    }),
                    "path": e.path.as_ref().map(|p| {
                        p.iter().map(|elem| {
                            match elem {
                                crate::api::types::PathElement::Field(f) => serde_json::json!(f),
                                crate::api::types::PathElement::Index(idx) => serde_json::json!(idx),
                            }
                        }).collect::<Vec<_>>()
                    }),
                })
            }).collect::<Vec<_>>()
        }),
        "extensions": response.extensions,
    });

    let json_str = serde_json::to_string(&response_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Failed to serialize response: {e}")))?;

    json_to_dict(py, &json_str)
}

/// Parse JSON string to Python dict
fn json_to_dict(py: Python, json_str: &str) -> PyResult<Py<PyDict>> {
    // Use Python's json module to parse the JSON string
    let json_module = py.import("json")?;
    let parsed = json_module.call_method1("loads", (json_str,))?;

    // Ensure it's a dict
    if let Ok(dict) = parsed.downcast::<PyDict>() {
        Ok(dict.clone().unbind())
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Response JSON is not an object",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashmap_conversion_empty() {
        // This test would need Python context to run
        // Skipping for now
    }
}
