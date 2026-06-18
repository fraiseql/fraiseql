//! MySQL database adapter.
//!
//! Provides connection pooling and query execution for MySQL.

mod adapter;
pub mod introspector;
mod where_generator;

pub use adapter::MySqlAdapter;
pub use introspector::*;
pub use where_generator::MySqlWhereGenerator;
