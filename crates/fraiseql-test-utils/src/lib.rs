//! # FraiseQL Test Utilities
//!
//! Shared testing infrastructure for all FraiseQL crates.
//!
//! ## Available helpers
//!
//! | Helper | Module | Purpose |
//! |--------|--------|---------|
//! | `database_url()` | `db` | Resolve `DATABASE_URL` or panic with actionable message |
//! | `setup_test_schema()` | `schema` | Compile a schema string into `CompiledSchema` |
//! | `assert_graphql_success()` | `assertions` | Assert response has no errors |
//! | `assert_no_graphql_errors()` | `assertions` | Assert `errors` field is absent |
//! | `assert_has_data()` | `assertions` | Assert `data` field is present and non-null |
//! | `assert_graphql_error_contains()` | `assertions` | Assert error message substring |
//! | `assert_graphql_error_code()` | `assertions` | Assert error extension code |
//! | `assert_field_path()` | `assertions` | Assert value at nested field path |
//! | `ManualClock` | (re-export) | Injectable clock for time-controlled tests |
//! | `get_test_id()` | `observers` | Unique UUID string for test data namespacing |
//! | `TestSagaExecutor` | `saga` | Execute saga steps in tests |
//!
//! ## Quick start
//!
//! ```ignore
//! use fraiseql_test_utils::{database_url, assert_graphql_success};
//!
//! #[tokio::test]
//! #[ignore = "requires DATABASE_URL"]
//! async fn my_integration_test() {
//!     let url = database_url();
//!     // ...
//! }
//! ```

pub mod assertions;
pub mod db;
pub mod failing_adapter;
pub mod fixtures;
pub mod mock_db;
pub mod observers;
pub mod saga;
pub mod schema;

// Re-export assertion helpers for direct use
pub use assertions::{
    assert_field_path, assert_graphql_error_code, assert_graphql_error_contains,
    assert_graphql_success, assert_has_data, assert_no_graphql_errors,
};

// Re-export database URL helper
pub use db::database_url;

// Re-export clock utilities for time-controlled testing
pub use fraiseql_core::utils::clock::{Clock, ManualClock, SystemClock};

// Re-export observer helpers
pub use observers::get_test_id;

// Re-export saga types for convenience
pub use saga::{SagaStepDef, SagaStepResult, StepStatusEnum, TestSagaExecutor};

/// Setup test environment
///
/// # Example
///
/// ```ignore
/// #[tokio::test]
/// async fn my_test() {
///     setup_test_env();
///     // Test code here
/// }
/// ```
pub const fn setup_test_env() {
    // Test environment setup hook (extensible for future use)
}

/// Create a temporary directory for test files
///
/// # Panics
///
/// Panics if the OS fails to create a temporary directory.
///
/// # Example
///
/// ```ignore
/// let temp_dir = create_temp_dir();
/// let file_path = temp_dir.path().join("test.json");
/// ```
#[must_use]
pub fn create_temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("failed to create temp directory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_test_env() {
        setup_test_env();
    }

    #[test]
    fn test_create_temp_dir() {
        let temp_dir = create_temp_dir();
        assert!(temp_dir.path().exists());
    }
}
