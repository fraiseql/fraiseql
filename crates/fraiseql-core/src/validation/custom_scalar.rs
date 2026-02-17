//! Custom scalar trait and validation for user-defined scalars.
//!
//! This module provides a trait-based system for defining custom GraphQL scalars
//! at runtime, allowing applications to implement their own validation logic.
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::validation::CustomScalar;
//!
//! struct Email;
//!
//! impl CustomScalar for Email {
//!     fn name(&self) -> &str {
//!         "Email"
//!     }
//!
//!     fn serialize(&self, value: &serde_json::Value) -> Result<serde_json::Value> {
//!         Ok(value.clone())
//!     }
//!
//!     fn parse_value(&self, value: &serde_json::Value) -> Result<serde_json::Value> {
//!         let str_val = value.as_str()
//!             .ok_or_else(|| FraiseQLError::parse("expected string"))?;
//!
//!         if !str_val.contains('@') {
//!             return Err(FraiseQLError::validation(
//!                 format!("invalid email format: {}", str_val)
//!             ));
//!         }
//!
//!         Ok(serde_json::Value::String(str_val.to_string()))
//!     }
//!
//!     fn parse_literal(&self, ast: &serde_json::Value) -> Result<serde_json::Value> {
//!         if let Some(m) = ast.as_object() {
//!             if let Some(value) = m.get("value") {
//!                 return self.parse_value(value);
//!             }
//!         }
//!         Err(FraiseQLError::parse("email literal must be string"))
//!     }
//! }
//! ```

use std::fmt;

use serde_json::Value;

use crate::error::Result;

/// Trait for implementing custom GraphQL scalar types.
///
/// Implement this trait to create custom scalars with validation logic.
/// Each method represents a different validation context in GraphQL.
pub trait CustomScalar: Send + Sync + fmt::Debug {
    /// Returns the GraphQL scalar type name (e.g., "Email", "Phone").
    fn name(&self) -> &str;

    /// Serialize a database value to a GraphQL response value.
    ///
    /// This is called when returning values from resolvers.
    fn serialize(&self, value: &Value) -> Result<Value>;

    /// Parse a variable value from a GraphQL operation.
    ///
    /// This is called when parsing variables in GraphQL operations.
    /// For example: `{ query: getUser($email: Email) }`
    fn parse_value(&self, value: &Value) -> Result<Value>;

    /// Parse a literal value from a GraphQL query string.
    ///
    /// For example: `{ user(email: "test@example.com") }`
    /// The `ast` parameter is typically an object with a "value" key for simple types.
    fn parse_literal(&self, ast: &Value) -> Result<Value>;
}

/// Result of custom scalar validation.
pub type CustomScalarResult = Result<Value>;
