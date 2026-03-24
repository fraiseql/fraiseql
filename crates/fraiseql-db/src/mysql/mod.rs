//! MySQL database adapter.
//!
//! Provides connection pooling and query execution for MySQL.

mod adapter;
mod helpers;
mod introspector;
mod where_generator;

pub use adapter::MySqlAdapter;
pub use introspector::MySqlIntrospector;
pub use where_generator::MySqlWhereGenerator;
