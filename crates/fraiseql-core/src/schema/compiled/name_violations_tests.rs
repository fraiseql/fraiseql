//! A compiled schema holds only GraphQL Names (§ 2.1.9), checked where every artifact loads.
//!
//! Each test renames one declaration of a schema that loads, so a refusal names that
//! declaration and nothing else.

use serde_json::{Value, json};

use super::schema::CompiledSchema;

/// A schema declaring one name of every kind the check walks.
fn every_kind() -> Value {
    json!({
      "types": [
        {"name": "Order", "sql_source": "v_order",
         "fields": [{"name": "id", "field_type": "Int", "nullable": false},
                    {"name": "total_cents", "field_type": "Int", "nullable": false,
                     "alias": "totalCents"}],
         "implements": ["Node"]}
      ],
      "interfaces": [
        {"name": "Node", "fields": [{"name": "id", "field_type": "Int", "nullable": false}]}
      ],
      "unions": [{"name": "Found", "member_types": ["Order"]}],
      "enums": [{"name": "OrderStatus", "values": [{"name": "OPEN"}, {"name": "_CLOSED"}]}],
      "input_types": [
        {"name": "OrderNote", "fields": [{"name": "text", "field_type": "String"}]}
      ],
      "queries": [
        {"name": "orders", "return_type": "Order", "returns_list": true,
         "sql_source": "v_order",
         "arguments": [{"name": "status", "arg_type": "String", "nullable": true}]}
      ],
      "mutations": [
        {"name": "createOrder", "return_type": "Order",
         "operation": {"Insert": {"table": "tb_order"}},
         "arguments": [{"name": "note", "arg_type": "String", "nullable": true}]}
      ],
      "subscriptions": [
        {"name": "orderPlaced", "return_type": "Order",
         "arguments": [{"name": "region", "arg_type": "String", "nullable": true}]}
      ],
      "directives": [
        {"name": "audited", "locations": ["FIELD_DEFINITION"],
         "arguments": [{"name": "reason", "arg_type": "String", "nullable": true}]}
      ]
    })
}

fn refusal_for(pointer: &str, name: &str) -> String {
    let mut doc = every_kind();
    *doc.pointer_mut(pointer).expect("the pointer is in the fixture") =
        Value::String(name.to_string());
    CompiledSchema::from_json(&doc.to_string(), false)
        .expect_err("a name that is not a GraphQL Name must not load")
        .to_string()
}

/// The positive control: without it, every refusal below would pass against a check that
/// refused every schema. It also loads a leading single `_`, which § 2.1.9 allows.
#[test]
fn a_schema_of_graphql_names_loads() {
    CompiledSchema::from_json(&every_kind().to_string(), false)
        .expect("every name in the fixture is a GraphQL Name");
}

#[test]
fn the_issue_schema_is_refused_naming_the_type_and_the_argument() {
    let doc = json!({
      "types": [
        {"name": "Quote'd'); DROP TABLE x; --", "sql_source": "v_user",
         "fields": [{"name": "id", "field_type": "ID", "nullable": false},
                    {"name": "name", "field_type": "String", "nullable": false}]}
      ],
      "queries": [
        {"name": "users", "return_type": "Quote'd'); DROP TABLE x; --", "returns_list": true,
         "sql_source": "v_user",
         "arguments": [{"name": "a'b", "arg_type": "String", "nullable": true}]}
      ]
    });
    let message = CompiledSchema::from_json(&doc.to_string(), false)
        .expect_err("the issue's schema must not load")
        .to_string();
    assert!(message.contains("type `Quote'd'); DROP TABLE x; --`"), "{message}");
    assert!(message.contains("argument `users(a'b:)`"), "{message}");
}

#[test]
fn a_type_name_is_checked() {
    let message = refusal_for("/types/0/name", "Or-der");
    assert!(message.contains("type `Or-der` is not a GraphQL name"), "{message}");
}

/// The issue's case: a field that is not a sort key, so the sort-key check never sees it.
#[test]
fn a_field_key_is_checked() {
    let message = refusal_for("/types/0/fields/1/name", "total cents'");
    assert!(
        message.contains("field `Order.total cents'` is not a GraphQL name"),
        "{message}"
    );
}

/// A field is published under its alias, so the alias is a name a client types too.
#[test]
fn a_field_alias_is_checked() {
    let message = refusal_for("/types/0/fields/1/alias", "total-cents");
    assert!(
        message.contains(
            "field `Order.total_cents` is published as `total-cents`, which is not a GraphQL name"
        ),
        "{message}"
    );
}

#[test]
fn an_interface_name_is_checked() {
    let message = refusal_for("/interfaces/0/name", "No de");
    assert!(message.contains("interface `No de` is not a GraphQL name"), "{message}");
}

#[test]
fn an_interface_field_is_checked() {
    let message = refusal_for("/interfaces/0/fields/0/name", "1d");
    assert!(message.contains("field `Node.1d` is not a GraphQL name"), "{message}");
}

#[test]
fn a_union_name_is_checked() {
    let message = refusal_for("/unions/0/name", "Fo|und");
    assert!(message.contains("union `Fo|und` is not a GraphQL name"), "{message}");
}

#[test]
fn an_enum_name_is_checked() {
    let message = refusal_for("/enums/0/name", "Order.Status");
    assert!(message.contains("enum `Order.Status` is not a GraphQL name"), "{message}");
}

#[test]
fn an_enum_value_is_checked() {
    let message = refusal_for("/enums/0/values/0/name", "OP EN");
    assert!(
        message.contains("enum value `OrderStatus.OP EN` is not a GraphQL name"),
        "{message}"
    );
}

