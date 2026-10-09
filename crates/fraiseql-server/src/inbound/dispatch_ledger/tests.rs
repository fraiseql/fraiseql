#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

//! #1175 on PostgreSQL: the ledger is written with the spine row, a dispatch lost to a crash
//! is replayed by the sweep exactly once per lease, and a settled dispatch is never swept.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use fraiseql_functions::{InboundMessage, IngestSource};
use sqlx::PgPool;

use super::{
    DispatchOutcome, IngestDispatcher, LedgerSettings, record_pending_in_tx, settle, sweep_once,
};
use crate::inbound::spine::{Emitted, PostgresInboundSpine, emit_in_tx};

/// Records every dispatch it is asked for, holding each for `hold` before answering
/// `outcome`.
struct Recording {
    calls:   Mutex<Vec<(String, String)>>,
    hold:    Duration,
    outcome: DispatchOutcome,
}

impl Recording {
    fn new(outcome: DispatchOutcome) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            hold: Duration::ZERO,
            outcome,
        }
    }

    fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().unwrap().clone()
    }
}

impl IngestDispatcher for Recording {
    fn dispatch<'a>(
        &'a self,
        message: &'a InboundMessage,
        function_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = DispatchOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push((message.idempotency_key.clone(), function_name.to_string()));
            tokio::time::sleep(self.hold).await;
            self.outcome
        })
    }
}

fn message(key: &str) -> InboundMessage {
    InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        key,
        chrono::Utc::now(),
    )
}

async fn pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    PostgresInboundSpine::new(pool.clone()).init().await.unwrap();
    // Each test owns the ledger while it runs (`--test-threads=1` in the integration leg).
    sqlx::query("TRUNCATE _fraiseql_inbound_message CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    Some((pool, svc))
}

/// Persist `key` and record a dispatch of each of `functions`, leased for `lease`, in one
/// transaction, as a receiver does; the dispatch it would then run never runs (the crash).
async fn receive(pool: &PgPool, key: &str, functions: &[&str], lease: Duration) -> uuid::Uuid {
    let mut tx = pool.begin().await.unwrap();
    let Emitted::New(id) = emit_in_tx(&mut tx, &message(key)).await.unwrap() else {
        panic!("{key} is new");
    };
    let functions: Vec<String> = functions.iter().map(ToString::to_string).collect();
    record_pending_in_tx(&mut tx, id, &functions, lease).await.unwrap();
    tx.commit().await.unwrap();
    id
}

