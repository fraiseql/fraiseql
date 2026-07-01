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
        CompensationStatus, FederatedType, FederationMetadata, FederationMutationExecutor,
        KeyDirective, MutationType, PostgresSagaStore, Saga, SagaCompensator, SagaExecutor,
        SagaState, SagaStep, StepState,
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
            compensation_mutation: None,
            compensation_variables: None,
        }
    }

    /// Compensation metadata round-trips through the store: a step saved with a
    /// compensation mutation + variables reloads with both fields intact, and a
    /// step saved without them reloads as `None` (backwards-compatible with rows
    /// that predate the compensation columns).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn saga_step_compensation_metadata_round_trips() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;

        let saga_id = Uuid::new_v4();
        store.save_saga(&new_saga(saga_id)).await.unwrap();

        // Step WITH compensation metadata.
        let mut step_with = new_step(
            saga_id,
            0,
            MutationType::Create,
            &typename,
            json!({"id": "c1", "total": "10"}),
        );
        step_with.compensation_mutation = Some("undo_create_order".to_string());
        step_with.compensation_variables = Some(json!({"id": "c1"}));
        store.save_saga_step(&step_with).await.unwrap();

        // Step WITHOUT compensation metadata (backwards-compatible None round-trip).
        let step_without = new_step(
            saga_id,
            1,
            MutationType::Create,
            &typename,
            json!({"id": "c2", "total": "20"}),
        );
        store.save_saga_step(&step_without).await.unwrap();

        let reloaded_with = store.load_saga_step(step_with.id).await.unwrap().unwrap();
        assert_eq!(
            reloaded_with.compensation_mutation.as_deref(),
            Some("undo_create_order"),
            "compensation_mutation must round-trip"
        );
        assert_eq!(
            reloaded_with.compensation_variables,
            Some(json!({"id": "c1"})),
            "compensation_variables must round-trip"
        );

        let reloaded_without = store.load_saga_step(step_without.id).await.unwrap().unwrap();
        assert!(reloaded_without.compensation_mutation.is_none(), "absent compensation → None");
        assert!(reloaded_without.compensation_variables.is_none(), "absent compensation → None");

        // The list path (load_saga_steps) must carry the metadata too.
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        let listed = steps.iter().find(|s| s.id == step_with.id).unwrap();
        assert_eq!(listed.compensation_mutation.as_deref(), Some("undo_create_order"));

        store.delete_saga(saga_id).await.unwrap();
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

    /// A completed CREATE step is really rolled back by its registered inverse
    /// (`delete…`) compensation: the row is deleted, the step transitions
    /// `Compensated`, and the reported result is a real success (never fabricated).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn compensate_step_local_rolls_back_a_completed_step() {
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
        let mut step = new_step(
            saga_id,
            0,
            MutationType::Create,
            &typename,
            json!({"id": id_a, "total": "10"}),
        );
        // Inverse of a create is a delete; the name drives the mutation kind.
        step.compensation_mutation = Some(format!("delete{typename}"));
        step.compensation_variables = Some(json!({"id": id_a}));
        store.save_saga_step(&step).await.unwrap();

        // Run the forward create so the row actually exists, then mark Completed.
        let forward = SagaExecutor::with_store(Arc::clone(&store));
        let fwd = forward.execute_step_local(&executor, &step).await;
        assert!(fwd.success, "forward create must succeed: {fwd:?}");
        store.update_saga_step_state(step.id, &StepState::Completed).await.unwrap();

        // Compensate the single step.
        let compensator = SagaCompensator::with_store(Arc::clone(&store));
        let result = compensator.compensate_step_local(&executor, &step).await.unwrap();
        assert!(result.success, "compensation must succeed: {result:?}");
        assert_eq!(result.step_number, 1, "0-based order maps to 1-indexed step number");

        // The step is persisted Compensated and the row is really gone.
        let reloaded = store.load_saga_step(step.id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.state,
            StepState::Compensated,
            "a compensated step persists Compensated"
        );
        let table = typename.to_lowercase();
        let rows = PostgresAdapter::new(&url)
            .await
            .unwrap()
            .execute_raw_query(&format!("SELECT id FROM \"{table}\" WHERE id = '{id_a}'"))
            .await
            .unwrap();
        assert!(rows.is_empty(), "the inverse delete really removed the row (not fabricated)");

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Reverse-order rollback: after a saga fails partway, only the *completed*
    /// step is compensated; the failed step and the never-run step are skipped, and
    /// the saga ends `Compensated`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn compensate_saga_local_rolls_back_completed_steps_in_reverse() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store.save_saga(&new_saga(saga_id)).await.unwrap();

        // Step 0 creates a row (completes); step 1 updates a missing row (fails);
        // step 2 never runs. Every step carries a delete compensation.
        let id_a = format!("a-{}", Uuid::new_v4());
        let id_c = format!("never-{}", Uuid::new_v4());
        let mut step0 = new_step(
            saga_id,
            0,
            MutationType::Create,
            &typename,
            json!({"id": id_a, "total": "1"}),
        );
        step0.compensation_mutation = Some(format!("delete{typename}"));
        step0.compensation_variables = Some(json!({"id": id_a}));
        let step1 = new_step(
            saga_id,
            1,
            MutationType::Update,
            &typename,
            json!({"id": "missing", "total": "9"}),
        );
        let step2 = new_step(
            saga_id,
            2,
            MutationType::Create,
            &typename,
            json!({"id": id_c, "total": "3"}),
        );
        store.save_saga_step(&step0).await.unwrap();
        store.save_saga_step(&step1).await.unwrap();
        store.save_saga_step(&step2).await.unwrap();

        // Forward: step0 Completed, step1 Failed, step2 Pending, saga Failed.
        SagaExecutor::with_store(Arc::clone(&store))
            .execute_saga_local(saga_id, &executor)
            .await
            .unwrap();

        // Compensate: only the completed step0 rolls back.
        let comp = SagaCompensator::with_store(Arc::clone(&store))
            .compensate_saga_local(saga_id, &executor)
            .await
            .unwrap();
        assert_eq!(comp.status, CompensationStatus::Compensated, "all completed steps rolled back");
        assert_eq!(
            comp.step_results.len(),
            1,
            "only the one completed step is compensated: {comp:?}"
        );
        assert!(comp.step_results[0].success, "the completed step compensated: {comp:?}");
        assert!(comp.failed_steps.is_empty(), "no compensation failures: {comp:?}");

        let mut steps = store.load_saga_steps(saga_id).await.unwrap();
        steps.sort_by_key(|s| s.order);
        assert_eq!(steps[0].state, StepState::Compensated, "completed step compensated");
        assert_eq!(steps[1].state, StepState::Failed, "failed step untouched");
        assert_eq!(steps[2].state, StepState::Pending, "never-run step untouched");
        assert_eq!(
            store.load_saga(saga_id).await.unwrap().unwrap().state,
            SagaState::Compensated,
            "saga reaches Compensated"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Best-effort: a completed step with *no* registered compensation cannot be
    /// rolled back — the saga is reported `PartiallyCompensated` and stays `Failed`,
    /// never marked `Compensated` having undone nothing (audit H33).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn compensate_saga_local_partial_when_compensation_unregistered() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store.save_saga(&new_saga(saga_id)).await.unwrap();

        // A single step that completes but has no compensation registered.
        let id_a = format!("a-{}", Uuid::new_v4());
        let step = new_step(
            saga_id,
            0,
            MutationType::Create,
            &typename,
            json!({"id": id_a, "total": "5"}),
        );
        store.save_saga_step(&step).await.unwrap();

        SagaExecutor::with_store(Arc::clone(&store))
            .execute_saga_local(saga_id, &executor)
            .await
            .unwrap();

        let comp = SagaCompensator::with_store(Arc::clone(&store))
            .compensate_saga_local(saga_id, &executor)
            .await
            .unwrap();
        assert_eq!(
            comp.status,
            CompensationStatus::PartiallyCompensated,
            "an uncompensatable completed step yields a partial result: {comp:?}"
        );
        assert_eq!(comp.failed_steps, vec![1], "step 1 could not be compensated");
        assert!(comp.error.is_some(), "a partial compensation carries an error summary");

        // The saga stays Failed (never fabricated Compensated) and the completed
        // step stays Completed (its forward work was not undone).
        assert_eq!(store.load_saga(saga_id).await.unwrap().unwrap().state, SagaState::Failed);
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        assert_eq!(steps[0].state, StepState::Completed, "an uncompensated step stays Completed");

        store.delete_saga(saga_id).await.unwrap();
    }
}

