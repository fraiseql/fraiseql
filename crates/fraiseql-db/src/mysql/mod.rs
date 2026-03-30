//! MySQL database adapter.
//!
//! Provides connection pooling and query execution for MySQL.

mod adapter;
#[allow(dead_code)] // Reason: relay pagination helpers wired for upcoming MySqlAdapter relay support
mod helpers;
pub mod introspector;
mod where_generator;

#[cfg(test)]
mod adapter_tests;

pub use adapter::MySqlAdapter;
pub use introspector::*;
pub use where_generator::MySqlWhereGenerator;
