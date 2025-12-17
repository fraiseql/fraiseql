use pyo3::prelude::*;
use pyo3::types::PyDict;

// Sub-modules
mod camel_case;
pub mod cascade;
pub mod core;
pub mod json_transform;
pub mod mutation;
pub mod pipeline;
pub mod schema_registry;

/// Version of the fraiseql_rs module
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Type alias for multi-field response field definition.
///
/// Represents a single field in a multi-field GraphQL query response:
/// - String: field_name (e.g., "users")
/// - String: type_name (e.g., "User")
/// - Vec<String>: json_rows (raw JSON from database)
/// - Option<String>: field_selections (JSON-encoded field selection metadata)
/// - Option<bool>: is_list (whether field returns list or single object)
type MultiFieldDef = (String, String, Vec<String>, Option<String>, Option<bool>);

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
/// It handles camelCase conversion, __typename injection, field projection, and aliases.
///
/// Examples:
///     >>> result = build_graphql_response(
///     ...     json_strings=['{"user_id": 1}', '{"user_id": 2}'],
///     ...     field_name="users",
///     ...     type_name="User",
///     ...     field_paths=None,
///     ...     field_selections=None,
///     ...     is_list=True,
///     ...     include_graphql_wrapper=True
///     ... )
///     >>> result.decode('utf-8')
///     '{"data":{"users":[{"__typename":"User","userId":1},{"__typename":"User","userId":2}]}}'
///
/// Args:
///     json_strings: List of JSON strings from database (snake_case keys)
///     field_name: GraphQL field name (e.g., "users", "user")
///     type_name: Optional type name for __typename injection
///     field_paths: Optional field projection paths (DEPRECATED - use field_selections)
///     field_selections: Optional field selections JSON string with aliases and type info
///     is_list: True for list responses (always array), False for single object responses
///     include_graphql_wrapper: True to wrap in {"data":{"field_name":...}} (default), False for field-only mode
///
/// Returns:
///     UTF-8 encoded GraphQL response bytes ready for HTTP
#[pyfunction]
#[pyo3(signature = (json_strings, field_name, type_name=None, field_paths=None, field_selections=None, is_list=None, include_graphql_wrapper=None))]
pub fn build_graphql_response(
    json_strings: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
    field_selections: Option<String>,
    is_list: Option<bool>,
    include_graphql_wrapper: Option<bool>,
) -> PyResult<Vec<u8>> {
    // Parse field_selections JSON string if provided
    let selections_json = match field_selections {
        Some(json_str) => {
            serde_json::from_str::<Vec<serde_json::Value>>(&json_str).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid field_selections JSON: {}",
                    e
                ))
            })?
        }
        None => Vec::new(),
    };

    let selections_opt = if selections_json.is_empty() {
        None
    } else {
        Some(selections_json)
    };

    pipeline::builder::build_graphql_response(
        json_strings,
        field_name,
        type_name,
        field_paths,
        selections_opt,
        is_list,
        include_graphql_wrapper,
    )
}

/// Initialize the GraphQL schema registry from Python
///
/// This function should be called once at application startup to initialize
/// the schema registry with type metadata from the GraphQL schema.
///
/// The registry stores type information for:
/// - Object types and their fields
/// - Nested object relationships
/// - List types
/// - Type metadata for runtime resolution
///
/// Examples:
///     >>> import json
///     >>> schema_ir = {"version": "1.0", "features": ["type_resolution"], "types": {...}}
///     >>> initialize_schema_registry(json.dumps(schema_ir))
///
/// Args:
///     schema_json: JSON string containing the schema IR from SchemaSerializer
///
/// Raises:
///     ValueError: If JSON is malformed, missing required fields, or has unsupported version
///     RuntimeError: If registry is already initialized
///
/// Returns:
///     None on success
#[pyfunction]
pub fn initialize_schema_registry(schema_json: String) -> PyResult<()> {
    // Validate input
    if schema_json.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Schema JSON cannot be empty",
        ));
    }

    // Parse the schema JSON with detailed error messages
    let registry = schema_registry::SchemaRegistry::from_json(&schema_json).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "Failed to parse schema JSON: {}. Expected format: {{\"version\": \"1.0\", \"features\": [...], \"types\": {{...}}}}",
            e
        ))
    })?;

    // Validate version (warn if using newer version)
    if registry.version() != "1.0" {
        eprintln!(
            "Warning: Schema IR version '{}' may not be fully compatible with Rust registry version 1.0",
            registry.version()
        );
    }

    // Log schema statistics (to stderr for visibility)
    eprintln!(
        "Initializing schema registry: version={}, features=[{}], types={}",
        registry.version(),
        registry
            .features
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        registry.type_count()
    );

    // Initialize the global registry
    let success = schema_registry::initialize_registry(registry);

    if !success {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Schema registry is already initialized. Re-initialization is not allowed. \
             This is expected behavior - the registry is a global singleton that can only be set once.",
        ));
    }

    eprintln!("✓ Schema registry initialized successfully");
    Ok(())
}

