use anyhow::{Context, Result};
use fraiseql_core::{
    schema::{
        EnumDefinition, EnumValueDefinition, FieldDefinition, FieldDenyPolicy, FieldType,
        InputFieldDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition,
        UnionDefinition,
    },
    validation::CustomTypeDef,
};

use super::SchemaConverter;
use crate::schema::intermediate::{
    IntermediateEnum, IntermediateEnumValue, IntermediateField, IntermediateInputField,
    IntermediateInputObject, IntermediateInterface, IntermediateScalar, IntermediateType,
    IntermediateUnion,
};

impl SchemaConverter {
    pub(super) fn convert_type(intermediate: IntermediateType) -> Result<TypeDefinition> {
        let fields = intermediate
            .fields
            .into_iter()
            .map(Self::convert_field)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert type '{}'", intermediate.name))?;

        Ok(TypeDefinition {
            name: intermediate.name.into(),
            fields,
            description: intermediate.description,
            sql_source: String::new().into(), // Not used for regular types (empty string)
            jsonb_column: String::new(),      // Not used for regular types (empty string)
            sql_projection_hint: None,        // Will be populated by optimizer in
            implements: intermediate.implements,
            requires_role: intermediate.requires_role,
            is_error: intermediate.is_error,
            relay: intermediate.relay,
            relationships: Vec::new(),
        })
    }

    /// Convert `IntermediateEnum` to `EnumDefinition`
    pub(super) fn convert_enum(intermediate: IntermediateEnum) -> EnumDefinition {
        let values = intermediate.values.into_iter().map(Self::convert_enum_value).collect();

        EnumDefinition {
            name: intermediate.name,
            values,
            description: intermediate.description,
        }
    }

    /// Convert `IntermediateEnumValue` to `EnumValueDefinition`
    pub(super) fn convert_enum_value(intermediate: IntermediateEnumValue) -> EnumValueDefinition {
        let deprecation = intermediate
            .deprecated
            .map(|d| fraiseql_core::schema::DeprecationInfo { reason: d.reason });

        EnumValueDefinition {
            name: intermediate.name,
            description: intermediate.description,
            deprecation,
        }
    }

    /// Convert `IntermediateScalar` to `CustomTypeDef`
    pub(super) fn convert_custom_scalar(intermediate: IntermediateScalar) -> Result<CustomTypeDef> {
        Ok(CustomTypeDef {
            name:             intermediate.name,
            description:      intermediate.description,
            specified_by_url: intermediate.specified_by_url,
            validation_rules: intermediate.validation_rules,
            elo_expression:   None,
            base_type:        intermediate.base_type,
        })
    }

    /// Convert `IntermediateInputObject` to `InputObjectDefinition`
    pub(super) fn convert_input_object(
        intermediate: IntermediateInputObject,
    ) -> InputObjectDefinition {
        let fields = intermediate.fields.into_iter().map(Self::convert_input_field).collect();

        InputObjectDefinition {
            name: intermediate.name,
            fields,
            description: intermediate.description,
            metadata: None,
        }
    }

    /// Convert `IntermediateInputField` to `InputFieldDefinition`
    pub(super) fn convert_input_field(
        intermediate: IntermediateInputField,
    ) -> InputFieldDefinition {
        let deprecation = intermediate
            .deprecated
            .map(|d| fraiseql_core::schema::DeprecationInfo { reason: d.reason });

        // Convert default value to JSON string if present
        let default_value = intermediate.default.map(|v| v.to_string());

        InputFieldDefinition {
            name: intermediate.name,
            field_type: intermediate.field_type,
            description: intermediate.description,
            default_value,
            deprecation,
            validation_rules: Vec::new(),
        }
    }

