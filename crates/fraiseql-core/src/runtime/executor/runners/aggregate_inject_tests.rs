//! Tests for a fact table's `inject_params` scoping its aggregate and window roots (#1800).
//!
//! A list root filters its rows by its query's `inject_params`. A fact table declares the same
//! filters for its roots, or inherits its linked type's, and every door composes them: the
//! GraphQL dispatch and the two embedder entries. A caller the filters cannot be resolved for
//! is refused before any SQL runs.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use super::aggregate_rls_tests::{schema_with_fact_table, tenant_security_context};
use crate::{
    compiler::fact_table::{FactRowScope, FactTableMetadata},
    error::{FraiseQLError, Result},
    runtime::{Executor, RuntimeConfig, executor::test_support::CapturingMockAdapter},
    schema::{
        CompiledSchema, FieldDefinition, FieldType, InjectedParamSource, QueryDefinition,
        TypeDefinition,
    },
    security::{DefaultRLSPolicy, SecurityContext},
};

const AGGREGATE: &str = "{ sales_aggregate { count } }";
const WINDOW: &str = "{ sales_window }";

fn aggregate_vars() -> serde_json::Value {
    serde_json::json!({ "aggregates": [{"count": true}] })
}

fn window_vars() -> serde_json::Value {
    serde_json::json!({
        "select": [{"type": "filter", "name": "tenant_id", "alias": "tenant_id"}],
        "windows": [{"function": {"type": "row_number"}, "alias": "rank"}]
    })
}

fn jwt(claim: &str) -> InjectedParamSource {
    InjectedParamSource::Jwt(claim.to_string())
}

fn with_table(
    mut schema: CompiledSchema,
    edit: impl FnOnce(&mut FactTableMetadata),
) -> CompiledSchema {
    let mut table = schema.get_fact_table("tf_sales").unwrap().clone();
    edit(&mut table);
    schema.add_fact_table("tf_sales".to_string(), table);
    schema.build_indexes();
    schema
}

/// `tf_sales` declaring `inject_params` of its own: `column` from the claim `claim`.
fn declaring(column: &str, claim: &str) -> CompiledSchema {
    with_table(schema_with_fact_table(), |table| {
        table.inject_params.insert(column.to_string(), jwt(claim));
    })
}

/// `tf_sales`, read as type `Sale`, which injects `column` from the claim `claim`.
fn inheriting(column: &str, claim: &str) -> CompiledSchema {
    let mut schema = schema_with_fact_table();
    let mut sale = TypeDefinition {
        fields: vec![
            FieldDefinition::new("revenue", FieldType::Float),
            FieldDefinition::new("tenant_id", FieldType::String),
            FieldDefinition::new("author_id", FieldType::String),
        ],
        ..TypeDefinition::new("Sale", "v_sale")
    };
    sale.inject_params.insert(column.to_string(), jwt(claim));
    schema.types.push(sale);
    with_table(schema, |table| table.type_name = Some("Sale".to_string()))
}

/// A principal carrying `claims` beside the fixture's `tenant_id`.
fn principal_with(claims: &[(&str, serde_json::Value)]) -> SecurityContext {
    let mut principal = tenant_security_context("tenant-abc");
    for (claim, value) in claims {
        principal.attributes.insert((*claim).to_string(), value.clone());
    }
    principal
}

struct Served {
    res:    Result<serde_json::Value>,
    sql:    Option<String>,
    params: Vec<serde_json::Value>,
}

async fn graphql(
    schema: CompiledSchema,
    config: RuntimeConfig,
    document: &str,
    vars: &serde_json::Value,
    principal: Option<&SecurityContext>,
) -> Served {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::with_config(schema, adapter.clone(), config);
    let res = match principal {
        Some(principal) => executor.execute_with_security(document, Some(vars), principal).await,
        None => executor.execute(document, Some(vars)).await,
    };
    Served {
        res,
        sql: adapter.captured_aggregate_sql(),
        params: adapter.captured_aggregate_params().unwrap_or_default(),
    }
}

