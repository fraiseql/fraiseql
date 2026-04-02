use serde::{Deserialize, Serialize};

use crate::schema::{
    field_type::{DeprecationInfo, FieldType},
    graphql_value::GraphQLValue,
};

/// Query/mutation/subscription argument definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArgumentDefinition {
    /// Argument name.
    pub name: String,

    /// Argument type.
    pub arg_type: FieldType,

    /// Is this argument optional?
    #[serde(default)]
    pub nullable: bool,

    /// Default value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<GraphQLValue>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation information (from @deprecated directive).
    /// When set, this argument is marked as deprecated in the schema.
    /// Per GraphQL spec, deprecated arguments should still be accepted but
    /// clients are encouraged to migrate to alternatives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<DeprecationInfo>,
}

impl ArgumentDefinition {
    /// Create a new required argument.
    #[must_use]
    pub fn new(name: impl Into<String>, arg_type: FieldType) -> Self {
        Self {
            name: name.into(),
            arg_type,
            nullable: false,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

    /// Create a new optional argument.
    #[must_use]
    pub fn optional(name: impl Into<String>, arg_type: FieldType) -> Self {
        Self {
            name: name.into(),
            arg_type,
            nullable: true,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

    /// Mark this argument as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{ArgumentDefinition, FieldType};
    ///
    /// let arg = ArgumentDefinition::optional("oldLimit", FieldType::Int)
    ///     .deprecated(Some("Use 'first' instead".to_string()));
    /// assert!(arg.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(DeprecationInfo { reason });
        self
    }

    /// Check if this argument is deprecated.
    #[must_use]
    pub const fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// Auto-wired query parameters.
///
/// These are standard parameters automatically added to list queries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Reason: these are intentional feature flags
pub struct AutoParams {
    /// Enable `where` filtering.
    #[serde(default)]
    pub has_where: bool,

    /// Enable `orderBy` sorting.
    #[serde(default)]
    pub has_order_by: bool,

    /// Enable `limit` pagination.
    #[serde(default)]
    pub has_limit: bool,

    /// Enable `offset` pagination.
    #[serde(default)]
    pub has_offset: bool,
}

impl AutoParams {
    /// Create with all auto-params enabled (common for list queries).
    #[must_use]
    pub const fn all() -> Self {
        Self {
            has_where:    true,
            has_order_by: true,
            has_limit:    true,
            has_offset:   true,
        }
    }

    /// Create with no auto-params (common for single-item queries).
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}