// ── Wired recovery loop end-to-end against real PostgreSQL (unstable-saga) ─────
//
// Ignored by default — these require a live PostgreSQL reachable via
// `DATABASE_URL` (the saga store is Postgres-only). The CI integration leg runs
// them with `--features unstable-saga --include-ignored` against the bound
// service. They exercise the additive `run_iteration_local` /
// `start_background_loop_local` wired methods; the fail-loud `run_iteration` /
// `start_background_loop` entry points are unchanged.

#[cfg(feature = "unstable-saga")]
mod recovery_pg {
    use std::{sync::Arc, time::Duration};

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        FederatedType, FederationMetadata, FederationMutationExecutor, KeyDirective, MutationType,
        PostgresSagaStore, RecoveryConfig, Saga, SagaRecoveryManager, SagaState, SagaStep,
        StepState,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    /// A unique, all-lowercase, identifier-safe entity type name per test run, so
    /// the replayed Create mutations target a table isolated from other tests.
    fn unique_typename() -> String {
        format!("sagarec{}", Uuid::new_v4().simple())
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

    /// A saga persisted directly in `Executing` state — the fingerprint of a crash
    /// mid-execution: the process died after marking the saga running but before
    /// driving its steps to a terminal state.
    fn stuck_saga(id: Uuid) -> Saga {
        Saga {
            id,
            state: SagaState::Executing,
            created_at: chrono::Utc::now(),
            completed_at: None,
            metadata: None,
        }
    }

    fn pending_step(
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
            compensation_mutation: None,
            compensation_variables: None,
        }
    }