/// § 3.9: an enum value is a Name but not `true`, `false` or `null`, which a document
/// would read as a Boolean or null literal.
#[test]
fn an_enum_value_may_not_be_a_literal() {
    let message = refusal_for("/enums/0/values/0/name", "true");
    assert!(
        message.contains("enum value `OrderStatus.true` is `true`, `false` or `null`"),
        "{message}"
    );
}

#[test]
fn an_input_type_name_is_checked() {
    let message = refusal_for("/input_types/0/name", "Order Note");
    assert!(message.contains("input type `Order Note` is not a GraphQL name"), "{message}");
}

#[test]
fn an_input_field_is_checked() {
    let message = refusal_for("/input_types/0/fields/0/name", "te'xt");
    assert!(
        message.contains("input field `OrderNote.te'xt` is not a GraphQL name"),
        "{message}"
    );
}

#[test]
fn a_query_name_is_checked() {
    let message = refusal_for("/queries/0/name", "or ders");
    assert!(message.contains("query `or ders` is not a GraphQL name"), "{message}");
}

#[test]
fn a_query_argument_is_checked() {
    let message = refusal_for("/queries/0/arguments/0/name", "sta-tus");
    assert!(
        message.contains("argument `orders(sta-tus:)` is not a GraphQL name"),
        "{message}"
    );
}

#[test]
fn a_mutation_name_is_checked() {
    let message = refusal_for("/mutations/0/name", "create;Order");
    assert!(message.contains("mutation `create;Order` is not a GraphQL name"), "{message}");
}

#[test]
fn a_mutation_argument_is_checked() {
    let message = refusal_for("/mutations/0/arguments/0/name", "no'te");
    assert!(
        message.contains("argument `createOrder(no'te:)` is not a GraphQL name"),
        "{message}"
    );
}

#[test]
fn a_subscription_name_is_checked() {
    let message = refusal_for("/subscriptions/0/name", "order placed");
    assert!(
        message.contains("subscription `order placed` is not a GraphQL name"),
        "{message}"
    );
}

#[test]
fn a_subscription_argument_is_checked() {
    let message = refusal_for("/subscriptions/0/arguments/0/name", "re gion");
    assert!(
        message.contains("argument `orderPlaced(re gion:)` is not a GraphQL name"),
        "{message}"
    );
}

#[test]
fn a_directive_name_is_checked() {
    let message = refusal_for("/directives/0/name", "audit ed");
    assert!(message.contains("directive `@audit ed` is not a GraphQL name"), "{message}");
}

#[test]
fn a_directive_argument_is_checked() {
    let message = refusal_for("/directives/0/arguments/0/name", "rea son");
    assert!(
        message.contains("argument `@audited(rea son:)` is not a GraphQL name"),
        "{message}"
    );
}

/// An operation is published under the naming convention, so the published name is the
/// one checked too: `_2fa_codes` is a Name, and under camelCase it publishes as `2faCodes`.
#[test]
fn an_operation_is_checked_under_the_name_it_is_published_as() {
    let mut doc = every_kind();
    doc["naming_convention"] = json!("camelCase");
    doc["queries"][0]["name"] = json!("_2fa_codes");
    let message = CompiledSchema::from_json(&doc.to_string(), false)
        .expect_err("a query published as a name that is not a Name must not load")
        .to_string();
    assert!(
        message
            .contains("query `_2fa_codes` is published as `2faCodes`, which is not a GraphQL name"),
        "{message}"
    );
}

/// § 3.1: a name starting with `__` is introspection's.
#[test]
fn a_name_starting_with_two_underscores_is_refused() {
    let message = refusal_for("/types/0/fields/0/name", "__id");
    assert!(
        message.contains("field `Order.__id` starts with `__`, which introspection reserves"),
        "{message}"
    );
}

/// Every offender is listed in one error, so an author fixes them in one pass.
#[test]
fn every_offending_name_is_listed_in_one_error() {
    let mut doc = every_kind();
    doc["types"][0]["name"] = json!("Or-der");
    doc["enums"][0]["values"][1]["name"] = json!("null");
    doc["directives"][0]["arguments"][0]["name"] = json!("__reason");
    let message = CompiledSchema::from_json(&doc.to_string(), false)
        .expect_err("three bad names must not load")
        .to_string();
    for expected in [
        "type `Or-der`",
        "enum value `OrderStatus.null`",
        "argument `@audited(__reason:)`",
    ] {
        assert!(message.contains(expected), "{expected} missing from: {message}");
    }
}

/// A sorting query lists its sort keys as values of a derived enum, built before the names
/// are checked. A native column's key is not a declared name, so the derived enum is where
/// one that cannot be an enum value is refused.
fn sort_key_refusal(edit: impl FnOnce(&mut Value)) -> String {
    let mut doc = every_kind();
    doc["queries"][0]["auto_params"] = json!({"has_order_by": true});
    edit(&mut doc);
    CompiledSchema::from_json(&doc.to_string(), false)
        .expect_err("a sort key that cannot be an enum value must not load")
        .to_string()
}

#[test]
fn a_native_column_sort_key_that_is_not_a_name_is_refused() {
    let message = sort_key_refusal(|doc| {
        doc["queries"][0]["native_columns"] =
            json!({"placed-at": {"pg_type": "timestamptz", "not_null": false}});
    });
    assert!(
        message.contains("enum value `OrderOrderByField.placed-at` is not a GraphQL name"),
        "{message}"
    );
}

/// `null` is a Name, so a field may be called it; as a sort key it would be an enum value,
/// which it cannot be (§ 3.9).
#[test]
fn a_field_sort_key_that_is_an_enum_literal_is_refused() {
    let message = sort_key_refusal(|doc| doc["types"][0]["fields"][1]["name"] = json!("null"));
    assert!(
        message.contains("enum value `OrderOrderByField.null` is `true`, `false` or `null`"),
        "{message}"
    );
}
