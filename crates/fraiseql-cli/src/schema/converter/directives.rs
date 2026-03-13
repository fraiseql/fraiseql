use anyhow::{Context, Result};
use fraiseql_core::schema::{DirectiveDefinition, DirectiveLocationKind};
use tracing::warn;

use super::SchemaConverter;
use crate::schema::intermediate::IntermediateDirective;

impl SchemaConverter {
    pub(super) fn convert_directive(
        intermediate: IntermediateDirective,
    ) -> Result<DirectiveDefinition> {
        let arguments = intermediate
            .arguments
            .into_iter()
            .map(Self::convert_argument)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert directive '{}'", intermediate.name))?;

        // Parse directive locations
        let locations = intermediate
            .locations
            .into_iter()
            .filter_map(|loc| Self::parse_directive_location(&loc))
            .collect();

        Ok(DirectiveDefinition {
            name: intermediate.name,
            description: intermediate.description,
            locations,
            arguments,
            is_repeatable: intermediate.repeatable,
        })
    }

    pub(super) fn parse_directive_location(location: &str) -> Option<DirectiveLocationKind> {
        match location {
            // Type System Directive Locations
            "SCHEMA" => Some(DirectiveLocationKind::Schema),
            "SCALAR" => Some(DirectiveLocationKind::Scalar),
            "OBJECT" => Some(DirectiveLocationKind::Object),
            "FIELD_DEFINITION" => Some(DirectiveLocationKind::FieldDefinition),
            "ARGUMENT_DEFINITION" => Some(DirectiveLocationKind::ArgumentDefinition),
            "INTERFACE" => Some(DirectiveLocationKind::Interface),
            "UNION" => Some(DirectiveLocationKind::Union),
            "ENUM" => Some(DirectiveLocationKind::Enum),
            "ENUM_VALUE" => Some(DirectiveLocationKind::EnumValue),
            "INPUT_OBJECT" => Some(DirectiveLocationKind::InputObject),
            "INPUT_FIELD_DEFINITION" => Some(DirectiveLocationKind::InputFieldDefinition),
            // Executable Directive Locations
            "QUERY" => Some(DirectiveLocationKind::Query),
            "MUTATION" => Some(DirectiveLocationKind::Mutation),
            "SUBSCRIPTION" => Some(DirectiveLocationKind::Subscription),
            "FIELD" => Some(DirectiveLocationKind::Field),
            "FRAGMENT_DEFINITION" => Some(DirectiveLocationKind::FragmentDefinition),
            "FRAGMENT_SPREAD" => Some(DirectiveLocationKind::FragmentSpread),
            "INLINE_FRAGMENT" => Some(DirectiveLocationKind::InlineFragment),
            "VARIABLE_DEFINITION" => Some(DirectiveLocationKind::VariableDefinition),
            _ => {
                warn!("Unknown directive location: {}", location);
                None
            },
        }
    }
}
