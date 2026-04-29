//! Per-type introspection builders.
//!
//! Functions that convert each schema type (object, enum, input object, interface,
//! union) into their corresponding `IntrospectionType` nodes, including built-in
//! scalar definitions.

use super::{
    super::{
        CompiledSchema, EnumDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition,
        UnionDefinition,
    },
    field_resolver::{build_field, build_validation_rule, type_ref},
    types::{
        IntrospectionEnumValue, IntrospectionField, IntrospectionInputValue, IntrospectionType, IntrospectionTypeRef,
        TypeKind,
    },
};

// =============================================================================
// Built-in scalar types
// =============================================================================

/// Return `IntrospectionType` nodes for all built-in GraphQL scalars.
pub(super) fn builtin_scalars() -> Vec<IntrospectionType> {
    vec![
        scalar_type("Int", "Built-in Int scalar"),
        scalar_type("Float", "Built-in Float scalar"),
        scalar_type("String", "Built-in String scalar"),
        scalar_type("Boolean", "Built-in Boolean scalar"),
        scalar_type("ID", "Built-in ID scalar"),
        // FraiseQL custom scalars (with specifiedByURL per GraphQL spec §3.5.5)
        scalar_type_with_url(
            "DateTime",
            "ISO-8601 datetime string",
            Some("https://scalars.graphql.org/andimarek/date-time"),
        ),
        scalar_type_with_url(
            "Date",
            "ISO-8601 date string",
            Some("https://scalars.graphql.org/andimarek/local-date"),
        ),
        scalar_type_with_url(
            "Time",
            "ISO-8601 time string",
            Some("https://scalars.graphql.org/andimarek/local-time"),
        ),
        scalar_type_with_url("UUID", "UUID string", Some("https://tools.ietf.org/html/rfc4122")),
        scalar_type_with_url(
            "JSON",
            "Arbitrary JSON value",
            Some("https://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf"),
        ),
        scalar_type("Decimal", "Decimal number"),
    ]
}

// =============================================================================
// Built-in object types
// =============================================================================

/// Return the `IntrospectionType` for the built-in `MutationError` object type.
///
/// `MutationError` is the implicit error member for mutations that declare
/// neither a `@fraiseql.union` nor a `{ReturnType}Error` convention type.
/// Clients can use `... on MutationError { message status metadata }` in any
/// mutation selection set without any schema-level declaration.
pub(super) fn builtin_mutation_error_type() -> IntrospectionType {
    let make_string_field = |name: &str, description: &str| IntrospectionField {
        name:               name.to_string(),
        description:        Some(description.to_string()),
        args:               vec![],
        field_type:         non_null_string(),
        is_deprecated:      false,
        deprecation_reason: None,
    };

    let message_field = make_string_field(
        "message",
        "Human-readable error summary safe to show to end users.",
    );
    let status_field = make_string_field(
        "status",
        "Machine-readable error class (e.g. \"validation\", \"not_found\").",
    );
    let metadata_field = IntrospectionField {
        name:               "metadata".to_string(),
        description:        Some(
            "Structured error payload from error_detail (may be null).".to_string(),
        ),
        args:               vec![],
        field_type:         type_ref("JSON"),  // nullable JSON
        is_deprecated:      false,
        deprecation_reason: None,
    };

    IntrospectionType {
        kind:               TypeKind::Object,
        name:               Some("MutationError".to_string()),
        description:        Some(
            "Built-in error type for mutations without an explicit @fraiseql.union. \
             Use `... on MutationError { message status metadata }` in your selection set."
                .to_string(),
        ),
        fields:             Some(vec![message_field, status_field, metadata_field]),
        interfaces:         Some(vec![]),
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            None,
        specified_by_u_r_l: None,
    }
}

/// Convenience: `String!` (non-null String) introspection type.
fn non_null_string() -> IntrospectionType {
    IntrospectionType {
        kind:               TypeKind::NonNull,
        name:               None,
        description:        None,
        fields:             None,
        interfaces:         None,
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            Some(Box::new(type_ref("String"))),
        specified_by_u_r_l: None,
    }
}

/// Create a scalar type introspection without a `specifiedByURL`.
fn scalar_type(name: &str, description: &str) -> IntrospectionType {
    scalar_type_with_url(name, description, None)
}

/// Create a scalar type introspection with an optional `specifiedByURL`.
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

// =============================================================================
// User-defined type builders
// =============================================================================

/// Build `__Type` for an object type definition.
pub(super) fn build_object_type(type_def: &TypeDefinition) -> IntrospectionType {
    let fields = type_def.fields.iter().map(build_field).collect();

    // Build interfaces that this type implements
    let interfaces: Vec<IntrospectionTypeRef> = type_def
        .implements
        .iter()
        .map(|name| IntrospectionTypeRef { name: name.clone() })
        .collect();

    IntrospectionType {
        kind:               TypeKind::Object,
        name:               Some(type_def.name.to_string()),
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

/// Build `__Type` for an enum definition.
pub(super) fn build_enum_type(enum_def: &EnumDefinition) -> IntrospectionType {
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

/// Build `__Type` for an input object definition.
pub(super) fn build_input_object_type(input_def: &InputObjectDefinition) -> IntrospectionType {
    let input_fields = input_def
        .fields
        .iter()
        .map(|f| {
            let validation_rules = f.validation_rules.iter().map(build_validation_rule).collect();

            IntrospectionInputValue {
                name: f.name.clone(),
                description: f.description.clone(),
                input_type: type_ref(&f.field_type),
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

/// Build `__Type` for an interface definition, including its implementors.
pub(super) fn build_interface_type(
    interface_def: &InterfaceDefinition,
    schema: &CompiledSchema,
) -> IntrospectionType {
    // Build fields for the interface
    let fields = interface_def.fields.iter().map(build_field).collect();

    // Find all types that implement this interface
    let possible_types: Vec<IntrospectionTypeRef> = schema
        .find_implementors(&interface_def.name)
        .iter()
        .map(|t| IntrospectionTypeRef {
            name: t.name.to_string(),
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

/// Build `__Type` for a union definition.
pub(super) fn build_union_type(union_def: &UnionDefinition) -> IntrospectionType {
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
