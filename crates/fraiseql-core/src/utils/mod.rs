//! Utility modules for FraiseQL.
//!
//! # Modules
//!
//! - `casing`: String case conversion (`snake_case`, camelCase, `PascalCase`)
//! - `clock`: Clock abstraction for deterministic time-based testing
//! - `operators`: GraphQL operator registry and validation
//! - `opaque_id`: ID encoding to prevent enumeration attacks
//! - `text`: UTF-8-safe string truncation for error/log/audit display

pub mod casing;
pub mod clock;
pub mod opaque_id;
pub mod operators;
pub mod text;

// Re-export commonly used items
pub use casing::{normalize_field_path, to_camel_case, to_snake_case};
pub use clock::{Clock, SystemClock};
pub use opaque_id::OpaqueId;
pub use operators::{OperatorCategory, OperatorInfo, get_operator_info, is_operator};
pub use text::{truncate_at_char_boundary, truncate_for_display};

#[cfg(test)]
mod tests;
