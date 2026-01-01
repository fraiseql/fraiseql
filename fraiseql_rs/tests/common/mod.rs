/// Common test utilities and fixtures
///
/// This module provides:
/// - TestDatabase container management
/// - Test fixtures and sample data
/// - Custom assertions
/// - Connection helpers
pub mod database;
pub mod fixtures;
pub mod assertions;

pub use database::TestDatabase;
pub use fixtures::*;
pub use assertions::*;

// Re-export commonly used test items
pub use tokio::test;
