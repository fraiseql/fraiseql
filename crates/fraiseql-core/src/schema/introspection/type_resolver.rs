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
    field_resolver::{build_field, build_validation_rule, type_ref_with_kind},
    types::{
        IntrospectionEnumValue, IntrospectionInputValue, IntrospectionType, IntrospectionTypeRef,
        TypeKind,
    },
};

// =============================================================================
// Built-in scalar types
// =============================================================================

/// The scalar type names introspection **publishes to clients** — and therefore
/// the names a client may legitimately write in a variable definition
/// (GraphQL § 5.8.2).
///
/// Derived from `builtin_scalars` rather than hand-listed, deliberately: the
/// two spellings of JSON in this tree are a live trap. The **authoring** table
/// `schema::BUILTIN_SCALARS` spells it `"Json"` (what an author writes in
/// `schema.json`), while everything client-facing publishes `"JSON"` — here,
/// `FieldType::to_graphql_string`, and the `type_ref` on every `where`
/// argument. A client writes what introspection told it, so § 5.8.2 must
/// resolve against *this* list; resolving against the authoring table would
/// reject `query Q($w: JSON)`, which is exactly what introspection instructs
/// clients to send.
///
/// A hand-copied list is how `BUILTIN_SCALARS` and its predecessor drifted
/// apart in the first place (#959).
#[must_use]
pub fn published_scalar_names() -> Vec<String> {
    builtin_scalars().into_iter().filter_map(|t| t.name).collect()
}

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

/// Parse a written GraphQL type — `[Foo!]!`, `Foo`, `[Foo]` — into the
/// `LIST`/`NON_NULL`/named reference chain introspection publishes.
///
/// The leaf's `kind` is resolved against the schema: a declared enum is `ENUM`,
/// a declared input object is `INPUT_OBJECT`, and anything else is `SCALAR`.
/// Falling back to `SCALAR` is the honest answer for a name the schema does not
/// define — the derived filter surface declares every type it names, and a name
/// that resolves to nothing is a defect in the *schema*, not something this
/// should paper over by guessing a kind.
fn input_type_ref(written: &str, schema: &CompiledSchema) -> IntrospectionType {
    let written = written.trim();

    let wrap = |kind: TypeKind, inner: IntrospectionType| IntrospectionType {
        kind,
        name: None,
        description: None,
        fields: None,
        interfaces: None,
        possible_types: None,
        enum_values: None,
        input_fields: None,
        of_type: Some(Box::new(inner)),
        specified_by_u_r_l: None,
    };

    if let Some(inner) = written.strip_suffix('!') {
        return wrap(TypeKind::NonNull, input_type_ref(inner, schema));
    }
    if let Some(inner) = written.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return wrap(TypeKind::List, input_type_ref(inner, schema));
    }

    let kind = if schema.find_enum(written).is_some() {
        TypeKind::Enum
    } else if schema.find_input_type(written).is_some() {
        TypeKind::InputObject
    } else {
        TypeKind::Scalar
    };
    type_ref_with_kind(written, kind)
}

/// Build `__Type` for an input object definition.
pub(super) fn build_input_object_type(
    input_def: &InputObjectDefinition,
    schema: &CompiledSchema,
) -> IntrospectionType {
    let input_fields = input_def
        .fields
        .iter()
        .map(|f| {
            let validation_rules = f.validation_rules.iter().map(build_validation_rule).collect();

            // An input field's type is stored as a *string*, so the reference has
            // to be reconstructed from it: the list and non-null wrappers become
            // LIST/NON_NULL nodes and the leaf name is resolved against the
            // schema for its kind. Publishing the whole string as one SCALAR name
            // advertised types like `"[OrderWhereInput!]"` — a name no client can
            // look up, and one that collapses a generated client's filter
            // argument into an opaque scalar.
            //
            // The field's own trailing `!` is dropped rather than read: `nullable`
            // is what carries requiredness (#414), and a filter operator field can
            // legitimately be optional while its type string is non-null.
            let named = input_type_ref(f.field_type.trim_end_matches('!'), schema);
            let input_type = if f.nullable {
                named
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
                    of_type:            Some(Box::new(named)),
                    specified_by_u_r_l: None,
                }
            };

            IntrospectionInputValue {
                name: f.name.clone(),
                description: f.description.clone(),
                input_type,
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
