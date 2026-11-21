//! A field the caller may not read refuses the document on every shape it can be selected
//! in, through the authenticated entry, before anything is read: on a second root, nested
//! under a root, in an aggregate's measures or group keys, in a window's selection, and
//! inside a `node(id:)` type fragment. A caller holding the scope is served each one.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use crate::{
    backend::types::JsonbValue,
    error::FraiseQLError,
    runtime::{Executor, RuntimeConfig, executor::test_support::MockAdapter},
    schema::{
        CompiledSchema, CursorType, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::SecurityContext,
};

const SCOPES: [&str; 3] = ["read:code", "read:cost", "read:segment"];

/// `Secret.code` needs `read:code`; `Thing.secret` is a `Secret`. `tf_sales` is read as
/// `Sale`, whose measure `cost` needs `read:cost` and dimension `segment` `read:segment`.
/// Each refuses. The role `reader` grants all three.
fn schema() -> CompiledSchema {
    let refused = |name: &str, ty: FieldType, scope: &str| {
        FieldDefinition::nullable(name, ty)
            .with_requires_scope(scope)
            .with_on_deny(FieldDenyPolicy::Reject)
    };
    let mut schema = CompiledSchema::new();
    schema.queries.push(
        QueryDefinition::new("things", "Thing")
            .returning_list()
            .with_sql_source("v_thing"),
    );
    schema.queries.push(
        QueryDefinition::new("secrets", "Secret")
            .returning_list()
            .with_sql_source("v_secret"),
    );
    // The connection a `node(id:)` lookup of a `Secret` resolves through.
    let mut connection = QueryDefinition::new("secretsConnection", "Secret")
        .returning_list()
        .with_sql_source("v_secret");
    connection.relay = true;
    connection.relay_cursor_column = Some("pk_secret".to_string());
    connection.relay_cursor_type = CursorType::Int64;
    schema.queries.push(connection);
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("name", FieldType::String),
            FieldDefinition::nullable("secret", FieldType::Object("Secret".to_string())),
        ],
        ..TypeDefinition::new("Thing", "v_thing")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("name", FieldType::String),
            refused("code", FieldType::String, "read:code"),
        ],
        ..TypeDefinition::new("Secret", "v_secret")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("revenue", FieldType::Float),
            refused("cost", FieldType::Float, "read:cost"),
            FieldDefinition::new("category", FieldType::String),
            refused("segment", FieldType::String, "read:segment"),
        ],
        ..TypeDefinition::new("Sale", "v_sale")
    });
    let measure =
        |name: &str| serde_json::json!({"name": name, "sql_type": "Decimal", "nullable": true});
    let path = |name: &str| serde_json::json!({"name": name, "json_path": format!("data->>'{name}'"), "data_type": "text"});
    schema.add_fact_table(
        "tf_sales".to_string(),
        serde_json::from_value(serde_json::json!({
            "table_name": "tf_sales",
            "type_name": "Sale",
            "measures": [measure("revenue"), measure("cost")],
            "dimensions": {"name": "data", "paths": [path("category"), path("segment")]},
            "denormalized_filters": []
        }))
        .unwrap(),
    );
    let mut security = SecurityConfig::default();
    security
        .add_role(RoleDefinition::new("reader", SCOPES.iter().map(ToString::to_string).collect()));
    schema.security = Some(security);
    schema.build_indexes();
    schema
}

/// Every view answers one row carrying each field, the nested `secret` included.
fn adapter() -> MockAdapter {
    MockAdapter::new(vec![JsonbValue::new(serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "name": "one",
        "code": "c-1",
        "secret": {"id": "00000000-0000-0000-0000-000000000002", "name": "two", "code": "c-2"}
    }))])
    .with_aggregate_rows(vec![serde_json::json!({"count": 1, "cost_sum": 2.0, "segment": "s"})])
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

/// How many reads reached the adapter: rows read and aggregate statements.
fn reads(adapter: &MockAdapter) -> usize {
    adapter.served_name_buffers.lock().unwrap().len() + adapter.aggregate_statements().len()
}

fn node_document(typename: bool) -> String {
    use base64::Engine as _;
    let id = base64::engine::general_purpose::STANDARD
        .encode("Secret:00000000-0000-0000-0000-000000000001");
    let typename = if typename { "__typename " } else { "" };
    format!(r#"{{ node(id: "{id}") {{ {typename}... on Secret {{ id code }} }} }}"#)
}

/// Each shape a field the caller may not read can be selected in.
fn documents() -> Vec<(&'static str, String)> {
    vec![
        ("a second root", "{ things { id } secrets { id code } }".to_string()),
        ("nested under the root", "{ things { id secret { id code } } }".to_string()),
        (
            "an aggregate's measure",
            "{ sales_aggregate(aggregates: [{cost_sum: true}]) { count } }".to_string(),
        ),
        (
            "an aggregate's group key",
            "{ sales_aggregate(groupBy: {segment: true}, aggregates: [{count: true}]) { count } }"
                .to_string(),
        ),
        (
            "a window's selection",
            r#"{ sales_window(select: [{type: "measure", name: "cost", alias: "cost"}], windows: []) }"#
                .to_string(),
        ),
        ("a node fragment", node_document(false)),
        ("a node fragment beside __typename", node_document(true)),
    ]
}

fn executor(adapter: &Arc<MockAdapter>) -> Executor {
    Executor::with_config_and_relay(schema(), Arc::clone(adapter), RuntimeConfig::default())
}

#[tokio::test]
async fn a_field_the_caller_may_not_read_refuses_every_shape_before_any_read() {
    for (shape, document) in documents() {
        let adapter = Arc::new(adapter());
        let refused =
            executor(&adapter).execute_with_security(&document, None, &principal(&[])).await;
        assert!(
            matches!(refused, Err(FraiseQLError::Authorization { .. })),
            "{shape}: {refused:?}"
        );
        assert_eq!(reads(&adapter), 0, "{shape}: read before the refusal");
    }
}

#[tokio::test]
async fn a_caller_holding_the_scope_is_served_every_shape() {
    for (shape, document) in documents() {
        let adapter = Arc::new(adapter());
        let served = executor(&adapter)
            .execute_with_security(&document, None, &principal(&["reader"]))
            .await;
        assert!(served.is_ok(), "{shape}: {served:?}");
        let served = served.unwrap();
        assert!(served.get("errors").is_none(), "{shape}: {served}");
        assert!(reads(&adapter) > 0, "{shape}: nothing read: {served}");
    }
}