/// Both roots, read by `principal`.
async fn both_roots(
    schema: &CompiledSchema,
    config: &RuntimeConfig,
    principal: Option<&SecurityContext>,
) -> [(&'static str, Served); 2] {
    [
        (
            "aggregate",
            graphql(schema.clone(), config.clone(), AGGREGATE, &aggregate_vars(), principal).await,
        ),
        (
            "window",
            graphql(schema.clone(), config.clone(), WINDOW, &window_vars(), principal).await,
        ),
    ]
}

fn message(res: &Result<serde_json::Value>) -> &str {
    match res {
        Err(FraiseQLError::Validation { message, .. }) => message,
        _ => "",
    }
}

fn assert_refused(served: &Served, expected: &[&str], what: &str) {
    let message = message(&served.res);
    for part in expected {
        assert!(
            message.contains(part),
            "{what}: expected {part:?} in the refusal, got {:?}",
            served.res
        );
    }
    assert!(
        served.sql.is_none(),
        "{what}: the statement reached the database: {:?}",
        served.sql
    );
}

fn assert_scoped(served: &Served, column: &str, value: &str, what: &str) {
    assert!(served.res.is_ok(), "{what}: {:?}", served.res);
    let sql = served.sql.as_deref().unwrap_or_default();
    assert!(sql.contains(column), "{what}: the scope's column is in the WHERE: {sql}");
    assert!(
        served.params.contains(&serde_json::json!(value)),
        "{what}: the caller's value is bound: {:?} in {sql}",
        served.params
    );
}

// ── Declared ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_declared_scope_filters_both_roots_to_the_callers_value() {
    let principal = principal_with(&[("org", serde_json::json!("org-7"))]);
    for (what, served) in
        both_roots(&declaring("tenant_id", "org"), &RuntimeConfig::default(), Some(&principal))
            .await
    {
        assert_scoped(&served, "tenant_id", "org-7", what);
    }
}

/// The caller's own filter is AND-ed after the scope: it narrows the caller's rows, never
/// replaces them.
#[tokio::test]
async fn the_callers_filter_is_composed_with_the_scope() {
    let principal = principal_with(&[("org", serde_json::json!("org-7"))]);
    let vars = serde_json::json!({
        "aggregates": [{"count": true}],
        "where": {"tenant_id_eq": "org-8"}
    });
    let served = graphql(
        declaring("tenant_id", "org"),
        RuntimeConfig::default(),
        AGGREGATE,
        &vars,
        Some(&principal),
    )
    .await;
    assert_scoped(&served, "tenant_id", "org-7", "scope");
    let sql = served.sql.unwrap();
    assert!(sql.contains(" AND "), "{sql}");
    assert!(
        served.params.contains(&serde_json::json!("org-8")),
        "the caller's filter: {sql}"
    );
}

/// Under a row policy, a declared scope composes with the policy's predicate, as a list
/// root's `inject_params` do. The policy binds the principal's tenant (`tenant-abc`), so
/// `org-7` is bound by the scope alone.
#[tokio::test]
async fn a_declared_scope_composes_with_a_row_policy() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let principal = principal_with(&[("org", serde_json::json!("org-7"))]);
    for (what, served) in
        both_roots(&declaring("tenant_id", "org"), &config, Some(&principal)).await
    {
        assert_scoped(&served, "tenant_id", "org-7", what);
        assert!(
            served.params.contains(&serde_json::json!("tenant-abc")),
            "{what}: the policy's predicate stays: {:?}",
            served.params
        );
    }
}

#[tokio::test]
async fn an_anonymous_caller_is_refused_a_declared_scope() {
    for (what, served) in
        both_roots(&declaring("tenant_id", "org"), &RuntimeConfig::default(), None).await
    {
        assert_refused(
            &served,
            &[
                "fact table 'tf_sales'",
                "its own inject params",
                "without a security context",
            ],
            what,
        );
    }
}

#[tokio::test]
async fn a_caller_without_the_claim_is_refused() {
    let principal = tenant_security_context("tenant-abc");
    for (what, served) in
        both_roots(&declaring("tenant_id", "org"), &RuntimeConfig::default(), Some(&principal))
            .await
    {
        assert_refused(&served, &["Inject param 'tenant_id'", "JWT claim 'org' not present"], what);
    }
}

// ── Inherited ───────────────────────────────────────────────────────────────

/// A fact table read as a type that injects one of its filter columns is scoped by it: the
/// read #1799 refused is served, filtered.
#[tokio::test]
async fn an_inherited_scope_filters_both_roots() {
    let principal = principal_with(&[("org", serde_json::json!("org-7"))]);
    for (what, served) in
        both_roots(&inheriting("tenant_id", "org"), &RuntimeConfig::default(), Some(&principal))
            .await
    {
        assert_scoped(&served, "tenant_id", "org-7", what);
    }
}

