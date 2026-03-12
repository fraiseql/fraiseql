//! Core type structs: `IntermediateType`, `IntermediateField`, `IntermediateEnum`,
//! `IntermediateEnumValue`, `IntermediateScalar`, `IntermediateDeprecation`.

use fraiseql_core::validation::ValidationRule;
use serde::{Deserialize, Serialize};

use super::fragments::IntermediateAppliedDirective;

/// Type definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IntermediateType {
    /// Type name (e.g., "User")
    pub name: String,

    /// Type fields
    pub fields: Vec<IntermediateField>,

    /// Type description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Interfaces this type implements (GraphQL spec §3.6)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,

    /// Role required to see this type in introspection and access queries returning it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,

    /// Whether this type is a mutation error type (tagged with `@fraiseql.error`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,

    /// Whether this type implements the Relay Node interface.
    /// When true, the compiler generates global node IDs (`base64("TypeName:uuid")`)
    /// and validates that `pk_{entity}` (BIGINT) is present in the view's data JSONB.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relay: bool,
}

/// Field definition in intermediate format
///
/// **NOTE**: Uses `type` field (not `field_type`)
/// This is the language-agnostic format. Rust conversion happens in converter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateField {
    /// Field name (e.g., "id")
    pub name: String,

    /// Field type name (e.g., "Int", "String", "User")
    ///
    /// **Language-agnostic**: All languages use "type", not "`field_type`"
    #[serde(rename = "type")]
    pub field_type: String,

    /// Is field nullable?
    pub nullable: bool,

    /// Field description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Applied directives (e.g., @deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<IntermediateAppliedDirective>>,

    /// Scope required to access this field (field-level access control)
    ///
    /// When set, users must have this scope in their JWT to query this field.
    /// Supports patterns like "read:Type.field" or custom scopes like "hr:view_pii".
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "name": "salary",
    ///   "type": "Int",
    ///   "nullable": false,
    ///   "requires_scope": "read:Employee.salary"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,

    /// Policy when the user lacks `requires_scope`: `"reject"` (default) or `"mask"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_deny: Option<String>,
}

// =============================================================================
// Enum Definitions
// =============================================================================

/// GraphQL enum type definition in intermediate format.
///
/// Enums represent a finite set of possible values.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "OrderStatus",
///   "values": [
///     {"name": "PENDING"},
///     {"name": "PROCESSING"},
///     {"name": "SHIPPED", "description": "Package has been shipped"},
///     {"name": "DELIVERED"}
///   ],
///   "description": "Possible states of an order"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateEnum {
    /// Enum type name (e.g., "OrderStatus")
    pub name: String,

    /// Possible values for this enum
    pub values: Vec<IntermediateEnumValue>,

    /// Enum description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A single value within an enum type.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "ACTIVE",
///   "description": "The item is currently active",
///   "deprecated": {"reason": "Use ENABLED instead"}
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateEnumValue {
    /// Value name (e.g., "PENDING")
    pub name: String,

    /// Value description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation info (if value is deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Deprecation information for enum values or input fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDeprecation {
    /// Deprecation reason (what to use instead)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// =============================================================================
// Custom Scalar Definitions
// =============================================================================

/// Custom scalar type definition in intermediate format.
///
/// Custom scalars allow applications to define domain-specific types with validation.
/// Scalars are defined in language SDKs (Python, TypeScript, Java, Go, Rust)
/// and compiled into the schema.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "Email",
///   "description": "Valid email address",
///   "specified_by_url": "https://tools.ietf.org/html/rfc5322",
///   "base_type": "String",
///   "validation_rules": [
///     {
///       "type": "pattern",
///       "value": {
///         "pattern": "^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$"
///       }
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateScalar {
    /// Scalar name (e.g., "Email", "Phone", "ISBN")
    pub name: String,

    /// Scalar description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// URL to specification/RFC (GraphQL spec §3.5.1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specified_by_url: Option<String>,

    /// Built-in validation rules
    #[serde(default)]
    pub validation_rules: Vec<ValidationRule>,

    /// Base type for type aliases (e.g., "String" for Email scalar)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type: Option<String>,
}
