//! The generated SDL carries `@deprecated` wherever introspection reports a deprecation.
//!
//! The SDL is what a federation gateway composes: a member that introspection reports
//! `isDeprecated: true` and the SDL prints without the directive is served by the supergraph
//! as live, and its clients lose the deprecation and its reason. The comparison walks the
//! introspection model, so a member kind it gains later is held to the same rule.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::collections::BTreeMap;

use apollo_parser::{Parser, cst};

use super::CompiledSchema;
use crate::schema::{
    ArgumentDefinition, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldType,
    InputFieldDefinition, InputObjectDefinition, InterfaceDefinition, IntrospectionBuilder,
    MutationDefinition, QueryDefinition, TypeDefinition,
};

/// A reason holding everything a careless printer or line scanner trips on: a quote, a
/// backslash, an unbalanced brace, a ` #` and a newline.
const AWKWARD: &str = "Use \"label\" \\ {id #1\nsee the guide";

/// Every deprecated member of [`schema`], by path, with its reason.
fn declared() -> BTreeMap<String, Option<String>> {
    [
        ("Status.LEGACY", Some("Use ACTIVE.")),
        ("Status.GONE", None),
        ("Node.legacyId", Some("Use id.")),
        ("ItemInput.oldName", Some("Use name.")),
        ("Item.oldLabel", Some(AWKWARD)),
        ("Item.bare", None),
        ("Query.items(oldStatus)", Some("Use status.")),
        ("Query.oldItems", Some("Use items.")),
        ("Mutation.oldCreateItem", Some("Use createItem.")),
    ]
    .into_iter()
    .map(|(path, reason)| (path.to_string(), reason.map(String::from)))
    .collect()
}

/// A schema with a deprecated member of each kind the SDL prints, and live ones beside them.
fn schema() -> CompiledSchema {
    let reason = |text: &str| Some(text.to_string());
    let mut schema = CompiledSchema::new();

    schema.enums.push(EnumDefinition::new("Status").with_values(vec![
        EnumValueDefinition::new("ACTIVE"),
        EnumValueDefinition::new("LEGACY").deprecated(reason("Use ACTIVE.")),
        EnumValueDefinition::new("GONE").deprecated(None),
    ]));
    schema.interfaces.push(InterfaceDefinition::new("Node").with_fields(vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("legacyId", FieldType::String).deprecated(reason("Use id.")),
    ]));
    schema
        .input_types
        .push(InputObjectDefinition::new("ItemInput").with_fields(vec![
            InputFieldDefinition::new("name", "String"),
            InputFieldDefinition::new("oldName", "String").deprecated(reason("Use name.")),
        ]));
    schema.types.push(
        TypeDefinition::new("Item", "v_item")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("label", FieldType::String))
            .with_field(
                FieldDefinition::new("oldLabel", FieldType::String).deprecated(reason(AWKWARD)),
            )
            .with_field(FieldDefinition::new("bare", FieldType::String).deprecated(None)),
    );
    // Declared after `Item`: what a scanner thrown off by `Item`'s reason gets wrong.
    schema.types.push(
        TypeDefinition::new("Other", "v_other")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );

    let mut items =
        QueryDefinition::new("items", "Item").returning_list().with_sql_source("v_item");
    items.arguments = vec![
        ArgumentDefinition::optional("status", FieldType::Enum("Status".to_string())),
        ArgumentDefinition::optional("oldStatus", FieldType::Enum("Status".to_string()))
            .deprecated(reason("Use status.")),
    ];
    schema.queries.push(items);
    // A deprecated field that takes an argument: the directive follows the return type.
    let mut old_items = QueryDefinition::new("oldItems", "Item")
        .returning_list()
        .with_sql_source("v_item")
        .deprecated(reason("Use items."));
    old_items.arguments = vec![ArgumentDefinition::optional(
        "status",
        FieldType::Enum("Status".to_string()),
    )];
    schema.queries.push(old_items);
    schema.mutations.push(MutationDefinition::new("createItem", "Item"));
    schema.mutations.push(
        MutationDefinition::new("oldCreateItem", "Item").deprecated(reason("Use createItem.")),
    );

    schema.build_indexes();
    schema
}

/// The `@deprecated` among `directives`, if it is there.
fn deprecated(directives: Option<cst::Directives>) -> Option<cst::Directive> {
    directives?
        .directives()
        .find(|d| d.name().is_some_and(|n| n.text() == "deprecated"))
}