async fn states(pool: &PgPool) -> Vec<(String, String, i32)> {
    sqlx::query_as(
        "SELECT function_name, state, attempts FROM _fraiseql_inbound_dispatch \
         ORDER BY function_name",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

fn due_now() -> LedgerSettings {
    LedgerSettings {
        lease: Duration::from_mins(1),
        ..LedgerSettings::default()
    }
}

/// Cycle 1: the ledger rows commit or roll back with the spine row.
#[tokio::test]
async fn the_ledger_is_written_in_the_receivers_transaction() {
    let Some((pool, _svc)) = pool().await else {
        eprintln!("SKIP the_ledger_is_written_in_the_receivers_transaction: no postgres");
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let Emitted::New(id) = emit_in_tx(&mut tx, &message("rolled-back")).await.unwrap() else {
        panic!("new");
    };
    record_pending_in_tx(&mut tx, id, &["classify".to_string()], Duration::from_mins(1))
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let spine: i64 = sqlx::query_scalar("SELECT count(*) FROM _fraiseql_inbound_message")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        (spine, states(&pool).await),
        (0, vec![]),
        "a rolled-back receive records nothing"
    );

    receive(&pool, "committed", &["classify", "notify"], Duration::from_mins(1)).await;
    assert_eq!(
        states(&pool).await,
        [
            ("classify".into(), "pending".into(), 1),
            ("notify".into(), "pending".into(), 1)
        ],
        "one pending row per function, with the spine row"
    );
}

/// Cycle 2: a dispatch lost after the commit is dispatched by the sweep once its lease runs
/// out, and not before; once settled it is not dispatched again.
#[tokio::test]
async fn a_dispatch_lost_after_commit_is_replayed_once_its_lease_runs_out() {
    let Some((pool, _svc)) = pool().await else {
        return;
    };
    receive(&pool, "in-flight", &["classify"], Duration::from_mins(10)).await;
    let dispatcher = Recording::new(DispatchOutcome::Dispatched);
    assert_eq!(sweep_once(&pool, &dispatcher, due_now()).await.unwrap(), 0, "leased: in flight");

    receive(&pool, "lost", &["classify"], Duration::ZERO).await;
    assert_eq!(sweep_once(&pool, &dispatcher, due_now()).await.unwrap(), 1);
    assert_eq!(dispatcher.calls(), [("lost".to_string(), "classify".to_string())]);
    assert_eq!(sweep_once(&pool, &dispatcher, due_now()).await.unwrap(), 0, "settled");
    assert_eq!(dispatcher.calls().len(), 1, "dispatched exactly once");
    let lost = states(&pool).await;
    assert!(lost.contains(&("classify".into(), "dispatched".into(), 2)), "{lost:?}");
}

/// Cycle 3: two sweepers racing for one due row dispatch it once.
#[tokio::test]
async fn two_sweepers_dispatch_a_due_row_once() {
    let Some((pool, _svc)) = pool().await else {
        return;
    };
    receive(&pool, "contended", &["classify"], Duration::ZERO).await;
    let dispatcher = Recording {
        hold: Duration::from_millis(200),
        ..Recording::new(DispatchOutcome::Dispatched)
    };
    let (a, b) = tokio::join!(
        sweep_once(&pool, &dispatcher, due_now()),
        sweep_once(&pool, &dispatcher, due_now())
    );
    assert_eq!(a.unwrap() + b.unwrap(), 1, "one sweeper claimed it");
    assert_eq!(dispatcher.calls().len(), 1);
}

/// Cycle 4: `dead_lettered` is terminal like `dispatched`; an unsettled dispatch stays
/// `pending` and is swept again when its lease runs out.
#[tokio::test]
async fn terminal_dispatches_are_never_swept_and_unsettled_ones_are() {
    let Some((pool, _svc)) = pool().await else {
        return;
    };
    let id = receive(&pool, "settled", &["dlq", "ok", "unsettled"], Duration::ZERO).await;
    settle(&pool, id, "dlq", DispatchOutcome::DeadLettered).await.unwrap();
    settle(&pool, id, "ok", DispatchOutcome::Dispatched).await.unwrap();
    settle(&pool, id, "unsettled", DispatchOutcome::Unsettled).await.unwrap();

    let dispatcher = Arc::new(Recording::new(DispatchOutcome::Unsettled));
    let immediately = LedgerSettings {
        lease: Duration::ZERO,
        ..LedgerSettings::default()
    };
    assert_eq!(sweep_once(&pool, dispatcher.as_ref(), immediately).await.unwrap(), 1);
    assert_eq!(sweep_once(&pool, dispatcher.as_ref(), immediately).await.unwrap(), 1, "again");
    assert_eq!(
        dispatcher.calls(),
        [
            ("settled".to_string(), "unsettled".to_string()),
            ("settled".into(), "unsettled".into())
        ]
    );
    assert_eq!(
        states(&pool).await,
        [
            ("dlq".into(), "dead_lettered".into(), 1),
            ("ok".into(), "dispatched".into(), 1),
            ("unsettled".into(), "pending".into(), 3)
        ]
    );
}

/// The sweep re-dispatches a message read back from the spine's JSON, and its function must
/// see the same idempotency token the first dispatch handed it, or an idempotent handler
/// cannot tell the replay from a new message.
#[test]
fn a_replayed_dispatch_carries_the_first_dispatchs_token() {
    use crate::routes::after_mutation::{dispatch_idempotency_token, plan_after_ingest_dispatch};

    let definitions = vec![fraiseql_functions::FunctionDefinition {
        name:        "classify".to_string(),
        trigger:     "after:ingest".to_string(),
        runtime:     fraiseql_functions::RuntimeType::Wasm,
        timeout_ms:  None,
        run_as:      None,
        when:        Vec::new(),
        re_runnable: false,
        retry:       None,
    }];
    let module = fraiseql_functions::FunctionModule {
        name:        "classify".to_string(),
        source_hash: "test".to_string(),
        bytecode:    bytes::Bytes::new(),
        runtime:     fraiseql_functions::RuntimeType::Wasm,
    };
    let hooks = crate::subsystems::BeforeMutationHooks::new(
        fraiseql_functions::TriggerRegistry::load_from_definitions(&definitions).unwrap(),
        std::iter::once(("classify".to_string(), module)).collect(),
        Arc::new(fraiseql_functions::FunctionObserver::new()),
    );
    let mut original = message("evt_1175");
    original.payload = Some(serde_json::json!({ "amount": 4242, "note": "é" }));
    let replayed: InboundMessage =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();

    let token = |message: &InboundMessage| {
        let plan = plan_after_ingest_dispatch(&hooks, message).pop().expect("classify triggers");
        dispatch_idempotency_token(
            Some(b"server-key".as_slice()),
            fraiseql_observers::DispatchSource::AfterIngest,
            &plan.module.name,
            &plan.payload,
        )
    };
    assert_eq!(token(&original), token(&replayed));
}

/// A sweeper never waits on a row another transaction holds: it skips it
/// (`SKIP LOCKED`), so one stuck claim cannot stall every replica's sweep.
#[tokio::test]
async fn a_sweeper_skips_a_row_another_transaction_holds() {
    let Some((pool, _svc)) = pool().await else {
        return;
    };
    receive(&pool, "held", &["classify"], Duration::ZERO).await;
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM _fraiseql_inbound_dispatch FOR UPDATE")
        .execute(&mut *holder)
        .await
        .unwrap();
    let dispatcher = Recording::new(DispatchOutcome::Dispatched);
    let swept =
        tokio::time::timeout(Duration::from_secs(5), sweep_once(&pool, &dispatcher, due_now()))
            .await
            .expect("the sweep must not wait on the held row")
            .unwrap();
    assert_eq!(swept, 0, "the held row is skipped");
    holder.rollback().await.unwrap();
}

/// A claim renews the lease: the next sweep leaves the row alone until it runs out again,
/// so a replay still in flight is not replayed a second time.
#[tokio::test]
async fn a_claimed_dispatch_is_left_alone_while_its_renewed_lease_runs() {
    let Some((pool, _svc)) = pool().await else {
        return;
    };
    receive(&pool, "renewed", &["classify"], Duration::ZERO).await;
    let dispatcher = Recording::new(DispatchOutcome::Unsettled);
    let renewing = LedgerSettings {
        lease: Duration::from_mins(10),
        ..LedgerSettings::default()
    };
    assert_eq!(sweep_once(&pool, &dispatcher, renewing).await.unwrap(), 1);
    assert_eq!(sweep_once(&pool, &dispatcher, renewing).await.unwrap(), 0, "lease renewed");
    assert_eq!(dispatcher.calls().len(), 1);
}

/// Settling is once: a late outcome for a row already terminal changes nothing.
#[tokio::test]
async fn a_settled_dispatch_keeps_its_first_outcome() {
    let Some((pool, _svc)) = pool().await else {
        return;
    };
    let id = receive(&pool, "settled-once", &["classify"], Duration::from_mins(1)).await;
    settle(&pool, id, "classify", DispatchOutcome::DeadLettered).await.unwrap();
    settle(&pool, id, "classify", DispatchOutcome::Dispatched).await.unwrap();
    assert_eq!(states(&pool).await, [("classify".into(), "dead_lettered".into(), 1)]);
}

/// Both serve entry points start the sweep, through one method, where an inbound path
/// records dispatches: a database, a webhook route (or a polled mailbox) and function hooks.
/// Without a route there is nothing to sweep, and nothing starts. No database is needed: the
/// pool is lazy and the sweep is only spawned.
#[tokio::test]
async fn the_server_starts_the_sweep_where_an_inbound_path_records_dispatches() {
    use crate::{Server, server_config::ServerConfig};

    let definitions = vec![fraiseql_functions::FunctionDefinition {
        name:        "classify".to_string(),
        trigger:     "after:ingest".to_string(),
        runtime:     fraiseql_functions::RuntimeType::Wasm,
        timeout_ms:  None,
        run_as:      None,
        when:        Vec::new(),
        re_runnable: false,
        retry:       None,
    }];
    let hooks = Arc::new(crate::subsystems::BeforeMutationHooks::new(
        fraiseql_functions::TriggerRegistry::load_from_definitions(&definitions).unwrap(),
        std::collections::HashMap::new(),
        Arc::new(fraiseql_functions::FunctionObserver::new()),
    ));
    let executor = Arc::new(fraiseql_core::runtime::Executor::new(
        fraiseql_core::schema::CompiledSchema::new(),
        Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
    ));
    let mut state = crate::routes::graphql::AppState::new(executor);
    state.before_mutation_hooks = Some(hooks);

    let started = |webhooks: bool| {
        let state = state.clone();
        async move {
            let mut config = ServerConfig {
                cors_enabled: false,
                ..ServerConfig::default()
            };
            if webhooks {
                // The boot check wants the route's secret set, and the crate forbids the
                // `unsafe` that setting one takes: `PATH` is set in every test process, and
                // a GitHub-style HMAC route accepts any secret.
                config.webhooks =
                    toml::from_str("[github]\nprovider = \"github\"\nsecret_env = \"PATH\"\n")
                        .unwrap();
            }
            let pool = sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://nobody@127.0.0.1:1/never")
                .unwrap();
            let mut server = Box::pin(Server::new(
                config,
                fraiseql_core::schema::CompiledSchema::new(),
                Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
                Some(pool),
            ))
            .await
            .unwrap();
            let before = server.lifecycle_task_count();
            server.spawn_inbound_dispatch_sweep(&state);
            server.lifecycle_task_count() - before
        }
    };
    assert_eq!(started(true).await, 1, "a webhook route with hooks starts the sweep");
    assert_eq!(started(false).await, 0, "no inbound path, no sweep");
}
