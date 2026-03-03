//! `__Directive` type introspection and built-in directive definitions.
//!
//! Provides `builtin_directives()` (skip, include, deprecated) and
//! `build_custom_directives()` for schema-defined directives.

use super::super::DirectiveDefinition;
use super::field_resolver::{build_arg_input_value, type_ref};
use super::types::{DirectiveLocation, IntrospectionDirective, IntrospectionInputValue, IntrospectionType, TypeKind};

/// Return the three built-in GraphQL directives: `@skip`, `@include`, `@deprecated`.
pub(super) fn builtin_directives() -> Vec<IntrospectionDirective> {
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
                    of_type: Some(Box::new(type_ref("Boolean"))),
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
                    of_type: Some(Box::new(type_ref("Boolean"))),
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
                input_type: type_ref("String"),
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
pub(super) fn build_custom_directives(
    directives: &[DirectiveDefinition],
) -> Vec<IntrospectionDirective> {
    directives.iter().map(|d| build_custom_directive(d)).collect()
}

/// Build a single introspection directive from a custom directive definition.
fn build_custom_directive(directive: &DirectiveDefinition) -> IntrospectionDirective {
    let locations: Vec<DirectiveLocation> =
        directive.locations.iter().map(|loc| DirectiveLocation::from(*loc)).collect();

    let args: Vec<IntrospectionInputValue> =
        directive.arguments.iter().map(build_arg_input_value).collect();

    IntrospectionDirective {
        name: directive.name.clone(),
        description: directive.description.clone(),
        locations,
        args,
        is_repeatable: directive.is_repeatable,
    }
}

