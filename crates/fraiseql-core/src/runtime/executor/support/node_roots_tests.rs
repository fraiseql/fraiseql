//! A `node(id:)` root is answered under its response key, and beside other roots it is one
//! root among them: planned with them, after them, before anything is read, then read with
//! them, its value completed as the type its id names.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
    backend::types::JsonbValue,
    runtime::{Executor, RuntimeConfig, executor::test_support::MockAdapter},
    schema::{
        CompiledSchema, CursorType, FieldDefinition, FieldType, InterfaceDefinition,
        QueryDefinition, TypeDefinition,
    },
    security::SecurityContext,
};

const ID: &str = "00000000-0000-0000-0000-000000000001";

/// `Kind`, a Node type as the compiler writes one (`at: DateTime`, `n: Int`), read by
/// `kinds` and its connection; `Plain`, no Node, read by `plains` and, for `admin` only,
/// by `audits`.
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
    schema.queries.push(
        QueryDefinition::new("plains", "Plain")
            .returning_list()
            .with_sql_source("v_plain"),
    );
    let mut audits = QueryDefinition::new("audits", "Plain")
        .returning_list()
        .with_sql_source("v_plain");
    audits.requires_role = Some("admin".to_string());
    schema.queries.push(audits);
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("name", FieldType::String),
            FieldDefinition::nullable("at", FieldType::DateTime),
            FieldDefinition::nullable("n", FieldType::Int),
        ],
        relay: true,
        implements: vec!["Node".to_string()],
        ..TypeDefinition::new("Kind", "v_kind")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("name", FieldType::String),
        ],
        ..TypeDefinition::new("Plain", "v_plain")
    });
    schema.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema.build_indexes();
    schema
}

/// Every view answers this one row: `at` and `n` in their formats, or, `out_of_format`,
/// a `DateTime` with no offset and an `Int` that is a string.
fn adapter(out_of_format: bool) -> Arc<MockAdapter> {
    let (at, n) = if out_of_format {
        (json!("2024-01-15T10:00:00"), json!("seven"))
    } else {
        (json!("2024-01-15T10:00:00Z"), json!(7))
    };
    Arc::new(MockAdapter::new(vec![JsonbValue::new(
        json!({"id": ID, "name": "one", "at": at, "n": n}),
    )]))
}

fn executor(adapter: &Arc<MockAdapter>) -> Executor {
    Executor::with_config_and_relay(schema(), Arc::clone(adapter), RuntimeConfig::default())
}

/// How many reads reached the adapter, a bare id's probes among them.
fn reads(adapter: &MockAdapter) -> usize {
    adapter.served_name_buffers.lock().unwrap().len()
}

fn typed_id() -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(format!("Kind:{ID}"))
}

