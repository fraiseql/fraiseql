//! GraphQL introspection types per GraphQL spec §4.1-4.2.
//!
//! This module provides standard GraphQL introspection support, enabling
//! tools like Apollo Sandbox, GraphiQL, and Altair to query the schema.
//!
//! # Architecture
//!
//! FraiseQL generates introspection responses at **compile time** for performance.
//! The `IntrospectionSchema` is built from `CompiledSchema` and cached.
//!
//! # Supported Queries
//!
//! - `__schema` - Returns the full schema introspection
//! - `__type(name: String!)` - Returns a specific type's introspection
//! - `__typename` - Handled at projection level, not here

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    CompiledSchema, DirectiveDefinition, DirectiveLocationKind, EnumDefinition, FieldDefinition,
    FieldType, InputObjectDefinition, InterfaceDefinition, QueryDefinition, TypeDefinition,
    UnionDefinition,
};

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

/// Validation rule for input field in introspection format (Phase 4).
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

    /// Validation rules for this input value (Phase 4).
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

// =============================================================================
// Introspection Builder
// =============================================================================

/// Builds introspection schema from compiled schema.
pub struct IntrospectionBuilder;

impl IntrospectionBuilder {
    /// Build complete introspection schema from compiled schema.
    #[must_use]
    pub fn build(schema: &CompiledSchema) -> IntrospectionSchema {
        let mut types = Vec::new();

        // Add built-in scalar types
        types.extend(Self::builtin_scalars());

        // Add user-defined types
        for type_def in &schema.types {
            types.push(Self::build_object_type(type_def));
        }

        // Add enum types
        for enum_def in &schema.enums {
            types.push(Self::build_enum_type(enum_def));
        }

        // Add input object types
        for input_def in &schema.input_types {
            types.push(Self::build_input_object_type(input_def));
        }

        // Add interface types
        for interface_def in &schema.interfaces {
            types.push(Self::build_interface_type(interface_def, schema));
        }

        // Add union types
        for union_def in &schema.unions {
            types.push(Self::build_union_type(union_def));
        }

        // Add Query root type
        types.push(Self::build_query_type(schema));

        // Add Mutation root type if mutations exist
        if !schema.mutations.is_empty() {
            types.push(Self::build_mutation_type(schema));
        }

        // Add Subscription root type if subscriptions exist
        if !schema.subscriptions.is_empty() {
            types.push(Self::build_subscription_type(schema));
        }

        // Build directives: built-in + custom
        let mut directives = Self::builtin_directives();
        directives.extend(Self::build_custom_directives(&schema.directives));

        IntrospectionSchema {
            description: Some("FraiseQL GraphQL Schema".to_string()),
            types,
            query_type: IntrospectionTypeRef {
                name: "Query".to_string(),
            },
            mutation_type: if schema.mutations.is_empty() {
                None
            } else {
                Some(IntrospectionTypeRef {
                    name: "Mutation".to_string(),
                })
            },
            subscription_type: if schema.subscriptions.is_empty() {
                None
            } else {
                Some(IntrospectionTypeRef {
                    name: "Subscription".to_string(),
                })
            },
            directives,
        }
    }

    /// Build a lookup map for `__type(name:)` queries.
    #[must_use]
    pub fn build_type_map(schema: &IntrospectionSchema) -> HashMap<String, IntrospectionType> {
        let mut map = HashMap::new();
        for t in &schema.types {
            if let Some(ref name) = t.name {
                map.insert(name.clone(), t.clone());
            }
        }
        map
    }

    /// Built-in GraphQL scalar types.
    fn builtin_scalars() -> Vec<IntrospectionType> {
        vec![
            Self::scalar_type("Int", "Built-in Int scalar"),
            Self::scalar_type("Float", "Built-in Float scalar"),
            Self::scalar_type("String", "Built-in String scalar"),
            Self::scalar_type("Boolean", "Built-in Boolean scalar"),
            Self::scalar_type("ID", "Built-in ID scalar"),
            // FraiseQL custom scalars (with specifiedByURL per GraphQL spec §3.5.5)
            Self::scalar_type_with_url(
                "DateTime",
                "ISO-8601 datetime string",
                Some("https://scalars.graphql.org/andimarek/date-time"),
            ),
            Self::scalar_type_with_url(
                "Date",
                "ISO-8601 date string",
                Some("https://scalars.graphql.org/andimarek/local-date"),
            ),
            Self::scalar_type_with_url(
                "Time",
                "ISO-8601 time string",
                Some("https://scalars.graphql.org/andimarek/local-time"),
            ),
            Self::scalar_type_with_url(
                "UUID",
                "UUID string",
                Some("https://tools.ietf.org/html/rfc4122"),
            ),
            Self::scalar_type_with_url(
                "JSON",
                "Arbitrary JSON value",
                Some("https://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf"),
            ),
            Self::scalar_type("Decimal", "Decimal number"),
        ]
    }

    /// Create a scalar type introspection.
    fn scalar_type(name: &str, description: &str) -> IntrospectionType {
        Self::scalar_type_with_url(name, description, None)
    }

    /// Create a scalar type introspection with optional `specifiedByURL`.
    fn scalar_type_with_url(
        name: &str,
        description: &str,
        specified_by_url: Option<&str>,
    ) -> IntrospectionType {
        IntrospectionType {
            kind:               TypeKind::Scalar,
            name:               Some(name.to_string()),
            description:        Some(description.to_string()),
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: specified_by_url.map(ToString::to_string),
        }
    }

