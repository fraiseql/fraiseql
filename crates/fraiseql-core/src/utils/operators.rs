//! GraphQL WHERE clause operator registry.
//!
//! The registry is **derived** from [`fraiseql_db::where_clause::WHERE_OPERATORS`],
//! which is the same list `WhereOperator::from_str` is generated from. Before
//! #828 this file held a second, hand-maintained table of 79 names; 27 of them
//! could not be parsed by the executor, so `GET /api/users?status[ne]=archived`
//! passed REST validation and then failed with `Unknown WHERE operator: ne`, and
//! the resulting 400 advertised two dozen more names that behaved the same way.
//!
//! Adding an operator now means adding one row to the table in `fraiseql-db`:
//! the parser, this registry, and everything built on it move together.

use std::{collections::HashMap, sync::LazyLock};

pub use fraiseql_db::where_clause::{OperatorCategory, WHERE_OPERATORS, WhereOperatorSpec};

/// Information about a single operator, as advertised to clients.
#[derive(Debug, Clone)]
pub struct OperatorInfo {
    /// GraphQL operator name (e.g., "eq", "contains")
    pub name:           &'static str,
    /// SQL operator or function (e.g., "=", "LIKE", "@>")
    pub sql_op:         &'static str,
    /// Category of operator
    pub category:       OperatorCategory,
    /// Whether this operator expects an array value
    pub requires_array: bool,
    /// Whether this operator needs special JSONB handling
    pub jsonb_operator: bool,
}

impl From<&'static WhereOperatorSpec> for OperatorInfo {
    fn from(spec: &'static WhereOperatorSpec) -> Self {
        Self {
            name:           spec.name,
            sql_op:         spec.sql_op,
            category:       spec.category,
            requires_array: spec.requires_array,
            jsonb_operator: spec.jsonb_operator,
        }
    }
}

/// Every operator name the runtime accepts, canonical spellings and aliases
/// alike, mapped to what that operator does.
///
/// Keyed by *every accepted* name, so a lookup and a parse agree: if the
/// registry has the key, `WhereOperator::from_str` resolves it.
pub static OPERATOR_REGISTRY: LazyLock<HashMap<&'static str, OperatorInfo>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    for spec in WHERE_OPERATORS {
        for name in spec.all_names() {
            m.insert(name, OperatorInfo::from(spec));
        }
    }
    m
});

/// Get operator information by name
///
/// # Example
/// ```
/// use fraiseql_core::utils::operators::get_operator_info;
///
/// let op = get_operator_info("eq").unwrap();
/// assert_eq!(op.sql_op, "=");
/// ```
#[must_use]
pub fn get_operator_info(name: &str) -> Option<&'static OperatorInfo> {
    OPERATOR_REGISTRY.get(name)
}

/// Check if a string is a valid operator name
///
/// # Example
/// ```
/// use fraiseql_core::utils::operators::is_operator;
///
/// assert!(is_operator("eq"));
/// assert!(is_operator("contains"));
/// assert!(!is_operator("unknown_operator"));
/// ```
#[must_use]
pub fn is_operator(name: &str) -> bool {
    OPERATOR_REGISTRY.contains_key(name)
}

/// Get all operators in a specific category
///
/// # Example
/// ```
/// use fraiseql_core::utils::operators::{OperatorCategory, get_operators_by_category};
///
/// let comparison_ops = get_operators_by_category(OperatorCategory::Comparison);
/// assert!(comparison_ops.len() >= 8);
/// ```
#[must_use]
pub fn get_operators_by_category(category: OperatorCategory) -> Vec<&'static OperatorInfo> {
    OPERATOR_REGISTRY.values().filter(|op| op.category == category).collect()
}

#[cfg(test)]
#[path = "operators_tests.rs"]
mod operators_tests;
