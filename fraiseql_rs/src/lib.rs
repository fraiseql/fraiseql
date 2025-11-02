use pyo3::prelude::*;
use pyo3::types::PyDict;

// Sub-modules
mod camel_case;
pub mod core;
mod json;
mod json_transform;
pub mod pipeline;

/// Version of the fraiseql_rs module
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Convert a snake_case string to camelCase
///
/// Examples:
///     >>> to_camel_case("user_name")
///     "userName"
///     >>> to_camel_case("email_address")
///     "emailAddress"
///
/// Args:
///     s: The snake_case string to convert
///
/// Returns:
///     The camelCase string
#[pyfunction]
fn to_camel_case(s: &str) -> String {
    camel_case::to_camel_case(s)
}

/// Transform all keys in a dictionary from snake_case to camelCase
///
/// Examples:
///     >>> transform_keys({"user_name": "John", "email_address": "..."})
///     {"userName": "John", "emailAddress": "..."}
///
/// Args:
///     obj: Dictionary with snake_case keys
///     recursive: If True, recursively transform nested dicts and lists (default: False)
///
/// Returns:
///     New dictionary with camelCase keys
#[pyfunction]
#[pyo3(signature = (obj, recursive=false))]
fn transform_keys(py: Python, obj: &Bound<'_, PyDict>, recursive: bool) -> PyResult<Py<PyDict>> {
    camel_case::transform_dict_keys(py, obj, recursive)
}

/// Transform a JSON string by converting all keys from snake_case to camelCase
///
/// This is the fastest way to transform JSON as it avoids Python dict conversion.
///
/// Examples:
///     >>> transform_json('{"user_name": "John", "email_address": "john@example.com"}')
///     '{"userName":"John","emailAddress":"john@example.com"}'
///
/// Args:
///     json_str: JSON string with snake_case keys
///
/// Returns:
///     Transformed JSON string with camelCase keys
///
/// Raises:
///     ValueError: If json_str is not valid JSON
#[pyfunction]
fn transform_json(json_str: &str) -> PyResult<String> {
    json_transform::transform_json_string(json_str)
}

/// Simple test function to verify PyO3 is working
#[pyfunction]
fn test_function() -> PyResult<&'static str> {
    Ok("Hello from Rust!")
}

//----------------------------------------------------------------------------
// Internal testing exports (for unit tests)
//----------------------------------------------------------------------------

/// Python wrapper for Arena (for testing)
///
/// This is NOT thread-safe and should only be used for testing!
#[pyclass(unsendable)]
struct Arena {
    inner: core::Arena,
}

#[pymethods]
impl Arena {
    #[new]
    fn new() -> Self {
        Arena {
            inner: core::Arena::with_capacity(8192),
        }
    }
}

/// Multi-architecture snake_to_camel (for testing)
///
/// This automatically dispatches to the best implementation for the current CPU.
#[pyfunction]
fn test_snake_to_camel(input: &[u8], arena: &Arena) -> Vec<u8> {
    let result = core::camel::snake_to_camel(input, &arena.inner);
    result.to_vec()
}

/// Build complete GraphQL response from PostgreSQL JSON rows
///
/// This is the unified API for building GraphQL responses from database JSON.
/// It handles camelCase conversion, __typename injection, and field projection.
///
/// Examples:
///     >>> result = build_graphql_response(
///     ...     json_strings=['{"user_id": 1}', '{"user_id": 2}'],
///     ...     field_name="users",
///     ...     type_name="User",
///     ...     field_paths=None
///     ... )
///     >>> result.decode('utf-8')
///     '{"data":{"users":[{"__typename":"User","userId":1},{"__typename":"User","userId":2}]}}'
///
/// Args:
///     json_strings: List of JSON strings from database (snake_case keys)
///     field_name: GraphQL field name (e.g., "users", "user")
///     type_name: Optional type name for __typename injection
///     field_paths: Optional field projection paths
///
/// Returns:
///     UTF-8 encoded GraphQL response bytes ready for HTTP
#[pyfunction]
pub fn build_graphql_response(
    json_strings: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
) -> PyResult<Vec<u8>> {
    pipeline::builder::build_graphql_response(
        json_strings,
        field_name,
        type_name,
        field_paths,
    )
}


/// A Python module implemented in Rust for ultra-fast GraphQL transformations.
///
/// This module provides:
/// - snake_case → camelCase conversion (SIMD optimized)
/// - JSON parsing and transformation (zero-copy)
/// - __typename injection
/// - Nested array resolution for list[CustomType]
/// - Nested object resolution
///
/// Performance target: 10-50x faster than pure Python implementation
#[pymodule]
fn _fraiseql_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Add version string
    m.add("__version__", VERSION)?;

    // Module metadata
    m.add("__doc__", "Ultra-fast GraphQL JSON transformation in Rust")?;
    m.add("__author__", "FraiseQL Contributors")?;

    // Set __all__ to control what's exported
    m.add("__all__", vec![
        "__version__",
        "__doc__",
        "__author__",
        "to_camel_case",
        "transform_keys",
        "transform_json",
        "test_function",
        "build_graphql_response",
    ])?;

    // Add functions
    m.add_function(wrap_pyfunction!(to_camel_case, m)?)?;
    m.add_function(wrap_pyfunction!(transform_keys, m)?)?;
    m.add_function(wrap_pyfunction!(transform_json, m)?)?;
    m.add_function(wrap_pyfunction!(test_function, m)?)?;

    // Add zero-copy pipeline exports
    m.add_function(wrap_pyfunction!(build_graphql_response, m)?)?;

    // Add internal testing exports (not in __all__)
    m.add_class::<Arena>()?;
    m.add_function(wrap_pyfunction!(test_snake_to_camel, m)?)?;

    Ok(())
}
