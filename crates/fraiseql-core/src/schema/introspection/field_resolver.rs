//! Field introspection helpers.
//!
//! Converts `FieldDefinition`, `FieldType`, and validation rules from the compiled
//! schema into their `__Field`, `__InputValue`, and related introspection types.

use super::super::{FieldDefinition, FieldType};
use super::types::{
    IntrospectionField, IntrospectionInputValue, IntrospectionType, IntrospectionValidationRule,
    TypeKind,
};

// =============================================================================
// Field and type-reference helpers (used by IntrospectionBuilder)
// =============================================================================

/// Build `__Field` introspection from `FieldDefinition`.
pub(super) fn build_field(field: &FieldDefinition) -> IntrospectionField {
    IntrospectionField {
        name:               field.output_name().to_string(),
        description:        field.description.clone(),
        args:               vec![], // Regular fields don't have args
        field_type:         field_type_to_introspection(&field.field_type, field.nullable),
        is_deprecated:      field.is_deprecated(),
        deprecation_reason: field.deprecation_reason().map(ToString::to_string),
    }
}

/// Convert `FieldType` to introspection type, wrapping in `NON_NULL` when not nullable.
pub(super) fn field_type_to_introspection(
    field_type: &FieldType,
    nullable: bool,
) -> IntrospectionType {
    let inner = match field_type {
        FieldType::Int => type_ref("Int"),
        FieldType::Float => type_ref("Float"),
        FieldType::String => type_ref("String"),
        FieldType::Boolean => type_ref("Boolean"),
        FieldType::Id => type_ref("ID"),
        FieldType::DateTime => type_ref("DateTime"),
        FieldType::Date => type_ref("Date"),
        FieldType::Time => type_ref("Time"),
        FieldType::Uuid => type_ref("UUID"),
        FieldType::Json => type_ref("JSON"),
        FieldType::Decimal => type_ref("Decimal"),
        FieldType::Object(name) => type_ref(name),
        FieldType::Enum(name) => type_ref(name),
        FieldType::Input(name) => type_ref(name),
        FieldType::Interface(name) => type_ref(name),
        FieldType::Union(name) => type_ref(name),
        FieldType::Scalar(name) => type_ref(name), // Rich/custom scalars
        FieldType::List(inner) => IntrospectionType {
            kind:               TypeKind::List,
            name:               None,
            description:        None,
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            Some(Box::new(field_type_to_introspection(inner, true))),
            specified_by_u_r_l: None,
        },
        FieldType::Vector => type_ref("JSON"), // Vectors are exposed as JSON
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

/// Create a named scalar/object type reference node.
///
/// The `kind` is set to `Scalar` as a placeholder; clients use `name` to resolve
/// the real kind from the type map.
pub fn type_ref(name: &str) -> IntrospectionType {
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

/// Convert a `ValidationRule` to its introspection representation.
pub(super) fn build_validation_rule(
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
            min:             min.map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
            max:             max.map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
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

/// Build `__InputValue` for a query/mutation/subscription argument.
pub(super) fn build_arg_input_value(
    arg: &super::super::ArgumentDefinition,
) -> IntrospectionInputValue {
    IntrospectionInputValue {
        name:               arg.name.clone(),
        description:        arg.description.clone(),
        input_type:         field_type_to_introspection(&arg.arg_type, arg.nullable),
        default_value:      arg.default_value.as_ref().map(|v| v.to_string()),
        is_deprecated:      arg.is_deprecated(),
        deprecation_reason: arg.deprecation_reason().map(ToString::to_string),
        validation_rules:   vec![],
    }
}
