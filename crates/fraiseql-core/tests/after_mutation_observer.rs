#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! #1340 / #1440 — the engine tells an [`AfterMutationObserver`] about every committed write.
//!
//! `after:mutation` dispatch used to read the response in two transport handlers, so:
//! - MCP, gRPC and the functions bridge never fired it;
//! - under `auto_error_union` a failed write (an error member, served as data) dispatched as if it
//!   had succeeded;
//! - the row image was the client's selection.
//!
//! These drive the real executor over a mock writer: a success fires once with the produced
//! entity type and the full row, a failure never fires, and the typed write entry that gRPC
//! uses fires as the document path does.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, MutationRowGate, WriteRequest, Writer},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::{
        AfterMutationObserver, CommittedMutation, Executor, RuntimeConfig, WriteSelections,
        dispatched_at,
    },
    schema::{CompiledSchema, SqlProjectionHint},
};
use serde_json::{Value, json};

/// A writer that answers every mutation with one fixed row.
struct OneRow(HashMap<String, Value>);

#[async_trait]
impl DatabaseAdapter for OneRow {
    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(&self, _sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl Writer for OneRow {
    async fn execute_write(
        &self,
        _request: &WriteRequest<'_>,
        gate: MutationRowGate<'_>,
    ) -> std::result::Result<Vec<HashMap<String, Value>>, FraiseQLError> {
        let rows = vec![self.0.clone()];
        gate(&rows)?;
        Ok(rows)
    }
}

/// What the observer was told, one entry per call.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    mutation:    String,
    entity_type: String,
    entity:      Value,
}

#[derive(Default)]
struct Recorder(Mutex<Vec<Seen>>);

impl AfterMutationObserver for Recorder {
    fn on_committed(&self, mutation: &CommittedMutation<'_>) {
        self.0.lock().unwrap().push(Seen {
            mutation:    mutation.mutation_name.to_string(),
            entity_type: mutation.entity_type.to_string(),
            entity:      mutation.entity.clone(),
        });
    }
}

/// `updateOrder` returns `UpdateOrderResult = Order | NotFoundError`, the shape
/// `auto_error_union` synthesizes.
fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "types": [
            {
                "name": "Order",
                "sql_source": "v_order",
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "status", "field_type": "String" },
                    { "name": "total", "field_type": "Int" }
                ]
            },
            {
                "name": "NotFoundError",
                "sql_source": "",
                "is_error": true,
                "fields": [ { "name": "message", "field_type": "String" } ]
            }
        ],
        "unions": [
            { "name": "UpdateOrderResult", "member_types": ["Order", "NotFoundError"] }
        ],
        "mutations": [
            {
                "name": "updateOrder",
                "return_type": "UpdateOrderResult",
                "sql_source": "fn_update_order",
                "operation": { "Update": { "table": "tb_order" } },
                "arguments": [ { "name": "id", "arg_type": "ID", "nullable": false } ]
            }
        ]
    }))
    .unwrap()
}

const ORDER_ID: &str = "00000000-0000-0000-0000-0000000000a1";

fn success_row(stamp: Option<&str>) -> HashMap<String, Value> {
    HashMap::from([
        ("succeeded".to_string(), json!(true)),
        ("state_changed".to_string(), json!(true)),
        (
            "entity".to_string(),
            json!({ "id": ORDER_ID, "status": "shipped", "total": 42 }),
        ),
        ("entity_type".to_string(), stamp.map_or(Value::Null, |s| json!(s))),
        ("entity_id".to_string(), json!(ORDER_ID)),
        ("cascade".to_string(), Value::Null),
    ])
}

fn failure_row() -> HashMap<String, Value> {
    HashMap::from([
        ("succeeded".to_string(), json!(false)),
        ("state_changed".to_string(), json!(false)),
        ("error_class".to_string(), json!("not_found")),
        ("message".to_string(), json!("no such order")),
        ("entity".to_string(), Value::Null),
        ("entity_type".to_string(), json!("NotFoundError")),
        ("cascade".to_string(), Value::Null),
        ("metadata".to_string(), json!({ "message": "no such order" })),
    ])
}

