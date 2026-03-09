use serde::{Deserialize, Serialize};

use crate::validation::ValidationRule;

/// Configuration for the custom type registry.
#[derive(Debug, Clone, Default)]
pub struct CustomTypeRegistryConfig {
    /// Maximum number of custom scalars allowed (None = unlimited).
    pub max_scalars: Option<usize>,

    /// Enable caching for future optimization.
    pub enable_caching: bool,
}

/// Definition of a custom scalar type at runtime.
///
/// Combines metadata with validation configuration for a single custom scalar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTypeDef {
    /// Scalar type name.
    pub name: String,

    /// Human-readable description of the scalar.
    pub description: Option<String>,

    /// URL to specification/RFC (GraphQL spec §3.5.1).
    pub specified_by_url: Option<String>,

    /// Built-in validation rules.
    #[serde(default)]
    pub validation_rules: Vec<ValidationRule>,

    /// ELO expression for custom validation.
    pub elo_expression: Option<String>,

    /// Base type for type aliases (e.g., "String" for Email scalar).
    pub base_type: Option<String>,
}

impl CustomTypeDef {
    /// Create a new custom type definition with minimal required fields.
    #[must_use]
    pub const fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            specified_by_url: None,
            validation_rules: Vec::new(),
            elo_expression: None,
            base_type: None,
        }
    }
}