/// The reason a `@deprecated` gives, as a GraphQL parser reads the string back.
fn reason_of(directive: &cst::Directive) -> Option<String> {
    directive
        .arguments()
        .into_iter()
        .flat_map(|a| a.arguments())
        .find(|a| a.name().is_some_and(|n| n.text() == "reason"))
        .and_then(|a| a.value())
        .map(|value| match value {
            cst::Value::StringValue(text) => String::from(text),
            other => panic!("a reason is a string, not {other:?}"),
        })
}

/// Every member `sdl` marks `@deprecated`, by path, as a GraphQL parser reads it back.
fn sdl_deprecations(sdl: &str) -> BTreeMap<String, Option<String>> {
    let tree = Parser::new(sdl).parse();
    let errors: Vec<String> = tree.errors().map(|e| e.message().to_string()).collect();
    assert!(errors.is_empty(), "the SDL parses: {errors:?}\n{sdl}");

    let mut found = BTreeMap::new();
    let mut fields = |owner: Option<cst::Name>, fields: Option<cst::FieldsDefinition>| {
        let owner = owner.unwrap().text().to_string();
        for field in fields.into_iter().flat_map(|f| f.field_definitions()) {
            let name = field.name().unwrap().text().to_string();
            if let Some(directive) = deprecated(field.directives()) {
                let reason = reason_of(&directive);
                found.insert(format!("{owner}.{name}"), reason);
            }
            let arguments = field
                .arguments_definition()
                .into_iter()
                .flat_map(|a| a.input_value_definitions());
            for argument in arguments {
                if let Some(directive) = deprecated(argument.directives()) {
                    let reason = reason_of(&directive);
                    let argument = argument.name().unwrap().text().to_string();
                    found.insert(format!("{owner}.{name}({argument})"), reason);
                }
            }
        }
    };
    let mut values = BTreeMap::new();
    for definition in tree.document().definitions() {
        match definition {
            cst::Definition::ObjectTypeDefinition(t) => fields(t.name(), t.fields_definition()),
            cst::Definition::ObjectTypeExtension(t) => fields(t.name(), t.fields_definition()),
            cst::Definition::InterfaceTypeDefinition(t) => fields(t.name(), t.fields_definition()),
            cst::Definition::InputObjectTypeDefinition(t) => {
                let owner = t.name().unwrap().text().to_string();
                let inputs = t
                    .input_fields_definition()
                    .into_iter()
                    .flat_map(|f| f.input_value_definitions());
                for input in inputs {
                    if let Some(directive) = deprecated(input.directives()) {
                        let reason = reason_of(&directive);
                        let name = input.name().unwrap().text().to_string();
                        values.insert(format!("{owner}.{name}"), reason);
                    }
                }
            },
            cst::Definition::EnumTypeDefinition(t) => {
                let owner = t.name().unwrap().text().to_string();
                let members =
                    t.enum_values_definition().into_iter().flat_map(|v| v.enum_value_definitions());
                for member in members {
                    if let Some(directive) = deprecated(member.directives()) {
                        let reason = reason_of(&directive);
                        let name = member.enum_value().unwrap().text().to_string();
                        values.insert(format!("{owner}.{name}"), reason);
                    }
                }
            },
            _ => {},
        }
    }
    found.extend(values);
    found
}

/// Every member introspection reports deprecated, by the same paths.
fn introspected_deprecations(schema: &CompiledSchema) -> BTreeMap<String, Option<String>> {
    let mut found = BTreeMap::new();
    for published in IntrospectionBuilder::build(schema).types {
        let Some(owner) = published.name.filter(|name| !name.starts_with("__")) else {
            continue;
        };
        for field in published.fields.into_iter().flatten() {
            if field.is_deprecated {
                found.insert(format!("{owner}.{}", field.name), field.deprecation_reason);
            }
            for argument in field.args {
                if argument.is_deprecated {
                    found.insert(
                        format!("{owner}.{}({})", field.name, argument.name),
                        argument.deprecation_reason,
                    );
                }
            }
        }
        for input in published.input_fields.into_iter().flatten() {
            if input.is_deprecated {
                found.insert(format!("{owner}.{}", input.name), input.deprecation_reason);
            }
        }
        for value in published.enum_values.into_iter().flatten() {
            if value.is_deprecated {
                found.insert(format!("{owner}.{}", value.name), value.deprecation_reason);
            }
        }
    }
    found
}

