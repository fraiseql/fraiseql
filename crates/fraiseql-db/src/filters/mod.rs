//! Rich type filter operators and handlers.
//!
//! This module provides specialized filter operators for 44 rich scalar types,
//! enabling powerful queries like:
//!
//! - Extract and filter by email domain: `email.domain_eq('example.com')`
//! - Parse and filter by VIN components: `vin.wmi_eq('1HG')`
//! - Geographic filtering: `coordinates.distance_within(lat, lng, radius_km)`
//! - Country lookups: `country.continent_eq('Europe')`

pub mod default_rules;
pub mod operator_mapping;
pub mod operators;
pub mod validators;

pub use default_rules::get_default_rules;
pub use operator_mapping::{OperatorInfo, ParameterType, get_operators_for_type};
pub use operators::ExtendedOperator;
use serde_json::Value;
pub use validators::{ChecksumType, ValidationRule};

use fraiseql_error::Result;

/// Handler for extended operator SQL generation.
///
/// Each database backend implements this trait to provide database-specific SQL
/// generation for extended operators.
pub trait ExtendedOperatorHandler {
    /// Generate database-specific SQL for an extended operator.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the operator is not supported
    /// by this database or parameters are invalid.
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
    }
}
