use pyo3::prelude::*;
use pyo3::types::PyDict;

// Sub-modules
mod camel_case;
mod json_transform;
mod typename_injection;
mod schema_registry;

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

/// Transform JSON with __typename injection for GraphQL
///
/// Combines camelCase transformation with __typename field injection
/// for proper GraphQL type identification and Apollo Client caching.
///
/// Examples:
///     >>> transform_json_with_typename('{"user_id": 1}', "User")
///     '{"__typename":"User","userId":1}'
///
///     >>> type_map = {"$": "User", "posts": "Post"}
///     >>> transform_json_with_typename('{"user_id": 1, "posts": [...]}', type_map)
///     '{"__typename":"User","userId":1,"posts":[{"__typename":"Post",...}]}'
///
/// Args:
///     json_str: JSON string with snake_case keys
///     type_info: Type information for __typename injection
///         - str: typename for root object (e.g., "User")
///         - dict: type map for nested objects (e.g., {"$": "User", "posts": "Post"})
///         - None: no typename injection (behaves like transform_json)
///
/// Returns:
///     Transformed JSON string with camelCase keys and __typename fields
///
/// Raises:
///     ValueError: If json_str is not valid JSON or type_info is invalid
#[pyfunction]
fn transform_json_with_typename(json_str: &str, type_info: &Bound<'_, PyAny>) -> PyResult<String> {
    typename_injection::transform_json_with_typename(json_str, type_info)
}

/// Transform JSON with schema-based automatic type resolution
///
/// Uses a GraphQL-like schema definition to automatically detect and apply
/// __typename to objects and arrays. This is more ergonomic than manual
/// type maps for complex schemas.
///
/// Examples:
///     >>> schema = {
///     ...     "User": {
///     ...         "fields": {
///     ...             "id": "Int",
///     ...             "name": "String",
///     ...             "posts": "[Post]"
///     ...         }
///     ...     },
///     ...     "Post": {
///     ...         "fields": {
///     ...             "id": "Int",
///     ...             "title": "String"
///     ...         }
///     ...     }
///     ... }
///     >>> transform_with_schema('{"id": 1, "posts": [...]}', "User", schema)
///     '{"__typename":"User","id":1,"posts":[{"__typename":"Post",...}]}'
///
/// Args:
///     json_str: JSON string with snake_case keys
///     root_type: Root type name from schema (e.g., "User")
///     schema: Schema definition dict mapping type names to field definitions
///
/// Returns:
///     Transformed JSON string with camelCase keys and __typename fields
///
/// Raises:
///     ValueError: If json_str is not valid JSON or schema is invalid
#[pyfunction]
fn transform_with_schema(
    json_str: &str,
    root_type: &str,
    schema: &Bound<'_, PyDict>,
) -> PyResult<String> {
    schema_registry::transform_with_schema(json_str, root_type, schema)
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
fn fraiseql_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Add version string
    m.add("__version__", VERSION)?;

    // Module metadata
    m.add("__doc__", "Ultra-fast GraphQL JSON transformation in Rust")?;
    m.add("__author__", "FraiseQL Contributors")?;

    // Add functions
    m.add_function(wrap_pyfunction!(to_camel_case, m)?)?;
    m.add_function(wrap_pyfunction!(transform_keys, m)?)?;
    m.add_function(wrap_pyfunction!(transform_json, m)?)?;
    m.add_function(wrap_pyfunction!(transform_json_with_typename, m)?)?;
    m.add_function(wrap_pyfunction!(transform_with_schema, m)?)?;

    // Add classes
    m.add_class::<schema_registry::SchemaRegistry>()?;

    Ok(())
}
