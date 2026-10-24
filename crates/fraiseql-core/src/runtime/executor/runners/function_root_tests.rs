//! A query read from a set-returning function.
//!
//! A function root binds its arguments, then its `inject_params`, to the function's
//! parameters, and is otherwise read like the relation it returns: the row policy and the
//! client's filter scope its rows, and the page caps bound them. What it cannot serve —
//! a count, a stream, a Relay page, a composed nested level, a row-shaped read — is
//! refused, never read as a relation of the function's name.

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;

use crate::{
    backend::types::JsonbValue,
    error::{FraiseQLError, Result},
    runtime::{
        RuntimeConfig,
        executor::{
            Executor,
            test_support::{CapturedSource, CapturingMockAdapter},
        },
    },
    schema::{
        ArgumentDefinition, AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldType,
        GraphQLValue, InjectedParamSource, QueryDefinition, TypeDefinition,
    },
    security::{RLSPolicy, RlsWhereClause, SecurityContext, rls_policy::RlsTarget},
};

// ── fixtures ─────────────────────────────────────────────────────────────────────────

/// `lineItems(assetId: ID!, asOf: String = "today")` read from `app.fn_line_items`, every
/// auto-param on, returning `LineItem { id, label }`.
fn schema_with(edit: impl FnOnce(&mut QueryDefinition)) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut query = QueryDefinition::new("lineItems", "LineItem")
        .returning_list()
        .with_sql_source_function("app.fn_line_items");
    query.auto_params = AutoParams::all();
    let mut as_of = ArgumentDefinition::optional("asOf", FieldType::String);
    as_of.default_value = Some(GraphQLValue::String("today".to_string()));
    query.arguments = vec![ArgumentDefinition::new("assetId", FieldType::Id), as_of];
    edit(&mut query);
    schema.queries.push(query);
    let mut item = TypeDefinition::new("LineItem", "v_line_item");
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(item);
    schema
}

fn plain_schema() -> CompiledSchema {
    schema_with(|_| {})
}

fn rows() -> Vec<JsonbValue> {
    vec![
        JsonbValue::new(serde_json::json!({"id": "l1", "label": "Rental"})),
        JsonbValue::new(serde_json::json!({"id": "l2", "label": "Service"})),
    ]
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          "user-42".into(),
        roles:            vec!["viewer".to_string()],
        tenant_id:        Some("tenant-abc".into()),
        scopes:           Vec::new(),
        attributes:       HashMap::from([(
            "tenant_id".to_string(),
            serde_json::json!("tenant-abc"),
        )]),
        request_id:       "req-function-root".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn tenant_scoped(query: &mut QueryDefinition) {
    query
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
}

/// A policy that scopes every read to the principal's own rows: `owner = <user id>`.
struct OwnerPolicy;

impl RLSPolicy for OwnerPolicy {
    fn evaluate(
        &self,
        context: &SecurityContext,
        _target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        Ok(Some(RlsWhereClause::new(crate::backend::WhereClause::Field {
            path:     vec!["owner".to_string()],
            operator: crate::backend::where_clause::WhereOperator::Eq,
            value:    serde_json::Value::String(context.user_id.to_string()),
        })))
    }
}

fn executor(
    schema: CompiledSchema,
    config: RuntimeConfig,
) -> (Executor, Arc<CapturingMockAdapter>) {
    let adapter = Arc::new(CapturingMockAdapter::new(rows()));
    (Executor::with_config(schema, adapter.clone(), config), adapter)
}

fn function_read(args: Vec<serde_json::Value>) -> Option<CapturedSource> {
    Some(CapturedSource {
        view: "app.fn_line_items".to_string(),
        args: Some(args),
    })
}

fn where_text(adapter: &CapturingMockAdapter) -> String {
    format!("{:?}", adapter.captured_where())
}

const DOCUMENT: &str = r#"{ lineItems(assetId: "a1", asOf: "2026-01-01") { id label } }"#;

// ── binding ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_function_root_reads_its_function_with_its_arguments_in_declaration_order() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let response = executor
        .execute_with_security(DOCUMENT, None, &principal())
        .await
        .expect("a function-backed read");
    assert_eq!(
        adapter.captured_source(),
        function_read(vec![serde_json::json!("a1"), serde_json::json!("2026-01-01")])
    );
    assert_eq!(response["data"]["lineItems"][1]["label"], "Service", "{response}");
}

