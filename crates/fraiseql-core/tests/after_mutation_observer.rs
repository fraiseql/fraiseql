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
        traits::{
            BatchRowGate, DatabaseAdapter, DispatchLedger, DispatchSettlement, DueDispatch,
            MutationRowGate, PendingDispatch, WriteRequest, Writer,
        },
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

/// A writer that answers every mutation with one fixed row. With `ledger`, it keeps a
/// dispatch ledger and records, per call, the durable dispatches the request carried once
/// its gate passed, as the PostgreSQL adapter inserts them in the write's transaction.
/// Clones share what they recorded, so a test can keep one after handing the other over.
#[derive(Clone)]
struct OneRow(HashMap<String, Value>, Option<Recorded>);

type Recorded = Arc<Mutex<Vec<Vec<PendingDispatch>>>>;

impl OneRow {
    const fn plain(row: HashMap<String, Value>) -> Self {
        Self(row, None)
    }

    fn ledgered(row: HashMap<String, Value>) -> Self {
        Self(row, Some(Arc::default()))
    }

    fn recorded(&self) -> Vec<Vec<PendingDispatch>> {
        self.1.as_ref().unwrap().lock().unwrap().clone()
    }

    fn record(&self, request: &WriteRequest<'_>) {
        if let (Some(recorded), Some(sink)) = (&self.1, request.dispatch_ledger) {
            recorded
                .lock()
                .unwrap()
                .push(sink.staged().into_iter().filter(|d| d.durable).collect());
        }
    }
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DispatchLedger for OneRow {
    async fn provision_dispatch_ledger(&self) -> Result<()> {
        Ok(())
    }

    async fn record_dispatches(
        &self,
        _dispatches: &[PendingDispatch],
        _lease: std::time::Duration,
    ) -> Result<()> {
        Ok(())
    }

    async fn claim_due_dispatches(
        &self,
        _tenant: Option<&str>,
        _batch: u32,
        _lease: std::time::Duration,
    ) -> Result<Vec<DueDispatch>> {
        Ok(vec![])
    }

    async fn settle_dispatch(
        &self,
        _source: &str,
        _ref_key: &str,
        _function_name: &str,
        _settlement: &DispatchSettlement,
    ) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl DatabaseAdapter for OneRow {
    // A test double: the session variables a read carries are accepted (#1115).
    fn applies_session_variables(&self) -> bool {
        true
    }

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
        request: &WriteRequest<'_>,
        gate: MutationRowGate<'_>,
    ) -> std::result::Result<Vec<HashMap<String, Value>>, FraiseQLError> {
        let rows = vec![self.0.clone()];
        gate(&rows)?;
        self.record(request);
        Ok(rows)
    }

    async fn execute_writes(
        &self,
        requests: &[WriteRequest<'_>],
        gate: BatchRowGate<'_>,
    ) -> std::result::Result<Vec<Vec<HashMap<String, Value>>>, FraiseQLError> {
        let mut all = Vec::new();
        for (index, request) in requests.iter().enumerate() {
            let rows = vec![self.0.clone()];
            gate(index, &rows)?;
            self.record(request);
            all.push(rows);
        }
        Ok(all)
    }

    fn dispatch_ledger(&self) -> Result<&dyn DispatchLedger> {
        match self.1 {
            Some(_) => Ok(self),
            None => Err(fraiseql_core::db::traits::dispatch_ledger_unsupported()),
        }
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
    (Executor::with_config(schema(), Arc::new(OneRow::plain(row)), config), recorder)
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
    let executor = Executor::with_config(
        schema(),
        Arc::new(OneRow::plain(success_row(Some("Order")))),
        config,
    );
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
    let executor = Executor::with_config(
        schema(),
        Arc::new(OneRow::plain(success_row(Some("Order")))),
        config,
    );
    let vars = json!({ "id": ORDER_ID });

    executor.execute(MUTATION, Some(&vars)).await.unwrap();
    dispatched_at(3, executor.execute(MUTATION, Some(&vars))).await.unwrap();
    assert_eq!(*depths.0.lock().unwrap(), [0, 3]);
}

/// An observer that records its dispatches (#1628): each committed `Order` calls for one,
/// durable or not, keyed by the entity id.
struct Staging {
    durable:   bool,
    recorded:  Mutex<Vec<Vec<PendingDispatch>>>,
    committed: Mutex<usize>,
}

impl Staging {
    fn new(durable: bool) -> Arc<Self> {
        Arc::new(Self {
            durable,
            recorded: Mutex::default(),
            committed: Mutex::default(),
        })
    }
}

fn dispatch_for(mutation: &CommittedMutation<'_>, durable: bool) -> PendingDispatch {
    PendingDispatch {
        source: "after:mutation".to_string(),
        ref_key: mutation.entity_id.unwrap_or_default().to_string(),
        function_name: "onOrder".to_string(),
        payload: mutation.entity.clone(),
        idempotency_token: "token".to_string(),
        caller: None,
        tenant: None,
        durable,
    }
}

impl AfterMutationObserver for Staging {
    fn on_committed(&self, _mutation: &CommittedMutation<'_>) {
        *self.committed.lock().unwrap() += 1;
    }

    fn ledger_lease(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_mins(1))
    }

    fn pending(&self, mutation: &CommittedMutation<'_>) -> Vec<PendingDispatch> {
        vec![dispatch_for(mutation, self.durable)]
    }

    fn on_recorded(&self, _mutation: &CommittedMutation<'_>, staged: Vec<PendingDispatch>) {
        self.recorded.lock().unwrap().push(staged);
    }
}

fn ledgered(row: HashMap<String, Value>, observer: &Arc<Staging>) -> (Executor, Arc<OneRow>) {
    let writer = Arc::new(OneRow::ledgered(row));
    let config = RuntimeConfig::default().with_after_mutation_observer(observer.clone());
    (Executor::with_config(schema(), writer.clone(), config), writer)
}

/// R1: the dispatches a committed write calls for travel in its own write request, so the
/// adapter records them in its transaction, and the observer is handed exactly those.
#[tokio::test]
async fn a_committed_write_records_its_dispatches_in_its_own_request() {
    let observer = Staging::new(true);
    let (executor, writer) = ledgered(success_row(Some("Order")), &observer);
    executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await.unwrap();

    let entity = json!({ "id": ORDER_ID, "status": "shipped", "total": 42 });
    let expected = PendingDispatch {
        source:            "after:mutation".to_string(),
        ref_key:           ORDER_ID.to_string(),
        function_name:     "onOrder".to_string(),
        payload:           entity,
        idempotency_token: "token".to_string(),
        caller:            None,
        tenant:            None,
        durable:           true,
    };
    assert_eq!(writer.recorded(), [vec![expected.clone()]], "recorded in the write");
    assert_eq!(*observer.recorded.lock().unwrap(), [vec![expected]], "handed over");
    assert_eq!(*observer.committed.lock().unwrap(), 0, "on_recorded replaces on_committed");
}

/// R1 (G4): the cache wrapper is the one production adapter in front of PostgreSQL, and it
/// keeps no ledger of its own: it forwards the one it wraps, so a write through it still
/// records its dispatches, rather than being refused as an adapter without a ledger.
#[tokio::test]
async fn a_write_through_the_cache_wrapper_records_its_dispatches() {
    use fraiseql_core::cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache};

    let observer = Staging::new(true);
    let inner = OneRow::ledgered(success_row(Some("Order")));
    let probe = inner.clone();
    let cached = CachedDatabaseAdapter::new(
        inner,
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );
    let config = RuntimeConfig::default().with_after_mutation_observer(observer.clone());
    let executor = Executor::with_config(schema(), Arc::new(cached), config);

    let response = executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await.unwrap();

    assert!(response.get("errors").is_none(), "the write commits: {response}");
    assert_eq!(probe.recorded().len(), 1, "recorded through the wrapper");
    assert_eq!(observer.recorded.lock().unwrap().len(), 1, "and handed over");
}

/// R1: a failed write commits nothing, so it records nothing and hands nothing over.
#[tokio::test]
async fn a_failed_write_records_no_dispatch() {
    let observer = Staging::new(true);
    let (executor, writer) = ledgered(failure_row(), &observer);
    executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await.unwrap();

    assert_eq!(writer.recorded(), [Vec::<PendingDispatch>::new()]);
    assert!(observer.recorded.lock().unwrap().is_empty());
}

/// R1: a dry run rolls back, so nothing is handed over to run.
#[tokio::test]
async fn a_dry_run_hands_no_dispatch_over() {
    let observer = Staging::new(true);
    let writer = Arc::new(OneRow::ledgered(success_row(Some("Order"))));
    let mut config = RuntimeConfig::default().with_after_mutation_observer(observer.clone());
    config.dry_run_mutations = true;
    let executor = Executor::with_config(schema(), writer, config);
    executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await.unwrap();
    assert!(observer.recorded.lock().unwrap().is_empty());
}

/// R1, fail closed: a durable dispatch the writer cannot record refuses the write, rather
/// than running from memory only and being lost to a crash.
#[tokio::test]
async fn a_writer_without_a_ledger_refuses_a_durable_dispatch() {
    let observer = Staging::new(true);
    let config = RuntimeConfig::default().with_after_mutation_observer(observer.clone());
    let executor = Executor::with_config(
        schema(),
        Arc::new(OneRow::plain(success_row(Some("Order")))),
        config,
    );
    let response = executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await;
    let refused = match response {
        Err(error) => error.to_string(),
        Ok(body) => body.get("errors").map_or_default(ToString::to_string),
    };
    assert!(refused.contains("function-dispatch ledger"), "refused: {refused}");
    assert!(observer.recorded.lock().unwrap().is_empty());
}

/// A dispatch that asks for no record (`re_runnable`) needs no ledger.
#[tokio::test]
async fn a_non_durable_dispatch_needs_no_ledger() {
    let observer = Staging::new(false);
    let config = RuntimeConfig::default().with_after_mutation_observer(observer.clone());
    let executor = Executor::with_config(
        schema(),
        Arc::new(OneRow::plain(success_row(Some("Order")))),
        config,
    );
    executor.execute(MUTATION, Some(&json!({ "id": ORDER_ID }))).await.unwrap();
    assert_eq!(observer.recorded.lock().unwrap().len(), 1);
}

/// R1: every item of a batch records its own dispatches in its own request.
#[tokio::test]
async fn each_batch_item_records_its_own_dispatches() {
    let observer = Staging::new(true);
    let (executor, writer) = ledgered(success_row(Some("Order")), &observer);
    executor
        .execute_mutation_batch(
            "updateOrder",
            &[json!({ "id": ORDER_ID }), json!({ "id": ORDER_ID })],
            None,
        )
        .await
        .unwrap();
    let recorded = writer.recorded();
    assert_eq!(recorded.len(), 2, "one request per item: {recorded:?}");
    assert!(recorded.iter().all(|item| item.len() == 1), "{recorded:?}");
    assert_eq!(observer.recorded.lock().unwrap().len(), 2);
}
