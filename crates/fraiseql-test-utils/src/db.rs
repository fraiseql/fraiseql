//! Database URL resolution for test infrastructure.
//!
//! The canonical implementation now lives in [`fraiseql_test_support`] — the single
//! provisioning authority — and is re-exported here so existing callers of
//! `fraiseql_test_utils::{database_url, try_database_url}` keep working unchanged.

pub use fraiseql_test_support::{database_url, try_database_url};
