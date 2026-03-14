/// Common test utilities and fixtures
///
/// This module provides:
/// - TestDatabase container management
/// - Test fixtures and sample data
/// - Custom assertions
/// - Connection helpers

pub mod assertions;
pub mod database;
pub mod fixtures;

#[allow(unused_imports)]
pub use assertions::*;
#[allow(unused_imports)]
pub use database::TestDatabase;
#[allow(unused_imports)]
pub use fixtures::*;
#[allow(unused_imports)]
pub use tokio::test;
