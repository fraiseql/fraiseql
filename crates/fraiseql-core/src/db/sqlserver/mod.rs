//! SQL Server database adapter.
//!
//! Provides connection pooling and query execution for Microsoft SQL Server.
//! Uses `tiberius` for native Rust TDS protocol support.

mod adapter;
mod where_generator;

pub use adapter::SqlServerAdapter;
pub use where_generator::SqlServerWhereGenerator;
