//! Convenience re-exports for the most commonly used fraiseql-core types.
//!
//! Import with: `use fraiseql_core::prelude::*;`

pub use crate::config::FraiseQLConfig;
pub use crate::schema::CompiledSchema;
pub use fraiseql_db::DatabaseAdapter;
pub use fraiseql_error::{FraiseQLError, Result};
