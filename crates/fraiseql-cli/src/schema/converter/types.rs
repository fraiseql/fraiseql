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
    IntermediateInputObject, IntermediateInterface, IntermediateRest, IntermediateScalar,
    IntermediateType, IntermediateUnion,
};

impl SchemaConverter {
    /// Convert an `IntermediateType` to a compiled `TypeDefinition`.
    ///
    /// # Errors
    ///
    /// Returns an error if any field in the type cannot be converted.
    pub(super) fn convert_type(intermediate: IntermediateType) -> Result<TypeDefinition> {
        let fields = intermediate
            .fields
            .into_iter()
            .map(Self::convert_field)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert type '{}'", intermediate.name))?;

        // Owned types bind their relation on the query that returns them, so the
        // type-level source is normally empty. An owner-split `extend type`
        // federation entity carries a type-level `sql_source` (it has no local
        // backing query) which the `_entities` resolver uses as its fallback
        // relation (#507); its fields project from the standard `data` jsonb
        // column — symmetric with the query path, whose `jsonb_column` also
        // defaults to `data`. Regular types keep both empty (byte-identical
        // compiled output, and the optimizer's projection-hint heuristic stays off).
        let type_sql_source = intermediate.sql_source.unwrap_or_default();
        let type_jsonb_column = if type_sql_source.is_empty() {
            String::new()
        } else {
            "data".to_string()
        };

        Ok(TypeDefinition {
            name: intermediate.name.into(),
            fields,
            description: intermediate.description,
            sql_source: type_sql_source.into(),
            jsonb_column: type_jsonb_column,
            sql_projection_hint: None, // Will be populated by optimizer in
            implements: intermediate.implements,
            requires_role: intermediate.requires_role,
            is_error: intermediate.is_error,
            relay: intermediate.relay,
            embedded: intermediate.embedded,
            internal: false,
            relationships: Vec::new(),
            subscription_policy: None,
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
    ///
    /// # Errors
    ///
    /// Currently infallible; always returns `Ok`. The `Result` return type is
    /// reserved for future validation of scalar definitions.
    /// Convert `IntermediateScalar` to `CustomTypeDef`.
    ///
    /// # `validation_rules` is refused rather than carried
    ///
    /// `CompiledSchema.custom_scalars` is `#[serde(skip)]`: this converter registers the
    /// scalar into an in-memory `CustomTypeRegistry` which is then **dropped** when the
    /// schema is written to `schema.compiled.json`. Nothing in `fraiseql-server` reads it
    /// back either — the only mention is `reload_gate.rs`, which explicitly ignores the
    /// field. So a declared `pattern`, `length` or `range` reached no runtime, from any
    /// SDK, and the author got `✓ Schema compiled successfully` and a server validating
    /// nothing.
    ///
    /// Carrying the rules through here without a runtime consumer would relocate the drop
    /// one layer later rather than fix it, which is the disposition `#779` got for
    /// observers. Making them real means serializing the registry *and* giving the
    /// executor a consumer; until then the declaration is refused by the name of the key
    /// that cannot be honoured.
    ///
    /// The scalar *declaration* itself is unaffected: the name becomes known to the
    /// compiler, so a field typed with it resolves as a scalar rather than an object
    /// reference. Only the unenforceable half is refused.
    ///
    /// # Errors
    ///
    /// Returns an error naming the scalar when it declares `validation_rules`.
    pub(super) fn convert_custom_scalar(intermediate: IntermediateScalar) -> Result<CustomTypeDef> {
        if !intermediate.validation_rules.is_empty() {
            anyhow::bail!(
                "Custom scalar '{}' declares `validation_rules`, which no compiled schema \
                 carries: `CompiledSchema.custom_scalars` is not serialized and the runtime \
                 reads no scalar rules, so the constraint would never be enforced. Remove the \
                 rules and validate in the database (a CHECK constraint or a domain type), or \
                 on the mutation's SQL function.",
                intermediate.name
            );
        }

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

    /// Reinterpret an `is_input`-marked entry of the `types` array as an input object (#848).
    ///
    /// Output-only attributes are **refused**, not ignored: `sql_source`, `relay`,
    /// `requires_role`, `is_error`, `implements` and `subscribable_tables` have no meaning on
    /// a GraphQL input object, and an author who set one has a mistaken model of what they
    /// are declaring. Dropping them silently is the defect class this phase exists to close.
    ///
    /// # Errors
    ///
    /// Returns an error naming the type and the offending attribute.
    pub(super) fn input_object_from_marked_type(
        intermediate: IntermediateType,
    ) -> Result<IntermediateInputObject> {
        let name = intermediate.name;

        // (attribute is set, attribute name) — checked in declaration order so the message
        // is stable.
        let rejected: [(bool, &str); 6] = [
            (intermediate.sql_source.is_some(), "sql_source"),
            (!intermediate.implements.is_empty(), "implements"),
            (intermediate.requires_role.is_some(), "requires_role"),
            (intermediate.is_error, "is_error"),
            (intermediate.relay, "relay"),
            (intermediate.subscribable_tables.is_some(), "subscribable_tables"),
        ];
        if let Some((_, attribute)) = rejected.into_iter().find(|(set, _)| *set) {
            anyhow::bail!(
                "Type '{name}' is declared `is_input: true` but also sets `{attribute}`, which \
                 only applies to output types. A GraphQL input object has no backing relation, \
                 no interfaces and no role gate — it is only ever a shape for arguments. Remove \
                 `{attribute}`, or remove `is_input` if '{name}' is meant to be an output type."
            );
        }

        let fields = intermediate
            .fields
            .into_iter()
            .map(|field| {
                if let Some(scope) = field.requires_scope {
                    anyhow::bail!(
                        "Input object '{name}' field '{}' declares `requires_scope` \
                         ({scope:?}). Field-level scopes gate values the server *returns*; an \
                         input field carries a value the client *sends*, so the gate would \
                         never run. Remove it, and validate the argument in the SQL function \
                         instead.",
                        field.name
                    );
                }
                Ok(IntermediateInputField {
                    name:        field.name,
                    field_type:  field.field_type,
                    nullable:    field.nullable,
                    description: field.description,
                    default:     None,
                    deprecated:  None,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(IntermediateInputObject {
            name,
            fields,
            description: intermediate.description,
        })
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
            // Carry per-field nullability into the compiled schema so the runtime
            // can enforce required input fields (#414). Output fields already do
            // this via `convert_field`.
            nullable: intermediate.nullable,
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
    ///
    /// # Errors
    ///
    /// Returns an error if the field's type string cannot be parsed into a
    /// `FieldType`.
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
            authorize: intermediate.authorize.unwrap_or(false),
            encryption: None,
            hierarchy: intermediate.hierarchy,
        })
    }

    /// Parse string type name to `FieldType` enum
    ///
    /// Handles built-in scalars, custom object types, and SDL list/non-null
    /// wrappers. A list field/argument arrives as the SDL string `"[Inner!]"`,
    /// which is unwrapped (recursively) into [`FieldType::List`] so the runtime
    /// projects it as a list rather than a single object (#434). A trailing `!`
    /// (non-null marker — list-element non-null, or a redundant marker; outer
    /// field nullability is tracked separately in `nullable`) is stripped before
    /// the base name is matched.
    ///
    /// # Errors
    ///
    /// Currently infallible; unrecognised type names are treated as
    /// `FieldType::Object`. The `Result` return type is reserved for future
    /// strict validation.
    pub(super) fn parse_field_type(type_name: &str) -> Result<FieldType> {
        let type_name = type_name.trim();

        // Strip a trailing non-null marker first — it can wrap a list ("[Inner!]!")
        // or a scalar/object ("Inner!"). Doing this before the list check lets a
        // non-null list unwrap correctly; outer field nullability is tracked
        // separately in `nullable`.
        let type_name = type_name.strip_suffix('!').unwrap_or(type_name).trim();

        // SDL list wrapper: "[Inner]" / "[Inner!]" → List(parse(Inner)). Recurse so
        // nested lists ("[[Inner!]!]") and the element non-null marker are handled.
        if let Some(inner) = type_name.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            return Ok(FieldType::List(Box::new(Self::parse_field_type(inner)?)));
        }

        Ok(match type_name {
            "String" => FieldType::String,
            "Int" => FieldType::Int,
            "Float" => FieldType::Float,
            "Boolean" => FieldType::Boolean,
            "ID" => FieldType::Id,
            "DateTime" => FieldType::DateTime,
            "Date" => FieldType::Date,
            "Time" => FieldType::Time,
            "Json" => FieldType::Json,
            "UUID" => FieldType::Uuid,
            "Decimal" => FieldType::Decimal,
            "Vector" => FieldType::Vector,
            // Custom object types (User, Post, etc.)
            custom => FieldType::Object(custom.to_string()),
        })
    }

    /// Check whether a string is a safe SQL identifier.
    ///
    /// Accepts up to three dot-separated segments (`name`, `schema.name`, or
    /// `catalog.schema.name`), each matching `[A-Za-z_][A-Za-z0-9_]*`.
    /// This prevents SQL injection via view names supplied in
    /// `additional_views` or `invalidates_fact_tables`.
    pub(crate) fn is_safe_sql_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() > 3 {
            return false;
        }
        parts.iter().all(|part| {
            if part.is_empty() {
                return false;
            }
            let mut chars = part.chars();
            let first = chars.next().expect("non-empty checked above");
            if !first.is_ascii_alphabetic() && first != '_' {
                return false;
            }
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    }

    /// Convert a per-operation `rest` block into the compiled `(path, method)` pair.
    ///
    /// One function for queries and mutations, because two copies of "what does a REST
    /// annotation mean" is how the pair got dropped in the first place: the block was
    /// declared by every SDK, understood by no consumer, and hardcoded to `None` at both
    /// converter sites (#846).
    ///
    /// Validation is loud, and deliberately so. The server's route derivation reads
    /// `rest_method` through `parse_http_method(..).unwrap_or(<default>)`, so an
    /// unrecognised verb silently becomes `GET` on a query or `POST` on a mutation —
    /// the author would get a route, just not the one they asked for. Compile time is
    /// where that has to fail, because it is the last point at which the authored intent
    /// is still visible.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is empty, does not start with `/`, contains a query
    /// string or fragment, or if the method is not one of `GET`, `POST`, `PUT`, `PATCH`,
    /// `DELETE`.
    pub(crate) fn convert_rest_annotation(
        operation_kind: &str,
        operation_name: &str,
        rest: Option<IntermediateRest>,
    ) -> Result<(Option<String>, Option<String>)> {
        let Some(rest) = rest else {
            return Ok((None, None));
        };

        if rest.path.is_empty() {
            anyhow::bail!("{operation_kind} '{operation_name}': rest.path must not be empty.");
        }
        if !rest.path.starts_with('/') {
            anyhow::bail!(
                "{operation_kind} '{operation_name}': rest.path must start with '/', got {:?}.",
                rest.path
            );
        }
        if let Some(bad) = rest.path.chars().find(|c| matches!(c, '?' | '#')) {
            anyhow::bail!(
                "{operation_kind} '{operation_name}': rest.path must be a path only — it must \
                 not contain {bad:?}. Got {:?}.",
                rest.path
            );
        }

        let method = match rest.method {
            None => None,
            Some(m) => {
                let upper = m.to_ascii_uppercase();
                if !matches!(upper.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
                    anyhow::bail!(
                        "{operation_kind} '{operation_name}': rest.method {m:?} is not a \
                         supported HTTP method. Use one of GET, POST, PUT, PATCH, DELETE."
                    );
                }
                Some(upper)
            },
        };

        Ok((Some(rest.path), method))
    }
}
