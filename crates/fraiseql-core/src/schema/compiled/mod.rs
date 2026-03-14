//! Compiled schema types - pure Rust, no authoring-language references.
//!
//! These types represent GraphQL schemas after compilation from authoring languages.
//! All data is owned by Rust - no foreign object references.

pub mod argument;
pub mod directive;
pub mod mutation;
pub mod query;
pub mod schema;
pub mod validation;

#[cfg(test)]
mod tests;

pub use argument::{ArgumentDefinition, AutoParams};
pub use directive::{DirectiveDefinition, DirectiveLocationKind};
pub use mutation::{MutationDefinition, MutationOperation};
pub use query::{CursorType, QueryDefinition};
pub use schema::{CURRENT_SCHEMA_FORMAT_VERSION, CompiledSchema};
pub use validation::is_safe_sql_identifier;