/// The fixture is what the comparison below believes it is: introspection reports exactly
/// the members declared deprecated, one of each kind.
#[test]
fn introspection_reports_each_declared_deprecation() {
    assert_eq!(introspected_deprecations(&schema()), declared());
}

#[test]
fn the_sdl_marks_every_member_introspection_reports_deprecated_and_no_other() {
    let schema = schema();

    assert_eq!(sdl_deprecations(&schema.raw_schema()), introspected_deprecations(&schema));
}

/// One assertion per member kind, so a kind the printer drops is named.
#[test]
fn each_kind_of_member_carries_its_deprecation() {
    let marked = sdl_deprecations(&schema().raw_schema());

    for (path, reason) in declared() {
        assert_eq!(marked.get(&path), Some(&reason), "`{path}` in {marked:?}");
    }
}

/// A reason is printed as a GraphQL string: a parser reads back the text that was declared.
#[test]
fn a_reason_reads_back_as_declared() {
    let marked = sdl_deprecations(&schema().raw_schema());

    assert_eq!(marked.get("Item.oldLabel"), Some(&Some(AWKWARD.to_string())));
}

/// On a field with arguments the directive is the field's, after its return type, and an
/// argument's own is inside the parentheses.
#[test]
fn a_field_with_arguments_carries_its_deprecation_after_its_type() {
    let sdl = schema().raw_schema();
    let line = |name: &str| {
        sdl.lines()
            .map(str::trim)
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("no `{name}` in:\n{sdl}"))
    };

    assert_eq!(
        line("oldItems("),
        r#"oldItems(status: Status): [Item!]! @deprecated(reason: "Use items.")"#
    );
    assert_eq!(
        line("items("),
        r#"items(status: Status, oldStatus: Status @deprecated(reason: "Use status.")): [Item!]!"#
    );
}

/// A deprecation with no reason is the bare directive, not an empty or invented reason.
#[test]
fn a_deprecation_without_a_reason_is_the_bare_directive() {
    let sdl = schema().raw_schema();

    assert!(sdl.lines().any(|line| line.trim() == "bare: String! @deprecated"), "{sdl}");
    assert!(sdl.lines().any(|line| line.trim() == "GONE @deprecated"), "{sdl}");
}

#[cfg(feature = "federation")]
mod service_sdl {
    use super::*;
    use crate::schema::{FederationConfig, FederationEntity};

    /// [`schema`] as a subgraph: `Item` and `Other` are entities, and `Item.oldLabel`, the
    /// field with the awkward reason, also takes a federation directive.
    fn subgraph() -> CompiledSchema {
        let mut schema = schema();
        schema.federation = Some(FederationConfig {
            enabled: true,
            version: Some("v2".to_string()),
            service_name: Some("items".to_string()),
            entities: vec![
                FederationEntity {
                    name: "Item".to_string(),
                    key_fields: vec!["id".to_string()],
                    shareable_fields: vec!["oldLabel".to_string()],
                    ..Default::default()
                },
                FederationEntity {
                    name: "Other".to_string(),
                    key_fields: vec!["id".to_string()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        schema.build_indexes();
        schema
    }

    #[test]
    fn the_service_sdl_marks_every_member_introspection_reports_deprecated_and_no_other() {
        let schema = subgraph();
        let sdl = schema.federation_service_sdl().unwrap();

        assert_eq!(sdl_deprecations(&sdl), introspected_deprecations(&schema));
    }

    /// The federation directives are added line by line over the generated SDL. A reason is
    /// free text: its braces and `#` are inside a string, and change nothing around it.
    #[test]
    fn a_reason_leaves_the_federation_directives_where_they_belong() {
        let sdl = subgraph().federation_service_sdl().unwrap();
        let marked = sdl_deprecations(&sdl);

        // The reason is intact: no directive was spliced into it.
        assert_eq!(marked.get("Item.oldLabel"), Some(&Some(AWKWARD.to_string())), "{sdl}");
        // Its field has the federation directive it was given, after the reason.
        let field = sdl.lines().find(|line| line.trim_start().starts_with("oldLabel:")).unwrap();
        assert!(field.trim_end().ends_with(") @shareable"), "{field}");
        // And the type declared after it is still read as a type: it has its key.
        assert!(sdl.contains("type Other @key(fields: \"id\") {"), "{sdl}");
    }
}
