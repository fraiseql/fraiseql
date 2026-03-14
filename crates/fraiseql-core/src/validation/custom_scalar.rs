//! Custom scalar trait and validation for user-defined scalars.
//!
//! This module provides a trait-based system for defining custom GraphQL scalars
//! at runtime, allowing applications to implement their own validation logic.
//!
//! # Example
//!
//! ```
//! use fraiseql_core::validation::CustomScalar;
//! use fraiseql_core::error::{FraiseQLError, Result};
//! use serde_json::Value;
//!
//! #[derive(Debug)]
//! struct Email;
//!
//! impl CustomScalar for Email {
//!     fn name(&self) -> &str {
//!         "Email"
//!     }
//!
//!     fn serialize(&self, value: &Value) -> Result<Value> {
//!         Ok(value.clone())
//!     }
//!
//!     fn parse_value(&self, value: &Value) -> Result<Value> {
//!         let str_val = value.as_str()
//!             .ok_or_else(|| FraiseQLError::parse("expected string"))?;
//!
//!         if !str_val.contains('@') {
//!             return Err(FraiseQLError::validation(
//!                 format!("invalid email format: {}", str_val)
//!             ));
//!         }
//!
//!         Ok(Value::String(str_val.to_string()))
//!     }
//!
//!     fn parse_literal(&self, ast: &Value) -> Result<Value> {
//!         self.parse_value(ast)
//!     }
//! }
//!
//! let email = Email;
//! assert_eq!(email.name(), "Email");
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::{Value, json};

    use super::CustomScalar;
    use crate::error::{FraiseQLError, Result};

    /// Minimal email scalar for testing the trait.
    #[derive(Debug)]
    struct EmailScalar;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: test impl of trait returning a literal
    impl CustomScalar for EmailScalar {
        fn name(&self) -> &str {
            "Email"
        }

        fn serialize(&self, value: &Value) -> Result<Value> {
            Ok(value.clone())
        }

        fn parse_value(&self, value: &Value) -> Result<Value> {
            let s = value
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Email must be a string"))?;
            if !s.contains('@') {
                return Err(FraiseQLError::validation(format!("invalid email: {s}")));
            }
            Ok(Value::String(s.to_string()))
        }

        fn parse_literal(&self, ast: &Value) -> Result<Value> {
            self.parse_value(ast)
        }
    }

    #[test]
    fn test_name() {
        let scalar = EmailScalar;
        assert_eq!(scalar.name(), "Email");
    }

    #[test]
    fn test_serialize_returns_value_unchanged() {
        let scalar = EmailScalar;
        let v = json!("user@example.com");
        assert_eq!(scalar.serialize(&v).unwrap(), v);
    }

    #[test]
    fn test_parse_value_valid_email() {
        let scalar = EmailScalar;
        let v = json!("user@example.com");
        assert_eq!(scalar.parse_value(&v).unwrap(), v);
    }

    #[test]
    fn test_parse_value_invalid_email_no_at() {
        let scalar = EmailScalar;
        let v = json!("notanemail");
        assert!(scalar.parse_value(&v).is_err());
    }

    #[test]
    fn test_parse_value_non_string_input() {
        let scalar = EmailScalar;
        let v = json!(42);
        let err = scalar.parse_value(&v).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("string") || msg.contains("Email"), "unexpected: {msg}");
    }

    #[test]
    fn test_parse_literal_delegates_to_parse_value() {
        let scalar = EmailScalar;
        let v = json!("lit@example.com");
        assert_eq!(scalar.parse_literal(&v).unwrap(), v);
    }

    #[test]
    fn test_custom_scalar_result_type_alias_is_result_value() {
        let result: super::CustomScalarResult = Ok(json!("ok"));
        assert!(result.is_ok());
    }
}