    /// Convert `IntermediateInterface` to `InterfaceDefinition`
    pub(super) fn convert_interface(
        intermediate: IntermediateInterface,
    ) -> Result<InterfaceDefinition> {
        let fields = intermediate
            .fields
            .into_iter()
            .map(Self::convert_field)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert interface '{}'", intermediate.name))?;

        Ok(InterfaceDefinition {
            name: intermediate.name,
            fields,
            description: intermediate.description,
        })
    }

    /// Convert `IntermediateUnion` to `UnionDefinition`
    pub(super) fn convert_union(intermediate: IntermediateUnion) -> UnionDefinition {
        let mut union_def =
            UnionDefinition::new(&intermediate.name).with_members(intermediate.member_types);
        if let Some(desc) = intermediate.description {
            union_def = union_def.with_description(&desc);
        }
        union_def
    }

    /// Convert `IntermediateField` to `FieldDefinition`
    ///
    /// **Key normalization**: `type` → `field_type`
    pub(super) fn convert_field(intermediate: IntermediateField) -> Result<FieldDefinition> {
        let field_type = Self::parse_field_type(&intermediate.field_type)?;

        // Extract deprecation info from @deprecated directive if present
        let deprecation = intermediate.directives.as_ref().and_then(|directives| {
            directives.iter().find(|d| d.name == "deprecated").map(|d| {
                let reason = d
                    .arguments
                    .as_ref()
                    .and_then(|args| args.get("reason").and_then(|v| v.as_str()).map(String::from));
                fraiseql_core::schema::DeprecationInfo { reason }
            })
        });

        Ok(FieldDefinition {
            name: intermediate.name.into(),
            field_type,
            nullable: intermediate.nullable,
            default_value: None,
            description: intermediate.description,
            vector_config: None,
            alias: None,
            deprecation,
            requires_scope: intermediate.requires_scope,
            on_deny: intermediate.on_deny.map_or(FieldDenyPolicy::default(), |v| {
                if v == "mask" {
                    FieldDenyPolicy::Mask
                } else {
                    FieldDenyPolicy::Reject
                }
            }),
            encryption: None,
            auto_generated: false,
            computed: false,
        })
    }

    /// Parse string type name to `FieldType` enum.
    ///
    /// Handles built-in scalars, custom object types, and strips trailing `!`
    /// (non-null markers) that authoring tools may emit. Nullability is tracked
    /// separately via the `nullable` field, so the `!` suffix is redundant.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the type name is empty after stripping.
    pub(super) fn parse_field_type(type_name: &str) -> Result<FieldType> {
        let normalized = type_name.trim_end_matches('!');
        if normalized != type_name {
            eprintln!(
                "warning: type \"{type_name}\" contains trailing `!` — \
                 use \"{normalized}\" instead (nullability is controlled by the `nullable` field)"
            );
        }

        match normalized {
            "String" => Ok(FieldType::String),
            "Int" => Ok(FieldType::Int),
            "Float" => Ok(FieldType::Float),
            "Boolean" => Ok(FieldType::Boolean),
            "ID" => Ok(FieldType::Id),
            "DateTime" => Ok(FieldType::DateTime),
            "Date" => Ok(FieldType::Date),
            "Time" => Ok(FieldType::Time),
            "Json" => Ok(FieldType::Json),
            "UUID" => Ok(FieldType::Uuid),
            "Decimal" => Ok(FieldType::Decimal),
            "Vector" => Ok(FieldType::Vector),
            // Custom object types (User, Post, etc.)
            custom => Ok(FieldType::Object(custom.to_string())),
        }
    }

    /// Check whether a string is a safe SQL identifier.
    ///
    /// Accepts identifiers matching `[A-Za-z_][A-Za-z0-9_]*` (no spaces, dots, or
    /// special characters).  This prevents SQL injection via view names supplied in
    /// `additional_views` or `invalidates_fact_tables`.
    pub(super) fn is_safe_sql_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let mut chars = s.chars();
        let first = chars.next().expect("non-empty checked above");
        if !first.is_ascii_alphabetic() && first != '_' {
            return false;
        }
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
}