    /// Build object type from TypeDefinition.
    fn build_object_type(type_def: &TypeDefinition) -> IntrospectionType {
        let fields = type_def.fields.iter().map(|f| Self::build_field(f)).collect();

        // Build interfaces that this type implements
        let interfaces: Vec<IntrospectionTypeRef> = type_def
            .implements
            .iter()
            .map(|name| IntrospectionTypeRef { name: name.clone() })
            .collect();

        IntrospectionType {
            kind:               TypeKind::Object,
            name:               Some(type_def.name.clone()),
            description:        type_def.description.clone(),
            fields:             Some(fields),
            interfaces:         Some(interfaces),
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build enum type from EnumDefinition.
    fn build_enum_type(enum_def: &EnumDefinition) -> IntrospectionType {
        let enum_values = enum_def
            .values
            .iter()
            .map(|v| IntrospectionEnumValue {
                name:               v.name.clone(),
                description:        v.description.clone(),
                is_deprecated:      v.deprecation.is_some(),
                deprecation_reason: v.deprecation.as_ref().and_then(|d| d.reason.clone()),
            })
            .collect();

        IntrospectionType {
            kind:               TypeKind::Enum,
            name:               Some(enum_def.name.clone()),
            description:        enum_def.description.clone(),
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        Some(enum_values),
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build input object type from InputObjectDefinition.
    fn build_input_object_type(input_def: &InputObjectDefinition) -> IntrospectionType {
        let input_fields = input_def
            .fields
            .iter()
            .map(|f| {
                let validation_rules = f
                    .validation_rules
                    .iter()
                    .map(|rule| Self::build_validation_rule(rule))
                    .collect();

                IntrospectionInputValue {
                    name: f.name.clone(),
                    description: f.description.clone(),
                    input_type: Self::type_ref(&f.field_type),
                    default_value: f.default_value.clone(),
                    is_deprecated: f.is_deprecated(),
                    deprecation_reason: f.deprecation.as_ref().and_then(|d| d.reason.clone()),
                    validation_rules,
                }
            })
            .collect();

        IntrospectionType {
            kind:               TypeKind::InputObject,
            name:               Some(input_def.name.clone()),
            description:        input_def.description.clone(),
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       Some(input_fields),
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build interface type from InterfaceDefinition.
    fn build_interface_type(
        interface_def: &InterfaceDefinition,
        schema: &CompiledSchema,
    ) -> IntrospectionType {
        // Build fields for the interface
        let fields = interface_def.fields.iter().map(|f| Self::build_field(f)).collect();

        // Find all types that implement this interface
        let possible_types: Vec<IntrospectionTypeRef> = schema
            .find_implementors(&interface_def.name)
            .iter()
            .map(|t| IntrospectionTypeRef {
                name: t.name.clone(),
            })
            .collect();

        IntrospectionType {
            kind:               TypeKind::Interface,
            name:               Some(interface_def.name.clone()),
            description:        interface_def.description.clone(),
            fields:             Some(fields),
            interfaces:         None,
            possible_types:     if possible_types.is_empty() {
                None
            } else {
                Some(possible_types)
            },
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build union type from UnionDefinition.
    fn build_union_type(union_def: &UnionDefinition) -> IntrospectionType {
        // Build possible types for the union
        let possible_types: Vec<IntrospectionTypeRef> = union_def
            .member_types
            .iter()
            .map(|name| IntrospectionTypeRef { name: name.clone() })
            .collect();

        IntrospectionType {
            kind:               TypeKind::Union,
            name:               Some(union_def.name.clone()),
            description:        union_def.description.clone(),
            fields:             None, // Unions don't have fields
            interfaces:         None,
            possible_types:     if possible_types.is_empty() {
                None
            } else {
                Some(possible_types)
            },
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build field introspection from FieldDefinition.
    fn build_field(field: &FieldDefinition) -> IntrospectionField {
        IntrospectionField {
            name:               field.output_name().to_string(),
            description:        field.description.clone(),
            args:               vec![], // Regular fields don't have args
            field_type:         Self::field_type_to_introspection(
                &field.field_type,
                field.nullable,
            ),
            is_deprecated:      field.is_deprecated(),
            deprecation_reason: field.deprecation_reason().map(ToString::to_string),
        }
    }

    /// Convert FieldType to introspection type.
    fn field_type_to_introspection(field_type: &FieldType, nullable: bool) -> IntrospectionType {
        let inner = match field_type {
            FieldType::Int => Self::type_ref("Int"),
            FieldType::Float => Self::type_ref("Float"),
            FieldType::String => Self::type_ref("String"),
            FieldType::Boolean => Self::type_ref("Boolean"),
            FieldType::Id => Self::type_ref("ID"),
            FieldType::DateTime => Self::type_ref("DateTime"),
            FieldType::Date => Self::type_ref("Date"),
            FieldType::Time => Self::type_ref("Time"),
            FieldType::Uuid => Self::type_ref("UUID"),
            FieldType::Json => Self::type_ref("JSON"),
            FieldType::Decimal => Self::type_ref("Decimal"),
            FieldType::Object(name) => Self::type_ref(name),
            FieldType::Enum(name) => Self::type_ref(name),
            FieldType::Input(name) => Self::type_ref(name),
            FieldType::Interface(name) => Self::type_ref(name),
            FieldType::Union(name) => Self::type_ref(name),
            FieldType::Scalar(name) => Self::type_ref(name), // Rich/custom scalars
            FieldType::List(inner) => IntrospectionType {
                kind:               TypeKind::List,
                name:               None,
                description:        None,
                fields:             None,
                interfaces:         None,
                possible_types:     None,
                enum_values:        None,
                input_fields:       None,
                of_type:            Some(Box::new(Self::field_type_to_introspection(inner, true))),
                specified_by_u_r_l: None,
            },
            FieldType::Vector => Self::type_ref("JSON"), // Vectors are exposed as JSON
        };

        if nullable {
            inner
        } else {
            // Wrap in NON_NULL
            IntrospectionType {
                kind:               TypeKind::NonNull,
                name:               None,
                description:        None,
                fields:             None,
                interfaces:         None,
                possible_types:     None,
                enum_values:        None,
                input_fields:       None,
                of_type:            Some(Box::new(inner)),
                specified_by_u_r_l: None,
            }
        }
    }

    /// Create a type reference.
    fn type_ref(name: &str) -> IntrospectionType {
        IntrospectionType {
            kind:               TypeKind::Scalar, // Will be overwritten if it's an object
            name:               Some(name.to_string()),
            description:        None,
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Convert ValidationRule to IntrospectionValidationRule.
    fn build_validation_rule(
        rule: &crate::validation::rules::ValidationRule,
    ) -> IntrospectionValidationRule {
        use crate::validation::rules::ValidationRule;

        match rule {
            ValidationRule::Required => IntrospectionValidationRule {
                rule_type:       "required".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     Some("Field is required".to_string()),
            },
            ValidationRule::Pattern { pattern, message } => IntrospectionValidationRule {
                rule_type:       "pattern".to_string(),
                pattern:         Some(pattern.clone()),
                pattern_message: message.clone(),
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     message.clone(),
            },
            ValidationRule::Length { min, max } => IntrospectionValidationRule {
                rule_type:       "length".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             min.map(|v| v as i64),
                max:             max.map(|v| v as i64),
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     None,
            },
            ValidationRule::Range { min, max } => IntrospectionValidationRule {
                rule_type:       "range".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             *min,
                max:             *max,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     None,
            },
            ValidationRule::Enum { values } => IntrospectionValidationRule {
                rule_type:       "enum".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  Some(values.clone()),
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     None,
            },
            ValidationRule::Checksum { algorithm } => IntrospectionValidationRule {
                rule_type:       "checksum".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       Some(algorithm.clone()),
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     None,
            },
            ValidationRule::CrossField { field, operator } => IntrospectionValidationRule {
                rule_type:       "cross_field".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: Some(field.clone()),
                operator:        Some(operator.clone()),
                field_list:      None,
                description:     None,
            },
            ValidationRule::Conditional { .. } => IntrospectionValidationRule {
                rule_type:       "conditional".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     Some("Conditional validation".to_string()),
            },
            ValidationRule::All(_) => IntrospectionValidationRule {
                rule_type:       "all".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     Some("All rules must pass".to_string()),
            },
            ValidationRule::Any(_) => IntrospectionValidationRule {
                rule_type:       "any".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     Some("At least one rule must pass".to_string()),
            },
            ValidationRule::OneOf { fields } => IntrospectionValidationRule {
                rule_type:       "one_of".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      Some(fields.clone()),
                description:     None,
            },
            ValidationRule::AnyOf { fields } => IntrospectionValidationRule {
                rule_type:       "any_of".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      Some(fields.clone()),
                description:     None,
            },
            ValidationRule::ConditionalRequired { .. } => IntrospectionValidationRule {
                rule_type:       "conditional_required".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     None,
            },
            ValidationRule::RequiredIfAbsent { .. } => IntrospectionValidationRule {
                rule_type:       "required_if_absent".to_string(),
                pattern:         None,
                pattern_message: None,
                min:             None,
                max:             None,
                allowed_values:  None,
                algorithm:       None,
                field_reference: None,
                operator:        None,
                field_list:      None,
                description:     None,
            },
        }
    }

    /// Build Query root type.
    fn build_query_type(schema: &CompiledSchema) -> IntrospectionType {
        let fields: Vec<IntrospectionField> =
            schema.queries.iter().map(|q| Self::build_query_field(q)).collect();

        IntrospectionType {
            kind:               TypeKind::Object,
            name:               Some("Query".to_string()),
            description:        Some("Root query type".to_string()),
            fields:             Some(fields),
            interfaces:         Some(vec![]),
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build Mutation root type.
    fn build_mutation_type(schema: &CompiledSchema) -> IntrospectionType {
        let fields: Vec<IntrospectionField> =
            schema.mutations.iter().map(|m| Self::build_mutation_field(m)).collect();

        IntrospectionType {
            kind:               TypeKind::Object,
            name:               Some("Mutation".to_string()),
            description:        Some("Root mutation type".to_string()),
            fields:             Some(fields),
            interfaces:         Some(vec![]),
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build Subscription root type.
    fn build_subscription_type(schema: &CompiledSchema) -> IntrospectionType {
        let fields: Vec<IntrospectionField> =
            schema.subscriptions.iter().map(|s| Self::build_subscription_field(s)).collect();

        IntrospectionType {
            kind:               TypeKind::Object,
            name:               Some("Subscription".to_string()),
            description:        Some("Root subscription type".to_string()),
            fields:             Some(fields),
            interfaces:         Some(vec![]),
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        }
    }

    /// Build query field introspection.
    fn build_query_field(query: &QueryDefinition) -> IntrospectionField {
        let return_type = Self::type_ref(&query.return_type);
        let return_type = if query.returns_list {
            IntrospectionType {
                kind:               TypeKind::List,
                name:               None,
                description:        None,
                fields:             None,
                interfaces:         None,
                possible_types:     None,
                enum_values:        None,
                input_fields:       None,
                of_type:            Some(Box::new(return_type)),
                specified_by_u_r_l: None,
            }
        } else {
            return_type
        };

        let return_type = if query.nullable {
            return_type
        } else {
            IntrospectionType {
                kind:               TypeKind::NonNull,
                name:               None,
                description:        None,
                fields:             None,
                interfaces:         None,
                possible_types:     None,
                enum_values:        None,
                input_fields:       None,
                of_type:            Some(Box::new(return_type)),
                specified_by_u_r_l: None,
            }
        };

        // Build arguments
        let args: Vec<IntrospectionInputValue> = query
            .arguments
            .iter()
            .map(|arg| IntrospectionInputValue {
                name:               arg.name.clone(),
                description:        arg.description.clone(),
                input_type:         Self::field_type_to_introspection(&arg.arg_type, arg.nullable),
                default_value:      arg.default_value.as_ref().map(|v| v.to_string()),
                is_deprecated:      arg.is_deprecated(),
                deprecation_reason: arg.deprecation_reason().map(ToString::to_string),
                validation_rules:   vec![],
            })
            .collect();

        IntrospectionField {
            name: query.name.clone(),
            description: query.description.clone(),
            args,
            field_type: return_type,
            is_deprecated: query.is_deprecated(),
            deprecation_reason: query.deprecation_reason().map(ToString::to_string),
        }
    }

    /// Build mutation field introspection.
    fn build_mutation_field(mutation: &super::MutationDefinition) -> IntrospectionField {
        // Mutations always return a single object (not a list)
        let return_type = Self::type_ref(&mutation.return_type);

        // Build arguments
        let args: Vec<IntrospectionInputValue> = mutation
            .arguments
            .iter()
            .map(|arg| IntrospectionInputValue {
                name:               arg.name.clone(),
                description:        arg.description.clone(),
                input_type:         Self::field_type_to_introspection(&arg.arg_type, arg.nullable),
                default_value:      arg.default_value.as_ref().map(|v| v.to_string()),
                is_deprecated:      arg.is_deprecated(),
                deprecation_reason: arg.deprecation_reason().map(ToString::to_string),
                validation_rules:   vec![],
            })
            .collect();

        IntrospectionField {
            name: mutation.name.clone(),
            description: mutation.description.clone(),
            args,
            field_type: return_type,
            is_deprecated: mutation.is_deprecated(),
            deprecation_reason: mutation.deprecation_reason().map(ToString::to_string),
        }
    }

    /// Build subscription field introspection.
    fn build_subscription_field(
        subscription: &super::SubscriptionDefinition,
    ) -> IntrospectionField {
        // Subscriptions typically return a single item per event
        let return_type = Self::type_ref(&subscription.return_type);

        // Build arguments
        let args: Vec<IntrospectionInputValue> = subscription
            .arguments
            .iter()
            .map(|arg| IntrospectionInputValue {
                name:               arg.name.clone(),
                description:        arg.description.clone(),
                input_type:         Self::field_type_to_introspection(&arg.arg_type, arg.nullable),
                default_value:      arg.default_value.as_ref().map(|v| v.to_string()),
                is_deprecated:      arg.is_deprecated(),
                deprecation_reason: arg.deprecation_reason().map(ToString::to_string),
                validation_rules:   vec![],
            })
            .collect();

        IntrospectionField {
            name: subscription.name.clone(),
            description: subscription.description.clone(),
            args,
            field_type: return_type,
            is_deprecated: subscription.is_deprecated(),
            deprecation_reason: subscription.deprecation_reason().map(ToString::to_string),
        }
    }

    /// Built-in GraphQL directives.
    fn builtin_directives() -> Vec<IntrospectionDirective> {
        vec![
            IntrospectionDirective {
                name: "skip".to_string(),
                description: Some(
                    "Directs the executor to skip this field or fragment when the `if` argument is true."
                        .to_string(),
                ),
                locations: vec![
                    DirectiveLocation::Field,
                    DirectiveLocation::FragmentSpread,
                    DirectiveLocation::InlineFragment,
                ],
                args: vec![IntrospectionInputValue {
                    name: "if".to_string(),
                    description: Some("Skipped when true.".to_string()),
                    input_type: IntrospectionType {
                        kind: TypeKind::NonNull,
                        name: None,
                        description: None,
                        fields: None,
                        interfaces: None,
                        possible_types: None,
                        enum_values: None,
                        input_fields: None,
                        of_type: Some(Box::new(Self::type_ref("Boolean"))),
                        specified_by_u_r_l: None,
                    },
                    default_value: None,
                    is_deprecated: false,
                    deprecation_reason: None,
                    validation_rules: vec![],
                }],
                is_repeatable: false,
            },
            IntrospectionDirective {
                name: "include".to_string(),
                description: Some(
                    "Directs the executor to include this field or fragment only when the `if` argument is true."
                        .to_string(),
                ),
                locations: vec![
                    DirectiveLocation::Field,
                    DirectiveLocation::FragmentSpread,
                    DirectiveLocation::InlineFragment,
                ],
                args: vec![IntrospectionInputValue {
                    name: "if".to_string(),
                    description: Some("Included when true.".to_string()),
                    input_type: IntrospectionType {
                        kind: TypeKind::NonNull,
                        name: None,
                        description: None,
                        fields: None,
                        interfaces: None,
                        possible_types: None,
                        enum_values: None,
                        input_fields: None,
                        of_type: Some(Box::new(Self::type_ref("Boolean"))),
                        specified_by_u_r_l: None,
                    },
                    default_value: None,
                    is_deprecated: false,
                    deprecation_reason: None,
                    validation_rules: vec![],
                }],
                is_repeatable: false,
            },
            IntrospectionDirective {
                name: "deprecated".to_string(),
                description: Some(
                    "Marks an element of a GraphQL schema as no longer supported.".to_string(),
                ),
                locations: vec![
                    DirectiveLocation::FieldDefinition,
                    DirectiveLocation::EnumValue,
                    DirectiveLocation::ArgumentDefinition,
                    DirectiveLocation::InputFieldDefinition,
                ],
                args: vec![IntrospectionInputValue {
                    name: "reason".to_string(),
                    description: Some(
                        "Explains why this element was deprecated.".to_string(),
                    ),
                    input_type: Self::type_ref("String"),
                    default_value: Some("\"No longer supported\"".to_string()),
                    is_deprecated: false,
                    deprecation_reason: None,
                    validation_rules: vec![],
                }],
                is_repeatable: false,
            },
        ]
    }

    /// Build introspection directives from custom directive definitions.
    fn build_custom_directives(directives: &[DirectiveDefinition]) -> Vec<IntrospectionDirective> {
        directives.iter().map(|d| Self::build_custom_directive(d)).collect()
    }

    /// Build a single introspection directive from a custom directive definition.
    fn build_custom_directive(directive: &DirectiveDefinition) -> IntrospectionDirective {
        let locations: Vec<DirectiveLocation> =
            directive.locations.iter().map(|loc| DirectiveLocation::from(*loc)).collect();

        let args: Vec<IntrospectionInputValue> = directive
            .arguments
            .iter()
            .map(|arg| IntrospectionInputValue {
                name:               arg.name.clone(),
                description:        arg.description.clone(),
                input_type:         Self::field_type_to_introspection(&arg.arg_type, arg.nullable),
                default_value:      arg.default_value.as_ref().map(|v| v.to_string()),
                is_deprecated:      arg.is_deprecated(),
                deprecation_reason: arg.deprecation_reason().map(ToString::to_string),
                validation_rules:   vec![],
            })
            .collect();

        IntrospectionDirective {
            name: directive.name.clone(),
            description: directive.description.clone(),
            locations,
            args,
            is_repeatable: directive.is_repeatable,
        }
    }
}

// =============================================================================
// Introspection Response Wrapper
// =============================================================================

/// Pre-built introspection responses for fast serving.
#[derive(Debug, Clone)]
pub struct IntrospectionResponses {
    /// Full `__schema` response JSON.
    pub schema_response: String,
    /// Map of type name -> `__type` response JSON.
    pub type_responses:  HashMap<String, String>,
}

impl IntrospectionResponses {
    /// Build introspection responses from compiled schema.
    ///
    /// This is called once at server startup and cached.
    #[must_use]
    pub fn build(schema: &CompiledSchema) -> Self {
        let introspection = IntrospectionBuilder::build(schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Build __schema response
        let schema_response = serde_json::json!({
            "data": {
                "__schema": introspection
            }
        })
        .to_string();

        // Build __type responses for each type
        let mut type_responses = HashMap::new();
        for (name, t) in type_map {
            let response = serde_json::json!({
                "data": {
                    "__type": t
                }
            })
            .to_string();
            type_responses.insert(name, response);
        }

        Self {
            schema_response,
            type_responses,
        }
    }

    /// Get response for `__type(name: "...")` query.
    #[must_use]
    pub fn get_type_response(&self, type_name: &str) -> String {
        self.type_responses.get(type_name).cloned().unwrap_or_else(|| {
            serde_json::json!({
                "data": {
                    "__type": null
                }
            })
            .to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AutoParams, FieldType};

    fn test_schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();

        // Add a User type
        schema.types.push(
            TypeDefinition::new("User", "v_user")
                .with_field(FieldDefinition::new("id", FieldType::Id))
                .with_field(FieldDefinition::new("name", FieldType::String))
                .with_field(FieldDefinition::nullable("email", FieldType::String))
                .with_description("A user in the system"),
        );

        // Add a users query
        schema.queries.push(QueryDefinition {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  Some("Get all users".to_string()),
            auto_params:  AutoParams::default(),
            deprecation:  None,
            jsonb_column: "data".to_string(),
        });

        // Add a user query with argument
        schema.queries.push(QueryDefinition {
            name:         "user".to_string(),
            return_type:  "User".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![crate::schema::ArgumentDefinition {
                name:          "id".to_string(),
                arg_type:      FieldType::Id,
                nullable:      false, // required
                default_value: None,
                description:   Some("User ID".to_string()),
                deprecation:   None,
            }],
            sql_source:   Some("v_user".to_string()),
            description:  Some("Get user by ID".to_string()),
            auto_params:  AutoParams::default(),
            deprecation:  None,
            jsonb_column: "data".to_string(),
        });

        schema
    }

    #[test]
    fn test_build_introspection_schema() {
        let schema = test_schema();
        let introspection = IntrospectionBuilder::build(&schema);

        // Should have Query type
        assert_eq!(introspection.query_type.name, "Query");

        // Should not have Mutation type (no mutations)
        assert!(introspection.mutation_type.is_none());

        // Should have built-in scalars
        let scalar_names: Vec<_> = introspection
            .types
            .iter()
            .filter(|t| t.kind == TypeKind::Scalar)
            .filter_map(|t| t.name.as_ref())
            .collect();
        assert!(scalar_names.contains(&&"Int".to_string()));
        assert!(scalar_names.contains(&&"String".to_string()));
        assert!(scalar_names.contains(&&"Boolean".to_string()));

        // Should have User type
        let user_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"User".to_string()));
        assert!(user_type.is_some());
        let user_type = user_type.unwrap();
        assert_eq!(user_type.kind, TypeKind::Object);
        assert!(user_type.fields.is_some());
        assert_eq!(user_type.fields.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_build_introspection_responses() {
        let schema = test_schema();
        let responses = IntrospectionResponses::build(&schema);

        // Should have schema response
        assert!(responses.schema_response.contains("__schema"));
        assert!(responses.schema_response.contains("Query"));

        // Should have type responses
        assert!(responses.type_responses.contains_key("User"));
        assert!(responses.type_responses.contains_key("Query"));
        assert!(responses.type_responses.contains_key("Int"));

        // Unknown type should return null
        let unknown = responses.get_type_response("Unknown");
        assert!(unknown.contains("null"));
    }

    #[test]
    fn test_query_field_introspection() {
        let schema = test_schema();
        let introspection = IntrospectionBuilder::build(&schema);

        let query_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Query".to_string()))
            .unwrap();

        let fields = query_type.fields.as_ref().unwrap();

        // Should have 'users' query
        let users_field = fields.iter().find(|f| f.name == "users").unwrap();
        assert_eq!(users_field.field_type.kind, TypeKind::NonNull);
        assert!(users_field.args.is_empty());

        // Should have 'user' query with argument
        let user_field = fields.iter().find(|f| f.name == "user").unwrap();
        assert!(!user_field.args.is_empty());
        assert_eq!(user_field.args[0].name, "id");
    }

    #[test]
    fn test_field_type_non_null() {
        let schema = test_schema();
        let introspection = IntrospectionBuilder::build(&schema);

        let user_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"User".to_string()))
            .unwrap();

        let fields = user_type.fields.as_ref().unwrap();

        // 'id' should be NON_NULL
        let id_field = fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_field.field_type.kind, TypeKind::NonNull);

        // 'email' should be nullable (not wrapped in NON_NULL)
        let email_field = fields.iter().find(|f| f.name == "email").unwrap();
        assert_ne!(email_field.field_type.kind, TypeKind::NonNull);
    }

    #[test]
    fn test_deprecated_field_introspection() {
        use crate::schema::DeprecationInfo;

        // Create a schema with a deprecated field
        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition {
            name:                "Product".to_string(),
            sql_source:          "products".to_string(),
            jsonb_column:        "data".to_string(),
            description:         None,
            sql_projection_hint: None,
            implements:          vec![],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition {
                    name:           "oldSku".to_string(),
                    field_type:     FieldType::String,
                    nullable:       false,
                    description:    Some("Legacy SKU field".to_string()),
                    default_value:  None,
                    vector_config:  None,
                    alias:          None,
                    deprecation:    Some(DeprecationInfo {
                        reason: Some("Use 'sku' instead".to_string()),
                    }),
                    requires_scope: None,
                },
                FieldDefinition::new("sku", FieldType::String),
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Product type
        let product_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Product".to_string()))
            .unwrap();

        let fields = product_type.fields.as_ref().unwrap();

        // 'oldSku' should be deprecated
        let old_sku_field = fields.iter().find(|f| f.name == "oldSku").unwrap();
        assert!(old_sku_field.is_deprecated);
        assert_eq!(old_sku_field.deprecation_reason, Some("Use 'sku' instead".to_string()));

        // 'sku' should NOT be deprecated
        let sku_field = fields.iter().find(|f| f.name == "sku").unwrap();
        assert!(!sku_field.is_deprecated);
        assert!(sku_field.deprecation_reason.is_none());

        // 'id' should NOT be deprecated
        let id_field = fields.iter().find(|f| f.name == "id").unwrap();
        assert!(!id_field.is_deprecated);
        assert!(id_field.deprecation_reason.is_none());
    }

    #[test]
    fn test_enum_type_introspection() {
        use crate::schema::{EnumDefinition, EnumValueDefinition};

        let mut schema = CompiledSchema::new();

        // Add an enum type with some values, one deprecated
        schema.enums.push(EnumDefinition {
            name:        "OrderStatus".to_string(),
            description: Some("Status of an order".to_string()),
            values:      vec![
                EnumValueDefinition {
                    name:        "PENDING".to_string(),
                    description: Some("Order is pending".to_string()),
                    deprecation: None,
                },
                EnumValueDefinition {
                    name:        "PROCESSING".to_string(),
                    description: None,
                    deprecation: None,
                },
                EnumValueDefinition {
                    name:        "SHIPPED".to_string(),
                    description: None,
                    deprecation: None,
                },
                EnumValueDefinition {
                    name:        "CANCELLED".to_string(),
                    description: Some("Order was cancelled".to_string()),
                    deprecation: Some(crate::schema::DeprecationInfo {
                        reason: Some("Use REFUNDED instead".to_string()),
                    }),
                },
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find OrderStatus enum
        let order_status = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"OrderStatus".to_string()))
            .unwrap();

        assert_eq!(order_status.kind, TypeKind::Enum);
        assert_eq!(order_status.description, Some("Status of an order".to_string()));

        // Should have enum_values
        let enum_values = order_status.enum_values.as_ref().unwrap();
        assert_eq!(enum_values.len(), 4);

        // Check PENDING value
        let pending = enum_values.iter().find(|v| v.name == "PENDING").unwrap();
        assert_eq!(pending.description, Some("Order is pending".to_string()));
        assert!(!pending.is_deprecated);
        assert!(pending.deprecation_reason.is_none());

        // Check CANCELLED value (deprecated)
        let cancelled = enum_values.iter().find(|v| v.name == "CANCELLED").unwrap();
        assert!(cancelled.is_deprecated);
        assert_eq!(cancelled.deprecation_reason, Some("Use REFUNDED instead".to_string()));

        // Enum should not have fields
        assert!(order_status.fields.is_none());
    }

    #[test]
    fn test_input_object_introspection() {
        use crate::schema::{InputFieldDefinition, InputObjectDefinition};

        let mut schema = CompiledSchema::new();

        // Add an input object type
        schema.input_types.push(InputObjectDefinition {
            name:        "UserFilter".to_string(),
            description: Some("Filter for user queries".to_string()),
            fields:      vec![
                InputFieldDefinition {
                    name:             "name".to_string(),
                    field_type:       "String".to_string(),
                    description:      Some("Filter by name".to_string()),
                    default_value:    None,
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
                InputFieldDefinition {
                    name:             "email".to_string(),
                    field_type:       "String".to_string(),
                    description:      None,
                    default_value:    None,
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
                InputFieldDefinition {
                    name:             "limit".to_string(),
                    field_type:       "Int".to_string(),
                    description:      Some("Max results".to_string()),
                    default_value:    Some("10".to_string()),
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
            ],
            metadata:    None,
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find UserFilter input type
        let user_filter = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"UserFilter".to_string()))
            .unwrap();

        assert_eq!(user_filter.kind, TypeKind::InputObject);
        assert_eq!(user_filter.description, Some("Filter for user queries".to_string()));

        // Should have input_fields
        let input_fields = user_filter.input_fields.as_ref().unwrap();
        assert_eq!(input_fields.len(), 3);

        // Check name field
        let name_field = input_fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name_field.description, Some("Filter by name".to_string()));
        assert!(name_field.default_value.is_none());

        // Check limit field with default value
        let limit_field = input_fields.iter().find(|f| f.name == "limit").unwrap();
        assert_eq!(limit_field.description, Some("Max results".to_string()));
        assert_eq!(limit_field.default_value, Some("10".to_string()));

        // Input object should not have fields
        assert!(user_filter.fields.is_none());
    }

    #[test]
    fn test_enum_in_type_map() {
        use crate::schema::EnumDefinition;

        let mut schema = CompiledSchema::new();
        schema.enums.push(EnumDefinition {
            name:        "Status".to_string(),
            description: None,
            values:      vec![],
        });

        let introspection = IntrospectionBuilder::build(&schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Enum should be in the type map
        assert!(type_map.contains_key("Status"));
        let status = type_map.get("Status").unwrap();
        assert_eq!(status.kind, TypeKind::Enum);
    }

    #[test]
    fn test_input_object_in_type_map() {
        use crate::schema::InputObjectDefinition;

        let mut schema = CompiledSchema::new();
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            description: None,
            fields:      vec![],
            metadata:    None,
        });

        let introspection = IntrospectionBuilder::build(&schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Input object should be in the type map
        assert!(type_map.contains_key("CreateUserInput"));
        let input = type_map.get("CreateUserInput").unwrap();
        assert_eq!(input.kind, TypeKind::InputObject);
    }

    #[test]
    fn test_interface_introspection() {
        use crate::schema::InterfaceDefinition;

        let mut schema = CompiledSchema::new();

        // Add a Node interface
        schema.interfaces.push(InterfaceDefinition {
            name:        "Node".to_string(),
            description: Some("An object with a globally unique ID".to_string()),
            fields:      vec![FieldDefinition::new("id", FieldType::Id)],
        });

        // Add types that implement the interface
        schema.types.push(TypeDefinition {
            name:                "User".to_string(),
            sql_source:          "users".to_string(),
            jsonb_column:        "data".to_string(),
            description:         Some("A user".to_string()),
            sql_projection_hint: None,
            implements:          vec!["Node".to_string()],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
            ],
        });

        schema.types.push(TypeDefinition {
            name:                "Post".to_string(),
            sql_source:          "posts".to_string(),
            jsonb_column:        "data".to_string(),
            description:         Some("A blog post".to_string()),
            sql_projection_hint: None,
            implements:          vec!["Node".to_string()],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("title", FieldType::String),
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Node interface
        let node = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Node".to_string()))
            .unwrap();

        assert_eq!(node.kind, TypeKind::Interface);
        assert_eq!(node.description, Some("An object with a globally unique ID".to_string()));

        // Interface should have fields
        let fields = node.fields.as_ref().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "id");

        // Interface should have possible_types (implementors)
        let possible_types = node.possible_types.as_ref().unwrap();
        assert_eq!(possible_types.len(), 2);
        assert!(possible_types.iter().any(|t| t.name == "User"));
        assert!(possible_types.iter().any(|t| t.name == "Post"));

        // Interface should not have enum_values or input_fields
        assert!(node.enum_values.is_none());
        assert!(node.input_fields.is_none());
    }

    #[test]
    fn test_type_implements_interface() {
        use crate::schema::InterfaceDefinition;

        let mut schema = CompiledSchema::new();

        // Add interfaces
        schema.interfaces.push(InterfaceDefinition {
            name:        "Node".to_string(),
            description: None,
            fields:      vec![FieldDefinition::new("id", FieldType::Id)],
        });

        schema.interfaces.push(InterfaceDefinition {
            name:        "Timestamped".to_string(),
            description: None,
            fields:      vec![FieldDefinition::new("createdAt", FieldType::DateTime)],
        });

        // Add a type that implements both interfaces
        schema.types.push(TypeDefinition {
            name:                "Comment".to_string(),
            sql_source:          "comments".to_string(),
            jsonb_column:        "data".to_string(),
            description:         None,
            sql_projection_hint: None,
            implements:          vec!["Node".to_string(), "Timestamped".to_string()],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("createdAt", FieldType::DateTime),
                FieldDefinition::new("text", FieldType::String),
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Comment type
        let comment = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Comment".to_string()))
            .unwrap();

        assert_eq!(comment.kind, TypeKind::Object);

        // Type should list interfaces it implements
        let interfaces = comment.interfaces.as_ref().unwrap();
        assert_eq!(interfaces.len(), 2);
        assert!(interfaces.iter().any(|i| i.name == "Node"));
        assert!(interfaces.iter().any(|i| i.name == "Timestamped"));
    }

    #[test]
    fn test_interface_in_type_map() {
        use crate::schema::InterfaceDefinition;

        let mut schema = CompiledSchema::new();
        schema.interfaces.push(InterfaceDefinition {
            name:        "Searchable".to_string(),
            description: None,
            fields:      vec![],
        });

        let introspection = IntrospectionBuilder::build(&schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Interface should be in the type map
        assert!(type_map.contains_key("Searchable"));
        let interface = type_map.get("Searchable").unwrap();
        assert_eq!(interface.kind, TypeKind::Interface);
    }

    #[test]
    fn test_filter_deprecated_fields() {
        // Create a type with some deprecated fields
        let introspection_type = IntrospectionType {
            kind:               TypeKind::Object,
            name:               Some("TestType".to_string()),
            description:        None,
            fields:             Some(vec![
                IntrospectionField {
                    name:               "id".to_string(),
                    description:        None,
                    args:               vec![],
                    field_type:         IntrospectionBuilder::type_ref("ID"),
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
                IntrospectionField {
                    name:               "oldField".to_string(),
                    description:        None,
                    args:               vec![],
                    field_type:         IntrospectionBuilder::type_ref("String"),
                    is_deprecated:      true,
                    deprecation_reason: Some("Use newField instead".to_string()),
                },
                IntrospectionField {
                    name:               "newField".to_string(),
                    description:        None,
                    args:               vec![],
                    field_type:         IntrospectionBuilder::type_ref("String"),
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
            ]),
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        };

        // With includeDeprecated = false, should only have 2 fields
        let filtered = introspection_type.filter_deprecated_fields(false);
        let fields = filtered.fields.as_ref().unwrap();
        assert_eq!(fields.len(), 2);
        assert!(fields.iter().any(|f| f.name == "id"));
        assert!(fields.iter().any(|f| f.name == "newField"));
        assert!(!fields.iter().any(|f| f.name == "oldField"));

        // With includeDeprecated = true, should have all 3 fields
        let unfiltered = introspection_type.filter_deprecated_fields(true);
        let fields = unfiltered.fields.as_ref().unwrap();
        assert_eq!(fields.len(), 3);
    }

    #[test]
    fn test_filter_deprecated_enum_values() {
        // Create an enum type with some deprecated values
        let introspection_type = IntrospectionType {
            kind:               TypeKind::Enum,
            name:               Some("Status".to_string()),
            description:        None,
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        Some(vec![
                IntrospectionEnumValue {
                    name:               "ACTIVE".to_string(),
                    description:        None,
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
                IntrospectionEnumValue {
                    name:               "INACTIVE".to_string(),
                    description:        None,
                    is_deprecated:      true,
                    deprecation_reason: Some("Use DISABLED instead".to_string()),
                },
                IntrospectionEnumValue {
                    name:               "DISABLED".to_string(),
                    description:        None,
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
            ]),
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        };

        // With includeDeprecated = false, should only have 2 values
        let filtered = introspection_type.filter_deprecated_enum_values(false);
        let values = filtered.enum_values.as_ref().unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|v| v.name == "ACTIVE"));
        assert!(values.iter().any(|v| v.name == "DISABLED"));
        assert!(!values.iter().any(|v| v.name == "INACTIVE"));

        // With includeDeprecated = true, should have all 3 values
        let unfiltered = introspection_type.filter_deprecated_enum_values(true);
        let values = unfiltered.enum_values.as_ref().unwrap();
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_specified_by_url_for_custom_scalars() {
        let schema = CompiledSchema::new();
        let introspection = IntrospectionBuilder::build(&schema);

        // Find DateTime scalar
        let datetime = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"DateTime".to_string()))
            .unwrap();

        assert_eq!(datetime.kind, TypeKind::Scalar);
        assert!(datetime.specified_by_u_r_l.is_some());
        assert!(datetime.specified_by_u_r_l.as_ref().unwrap().contains("date-time"));

        // Find UUID scalar
        let uuid = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"UUID".to_string()))
            .unwrap();

        assert_eq!(uuid.kind, TypeKind::Scalar);
        assert!(uuid.specified_by_u_r_l.is_some());
        assert!(uuid.specified_by_u_r_l.as_ref().unwrap().contains("rfc4122"));

        // Built-in scalars (Int, String, etc.) should NOT have specifiedByURL
        let int = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Int".to_string()))
            .unwrap();

        assert_eq!(int.kind, TypeKind::Scalar);
        assert!(int.specified_by_u_r_l.is_none());
    }

    #[test]
    fn test_deprecated_query_introspection() {
        use crate::schema::{ArgumentDefinition, AutoParams, DeprecationInfo};

        let mut schema = CompiledSchema::new();

        // Add a deprecated query
        schema.queries.push(QueryDefinition {
            name:         "oldUsers".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  Some("Old way to get users".to_string()),
            auto_params:  AutoParams::default(),
            deprecation:  Some(DeprecationInfo {
                reason: Some("Use 'users' instead".to_string()),
            }),
            jsonb_column: "data".to_string(),
        });

        // Add a non-deprecated query with a deprecated argument
        schema.queries.push(QueryDefinition {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![
                ArgumentDefinition {
                    name:          "first".to_string(),
                    arg_type:      FieldType::Int,
                    nullable:      true,
                    default_value: None,
                    description:   Some("Number of results to return".to_string()),
                    deprecation:   None,
                },
                ArgumentDefinition {
                    name:          "limit".to_string(),
                    arg_type:      FieldType::Int,
                    nullable:      true,
                    default_value: None,
                    description:   Some("Old parameter for limiting results".to_string()),
                    deprecation:   Some(DeprecationInfo {
                        reason: Some("Use 'first' instead".to_string()),
                    }),
                },
            ],
            sql_source:   Some("v_user".to_string()),
            description:  Some("Get users with pagination".to_string()),
            auto_params:  AutoParams::default(),
            deprecation:  None,
            jsonb_column: "data".to_string(),
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Query type
        let query_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Query".to_string()))
            .unwrap();

        let fields = query_type.fields.as_ref().unwrap();

        // 'oldUsers' should be deprecated
        let old_users = fields.iter().find(|f| f.name == "oldUsers").unwrap();
        assert!(old_users.is_deprecated);
        assert_eq!(old_users.deprecation_reason, Some("Use 'users' instead".to_string()));

        // 'users' should NOT be deprecated
        let users = fields.iter().find(|f| f.name == "users").unwrap();
        assert!(!users.is_deprecated);
        assert!(users.deprecation_reason.is_none());

        // 'users' should have 2 arguments
        assert_eq!(users.args.len(), 2);

        // 'first' argument should NOT be deprecated
        let first_arg = users.args.iter().find(|a| a.name == "first").unwrap();
        assert!(!first_arg.is_deprecated);
        assert!(first_arg.deprecation_reason.is_none());

        // 'limit' argument should be deprecated
        let limit_arg = users.args.iter().find(|a| a.name == "limit").unwrap();
        assert!(limit_arg.is_deprecated);
        assert_eq!(limit_arg.deprecation_reason, Some("Use 'first' instead".to_string()));
    }

    #[test]
    fn test_deprecated_input_field_introspection() {
        use crate::schema::{DeprecationInfo, InputFieldDefinition, InputObjectDefinition};

        let mut schema = CompiledSchema::new();

        // Add an input type with a deprecated field
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            description: Some("Input for creating a user".to_string()),
            fields:      vec![
                InputFieldDefinition {
                    name:             "name".to_string(),
                    field_type:       "String!".to_string(),
                    default_value:    None,
                    description:      Some("User name".to_string()),
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
                InputFieldDefinition {
                    name:             "oldEmail".to_string(),
                    field_type:       "String".to_string(),
                    default_value:    None,
                    description:      Some("Legacy email field".to_string()),
                    deprecation:      Some(DeprecationInfo {
                        reason: Some("Use 'email' instead".to_string()),
                    }),
                    validation_rules: Vec::new(),
                },
            ],
            metadata:    None,
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find CreateUserInput type
        let create_user_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"CreateUserInput".to_string()))
            .unwrap();

        let input_fields = create_user_input.input_fields.as_ref().unwrap();

        // 'name' should NOT be deprecated
        let name_field = input_fields.iter().find(|f| f.name == "name").unwrap();
        assert!(!name_field.is_deprecated);
        assert!(name_field.deprecation_reason.is_none());

        // 'oldEmail' should be deprecated
        let old_email = input_fields.iter().find(|f| f.name == "oldEmail").unwrap();
        assert!(old_email.is_deprecated);
        assert_eq!(old_email.deprecation_reason, Some("Use 'email' instead".to_string()));
    }
}