    /// A single recovery tick re-drives a crash-interrupted (`Executing`) saga to a
    /// terminal state, records a recovery attempt, and counts the work in `stats`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn run_iteration_local_drives_stuck_saga_to_terminal_state() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // A saga left Executing with a single un-run step whose Create will succeed.
        let saga_id = Uuid::new_v4();
        store.save_saga(&stuck_saga(saga_id)).await.unwrap();
        let entity_id = format!("r-{}", Uuid::new_v4());
        store
            .save_saga_step(&pending_step(
                saga_id,
                0,
                MutationType::Create,
                &typename,
                json!({"id": entity_id, "total": "10"}),
            ))
            .await
            .unwrap();

        let manager = SagaRecoveryManager::new(Arc::clone(&store), RecoveryConfig::default());

        // `find_stuck_sagas` scans globally, so a recovery record may already exist
        // from unrelated rows — measure the delta this tick creates.
        let recovery_before = store.recovery_count().await.unwrap();
        manager.run_iteration_local(&executor).await.unwrap();

        // The saga is no longer stuck: its steps were replayed to completion.
        assert_eq!(
            store.load_saga(saga_id).await.unwrap().unwrap().state,
            SagaState::Completed,
            "recovery must drive the stuck saga to a terminal state, never leave it Executing"
        );
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        assert!(
            steps.iter().all(|s| s.state == StepState::Completed),
            "the replayed step is persisted Completed: {steps:?}"
        );

        // A recovery record was written for the audit trail.
        assert!(
            store.recovery_count().await.unwrap() > recovery_before,
            "a recovery attempt must be recorded for the stuck saga"
        );

