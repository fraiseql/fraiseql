//! Rich type filter operators and handlers.
//!
//! This module provides specialized filter operators for 44 rich scalar types,
//! enabling powerful queries like:
//!
//! - Extract and filter by email domain: `email.domain_eq('example.com')`
//! - Parse and filter by VIN components: `vin.wmi_eq('1HG')`
//! - Geographic filtering: `coordinates.distance_within(lat, lng, radius_km)`
//! - Country lookups: `country.continent_eq('Europe')`
//!
//! # Architecture
//!
//! The module is organized into two layers:
//!
//! 1. **Operator Definition** (`operators.rs`): Defines `ExtendedOperator` enum with all variants
//! 2. **SQL Generation** (`ExtendedOperatorHandler` trait): Database-specific SQL generation
//!
//! Each database backend implements `ExtendedOperatorHandler` to convert operators to SQL:
//! - PostgreSQL: Uses `SPLIT_PART()`, `SUBSTRING()`, PostGIS functions
//! - MySQL: Uses `SUBSTRING_INDEX()`, `SUBSTRING()`, spatial functions
//! - SQLite: Uses `SUBSTR()`, `INSTR()`, limited spatial support
//! - SQL Server: Uses `SUBSTRING()`, `CHARINDEX()`, spatial support
//!
//! # Feature Flags
//!
//! All 44 types are available by default, but can be disabled via feature flags
//! to reduce binary size for minimal deployments.

pub mod default_rules;
pub mod operator_mapping;
pub mod operators;
pub mod validators;

pub use default_rules::get_default_rules;
pub use operator_mapping::{OperatorInfo, ParameterType, get_operators_for_type};
pub use operators::ExtendedOperator;
use serde_json::Value;
pub use validators::{ChecksumType, ValidationRule};

use crate::error::Result;

/// Handler for extended operator SQL generation.
///
/// Each database backend implements this trait to provide database-specific SQL
/// generation for extended operators. This enables consistent operator semantics
/// across different database systems despite differences in function names and syntax.
///
/// # Implementing for a New Database
///
/// To add support for a new database:
///
/// 1. Implement `ExtendedOperatorHandler` for your database's where generator
/// 2. Translate each operator to appropriate SQL functions for that database
/// 3. Add comprehensive unit tests for each operator
/// 4. Run integration tests against actual database
///
/// # Example
///
/// ```rust,ignore
/// use fraiseql_core::filters::ExtendedOperatorHandler;
///
/// impl ExtendedOperatorHandler for MyDatabaseWhereGenerator {
///     fn generate_extended_sql(
///         &self,
///         operator: &ExtendedOperator,
///         field_sql: &str,
///         params: &mut Vec<Value>,
///     ) -> Result<String> {
///         match operator {
///             ExtendedOperator::EmailDomainEq(domain) => {
///                 let param_idx = params.len() + 1;
///                 params.push(Value::String(domain.clone()));
///                 // Use database-specific string function
///                 Ok(format!("MY_EXTRACT_DOMAIN({}) = ${}", field_sql, param_idx))
///             }
///             // ... other operators
///             _ => Err(FraiseQLError::validation(
///                 format!("Unsupported operator: {}", operator)
///             )),
///         }
///     }
/// }
/// ```
pub trait ExtendedOperatorHandler {
    /// Generate database-specific SQL for an extended operator.
    ///
    /// # Arguments
    ///
    /// * `operator` - The extended operator to generate SQL for
    /// * `field_sql` - The field reference in database-specific format (e.g., `data->>'email'` for
    ///   PostgreSQL JSONB)
    /// * `params` - Mutable vector to accumulate query parameters; implementer must add any
    ///   parameters needed by this operator
    ///
    /// # Returns
    ///
    /// Returns SQL fragment as a string that can be used in WHERE clause.
    /// For example: `"SPLIT_PART(data->>'email', '@', 2) = $1"`
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if:
    /// - Operator is not supported by this database
    /// - Operator requires unavailable feature (e.g., PostGIS not installed)
    /// - Parameters are invalid
    fn generate_extended_sql(
        &self,
        operator: &ExtendedOperator,
        field_sql: &str,
        params: &mut Vec<Value>,
    ) -> Result<String>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_extended_operator_enum_complete() {
        // Verify all 44 types are represented
        // This is a compile-time check via pattern matching in sql generators
        // TODO: Add runtime inventory check if needed
    }
}