fn executor(row: HashMap<String, Value>) -> (Executor, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let config = RuntimeConfig::default().with_after_mutation_observer(recorder.clone());
    (Executor::with_config(schema(), Arc::new(OneRow(row)), config), recorder)
}

const MUTATION: &str = "mutation($id: ID!) { updateOrder(id: $id) { ... on Order { id } } }";

/// A success fires once, keyed on the entity the write produced (`Order`, not the
/// synthesized `UpdateOrderResult`), with the full row: `total` was not selected.
#[tokio::test]
async fn a_committed_success_is_observed_once_as_the_produced_entity() {
    for stamp in [Some("Order"), None] {
        let (executor, recorder) = executor(success_row(stamp));
        executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await.unwrap();

        let seen = recorder.0.lock().unwrap().clone();
        assert_eq!(
            seen,
            [Seen {
                mutation:    "updateOrder".to_string(),
                entity_type: "Order".to_string(),
                entity:      json!({ "id": ORDER_ID, "status": "shipped", "total": 42 }),
            }],
            "stamp {stamp:?}"
        );
    }
}

/// A failure is served as the union's error member, which is data, not an error. It is
/// still not a committed write, so nothing is observed, whatever the client selected.
#[tokio::test]
async fn a_failure_served_as_union_data_is_never_observed() {
    for query in [
        MUTATION,
        "mutation($id: ID!) { updateOrder(id: $id) { __typename ... on NotFoundError { message } } }",
    ] {
        let (executor, recorder) = executor(failure_row());
        let response = executor.execute(query, Some(&json!({ "id": ORDER_ID }))).await.unwrap();
        assert!(response.get("data").is_some(), "the failure is served as data: {response}");
        assert!(recorder.0.lock().unwrap().is_empty(), "{query}");
    }
}

/// The typed write entry (`execute_mutation_as`, the gRPC transport's path) is observed
/// exactly as the document path is (#1440).
#[tokio::test]
async fn the_typed_write_entry_is_observed_too() {
    let (executor, recorder) = executor(success_row(Some("Order")));
    let typename = [FieldSelection {
        name:          "__typename".to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }];
    executor
        .execute_mutation_as(
            "updateOrder",
            Some(&json!({ "id": ORDER_ID })),
            None,
            WriteSelections::new(&typename).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(recorder.0.lock().unwrap().len(), 1);
}

/// A dry run rolls back, so it committed nothing and is not observed.
#[tokio::test]
async fn a_dry_run_is_not_observed() {
    let recorder = Arc::new(Recorder::default());
    let mut config = RuntimeConfig::default().with_after_mutation_observer(recorder.clone());
    config.dry_run_mutations = true;
    let executor =
        Executor::with_config(schema(), Arc::new(OneRow(success_row(Some("Order")))), config);
    executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await.unwrap();
    assert!(recorder.0.lock().unwrap().is_empty());
}

/// A write made inside a dispatch scope reports its depth; the chain guard reads it.
#[tokio::test]
async fn a_write_inside_a_dispatch_scope_reports_its_depth() {
    #[derive(Default)]
    struct Depths(Mutex<Vec<u8>>);
    impl AfterMutationObserver for Depths {
        fn on_committed(&self, mutation: &CommittedMutation<'_>) {
            self.0.lock().unwrap().push(mutation.dispatch_depth);
        }
    }
    let depths = Arc::new(Depths::default());
    let config = RuntimeConfig::default().with_after_mutation_observer(depths.clone());
    let executor =
        Executor::with_config(schema(), Arc::new(OneRow(success_row(Some("Order")))), config);
    let vars = json!({ "id": ORDER_ID });

    executor.execute(MUTATION, Some(&vars)).await.unwrap();
    dispatched_at(3, executor.execute(MUTATION, Some(&vars))).await.unwrap();
    assert_eq!(*depths.0.lock().unwrap(), [0, 3]);
}
