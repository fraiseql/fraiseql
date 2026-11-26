//! A `node(id:)` value is completed as its resolved type: a field the client selected only
//! inside a type fragment is held to its type and format, as through the type's own root,
//! whether or not the client selected `__typename`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
    backend::types::JsonbValue,
    runtime::{
        Executor, RuntimeConfig,
        executor::{ResponseBody, test_support::MockAdapter},
    },
    schema::{
        CompiledSchema, CursorType, FieldDefinition, FieldType, InterfaceDefinition,
        QueryDefinition, TypeDefinition,
    },
    security::SecurityContext,
};

const ID: &str = "00000000-0000-0000-0000-000000000001";

/// `Kind`, a Node type as the compiler writes one (implementing the declared `Node`):
/// `at: DateTime`, `n: Int`, and `child: Kind`, read by `kinds` and through its connection.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema
        .queries
        .push(QueryDefinition::new("kinds", "Kind").returning_list().with_sql_source("v_kind"));
    let mut connection = QueryDefinition::new("kindsConnection", "Kind")
        .returning_list()
        .with_sql_source("v_kind");
    connection.relay = true;
    connection.relay_cursor_column = Some("pk_kind".to_string());
    connection.relay_cursor_type = CursorType::Int64;
    schema.queries.push(connection);
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("at", FieldType::DateTime),
            FieldDefinition::nullable("n", FieldType::Int),
            FieldDefinition::nullable("child", FieldType::Object("Kind".to_string())),
        ],
        relay: true,
        implements: vec!["Node".to_string()],
        ..TypeDefinition::new("Kind", "v_kind")
    });
    schema.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema.build_indexes();
    schema
}

/// A row holding a `DateTime` with no offset and an `Int` that is a string, at the root and
/// in its `child`.
fn executor(at: &str, n: &Value) -> Executor {
    let row = json!({"id": ID, "at": at, "n": n, "child": {"id": ID, "at": at, "n": n}});
    let adapter = Arc::new(MockAdapter::new(vec![JsonbValue::new(row)]));
    Executor::with_config_and_relay(schema(), adapter, RuntimeConfig::default())
}

fn out_of_format() -> Executor {
    executor("2024-01-15T10:00:00", &json!("seven"))
}

fn typed_id() -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(format!("Kind:{ID}"))
}

/// The `errors` messages, in order.
fn messages(response: &Value) -> Vec<String> {
    response["errors"].as_array().map_or_default(|errors| {
        errors.iter().map(|e| e["message"].as_str().unwrap().to_string()).collect()
    })
}

fn paths(response: &Value) -> Vec<Value> {
    response["errors"]
        .as_array()
        .map_or_default(|errors| errors.iter().map(|e| e["path"].clone()).collect())
}

/// The field errors the type's own root gives the same row.
async fn through_the_root() -> Vec<String> {
    let root = out_of_format().execute("{ kinds { at n } }", None).await.unwrap();
    let expected = messages(&root);
    assert_eq!(expected.len(), 2, "the root refuses both fields: {root}");
    expected
}

#[tokio::test]
async fn a_node_fragments_fields_are_completed_as_through_the_types_own_root() {
    let expected = through_the_root().await;
    let id = typed_id();
    for document in [
        format!(r#"{{ node(id: "{id}") {{ ... on Kind {{ at n }} }} }}"#),
        format!(r#"{{ node(id: "{id}") {{ __typename ... on Kind {{ at n }} }} }}"#),
        format!(r#"{{ node(id: "{id}") {{ ...K }} }} fragment K on Kind {{ at n }}"#),
        format!(r#"{{ node(id: "{ID}") {{ ... on Kind {{ at n }} }} }}"#),
    ] {
        let response = out_of_format().execute(&document, None).await.unwrap();
        assert_eq!(messages(&response), expected, "{document}: {response}");
        assert_eq!(paths(&response), [json!(["node", "at"]), json!(["node", "n"])], "{document}");
        assert_eq!(response["data"]["node"]["at"], Value::Null, "{document}: {response}");
        assert_eq!(response["data"]["node"]["n"], Value::Null, "{document}: {response}");
    }
}

#[tokio::test]
async fn an_object_under_a_node_fragment_is_completed() {
    let expected = through_the_root().await;
    let document =
        format!(r#"{{ node(id: "{}") {{ ... on Kind {{ child {{ at n }} }} }} }}"#, typed_id());
    let response = out_of_format().execute(&document, None).await.unwrap();
    assert_eq!(messages(&response), expected, "{response}");
    assert_eq!(
        paths(&response),
        [
            json!(["node", "child", "at"]),
            json!(["node", "child", "n"])
        ],
        "{response}"
    );
}

/// The resolved type steers completion only: a `__typename` the client did not select is
/// not served, and a value in its format is served unchanged.
#[tokio::test]
async fn a_node_value_in_its_format_is_served_as_selected() {
    let document = format!(r#"{{ node(id: "{}") {{ ... on Kind {{ at n }} }} }}"#, typed_id());
    let response = executor("2024-01-15T10:00:00Z", &json!(7))
        .execute(&document, None)
        .await
        .unwrap();
    assert_eq!(response, json!({"data": {"node": {"at": "2024-01-15T10:00:00Z", "n": 7}}}));
}

/// A node read is never answered as the database's bytes: it is always completed.
#[tokio::test]
async fn a_node_read_is_answered_completed_through_the_body_entry() {
    let document = format!(r#"{{ node(id: "{}") {{ ... on Kind {{ at n }} }} }}"#, typed_id());
    let context = SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::default(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       fraiseql_time::Timestamp::now()
            + fraiseql_time::jiff::SignedDuration::from_hours(1),
        authenticated_at: fraiseql_time::Timestamp::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    };
    let body = out_of_format()
        .execute_operation_with_security_body(&document, None, &context, None)
        .await
        .unwrap();
    assert!(matches!(body, ResponseBody::Value(_)), "a node read was answered raw");
    let ResponseBody::Value(response) = body else {
        return;
    };
    assert_eq!(messages(&response).len(), 2, "{response}");
}