#[tokio::test]
async fn a_function_roots_arguments_are_parameters_not_filters() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    executor
        .execute_with_security(DOCUMENT, None, &principal())
        .await
        .expect("a function-backed read");
    assert!(adapter.captured_where().is_none(), "{}", where_text(&adapter));
}

#[tokio::test]
async fn an_omitted_argument_binds_its_default() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    executor
        .execute_with_security(r#"{ lineItems(assetId: "a1") { id } }"#, None, &principal())
        .await
        .expect("a function-backed read");
    assert_eq!(
        adapter.captured_source(),
        function_read(vec![serde_json::json!("a1"), serde_json::json!("today")])
    );
}

#[tokio::test]
async fn an_omitted_argument_with_no_default_binds_null() {
    let schema = schema_with(|q| q.arguments[1].default_value = None);
    let (executor, adapter) = executor(schema, RuntimeConfig::default());
    executor
        .execute_with_security(r#"{ lineItems(assetId: "a1") { id } }"#, None, &principal())
        .await
        .expect("a function-backed read");
    assert_eq!(
        adapter.captured_source(),
        function_read(vec![serde_json::json!("a1"), serde_json::Value::Null])
    );
}

#[tokio::test]
async fn a_variable_binds_like_an_inline_argument() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    executor
        .execute_with_security(
            "query($a: ID!) { lineItems(assetId: $a) { id } }",
            Some(&serde_json::json!({"a": "a9"})),
            &principal(),
        )
        .await
        .expect("a function-backed read");
    assert_eq!(
        adapter.captured_source(),
        function_read(vec![serde_json::json!("a9"), serde_json::json!("today")])
    );
}

#[tokio::test]
async fn a_function_roots_inject_params_follow_its_arguments() {
    let (executor, adapter) = executor(schema_with(tenant_scoped), RuntimeConfig::default());
    executor
        .execute_with_security(DOCUMENT, None, &principal())
        .await
        .expect("a tenant-bound function read");
    assert_eq!(
        adapter.captured_source(),
        function_read(vec![
            serde_json::json!("a1"),
            serde_json::json!("2026-01-01"),
            serde_json::json!("tenant-abc"),
        ])
    );
    assert!(adapter.captured_where().is_none(), "{}", where_text(&adapter));
}

#[tokio::test]
async fn an_anonymous_read_of_an_injected_function_root_is_refused() {
    let (executor, adapter) = executor(schema_with(tenant_scoped), RuntimeConfig::default());
    let err = executor
        .execute(DOCUMENT, None)
        .await
        .expect_err("no principal, no tenant to bind");
    assert!(err.to_string().contains("security context"), "{err}");
    assert!(adapter.captured_source().is_none());
}

#[tokio::test]
async fn a_direct_read_of_a_function_root_binds_its_arguments() {
    let (executor, adapter) = executor(schema_with(tenant_scoped), RuntimeConfig::default());
    let qm = executor
        .matcher()
        .match_query(DOCUMENT, None)
        .expect("the fixture document matches");
    executor
        .execute_query_direct(&qm, None, Some(&principal()), None)
        .await
        .expect("a direct function-backed read");
    assert_eq!(
        adapter.captured_source(),
        function_read(vec![
            serde_json::json!("a1"),
            serde_json::json!("2026-01-01"),
            serde_json::json!("tenant-abc"),
        ])
    );
}

// ── the rows are scoped like a view's ────────────────────────────────────────────────

#[tokio::test]
async fn a_function_roots_rows_are_scoped_by_the_row_policy() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(OwnerPolicy));
    let (executor, adapter) = executor(plain_schema(), config);
    executor
        .execute_with_security(DOCUMENT, None, &principal())
        .await
        .expect("a policy-scoped function read");
    assert!(where_text(&adapter).contains("owner"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn an_anonymous_read_of_a_function_root_under_a_row_policy_is_refused() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(OwnerPolicy));
    let (executor, adapter) = executor(plain_schema(), config);
    executor
        .execute(DOCUMENT, None)
        .await
        .expect_err("the policy cannot be evaluated without a principal");
    assert!(adapter.captured_source().is_none());
}

