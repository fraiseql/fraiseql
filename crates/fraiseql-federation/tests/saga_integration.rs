//! Integration tests for `SagaExecutor` and `SagaCompensator`.
//!
//! Distributed saga execution/compensation is unwired (audit H32/H33): the
//! previous bodies fabricated success and persisted it. Every execution entry
//! point now fails loud with `SagaStoreError::NotImplemented`. These tests pin
//! that contract from outside the crate. Real execution is tracked in
//! <https://github.com/fraiseql/fraiseql/issues/429>.

#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_federation::{SagaCompensator, SagaExecutor, saga_store::SagaStoreError};
use serde_json::json;
use uuid::Uuid;

// ── Forward phase fails loud ─────────────────────────────────────────────────

#[tokio::test]
async fn test_execute_step_fails_loud() {
    let executor = SagaExecutor::new();
    let result = executor
        .execute_step(Uuid::new_v4(), 1, "createOrder", &json!({"id": "o1"}), "orders")
        .await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step must fail loud, never fabricate a Completed step: {result:?}"
    );
}

#[tokio::test]
async fn test_execute_saga_fails_loud() {
    let executor = SagaExecutor::new();
    let result = executor.execute_saga(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_saga must fail loud, never return fabricated results: {result:?}"
    );
}

#[tokio::test]
async fn test_get_execution_state_fails_loud() {
    let executor = SagaExecutor::new();
    let result = executor.get_execution_state(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "get_execution_state must fail loud: {result:?}"
    );
}

// ── Compensation phase fails loud ────────────────────────────────────────────

#[tokio::test]
async fn test_compensate_saga_fails_loud() {
    let compensator = SagaCompensator::new();
    let result = compensator.compensate_saga(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "compensate_saga must fail loud, never mark a saga Compensated having undone nothing: {result:?}"
    );
}

#[tokio::test]
async fn test_compensation_status_query_no_store_is_none() {
    // The status *query* is read-only and never persists: with no store and no
    // compensation ever written it honestly reports "unknown" (None).
    let compensator = SagaCompensator::new();
    let status = compensator.get_compensation_status(Uuid::new_v4()).await.unwrap();
    assert!(status.is_none(), "no-store compensation status must return None");
}

// ── Wired forward phase end-to-end against real PostgreSQL (unstable-saga) ────
//
// Ignored by default — these require a live PostgreSQL reachable via
// `DATABASE_URL` (the saga store is Postgres-only). The CI integration leg runs
// them with `--features unstable-saga --include-ignored` against the bound
// service. They exercise the additive `execute_saga_local` / `execution_state`
// wired methods; the fail-loud entry points above are unchanged.

#[cfg(feature = "unstable-saga")]
mod wired_pg {
    use std::sync::Arc;

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        FederatedType, FederationMetadata, FederationMutationExecutor, KeyDirective, MutationType,
        PostgresSagaStore, Saga, SagaExecutor, SagaState, SagaStep, StepState,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    /// A unique, all-lowercase, identifier-safe entity type name per test run.
    /// `execute_local_mutation` targets `lowercase(typename)` as the table, so a
    /// unique name isolates each test from pre-existing fixtures and from other
    /// tests sharing the database.
    fn unique_typename() -> String {
        format!("sagafwd{}", Uuid::new_v4().simple())
    }