        // Stats reflect exactly one iteration and at least our saga processed.
        let stats = manager.get_stats();
        assert_eq!(stats.iterations, 1, "one run_iteration_local call is one iteration");
        assert!(stats.sagas_processed >= 1, "the stuck saga was processed: {stats:?}");
        assert!(
            stats.executing_sagas_found >= 1,
            "the stuck (Executing) saga was found: {stats:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Resilience: one saga's failing replay must not abort the iteration — a second
    /// stuck saga in the same tick is still driven to a terminal state.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn run_iteration_local_continues_past_a_failing_saga() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // Saga 1: its only step Updates a row that does not exist → replay fails, the
        // saga ends Failed (a bad per-saga outcome, but not an infrastructure error).
        let failing_id = Uuid::new_v4();
        store.save_saga(&stuck_saga(failing_id)).await.unwrap();
        store
            .save_saga_step(&pending_step(
                failing_id,
                0,
                MutationType::Update,
                &typename,
                json!({"id": "missing", "total": "9"}),
            ))
            .await
            .unwrap();

        // Saga 2: a clean Create that will complete on replay.
        let ok_id = Uuid::new_v4();
        store.save_saga(&stuck_saga(ok_id)).await.unwrap();
        let entity_id = format!("ok-{}", Uuid::new_v4());
        store
            .save_saga_step(&pending_step(
                ok_id,
                0,
                MutationType::Create,
                &typename,
                json!({"id": entity_id, "total": "5"}),
            ))
            .await
            .unwrap();

        let manager = SagaRecoveryManager::new(Arc::clone(&store), RecoveryConfig::default());
        manager.run_iteration_local(&executor).await.unwrap();

        // Both sagas were driven out of Executing; the failing one did not stop the
        // second from being processed.
        assert_eq!(
            store.load_saga(failing_id).await.unwrap().unwrap().state,
            SagaState::Failed,
            "the failing saga reaches a terminal Failed state"
        );
        assert_eq!(
            store.load_saga(ok_id).await.unwrap().unwrap().state,
            SagaState::Completed,
            "the second saga is still processed despite the first one's failure"
        );
        assert!(
            manager.get_stats().sagas_processed >= 2,
            "both stuck sagas were processed in one iteration: {:?}",
            manager.get_stats()
        );

        store.delete_saga(failing_id).await.unwrap();
        store.delete_saga(ok_id).await.unwrap();
    }

    /// The background loop starts, recovers a stuck saga across a couple of ticks,
    /// and stops cleanly when asked.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn start_background_loop_local_recovers_then_stops() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // Seed the stuck saga before starting so the first tick can pick it up.
        let saga_id = Uuid::new_v4();
        store.save_saga(&stuck_saga(saga_id)).await.unwrap();
        let entity_id = format!("bg-{}", Uuid::new_v4());
        store
            .save_saga_step(&pending_step(
                saga_id,
                0,
                MutationType::Create,
                &typename,
                json!({"id": entity_id, "total": "7"}),
            ))
            .await
            .unwrap();

        // A short interval keeps the test fast (default is 5s).
        let config = RecoveryConfig {
            check_interval:          Duration::from_millis(150),
            max_sagas_per_iteration: 50,
            stale_age_hours:         24,
        };
        let manager = Arc::new(SagaRecoveryManager::new(Arc::clone(&store), config));

        Arc::clone(&manager)
            .start_background_loop_local(Arc::new(executor))
            .await
            .unwrap();
        assert!(manager.is_running(), "the loop reports running after start");

        // Wait a couple of ticks for at least one iteration to fire.
        tokio::time::sleep(config.check_interval * 3).await;

        manager.stop_background_loop().await.unwrap();
        assert!(!manager.is_running(), "the loop reports stopped after stop");

        assert!(
            manager.get_stats().iterations >= 1,
            "the background loop ran at least one iteration: {:?}",
            manager.get_stats()
        );
        assert_eq!(
            store.load_saga(saga_id).await.unwrap().unwrap().state,
            SagaState::Completed,
            "the background loop drove the stuck saga to a terminal state"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// The CAS guard refuses a second concurrent loop: a start while already running
    /// fails loud rather than spawning a duplicate.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn start_background_loop_local_rejects_double_start() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let config = RecoveryConfig {
            check_interval:          Duration::from_millis(150),
            max_sagas_per_iteration: 50,
            stale_age_hours:         24,
        };
        let manager = Arc::new(SagaRecoveryManager::new(Arc::clone(&store), config));
        let executor = Arc::new(executor);

        Arc::clone(&manager)
            .start_background_loop_local(Arc::clone(&executor))
            .await
            .unwrap();

        // A second start while running must fail loud, not spawn a duplicate loop.
        let second = Arc::clone(&manager).start_background_loop_local(Arc::clone(&executor)).await;
        assert!(second.is_err(), "a double start must be rejected: {second:?}");

        // Clean up: stop the single running loop.
        manager.stop_background_loop().await.unwrap();
        assert!(!manager.is_running());
    }
}

