//! GraphQL introspection data types per GraphQL spec §4.1.
//!
//! Defines all structs and enums used in introspection responses:
//! `__Schema`, `__Type`, `__Field`, `__InputValue`, `__EnumValue`, `__Directive`,
//! `__TypeKind`, and `__DirectiveLocation`.

use serde::{Deserialize, Serialize};

use super::super::DirectiveLocationKind;

// =============================================================================
// GraphQL Introspection Types (per spec §4.1)
// =============================================================================

/// `__Schema` introspection type.
///
/// Root type for schema introspection queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionSchema {
    /// Description of the schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// All types in the schema.
    pub types: Vec<IntrospectionType>,

    /// The root Query type.
    pub query_type: IntrospectionTypeRef,

    /// The root Mutation type (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation_type: Option<IntrospectionTypeRef>,

    /// The root Subscription type (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<IntrospectionTypeRef>,

    /// All directives supported by the schema.
    pub directives: Vec<IntrospectionDirective>,
}

/// `__Type` introspection type.
///
/// Represents any type in the GraphQL type system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionType {
    /// The kind of type (SCALAR, OBJECT, etc.).
    pub kind: TypeKind,

    /// The name of the type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Description of the type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Fields (for OBJECT and INTERFACE types).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<IntrospectionField>>,

    /// Interfaces this type implements (for OBJECT types).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interfaces: Option<Vec<IntrospectionTypeRef>>,

    /// Possible types (for INTERFACE and UNION types).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub possible_types: Option<Vec<IntrospectionTypeRef>>,

    /// Enum values (for ENUM types).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<IntrospectionEnumValue>>,

    /// Input fields (for INPUT_OBJECT types).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_fields: Option<Vec<IntrospectionInputValue>>,

    /// The wrapped type (for NON_NULL and LIST types).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of_type: Option<Box<IntrospectionType>>,

    /// Specified by URL (for custom scalars per GraphQL spec §3.5.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specified_by_u_r_l: Option<String>,
}

impl IntrospectionType {
    /// Filter out deprecated fields if `include_deprecated` is false.
    ///
    /// Per GraphQL spec, the `fields` introspection field accepts an
    /// `includeDeprecated` argument (default: false).
    #[must_use]
    pub fn filter_deprecated_fields(&self, include_deprecated: bool) -> Self {
        let mut result = self.clone();

        if !include_deprecated {
            if let Some(ref fields) = result.fields {
                result.fields = Some(fields.iter().filter(|f| !f.is_deprecated).cloned().collect());
            }
        }

        result
    }

    /// Filter out deprecated enum values if `include_deprecated` is false.
    ///
    /// Per GraphQL spec, the `enumValues` introspection field accepts an
    /// `includeDeprecated` argument (default: false).
    #[must_use]
    pub fn filter_deprecated_enum_values(&self, include_deprecated: bool) -> Self {
        let mut result = self.clone();

        if !include_deprecated {
            if let Some(ref values) = result.enum_values {
                result.enum_values =
                    Some(values.iter().filter(|v| !v.is_deprecated).cloned().collect());
            }
        }

        result
    }

    /// Filter out all deprecated items (fields and enum values).
    ///
    /// Convenience method combining both filters.
    #[must_use]
    pub fn filter_all_deprecated(&self, include_deprecated: bool) -> Self {
        self.filter_deprecated_fields(include_deprecated)
            .filter_deprecated_enum_values(include_deprecated)
    }
}

/// Type reference (simplified type with just name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionTypeRef {
    /// The name of the referenced type.
    pub name: String,
}

/// `__Field` introspection type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionField {
    /// Field name.
    pub name: String,

    /// Field description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Field arguments.
    pub args: Vec<IntrospectionInputValue>,

    /// Field return type.
    #[serde(rename = "type")]
    pub field_type: IntrospectionType,

    /// Whether the field is deprecated.
    pub is_deprecated: bool,

    /// Deprecation reason (if deprecated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_reason: Option<String>,
}

/// Validation rule for input field in introspection format.
///
/// Converts internal ValidationRule enums to introspection-friendly format
/// that clients can query and use for UI generation, form validation, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionValidationRule {
    /// Rule type name (required, pattern, range, enum, etc.)
    pub rule_type: String,

    /// Pattern regex (for pattern rules)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Pattern error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_message: Option<String>,

    /// Minimum value or length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,

    /// Maximum value or length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,

    /// Allowed enum values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<String>>,

    /// Checksum algorithm name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,

    /// Referenced field name (for cross-field rules)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_reference: Option<String>,

    /// Comparison operator (for cross-field rules)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,

    /// List of field names (for one_of, any_of, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_list: Option<Vec<String>>,

    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `__InputValue` introspection type.
