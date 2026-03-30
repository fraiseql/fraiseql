//! SQL Server database adapter.
//!
//! Provides connection pooling and query execution for Microsoft SQL Server.
//! Uses `tiberius` for native Rust TDS protocol support.

mod adapter;
#[allow(dead_code)] // Reason: relay and param helpers wired for upcoming SqlServerAdapter features
mod helpers;
pub mod introspector;
mod where_generator;

#[cfg(test)]
mod adapter_tests;

pub use adapter::SqlServerAdapter;
pub use introspector::*;
pub use where_generator::SqlServerWhereGenerator;
