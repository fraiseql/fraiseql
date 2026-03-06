//! MySQL database adapter.
//!
//! Provides connection pooling and query execution for MySQL.

mod adapter;
mod where_generator;

pub use adapter::MySqlAdapter;
pub use where_generator::MySqlWhereGenerator;