fn messages(response: &Value) -> Vec<String> {
    response["errors"]
        .as_array()
        .map(|errors| errors.iter().map(|e| e["message"].as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn paths(response: &Value) -> Vec<Value> {
    response["errors"]
        .as_array()
        .map(|errors| errors.iter().map(|e| e["path"].clone()).collect())
        .unwrap_or_default()
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::default(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        authenticated_at: chrono::Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

#[tokio::test]
async fn a_node_root_is_served_beside_another_in_either_order() {
    let node = format!(r#"node(id: "{}") {{ id ... on Kind {{ name }} }}"#, typed_id());
    for document in [
        format!("{{ {node} plains {{ id name }} }}"),
        format!("{{ plains {{ id name }} {node} }}"),
    ] {
        let adapter = adapter(false);
        let response = executor(&adapter).execute(&document, None).await.unwrap();
        assert_eq!(
            response,
            json!({"data": {
                "node": {"id": ID, "name": "one"},
                "plains": [{"id": ID, "name": "one"}],
            }}),
            "{document}"
        );
        assert_eq!(reads(&adapter), 2, "{document}");
    }
}

#[tokio::test]
async fn each_of_two_node_roots_is_answered_under_its_key() {
    let document =
        format!(r#"{{ a: node(id: "{}") {{ id }} b: node(id: "{ID}") {{ id }} }}"#, typed_id());
    let response = executor(&adapter(false)).execute(&document, None).await.unwrap();
    assert_eq!(response, json!({"data": {"a": {"id": ID}, "b": {"id": ID}}}));
}

/// The id is the root's own argument, whatever its variable is called.
#[tokio::test]
async fn a_node_id_passed_as_any_variable_is_read() {
    let response = executor(&adapter(false))
        .execute("query($n: ID!) { node(id: $n) { id } }", Some(&json!({"n": typed_id()})))
        .await
        .unwrap();
    assert_eq!(response, json!({"data": {"node": {"id": ID}}}));
}

/// An aliased lookup is answered under its alias, and completed there as `Kind`: the field
/// errors the type's own root gives the row, at the alias's paths, and no `node` key.
#[tokio::test]
async fn an_aliased_node_root_is_answered_and_completed_under_its_alias() {
    let root = executor(&adapter(true)).execute("{ kinds { at n } }", None).await.unwrap();
    let expected = messages(&root);
    assert_eq!(expected.len(), 2, "the root refuses both fields: {root}");

    let found = format!(r#"found: node(id: "{}") {{ ... on Kind {{ at n }} }}"#, typed_id());
    for document in [
        format!("{{ {found} }}"),
        format!("{{ {found} plains {{ id }} }}"),
    ] {
        let response = executor(&adapter(true)).execute(&document, None).await.unwrap();
        assert_eq!(messages(&response), expected, "{document}: {response}");
        assert_eq!(paths(&response), [json!(["found", "at"]), json!(["found", "n"])], "{document}");
        assert_eq!(
            response["data"]["found"],
            json!({"at": null, "n": null}),
            "{document}: {response}"
        );
        assert!(response["data"].get("node").is_none(), "{document}: {response}");
    }
}

/// A node root refused while planned refuses the document, and nothing is read.
#[tokio::test]
async fn a_node_root_refused_while_planned_refuses_the_document_before_any_read() {
    let node = format!(r#"node(id: "{}") {{ ... on Kind {{ nope }} }}"#, typed_id());
    for document in [
        format!("{{ {node} plains {{ id }} }}"),
        format!("{{ plains {{ id }} {node} }}"),
    ] {
        let adapter = adapter(false);
        let refused = executor(&adapter).execute(&document, None).await.unwrap_err();
        assert!(refused.to_string().contains("nope"), "{document}: {refused}");
        assert_eq!(reads(&adapter), 0, "{document}: a root was read");
    }
}

/// Node roots are planned last: a sibling its role refuses refuses the document before a
/// bare id is probed for its type, wherever the node root stands.
#[tokio::test]
async fn a_sibling_refused_while_planned_refuses_before_a_node_root_is_probed() {
    for node in [
        format!(r#"node(id: "{ID}") {{ id }}"#),
        format!(r#"node(id: "{}") {{ id }}"#, typed_id()),
    ] {
        for document in [
            format!("{{ {node} audits {{ id }} }}"),
            format!("{{ audits {{ id }} {node} }}"),
        ] {
            let adapter = adapter(false);
            let refused = executor(&adapter)
                .execute_with_security(&document, None, &principal())
                .await
                .unwrap_err();
            assert!(refused.to_string().contains("audits"), "{document}: {refused}");
            assert_eq!(reads(&adapter), 0, "{document}: the adapter was called");
        }
    }
}

/// A node root whose read fails is `null` with a field error at its key; the others are
/// served.
#[tokio::test]
async fn a_node_roots_failed_read_is_a_field_error_at_its_key() {
    let mut failing = MockAdapter::new(vec![JsonbValue::new(json!({"id": ID, "name": "one"}))]);
    failing.failing_views.insert("v_kind".to_string(), "read failed".to_string());
    let executor =
        Executor::with_config_and_relay(schema(), Arc::new(failing), RuntimeConfig::default());
    let document = format!(r#"{{ node(id: "{}") {{ id }} plains {{ id }} }}"#, typed_id());
    let response = executor.execute(&document, None).await.unwrap();
    assert_eq!(response["data"], json!({"node": null, "plains": [{"id": ID}]}), "{response}");
    assert_eq!(paths(&response), [json!(["node"])], "{response}");
}

/// `@skip` on a node root omits its key, and nothing is read for it.
#[tokio::test]
async fn a_skipped_node_root_is_omitted() {
    let node = format!(r#"node(id: "{}") @skip(if: true) {{ id }}"#, typed_id());
    let first = adapter(false);
    let alone = executor(&first).execute(&format!("{{ {node} }}"), None).await.unwrap();
    assert_eq!(alone, json!({"data": {}}));
    assert_eq!(reads(&first), 0);

    let adapter = adapter(false);
    let beside = executor(&adapter)
        .execute(&format!("{{ {node} plains {{ id }} }}"), None)
        .await
        .unwrap();
    assert_eq!(beside, json!({"data": {"plains": [{"id": ID}]}}));
    assert_eq!(reads(&adapter), 1);
}
