//! # FraiseQL Test Utilities
//!
//! Shared testing infrastructure for all FraiseQL crates.
//!
//! ## Available helpers
//!
//! | Helper | Module | Purpose |
//! |--------|--------|---------|
//! | `assert_graphql_success()` | `assertions` | Assert response has no errors |
//! | `assert_no_graphql_errors()` | `assertions` | Assert `errors` field is absent |
//! | `assert_has_data()` | `assertions` | Assert `data` field is present and non-null |
//! | `assert_graphql_error_contains()` | `assertions` | Assert error message substring |
//! | `assert_graphql_error_code()` | `assertions` | Assert error extension code |
//! | `assert_field_path()` | `assertions` | Assert value at nested field path |
//! | `ManualClock` | (re-export) | Injectable clock for time-controlled tests |
//! | `FailingAdapter` | `failing_adapter` | A `DatabaseAdapter` that fails on demand |
//! | `TestSchemaBuilder` | `schema_builder` | Build compiled schemas in tests |
//!
//! ## Quick start
//!
//! ```
//! use fraiseql_test_utils::assert_graphql_success;
//!
//! assert_graphql_success(&serde_json::json!({ "data": { "ok": true } }));
//! ```
//!
//! Database URLs come from `fraiseql_test_support::{database_url, try_database_url}`.

pub mod assertions;
pub mod failing_adapter;
pub mod schema_builder;

// Re-export assertion helpers for direct use
pub use assertions::{
    assert_field_path, assert_graphql_error_code, assert_graphql_error_contains,
    assert_graphql_success, assert_has_data, assert_no_graphql_errors,
};
// Re-export clock utilities for time-controlled testing
pub use fraiseql_core::utils::clock::{Clock, ManualClock, SystemClock};
// Re-export schema builder types for convenience
pub use schema_builder::{
    TestFieldBuilder, TestMutationBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