#[tokio::test]
async fn a_function_roots_rows_are_filtered_by_the_clients_where() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    executor
        .execute_with_security(
            r#"{ lineItems(assetId: "a1", where: { label: { eq: "Rental" } }) { id } }"#,
            None,
            &principal(),
        )
        .await
        .expect("a filtered function read");
    assert!(where_text(&adapter).contains("Rental"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_function_roots_page_is_held_to_the_page_ceiling() {
    let config = RuntimeConfig {
        max_page_size: Some(10),
        ..RuntimeConfig::default()
    };
    let (executor, adapter) = executor(plain_schema(), config);
    executor
        .execute_with_security(
            r#"{ lineItems(assetId: "a1", limit: 50) { id } }"#,
            None,
            &principal(),
        )
        .await
        .expect_err("a page over the ceiling");
    assert!(adapter.captured_source().is_none());
}

// ── what a function root does not serve ──────────────────────────────────────────────

fn is_unsupported(err: &FraiseQLError) -> bool {
    matches!(err, FraiseQLError::Unsupported { .. })
}

#[tokio::test]
async fn a_count_of_a_function_root_is_refused() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let qm = executor
        .matcher()
        .match_query(DOCUMENT, None)
        .expect("the fixture document matches");
    let err = executor
        .count_rows(&qm, None, Some(&principal()))
        .await
        .expect_err("a function root has no count");
    assert!(is_unsupported(&err), "{err:?}");
    assert!(adapter.captured_where().is_none());
}

#[tokio::test]
async fn a_stream_of_a_function_root_is_refused() {
    let (executor, _) = executor(plain_schema(), RuntimeConfig::default());
    let qm = executor
        .matcher()
        .match_query(DOCUMENT, None)
        .expect("the fixture document matches");
    let err = executor
        .stream_query_direct(qm, None, Some(principal()))
        .await
        .err()
        .expect("a function root is not streamed");
    assert!(is_unsupported(&err), "{err:?}");
}

#[tokio::test]
async fn a_relay_function_root_is_refused() {
    let schema = schema_with(|q| {
        q.relay = true;
        q.relay_cursor_column = Some("pk_line_item".to_string());
        q.relay_cursor_type = CursorType::Int64;
    });
    let adapter = Arc::new(CapturingMockAdapter::new(rows()));
    let executor =
        Executor::with_config_and_relay(schema, adapter.clone(), RuntimeConfig::default());
    let err = executor
        .execute_with_security(
            r#"{ lineItems(assetId: "a1") { edges { node { id } } } }"#,
            None,
            &principal(),
        )
        .await
        .expect_err("a function root is not a connection");
    assert!(is_unsupported(&err), "{err:?}");
    assert!(adapter.captured_where().is_none());
}

// ── a function root is no relation of its type ───────────────────────────────────────

/// The fixture's function root, gated to `auditor`, listed before `lineItemRows`: the
/// relation `v_line_item` a lookup of a `LineItem` reads, open to anyone.
fn schema_with_a_relation_read() -> CompiledSchema {
    let mut schema = schema_with(|q| q.requires_role = Some("auditor".to_string()));
    schema.queries.push(
        QueryDefinition::new("lineItemRows", "LineItem")
            .returning_list()
            .with_sql_source("v_line_item"),
    );
    schema.types[0].relay = true;
    schema
}

const ITEM_ID: &str = "00000000-0000-0000-0000-000000000001";

fn typed_node_document() -> String {
    use base64::Engine as _;
    let id = base64::engine::general_purpose::STANDARD.encode(format!("LineItem:{ITEM_ID}"));
    format!(r#"{{ node(id: "{id}") {{ ... on LineItem {{ id }} }} }}"#)
}

#[tokio::test]
async fn a_node_lookup_reads_the_types_relation_not_its_function_root() {
    let adapter = Arc::new(CapturingMockAdapter::new(rows()));
    let executor = Executor::with_config_and_relay(
        schema_with_a_relation_read(),
        adapter.clone(),
        RuntimeConfig::default(),
    );
    executor
        .execute_with_security(&typed_node_document(), None, &principal())
        .await
        .expect("the relation's gates admit a viewer");
    assert_eq!(
        adapter.captured_source(),
        Some(CapturedSource {
            view: "v_line_item".to_string(),
            args: None,
        })
    );
}

#[tokio::test]
async fn a_bare_id_node_lookup_probes_the_types_relation_not_its_function_root() {
    let adapter = Arc::new(CapturingMockAdapter::new(rows()));
    let executor = Executor::with_config_and_relay(
        schema_with_a_relation_read(),
        adapter.clone(),
        RuntimeConfig::default(),
    );
    let response = executor
        .execute_with_security(
            &format!(r#"{{ node(id: "{ITEM_ID}") {{ ... on LineItem {{ id }} }} }}"#),
            None,
            &principal(),
        )
        .await
        .expect("the relation's gates admit a viewer");
    assert!(!response["data"]["node"].is_null(), "{response}");
}

#[test]
fn a_nested_levels_own_read_is_the_types_relation_not_its_function_root() {
    let schema = schema_with_a_relation_read();
    let read = super::super::query_nested::own_read(&schema, "LineItem").map(|q| q.name.as_str());
    assert_eq!(read, Some("lineItemRows"));
}

#[cfg(feature = "federation")]
#[test]
fn a_function_root_is_no_entity_source() {
    let schema = schema_with_a_relation_read();
    let sources = schema.entity_sources();
    assert_eq!(sources.get("LineItem").map(|s| s.relation.as_str()), Some("v_line_item"));
}

// ── what a function root's call does not bind ────────────────────────────────────────

#[tokio::test]
async fn an_auto_param_declared_as_an_argument_is_not_bound_to_the_function() {
    let schema = schema_with(|q| {
        q.arguments.push(ArgumentDefinition::optional("limit", FieldType::Int));
    });
    let (executor, adapter) = executor(schema, RuntimeConfig::default());
    executor
        .execute_with_security(DOCUMENT, None, &principal())
        .await
        .expect("a function-backed read");
    assert_eq!(
        adapter.captured_source(),
        function_read(vec![serde_json::json!("a1"), serde_json::json!("2026-01-01")])
    );
}

#[tokio::test]
async fn a_function_root_as_the_root_of_a_composed_read_is_refused() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let qm = executor
        .matcher()
        .match_query(DOCUMENT, None)
        .expect("the fixture document matches");
    let err = executor
        .execute_query_composed(&qm, &[], &[], None, Some(&principal()), None)
        .await
        .expect_err("a composed level reads a relation");
    assert!(is_unsupported(&err), "{err:?}");
    assert!(adapter.captured_composed().is_none());
}

#[tokio::test]
async fn a_row_shaped_read_of_a_function_root_is_refused() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let qm = executor
        .matcher()
        .match_query(DOCUMENT, None)
        .expect("the fixture document matches");
    let err = executor
        .execute_row_read(&qm, None, Some(&principal()), &[])
        .await
        .expect_err("a row-shaped read reads the type's row view, not the call");
    assert!(is_unsupported(&err), "{err:?}");
    assert!(adapter.captured_row_read().is_none());
}

/// `LineItem.product` is a `Product`, whose own read scopes its rows to the tenant: a
/// level a document read composes, which a function root's call cannot carry.
#[tokio::test]
async fn a_function_root_selecting_a_row_scoped_nested_level_is_refused() {
    let mut schema = plain_schema();
    schema.types[0]
        .fields
        .push(FieldDefinition::new("product", FieldType::Object("Product".to_string())));
    schema.types[0].relationships.push(crate::schema::Relationship {
        name:           "product".to_string(),
        target_type:    "Product".to_string(),
        cardinality:    crate::schema::Cardinality::ManyToOne,
        foreign_key:    "fk_product".to_string(),
        referenced_key: "id".to_string(),
        native_join:    None,
    });
    let mut product = TypeDefinition::new("Product", "v_product");
    product.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("tenant_id", FieldType::String),
    ];
    schema.types.push(product);
    let mut products = QueryDefinition::new("products", "Product")
        .returning_list()
        .with_sql_source("v_product");
    tenant_scoped(&mut products);
    schema.queries.push(products);
    schema.build_indexes();
    let (executor, adapter) = executor(schema, RuntimeConfig::default());
    let err = executor
        .execute_with_security(
            r#"{ lineItems(assetId: "a1") { id product { id } } }"#,
            None,
            &principal(),
        )
        .await
        .expect_err("the nested level needs a composed statement");
    assert!(err.to_string().contains("nested levels"), "{err}");
    assert!(adapter.captured_source().is_none() && adapter.captured_composed().is_none());
}
