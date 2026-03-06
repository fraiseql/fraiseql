//! Rich type filter operators and handlers.
//!
//! Re-exported from `fraiseql-db`. See that crate for full documentation.

pub use fraiseql_db::filters::{
    ChecksumType, ExtendedOperator, ExtendedOperatorHandler, OperatorInfo, ParameterType,
    ValidationRule, get_default_rules, get_operators_for_type,
};
