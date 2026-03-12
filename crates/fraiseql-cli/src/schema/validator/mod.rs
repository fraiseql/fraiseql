//! Enhanced Schema Validation
//!
//! Provides detailed validation error reporting with line numbers and context.

pub mod schema_validator;
pub mod sql_identifier;
pub mod types;

#[cfg(test)]
mod tests;

pub use schema_validator::SchemaValidator;
pub use sql_identifier::validate_sql_identifier;
pub use types::{ErrorSeverity, ValidationError, ValidationReport};
