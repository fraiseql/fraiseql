//! Utility modules for FraiseQL.
//!
//! # Modules
//!
//! - `casing`: String case conversion (snake_case, camelCase, PascalCase)
//! - `operators`: GraphQL operator registry and validation
//! - `vector`: pgvector support for similarity search
//! - `opaque_id`: ID encoding to prevent enumeration attacks

pub mod casing;
pub mod opaque_id;
pub mod operators;
pub mod vector;

// Re-export commonly used items
pub use casing::{normalize_field_path, to_camel_case, to_snake_case};
pub use opaque_id::OpaqueId;
pub use operators::{OperatorCategory, OperatorInfo, get_operator_info, is_operator};
pub use vector::{PlaceholderStyle, VectorParam, VectorQueryBuilder, VectorSearchQuery};