    fn entity_metadata(typename: &str) -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types: vec![FederatedType {
                name:                typename.to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:          false,
                external_fields:     Vec::new(),
                shareable_fields:    Vec::new(),
                inaccessible_fields: Vec::new(),
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            }],
            remote_subscription_fields: std::collections::HashMap::new(),
        }
    }

    /// Build a saga store + a mutation executor over a freshly-created entity
    /// table named after `typename`.
    async fn setup(
        url: &str,
        typename: &str,
    ) -> (PostgresSagaStore, FederationMutationExecutor<PostgresAdapter>) {
        let store = PostgresSagaStore::new(url).await.unwrap();
        store.migrate_schema().await.unwrap();
        let adapter = PostgresAdapter::new(url).await.unwrap();
        let table = typename.to_lowercase();
        adapter
            .execute_raw_query(&format!("DROP TABLE IF EXISTS \"{table}\""))
            .await
            .unwrap();
        adapter
            .execute_raw_query(&format!(
                "CREATE TABLE \"{table}\" (id TEXT PRIMARY KEY, total TEXT)"
            ))
            .await
            .unwrap();
        let executor =
            FederationMutationExecutor::new(Arc::new(adapter), entity_metadata(typename), false);
        (store, executor)
    }

    fn new_saga(id: Uuid) -> Saga {
        Saga {
            id,
            state: SagaState::Pending,
            created_at: chrono::Utc::now(),
            completed_at: None,
            metadata: None,
        }
    }

    fn new_step(
        saga_id: Uuid,
        order: usize,
        mt: MutationType,
        typename: &str,
        variables: serde_json::Value,
    ) -> SagaStep {
        SagaStep {
            id: Uuid::new_v4(),
            saga_id,
            order,
            subgraph: "orders".to_string(),
            mutation_type: mt,
            typename: typename.to_string(),
            variables,
            state: StepState::Pending,
            result: None,
            started_at: None,
            completed_at: None,
        }
    }

    /// Happy path: every step's real mutation runs, the saga is persisted
    /// `Completed`, and each step is persisted `Completed` with its read-back
    /// result.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn execute_saga_local_completes_all_steps_and_persists_state() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store.save_saga(&new_saga(saga_id)).await.unwrap();
        let id_a = format!("a-{}", Uuid::new_v4());
        let id_b = format!("b-{}", Uuid::new_v4());
        store
            .save_saga_step(&new_step(
                saga_id,
                0,
                MutationType::Create,
                &typename,
                json!({"id": id_a, "total": "10"}),
            ))
            .await
            .unwrap();
        store
            .save_saga_step(&new_step(
                saga_id,
                1,
                MutationType::Create,
                &typename,
                json!({"id": id_b, "total": "20"}),
            ))
            .await
            .unwrap();

        let results = SagaExecutor::with_store(Arc::clone(&store))
            .execute_saga_local(saga_id, &executor)
            .await
            .unwrap();

        assert_eq!(results.len(), 2, "both steps executed: {results:?}");
        assert!(results.iter().all(|r| r.success), "every step succeeded: {results:?}");

        // Persisted saga + step state reflects real completion.
        assert_eq!(store.load_saga(saga_id).await.unwrap().unwrap().state, SagaState::Completed);
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        assert!(
            steps.iter().all(|s| s.state == StepState::Completed),
            "all steps persisted Completed: {steps:?}"
        );
        assert!(
            steps.iter().all(|s| s.result.is_some()),
            "each completed step persisted its read-back result: {steps:?}"
        );
    }

    /// Failure path: a failed step stops the saga, persists a real `Failed`
    /// transition (never a fabricated `Completed`), and leaves later steps
    /// unexecuted.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn execute_saga_local_stops_and_fails_on_step_error() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store.save_saga(&new_saga(saga_id)).await.unwrap();
        let id_a = format!("ok-{}", Uuid::new_v4());
        let id_c = format!("never-{}", Uuid::new_v4());
        // Step 1 creates a row; step 2 updates a row that does not exist (→ fail);
        // step 3 must never run.
        store
            .save_saga_step(&new_step(
                saga_id,
                0,
                MutationType::Create,
                &typename,
                json!({"id": id_a, "total": "1"}),
            ))
            .await
            .unwrap();
        store
            .save_saga_step(&new_step(
                saga_id,
                1,
                MutationType::Update,
                &typename,
                json!({"id": "missing", "total": "9"}),
            ))
            .await
            .unwrap();
        store
            .save_saga_step(&new_step(
                saga_id,
                2,
                MutationType::Create,
                &typename,
                json!({"id": id_c, "total": "3"}),
            ))
            .await
            .unwrap();

        let results = SagaExecutor::with_store(Arc::clone(&store))
            .execute_saga_local(saga_id, &executor)
            .await
            .unwrap();

        assert_eq!(results.len(), 2, "execution stops after the failing step: {results:?}");
        assert!(results[0].success, "step 1 succeeded: {:?}", results[0]);
        assert!(!results[1].success, "step 2 failed: {:?}", results[1]);

        assert_eq!(
            store.load_saga(saga_id).await.unwrap().unwrap().state,
            SagaState::Failed,
            "a failed step must mark the saga Failed, never Completed"
        );
        let mut steps = store.load_saga_steps(saga_id).await.unwrap();
        steps.sort_by_key(|s| s.order);
        assert_eq!(steps[0].state, StepState::Completed);
        assert_eq!(steps[1].state, StepState::Failed);
        assert_eq!(steps[2].state, StepState::Pending, "the step after the failure never executed");

        // The execution-state view reflects the persisted reality.
        let state = SagaExecutor::with_store(Arc::clone(&store))
            .execution_state(saga_id)
            .await
            .unwrap();
        assert!(state.failed, "execution state reports failure");
        assert_eq!(state.completed_steps, 1, "exactly one step completed");
        assert_eq!(state.total_steps, 3);
    }
}