// ── Wired coordinator end-to-end against real PostgreSQL (unstable-saga) ──────
//
// Ignored by default — these require a live PostgreSQL reachable via
// `DATABASE_URL` (the saga store is Postgres-only). The CI integration leg runs
// them with `--features unstable-saga --include-ignored` against the bound
// service. They exercise the additive `WiredSagaCoordinator`, which ties forward
// execution + compensation into one handle; the loud-fail `SagaCoordinator`
// entry points are unchanged and covered by its own unit tests.

#[cfg(feature = "unstable-saga")]
mod coordinator_pg {
    use std::sync::Arc;

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        CompensationStrategy, FederatedType, FederationMetadata, FederationMutationExecutor,
        KeyDirective, PostgresSagaStore, SagaCoordinatorStep, SagaState, SagaStoreError, StepState,
        WiredSagaCoordinator,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    /// A unique, all-lowercase, identifier-safe entity type name per test run.
    /// `execute_local_mutation` targets `lowercase(typename)` as the table, so a
    /// unique name isolates each test from other tests sharing the database.
    fn unique_typename() -> String {
        format!("sagacoord{}", Uuid::new_v4().simple())
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

    /// A create step whose forward `create…` mutation writes a row and whose
    /// `delete…` compensation undoes it.
    fn create_step(number: u32, typename: &str, id: &str, total: &str) -> SagaCoordinatorStep {
        SagaCoordinatorStep::new(
            number,
            "orders",
            typename,
            format!("create{typename}"),
            json!({"id": id, "total": total}),
            format!("delete{typename}"),
            json!({"id": id}),
        )
    }

    /// Cycle 1: `create_saga` persists the saga (`Pending`) and every step, carrying
    /// the compensation metadata through to the store.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn create_saga_persists_saga_and_steps() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let coordinator =
            WiredSagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

        let id_a = format!("a-{}", Uuid::new_v4());
        let id_b = format!("b-{}", Uuid::new_v4());
        let id_c = format!("c-{}", Uuid::new_v4());
        let steps = vec![
            create_step(1, &typename, &id_a, "1"),
            create_step(2, &typename, &id_b, "2"),
            create_step(3, &typename, &id_c, "3"),
        ];

        let saga_id = coordinator.create_saga(steps).await.unwrap();

        // The saga is persisted Pending.
        assert_eq!(
            store.load_saga(saga_id).await.unwrap().unwrap().state,
            SagaState::Pending,
            "a created saga starts Pending"
        );

        // All three steps are loadable (ordered) with compensation metadata intact.
        let loaded = store.load_saga_steps(saga_id).await.unwrap();
        assert_eq!(loaded.len(), 3, "every step persisted: {loaded:?}");
        let expected_comp = format!("delete{typename}");
        assert_eq!(
            loaded[0].compensation_mutation.as_deref(),
            Some(expected_comp.as_str()),
            "compensation_mutation round-trips through create_saga"
        );
        assert!(
            loaded.iter().all(|s| s.state == StepState::Pending),
            "created steps start Pending: {loaded:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Cycle 1 (validation): `create_saga` rejects empty and out-of-order steps
    /// before touching the store.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn create_saga_rejects_invalid_steps() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let coordinator =
            WiredSagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

        let empty = coordinator.create_saga(vec![]).await;
        assert!(
            matches!(empty, Err(SagaStoreError::Database(_))),
            "empty steps rejected before persistence: {empty:?}"
        );

        // A single step numbered 2 is out of sequence (must start at 1).
        let out_of_order = coordinator.create_saga(vec![create_step(2, &typename, "x", "1")]).await;
        assert!(
            matches!(out_of_order, Err(SagaStoreError::Database(_))),
            "non-sequential steps rejected: {out_of_order:?}"
        );
    }

    /// Cycle 2: `execute_saga` happy path — every step's real mutation runs, the
    /// saga completes with no compensation, and status/result reflect completion.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn execute_saga_completes_all_steps() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let coordinator =
            WiredSagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

        let id_a = format!("a-{}", Uuid::new_v4());
        let id_b = format!("b-{}", Uuid::new_v4());
        let saga_id = coordinator
            .create_saga(vec![
                create_step(1, &typename, &id_a, "10"),
                create_step(2, &typename, &id_b, "20"),
            ])
            .await
            .unwrap();

        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();
        assert_eq!(result.state, SagaState::Completed, "all steps succeed: {result:?}");
        assert!(!result.compensated, "no compensation on success: {result:?}");
        assert_eq!(result.completed_steps, 2, "both steps completed: {result:?}");

        let status = coordinator.get_saga_status(saga_id).await.unwrap();
        assert!(
            (status.progress_percentage - 100.0).abs() < 1e-9,
            "a fully-executed saga is 100% complete: {status:?}"
        );

        let final_result = coordinator.get_saga_result(saga_id).await.unwrap();
        assert_eq!(final_result.completed_steps, 2, "result reports two completed steps");

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Cycle 3: `execute_saga` failure path — a failing step fails the saga and, under
    /// `Automatic`, the completed steps are really rolled back.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn execute_saga_failure_triggers_compensation() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let coordinator =
            WiredSagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

        let id_a = format!("a-{}", Uuid::new_v4());
        let id_b = format!("b-{}", Uuid::new_v4());
        // Steps 1–2 create rows (each with a delete compensation); step 3 updates a
        // row that does not exist → fails at execution, triggering rollback. Step 3
        // has no compensation (it never completes, so nothing to undo).
        let step3 = SagaCoordinatorStep::new(
            3,
            "orders",
            typename.as_str(),
            format!("update{typename}"),
            json!({"id": "missing", "total": "9"}),
            String::new(),
            json!({}),
        );
        let saga_id = coordinator
            .create_saga(vec![
                create_step(1, &typename, &id_a, "1"),
                create_step(2, &typename, &id_b, "2"),
                step3,
            ])
            .await
            .unwrap();

        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();
        assert_eq!(result.state, SagaState::Failed, "a failed step fails the saga: {result:?}");
        assert!(result.compensated, "Automatic strategy rolls back on failure: {result:?}");

        let mut steps = store.load_saga_steps(saga_id).await.unwrap();
        steps.sort_by_key(|s| s.order);
        assert_eq!(steps[0].state, StepState::Compensated, "step 1 rolled back");
        assert_eq!(steps[1].state, StepState::Compensated, "step 2 rolled back");
        assert_eq!(
            steps[2].state,
            StepState::Failed,
            "the failed step never completed → not compensated"
        );

        // The compensations really removed the created rows (not fabricated).
        let table = typename.to_lowercase();
        let rows = PostgresAdapter::new(&url)
            .await
            .unwrap()
            .execute_raw_query(&format!(
                "SELECT id FROM \"{table}\" WHERE id IN ('{id_a}', '{id_b}')"
            ))
            .await
            .unwrap();
        assert!(rows.is_empty(), "both created rows were rolled back: {rows:?}");

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Cycle 4: `cancel_saga` on a `Pending` saga + `list_in_flight_sagas` membership.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn cancel_pending_saga_and_list_in_flight() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let coordinator =
            WiredSagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

        let id_a = format!("a-{}", Uuid::new_v4());
        let id_b = format!("b-{}", Uuid::new_v4());
        let saga_id = coordinator
            .create_saga(vec![
                create_step(1, &typename, &id_a, "1"),
                create_step(2, &typename, &id_b, "2"),
            ])
            .await
            .unwrap();

        // A never-executed saga is in-flight (Pending).
        let in_flight = coordinator.list_in_flight_sagas().await.unwrap();
        assert!(in_flight.contains(&saga_id), "a Pending saga is in-flight: {in_flight:?}");

        // Cancel it: nothing completed → nothing to compensate.
        let result = coordinator.cancel_saga(saga_id, &executor).await.unwrap();
        assert_eq!(result.state, SagaState::Cancelled, "cancel yields Cancelled: {result:?}");

        // No longer in-flight.
        let after = coordinator.list_in_flight_sagas().await.unwrap();
        assert!(!after.contains(&saga_id), "a cancelled saga is not in-flight: {after:?}");

        // Cancelling an already-terminal saga fails loud.
        let second = coordinator.cancel_saga(saga_id, &executor).await;
        assert!(
            matches!(second, Err(SagaStoreError::InvalidStateTransition { .. })),
            "a second cancel on a terminal saga is rejected: {second:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }
}
