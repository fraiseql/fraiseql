//! Schema compilation helpers for tests.

use fraiseql_core::{compiler::Compiler, CompiledSchema};

/// Compile a raw schema JSON string into a [`CompiledSchema`] for use in tests.
///
/// # Panics
///
/// Panics with a descriptive message if the schema JSON is invalid.
///
/// # Example
///
/// ```ignore
/// use fraiseql_test_utils::schema::setup_test_schema;
///
/// let schema = setup_test_schema(r#"{"types": [], "queries": []}"#);
/// ```
#[must_use]
pub fn setup_test_schema(schema_json: &str) -> CompiledSchema {
    Compiler::new().compile(schema_json).expect("test schema must be valid")
}