#[tokio::test]
async fn an_anonymous_caller_is_refused_an_inherited_scope() {
    for (what, served) in
        both_roots(&inheriting("tenant_id", "org"), &RuntimeConfig::default(), None).await
    {
        assert_refused(
            &served,
            &[
                "fact table 'tf_sales'",
                "inherited from type 'Sale'",
                "without a security context",
            ],
            what,
        );
    }
}

/// Under a row policy, the policy alone scopes a fact table that inherits its type's
/// `inject_params`, as before: inheritance replaces #1799's refusal and changes no read a
/// policy already served.
#[tokio::test]
async fn under_a_row_policy_an_inherited_scope_adds_nothing() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let principal = principal_with(&[("org", serde_json::json!("org-7"))]);
    for (what, served) in
        both_roots(&inheriting("tenant_id", "org"), &config, Some(&principal)).await
    {
        assert!(served.res.is_ok(), "{what}: {:?}", served.res);
        assert!(
            served.params.contains(&serde_json::json!("tenant-abc")),
            "{what}: the policy scopes it: {:?}",
            served.params
        );
        assert!(
            !served.params.contains(&serde_json::json!("org-7")),
            "{what}: and nothing else: {:?}",
            served.params
        );
    }
}

/// An inherited filter on a column the fact table does not have cannot scope it: the read is
/// refused as #1799 refused it, naming why the inheritance does not apply.
#[tokio::test]
async fn an_inherited_scope_on_a_column_the_table_lacks_is_refused() {
    let principal = principal_with(&[("org", serde_json::json!("org-7"))]);
    for (what, served) in
        both_roots(&inheriting("owner_id", "org"), &RuntimeConfig::default(), Some(&principal))
            .await
    {
        assert_refused(
            &served,
            &[
                "no row policy scopes its aggregate and window roots",
                "inherited from type 'Sale'",
                "'owner_id' is not one of its denormalized filter columns",
            ],
            what,
        );
    }
}

/// A fact table declaring `row_scope = "relation"` is served unfiltered by the server, whatever
/// its type injects, as before.
#[tokio::test]
async fn a_relation_scoped_table_inherits_nothing() {
    let schema = with_table(inheriting("author_id", "sub"), |table| {
        table.row_scope = FactRowScope::Relation;
    });
    let principal = tenant_security_context("tenant-abc");
    for (what, served) in both_roots(&schema, &RuntimeConfig::default(), Some(&principal)).await {
        assert!(served.res.is_ok(), "{what}: {:?}", served.res);
        assert!(!served.sql.unwrap().contains("author_id"), "{what}: the relation scopes itself");
    }
}

// ── The embedder entries ────────────────────────────────────────────────────

/// The embedder entries carry no principal: a scoped fact table is refused there, from the
/// schema's registration, whatever the metadata handed in says.
#[tokio::test]
async fn the_embedder_entries_refuse_a_scoped_fact_table() {
    for schema in [
        declaring("tenant_id", "org"),
        inheriting("tenant_id", "org"),
    ] {
        let registered = schema.get_fact_table("tf_sales").unwrap().clone();
        let stripped = FactTableMetadata {
            type_name: None,
            inject_params: indexmap::IndexMap::new(),
            ..registered.clone()
        };
        for metadata in [registered, stripped] {
            let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
            let executor = Executor::new(schema.clone(), adapter.clone());
            let aggregate = executor
                .execute_aggregate_query(&aggregate_vars(), "sales_aggregate", &metadata)
                .await;
            let window =
                executor.execute_window_query(&window_vars(), "sales_window", &metadata).await;
            for (what, res) in [("aggregate entry", aggregate), ("window entry", window)] {
                assert!(
                    message(&res).contains("without a security context"),
                    "{what} ({:?}): {res:?}",
                    metadata.inject_params
                );
            }
            let sql = adapter.captured_aggregate_sql();
            assert!(sql.is_none(), "the statement reached the database: {sql:?}");
        }
    }
}

/// The positive control: a fact table nothing scopes is served from every door, unfiltered.
#[tokio::test]
async fn a_fact_table_nothing_scopes_is_served() {
    let mut schema = schema_with_fact_table();
    schema.queries.push(QueryDefinition::new("rows", "Row"));
    schema.build_indexes();
    for (what, served) in both_roots(&schema, &RuntimeConfig::default(), None).await {
        assert!(served.res.is_ok(), "{what}: {:?}", served.res);
        assert!(!served.sql.unwrap().contains("WHERE"), "{what}: unfiltered");
    }
}
