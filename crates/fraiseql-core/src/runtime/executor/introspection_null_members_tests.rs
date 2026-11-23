//! A selected introspection member is always in the response: `null` where it does not
//! apply, a list where the spec says it is one (GraphQL § 4.5, `__Type`; § 7, Response).
//!
//! Every case is a query through [`Executor::execute`], the path a client takes.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use serde_json::Value;

use super::{Executor, test_support::*};
use crate::schema::{
    CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldType,
    InputFieldDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition,
    UnionDefinition,
};

/// Every `__Type` member a client can select, as its response key.
const TYPE_MEMBERS: [&str; 10] = [
    "kind",
    "name",
    "description",
    "fields",
    "interfaces",
    "possibleTypes",
    "enumValues",
    "inputFields",
    "ofType",
    "specifiedByURL",
];

const ALL_TYPE_MEMBERS: &str = "kind name description fields { name } interfaces { name } \
                                possibleTypes { name } enumValues { name } inputFields { name } \
                                ofType { name } specifiedByURL";

/// `test_schema()` with a type of every kind: the `User` object its query returns (one
/// nullable field, one non-null), an enum, an input object, an interface no type
/// implements, and a union.
fn schema_of_every_kind() -> CompiledSchema {
    let mut schema = test_schema();
    let mut user = TypeDefinition::new("User", "v_user");
    user.fields.push(FieldDefinition::new("name", FieldType::String));
    let mut id = FieldDefinition::new("id", FieldType::Id);
    id.nullable = false;
    user.fields.push(id);
    schema.types.push(user);
    schema
        .enums
        .push(EnumDefinition::new("Status").with_value(EnumValueDefinition::new("ACTIVE")));
    schema.input_types.push(
        InputObjectDefinition::new("Filter").with_field(InputFieldDefinition::new("q", "String")),
    );
    schema.interfaces.push(
        InterfaceDefinition::new("Named")
            .with_field(FieldDefinition::new("name", FieldType::String)),
    );
    schema.unions.push(UnionDefinition::new("Found").with_member("User"));
    schema
}

async fn data(query: &str) -> Value {
    let executor = Executor::new(schema_of_every_kind(), Arc::new(MockAdapter::new(vec![])));
    let response = executor.execute(query, None).await.unwrap();
    assert!(response.get("errors").is_none(), "{response}");
    response["data"].clone()
}

/// The `__Type` named `name` in `types`.
fn type_named<'a>(types: &'a [Value], name: &str) -> &'a serde_json::Map<String, Value> {
    types
        .iter()
        .find(|t| t["name"] == name)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("no type `{name}` in introspection"))
}

fn assert_null(object: &serde_json::Map<String, Value>, key: &str) {
    assert_eq!(
        object.get(key),
        Some(&Value::Null),
        "`{key}` must be present and null: {object:?}"
    );
}

fn assert_list(object: &serde_json::Map<String, Value>, key: &str) -> usize {
    match object.get(key) {
        Some(Value::Array(items)) => items.len(),
        other => panic!("`{key}` must be a list, got {other:?} in {object:?}"),
    }
}

#[tokio::test]
async fn every_type_carries_every_selected_member() {
    let data = data(&format!("{{ __schema {{ types {{ {ALL_TYPE_MEMBERS} }} }} }}")).await;
    let types = data["__schema"]["types"].as_array().unwrap();
    for t in types {
        let object = t.as_object().unwrap();
        for member in TYPE_MEMBERS {
            assert!(object.contains_key(member), "`{member}` missing from {object:?}");
        }
    }
}