/// Filter GraphQL Cascade data based on field selections
///
/// This function filters cascade data (updated entities, deletions, invalidations)
/// based on GraphQL query field selections to reduce response payload size.
///
/// Examples:
///     >>> cascade_json = '{"updated": [{"__typename": "Post", "id": "1"}]}'
///     >>> selections = '{"fields": ["updated"], "updated": {"include": ["Post"]}}'
///     >>> result = filter_cascade_data(cascade_json, selections)
///
/// Args:
///     cascade_json: JSON string of cascade data from PostgreSQL
///     selections_json: Optional JSON string of GraphQL field selections
///
/// Returns:
///     Filtered cascade data as JSON string
///
/// Raises:
///     ValueError: If JSON is malformed or filtering fails
#[pyfunction]
#[pyo3(signature = (cascade_json, selections_json=None))]
pub fn filter_cascade_data(cascade_json: &str, selections_json: Option<&str>) -> PyResult<String> {
    cascade::filter_cascade_data(cascade_json, selections_json)
        .map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Build complete GraphQL mutation response from PostgreSQL JSON
///
/// This function transforms PostgreSQL mutation_response JSON into GraphQL responses.
/// It supports both simple format (just entity JSONB) and full format with status.
///
/// Examples:
///     >>> # Simple format
///     >>> result = build_mutation_response(
///     ...     '{"id": "123", "name": "John"}',
///     ...     "createUser",
///     ...     "CreateUserSuccess",
///     ...     "CreateUserError",
///     ...     "user",
///     ...     "User",
///     ...     None
///     ... )
///
/// Args:
///     mutation_json: Raw JSON from PostgreSQL (simple or full format)
///     field_name: GraphQL field name (e.g., "createUser")
///     success_type: Success type name (e.g., "CreateUserSuccess")
///     error_type: Error type name (e.g., "CreateUserError")
///     entity_field_name: Field name for entity (e.g., "user")
///     entity_type: Entity type for __typename (e.g., "User") - REQUIRED for simple format
///     cascade_selections: Optional cascade selections JSON string
///
/// Returns:
///     UTF-8 encoded GraphQL response bytes ready for HTTP
///
/// Raises:
///     ValueError: If JSON is malformed or transformation fails
#[pyfunction]
#[pyo3(signature = (mutation_json, field_name, success_type, error_type, entity_field_name=None, entity_type=None, cascade_selections=None, auto_camel_case=true, success_type_fields=None, error_type_fields=None))]
#[allow(clippy::too_many_arguments)]
pub fn build_mutation_response(
    mutation_json: &str,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    entity_type: Option<&str>,
    cascade_selections: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<Vec<String>>,
    error_type_fields: Option<Vec<String>>,
) -> PyResult<Vec<u8>> {
    mutation::build_mutation_response(
        mutation_json,
        field_name,
        success_type,
        error_type,
        entity_field_name,
        entity_type,
        cascade_selections,
        auto_camel_case,
        success_type_fields,
        error_type_fields,
    )
    .map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Reset the schema registry for testing purposes
///
/// **WARNING**: This function is only intended for use in tests.
/// It clears the global schema registry, allowing it to be re-initialized
/// with a different schema.
///
/// Calling this in production can cause undefined behavior if other code
/// holds references to the registry.
///
/// Examples:
///     >>> from fraiseql import _fraiseql_rs
///     >>> _fraiseql_rs.reset_schema_registry_for_testing()
///     >>> # Now you can call initialize_schema_registry with a new schema
///
/// Returns:
///     None
#[pyfunction]
pub fn reset_schema_registry_for_testing() -> PyResult<()> {
    schema_registry::reset_for_testing();
    Ok(())
}

/// Check if the schema registry is initialized
///
/// Returns:
///     True if the registry has been initialized, False otherwise
#[pyfunction]
pub fn is_schema_registry_initialized() -> bool {
    schema_registry::is_initialized()
}

/// Build complete multi-field GraphQL response from PostgreSQL JSON rows
///
/// This function handles multi-field queries entirely in Rust, bypassing graphql-core
/// to avoid type validation errors. It processes multiple root fields and combines
/// them into a single {"data": {...}} response.
///
/// Examples:
///     >>> # Query: { dnsServers { id } gateways { id } }
///     >>> result = build_multi_field_response([
///     ...     ("dnsServers", "DnsServer", ['{"id": 1}', '{"id": 2}'], '["id"]', True),
///     ...     ("gateways", "Gateway", ['{"id": 10}'], '["id"]', True)
///     ... ])
///     >>> result.decode('utf-8')
///     '{"data":{"dnsServers":[{"__typename":"DnsServer","id":1},{"__typename":"DnsServer","id":2}],"gateways":[{"__typename":"Gateway","id":10}]}}'
///
/// Args:
///     fields: List of tuples, each containing:
///         - field_name (str): GraphQL field name (e.g., "dnsServers")
///         - type_name (str): GraphQL type name (e.g., "DnsServer")
///         - json_rows (list[str]): List of JSON strings from database
///         - field_selections (str): JSON string with field selections info
///         - is_list (bool): True for list fields, False for single object fields
///
/// Returns:
///     UTF-8 encoded GraphQL response bytes: {"data": {"field1": [...], "field2": [...]}}
///
/// Raises:
///     ValueError: If field data is malformed or transformation fails
#[pyfunction]
pub fn build_multi_field_response(
    fields: Vec<MultiFieldDef>,
) -> PyResult<Vec<u8>> {
    pipeline::builder::build_multi_field_response(fields)
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
#[pyo3(name = "_fraiseql_rs")]
fn fraiseql_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Add version string
    m.add("__version__", VERSION)?;

    // Module metadata
    m.add("__doc__", "Ultra-fast GraphQL JSON transformation in Rust")?;
    m.add("__author__", "FraiseQL Contributors")?;

    // Set __all__ to control what's exported
    m.add(
        "__all__",
        vec![
            "__version__",
            "__doc__",
            "__author__",
            "to_camel_case",
            "transform_keys",
            "transform_json",
            "test_function",
            "build_graphql_response",
            "build_multi_field_response",
            "initialize_schema_registry",
            "filter_cascade_data",
            "build_mutation_response",
        ],
    )?;

    // Add functions
    m.add_function(wrap_pyfunction!(to_camel_case, m)?)?;
    m.add_function(wrap_pyfunction!(transform_keys, m)?)?;
    m.add_function(wrap_pyfunction!(transform_json, m)?)?;
    m.add_function(wrap_pyfunction!(test_function, m)?)?;

    // Add zero-copy pipeline exports
    m.add_function(wrap_pyfunction!(build_graphql_response, m)?)?;
    m.add_function(wrap_pyfunction!(build_multi_field_response, m)?)?;

    // Add schema registry initialization
    m.add_function(wrap_pyfunction!(initialize_schema_registry, m)?)?;

    // Add cascade filtering
    m.add_function(wrap_pyfunction!(filter_cascade_data, m)?)?;

    // Add mutation response building
    m.add_function(wrap_pyfunction!(build_mutation_response, m)?)?;

    // Add internal testing exports (not in __all__)
    m.add_class::<Arena>()?;
    m.add_function(wrap_pyfunction!(test_snake_to_camel, m)?)?;

    // Add testing utilities (for pytest fixtures)
    m.add_function(wrap_pyfunction!(reset_schema_registry_for_testing, m)?)?;
    m.add_function(wrap_pyfunction!(is_schema_registry_initialized, m)?)?;

    Ok(())
}
