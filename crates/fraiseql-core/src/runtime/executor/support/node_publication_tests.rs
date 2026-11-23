//! `node(id:)` is answered only where the schema publishes it, and only for a Node type: a
//! schema with no Node type refuses it as it refuses any field it does not declare, and an
//! id naming no Node the caller may read is `null`, read or not.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::json;

use crate::{
    backend::types::JsonbValue,
    error::FraiseQLError,
    runtime::{Executor, RuntimeConfig, executor::test_support::MockAdapter},
    schema::{
        CompiledSchema, CursorType, FieldDefinition, FieldType, InterfaceDefinition,
        QueryDefinition, TypeDefinition,
    },
    security::SecurityContext,
};

const ID: &str = "00000000-0000-0000-0000-000000000001";

/// A type `name` over `v_<name>` read by the list query `<name>s`; `node` makes it a Node
/// type as the compiler writes one.
fn add_type(schema: &mut CompiledSchema, name: &str, node: bool) {
    let view = format!("v_{}", name.to_lowercase());
    let mut query = QueryDefinition::new(format!("{}s", name.to_lowercase()), name)
        .returning_list()
        .with_sql_source(&view);
    if node {
        query.relay = true;
        query.relay_cursor_column = Some("pk".to_string());
        query.relay_cursor_type = CursorType::Int64;
    }
    schema.queries.push(query);
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("name", FieldType::String),
        ],
        relay: node,
        implements: if node {
            vec!["Node".to_string()]
        } else {
            vec![]
        },
        ..TypeDefinition::new(name, &view)
    });
}

/// `Kind` a Node type and `Plain` not, or, without Node types, `Plain` alone.
fn schema(with_nodes: bool) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    add_type(&mut schema, "Plain", false);
    if with_nodes {
        add_type(&mut schema, "Kind", true);
        schema.interfaces.push(
            InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
        );
    }
    schema.build_indexes();
    schema
}

fn adapter() -> Arc<MockAdapter> {
    Arc::new(MockAdapter::new(vec![JsonbValue::new(json!({"id": ID, "name": "one"}))]))
}

/// How many reads reached the adapter.
fn reads(adapter: &MockAdapter) -> usize {
    adapter.served_name_buffers.lock().unwrap().len()
}

fn typed(type_name: &str) -> String {
    use base64::Engine as _;
    let id = base64::engine::general_purpose::STANDARD.encode(format!("{type_name}:{ID}"));
    format!(r#"{{ node(id: "{id}") {{ id ... on {type_name} {{ name }} }} }}"#)
}

/// `node` read by a typed id of `type_name`, selecting `Node`'s own field: a fragment on a
/// type that is no Node is refused before any id is read.
fn bare(type_name: &str) -> String {
    use base64::Engine as _;
    let id = base64::engine::general_purpose::STANDARD.encode(format!("{type_name}:{ID}"));
    format!(r#"{{ node(id: "{id}") {{ id }} }}"#)
}

fn principal(roles: &[&str]) -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            roles.iter().map(ToString::to_string).collect(),
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

/// With no Node type, `node` is a field the schema does not declare.
#[tokio::test]
async fn node_is_refused_as_an_undeclared_field_where_the_schema_publishes_none() {
    let undeclared =
        Executor::with_config_and_relay(schema(false), adapter(), RuntimeConfig::default())
            .execute(r#"{ nope(id: "x") { id } }"#, None)
            .await
            .unwrap_err();
    let adapter = adapter();
    let executor = Executor::with_config_and_relay(
        schema(false),
        Arc::clone(&adapter),
        RuntimeConfig::default(),
    );
    let refused = executor.execute(&typed("Plain"), None).await.unwrap_err();
    assert!(matches!(refused, FraiseQLError::Validation { .. }), "{refused:?}");
    assert_eq!(refused.to_string(), undeclared.to_string().replace("nope", "node"));
    assert_eq!(reads(&adapter), 0, "a lookup was read");
}

/// An id naming a type that is no Node names no node.
#[tokio::test]
async fn an_id_naming_a_type_that_is_no_node_is_null() {
    let adapter = adapter();
    let executor = Executor::with_config_and_relay(
        schema(true),
        Arc::clone(&adapter),
        RuntimeConfig::default(),
    );
    let response = executor.execute(&bare("Plain"), None).await.unwrap();
    assert_eq!(response, json!({"data": {"node": null}}));
    assert_eq!(reads(&adapter), 0, "a lookup was read");
}

/// A Node the caller's roles may not read is `null`, as one its row policy hides is, and
/// the answer names no query.
#[tokio::test]
async fn a_node_hidden_by_its_querys_role_is_null() {
    let mut gated = schema(true);
    for query in gated.queries.iter_mut().filter(|q| q.return_type == "Kind") {
        query.requires_role = Some("admin".to_string());
    }
    let adapter = adapter();
    let executor = Executor::with_config_and_relay(
        gated.clone(),
        Arc::clone(&adapter),
        RuntimeConfig::default(),
    );
    let response = executor
        .execute_with_security(&typed("Kind"), None, &principal(&[]))
        .await
        .unwrap();
    assert_eq!(response, json!({"data": {"node": null}}));
    assert_eq!(reads(&adapter), 0, "a lookup was read");

    let admin = Executor::with_config_and_relay(gated, adapter, RuntimeConfig::default())
        .execute_with_security(&typed("Kind"), None, &principal(&["admin"]))
        .await
        .unwrap();
    assert_eq!(admin, json!({"data": {"node": {"id": ID, "name": "one"}}}));
}

/// An expired token is refused as expired, though the role it lacks would hide the node.
#[tokio::test]
async fn an_expired_token_is_refused_where_the_role_would_hide_the_node() {
    let mut gated = schema(true);
    for query in gated.queries.iter_mut().filter(|q| q.return_type == "Kind") {
        query.requires_role = Some("admin".to_string());
    }
    let mut expired = principal(&[]);
    expired.expires_at = chrono::Utc::now() - chrono::Duration::hours(1);
    let adapter = adapter();
    let executor =
        Executor::with_config_and_relay(gated, Arc::clone(&adapter), RuntimeConfig::default());
    let refused = executor
        .execute_with_security(&typed("Kind"), None, &expired)
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("expired"), "{refused:?}");
    assert_eq!(reads(&adapter), 0, "a lookup was read");
}

#[tokio::test]
async fn a_node_the_caller_may_read_is_served() {
    let adapter = adapter();
    let executor = Executor::with_config_and_relay(
        schema(true),
        Arc::clone(&adapter),
        RuntimeConfig::default(),
    );
    let response = executor.execute(&typed("Kind"), None).await.unwrap();
    assert_eq!(response, json!({"data": {"node": {"id": ID, "name": "one"}}}));
    assert_eq!(reads(&adapter), 1);
}
