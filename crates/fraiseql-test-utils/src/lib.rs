//! Common test utilities for FraiseQL
//!
//! This crate provides shared testing infrastructure for all FraiseQL crates,
//! including mock implementations, test fixtures, and common assertions.

pub mod assertions;
pub mod failing_adapter;
pub mod fixtures;
pub mod mock_db;

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