///
/// Per GraphQL spec, input values (arguments and input fields) can be deprecated.
/// The `isDeprecated` and `deprecationReason` fields are part of the June 2021 spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionInputValue {
    /// Input name.
    pub name: String,

    /// Input description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Input type.
    #[serde(rename = "type")]
    pub input_type: IntrospectionType,

    /// Default value (as JSON string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,

    /// Whether the input value is deprecated.
    pub is_deprecated: bool,

    /// Deprecation reason (if deprecated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_reason: Option<String>,

    /// Validation rules for this input value.
    #[serde(default)]
    pub validation_rules: Vec<IntrospectionValidationRule>,
}

/// `__EnumValue` introspection type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionEnumValue {
    /// Enum value name.
    pub name: String,

    /// Enum value description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether the value is deprecated.
    pub is_deprecated: bool,

    /// Deprecation reason (if deprecated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_reason: Option<String>,
}

/// `__Directive` introspection type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionDirective {
    /// Directive name.
    pub name: String,

    /// Directive description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Valid locations for this directive.
    pub locations: Vec<DirectiveLocation>,

    /// Directive arguments.
    pub args: Vec<IntrospectionInputValue>,

    /// Whether the directive is repeatable.
    #[serde(default)]
    pub is_repeatable: bool,
}

/// `__TypeKind` enum per GraphQL spec §4.1.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum TypeKind {
    /// A scalar type (Int, String, Boolean, etc.)
    Scalar,
    /// An object type with fields.
    Object,
    /// An abstract interface type.
    Interface,
    /// A union of multiple object types.
    Union,
    /// An enumeration type.
    Enum,
    /// An input object type for mutations.
    InputObject,
    /// A list wrapper type.
    List,
    /// A non-null wrapper type.
    NonNull,
}

/// `__DirectiveLocation` enum per GraphQL spec §4.1.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum DirectiveLocation {
    /// Directive on query operation.
    Query,
    /// Directive on mutation operation.
    Mutation,
    /// Directive on subscription operation.
    Subscription,
    /// Directive on field selection.
    Field,
    /// Directive on fragment definition.
    FragmentDefinition,
    /// Directive on fragment spread.
    FragmentSpread,
    /// Directive on inline fragment.
    InlineFragment,
    /// Directive on variable definition.
    VariableDefinition,
    /// Directive on schema definition.
    Schema,
    /// Directive on scalar type definition.
    Scalar,
    /// Directive on object type definition.
    Object,
    /// Directive on field definition.
    FieldDefinition,
    /// Directive on argument definition.
    ArgumentDefinition,
    /// Directive on interface definition.
    Interface,
    /// Directive on union definition.
    Union,
    /// Directive on enum definition.
    Enum,
    /// Directive on enum value definition.
    EnumValue,
    /// Directive on input object definition.
    InputObject,
    /// Directive on input field definition.
    InputFieldDefinition,
}

impl From<DirectiveLocationKind> for DirectiveLocation {
    fn from(kind: DirectiveLocationKind) -> Self {
        match kind {
            DirectiveLocationKind::Query => Self::Query,
            DirectiveLocationKind::Mutation => Self::Mutation,
            DirectiveLocationKind::Subscription => Self::Subscription,
            DirectiveLocationKind::Field => Self::Field,
            DirectiveLocationKind::FragmentDefinition => Self::FragmentDefinition,
            DirectiveLocationKind::FragmentSpread => Self::FragmentSpread,
            DirectiveLocationKind::InlineFragment => Self::InlineFragment,
            DirectiveLocationKind::VariableDefinition => Self::VariableDefinition,
            DirectiveLocationKind::Schema => Self::Schema,
            DirectiveLocationKind::Scalar => Self::Scalar,
            DirectiveLocationKind::Object => Self::Object,
            DirectiveLocationKind::FieldDefinition => Self::FieldDefinition,
            DirectiveLocationKind::ArgumentDefinition => Self::ArgumentDefinition,
            DirectiveLocationKind::Interface => Self::Interface,
            DirectiveLocationKind::Union => Self::Union,
            DirectiveLocationKind::Enum => Self::Enum,
            DirectiveLocationKind::EnumValue => Self::EnumValue,
            DirectiveLocationKind::InputObject => Self::InputObject,
            DirectiveLocationKind::InputFieldDefinition => Self::InputFieldDefinition,
        }
    }
}