#[tokio::test]
async fn each_kind_answers_null_for_the_members_it_does_not_have() {
    let data = data(&format!("{{ __schema {{ types {{ {ALL_TYPE_MEMBERS} }} }} }}")).await;
    let types = data["__schema"]["types"].as_array().unwrap();

    let scalar = type_named(types, "String");
    for member in [
        "fields",
        "interfaces",
        "possibleTypes",
        "enumValues",
        "inputFields",
        "ofType",
    ] {
        assert_null(scalar, member);
    }

    let object = type_named(types, "User");
    assert!(assert_list(object, "fields") > 0);
    assert_list(object, "interfaces");
    for member in [
        "possibleTypes",
        "enumValues",
        "inputFields",
        "ofType",
        "specifiedByURL",
    ] {
        assert_null(object, member);
    }

    let enumeration = type_named(types, "Status");
    assert_eq!(assert_list(enumeration, "enumValues"), 1);
    for member in [
        "fields",
        "interfaces",
        "possibleTypes",
        "inputFields",
        "ofType",
    ] {
        assert_null(enumeration, member);
    }

    let input = type_named(types, "Filter");
    assert_eq!(assert_list(input, "inputFields"), 1);
    for member in [
        "fields",
        "interfaces",
        "possibleTypes",
        "enumValues",
        "ofType",
    ] {
        assert_null(input, member);
    }

    let union = type_named(types, "Found");
    assert_eq!(assert_list(union, "possibleTypes"), 1);
    for member in [
        "fields",
        "interfaces",
        "enumValues",
        "inputFields",
        "ofType",
    ] {
        assert_null(union, member);
    }
}

/// An interface's `interfaces` and `possibleTypes` are lists even when empty: an interface
/// may implement interfaces, and its possible types are a set. `null` is for other kinds.
#[tokio::test]
async fn an_interface_nothing_implements_answers_empty_lists() {
    let data = data(&format!("{{ __schema {{ types {{ {ALL_TYPE_MEMBERS} }} }} }}")).await;
    let types = data["__schema"]["types"].as_array().unwrap();

    let interface = type_named(types, "Named");
    assert_eq!(assert_list(interface, "fields"), 1);
    assert_eq!(assert_list(interface, "interfaces"), 0);
    assert_eq!(assert_list(interface, "possibleTypes"), 0);
    for member in ["enumValues", "inputFields", "ofType"] {
        assert_null(interface, member);
    }
}

#[tokio::test]
async fn a_wrapper_type_answers_null_for_its_name_and_a_named_type_for_its_of_type() {
    let data = data(
        "{ __type(name: \"User\") { fields { type { kind name ofType { kind name ofType { name } } } } } }",
    )
    .await;
    let fields = data["__type"]["fields"].as_array().unwrap();
    let wrapped = fields
        .iter()
        .map(|f| f["type"].as_object().unwrap())
        .find(|t| t["kind"] != "SCALAR" && t["kind"] != "OBJECT" && t["kind"] != "ENUM")
        .expect("the fixture's `User` has a non-null or list field");
    assert_null(wrapped, "name");
    let named = wrapped["ofType"].as_object().unwrap();
    let innermost = if named["kind"] == "LIST" || named["kind"] == "NON_NULL" {
        named["ofType"].as_object().unwrap()
    } else {
        named
    };
    assert_null(innermost, "ofType");
}

#[tokio::test]
async fn a_field_enum_value_and_input_value_answer_null_descriptions_and_reasons() {
    let data = data(
        "{ user: __type(name: \"User\") { fields { name description deprecationReason } } \
           status: __type(name: \"Status\") { enumValues { name description deprecationReason } } \
           filter: __type(name: \"Filter\") { inputFields { name description defaultValue deprecationReason } } }",
    )
    .await;

    let field = data["user"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "name")
        .and_then(Value::as_object)
        .unwrap();
    assert_null(field, "description");
    assert_null(field, "deprecationReason");

    let value = data["status"]["enumValues"][0].as_object().unwrap();
    assert_null(value, "description");
    assert_null(value, "deprecationReason");

    let input = data["filter"]["inputFields"][0].as_object().unwrap();
    assert_null(input, "description");
    assert_null(input, "defaultValue");
    assert_null(input, "deprecationReason");
}

#[tokio::test]
async fn the_schema_answers_null_for_the_root_types_it_does_not_have() {
    let data =
        data("{ __schema { description mutationType { name } subscriptionType { name } } }").await;
    let schema = data["__schema"].as_object().unwrap();
    assert!(schema.contains_key("description"), "{schema:?}");
    assert_null(schema, "mutationType");
    assert_null(schema, "subscriptionType");
}
