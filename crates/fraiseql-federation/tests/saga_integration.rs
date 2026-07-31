//! Integration tests for the saga subsystem against real PostgreSQL.
//!
//! These exercise `SagaCoordinator`, `SagaExecutor`, `SagaCompensator`, and
//! `SagaRecoveryManager` end-to-end under the `saga` feature. They are `#[ignore]`d
//! by default — they require a live PostgreSQL reachable via `DATABASE_URL` (the
//! saga store is Postgres-only). The CI integration leg runs them with
//! `--features saga,test-utils --include-ignored`.

#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

// ── Forward phase end-to-end against real PostgreSQL (saga) ────
//
// Ignored by default — these require a live PostgreSQL reachable via
// `DATABASE_URL` (the saga store is Postgres-only). The CI integration leg runs
// them with `--features saga --include-ignored` against the bound
// service. They exercise the additive `execute_saga` / `execution_state`
// wired methods; the fail-loud entry points above are unchanged.

#[cfg(feature = "saga")]
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
            remote: false,
            mutation_type: mt,
            mutation_name: None,
            typename: typename.to_string(),
            variables,
            state: StepState::Pending,
            result: None,
            started_at: None,
            completed_at: None,
            compensation_mutation: None,
            compensation_variables: None,
            required_fields: Vec::new(),
            compensation_error: None,
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

    /// The full mutation name round-trips through the store: a step saved with a
    /// `mutation_name` reloads with it intact (both the single-load and list
    /// paths), and a step saved without one reloads as `None` — backwards
    /// compatible with rows that predate the `mutation_name` column (#429
    /// hardening: full remote mutation-name persistence). `setup` runs
    /// `migrate_schema`, so this also exercises the idempotent ALTER.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn saga_step_mutation_name_round_trips() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;

        let saga_id = Uuid::new_v4();
        store.save_saga(&new_saga(saga_id)).await.unwrap();

        // Step WITH a full mutation name.
        let mut named = new_step(saga_id, 0, MutationType::Create, &typename, json!({"id": "n1"}));
        named.mutation_name = Some("createOrder".to_string());
        store.save_saga_step(&named).await.unwrap();

        // Step WITHOUT a mutation name (backwards-compatible None round-trip).
        let unnamed = new_step(saga_id, 1, MutationType::Create, &typename, json!({"id": "n2"}));
        store.save_saga_step(&unnamed).await.unwrap();

        let reloaded_named = store.load_saga_step(named.id).await.unwrap().unwrap();
        assert_eq!(
            reloaded_named.mutation_name.as_deref(),
            Some("createOrder"),
            "mutation_name must round-trip through load_saga_step"
        );
        let reloaded_unnamed = store.load_saga_step(unnamed.id).await.unwrap().unwrap();
        assert!(reloaded_unnamed.mutation_name.is_none(), "absent mutation_name → None");

        // The list path (load_saga_steps) must carry the name too.
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        let listed = steps.iter().find(|s| s.id == named.id).unwrap();
        assert_eq!(listed.mutation_name.as_deref(), Some("createOrder"));

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Happy path: every step's real mutation runs, the saga is persisted
    /// `Completed`, and each step is persisted `Completed` with its read-back
    /// result.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn execute_saga_completes_all_steps_and_persists_state() {
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
            .execute_saga(saga_id, &executor, &std::collections::HashMap::new(), None, None)
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
    async fn execute_saga_stops_and_fails_on_step_error() {
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
            .execute_saga(saga_id, &executor, &std::collections::HashMap::new(), None, None)
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
    async fn compensate_step_rolls_back_a_completed_step() {
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

        // Run the forward create so the row actually exists, then mark Completed
        // via the legal Pending → Executing → Completed chain (the store rejects
        // transition shortcuts since #744).
        let forward = SagaExecutor::with_store(Arc::clone(&store));
        let fwd = forward.execute_step(&executor, &step).await;
        assert!(fwd.success, "forward create must succeed: {fwd:?}");
        store.update_saga_step_state(step.id, &StepState::Executing).await.unwrap();
        store.update_saga_step_state(step.id, &StepState::Completed).await.unwrap();

        // Compensate the single step.
        let compensator = SagaCompensator::with_store(Arc::clone(&store));
        let result = compensator.compensate_step(&executor, &step, None).await.unwrap();
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
    async fn compensate_saga_rolls_back_completed_steps_in_reverse() {
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
            .execute_saga(saga_id, &executor, &std::collections::HashMap::new(), None, None)
            .await
            .unwrap();

        // Compensate: only the completed step0 rolls back.
        let comp = SagaCompensator::with_store(Arc::clone(&store))
            .compensate_saga(saga_id, &executor, &std::collections::HashMap::new(), None)
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
    async fn compensate_saga_partial_when_compensation_unregistered() {
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
            .execute_saga(saga_id, &executor, &std::collections::HashMap::new(), None, None)
            .await
            .unwrap();

        let comp = SagaCompensator::with_store(Arc::clone(&store))
            .compensate_saga(saga_id, &executor, &std::collections::HashMap::new(), None)
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

// ── Wired recovery loop end-to-end against real PostgreSQL (saga) ─────
//
// Ignored by default — these require a live PostgreSQL reachable via
// `DATABASE_URL` (the saga store is Postgres-only). The CI integration leg runs
// them with `--features saga --include-ignored` against the bound
// service. They exercise the additive `run_iteration` /
// `start_background_loop` wired methods; the fail-loud `run_iteration` /
// `start_background_loop` entry points are unchanged.

#[cfg(feature = "saga")]
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

    /// Backdate a saga's `updated_at` so it reads as genuinely stale: a crashed
    /// driver stopped heart-beating the row, which is what makes the saga
    /// claimable under `claim_stuck_sagas`' staleness threshold (#745).
    async fn backdate_saga(url: &str, saga_id: Uuid) {
        let adapter = PostgresAdapter::new(url).await.unwrap();
        adapter
            .execute_raw_query(&format!(
                "UPDATE tb_federation_sagas SET updated_at = NOW() - INTERVAL '1 hour' \
                 WHERE id = '{saga_id}'"
            ))
            .await
            .unwrap();
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
            remote: false,
            mutation_type: mt,
            mutation_name: None,
            typename: typename.to_string(),
            variables,
            state: StepState::Pending,
            result: None,
            started_at: None,
            completed_at: None,
            compensation_mutation: None,
            compensation_variables: None,
            required_fields: Vec::new(),
            compensation_error: None,
        }
    }

    /// A single recovery tick re-drives a crash-interrupted (`Executing`) saga to a
    /// terminal state, records a recovery attempt, and counts the work in `stats`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn run_iteration_drives_stuck_saga_to_terminal_state() {
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
        backdate_saga(&url, saga_id).await;
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
        manager.run_iteration(&executor).await.unwrap();

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
        assert_eq!(stats.iterations, 1, "one run_iteration call is one iteration");
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
    async fn run_iteration_continues_past_a_failing_saga() {
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
        backdate_saga(&url, failing_id).await;
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
        backdate_saga(&url, ok_id).await;
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
        manager.run_iteration(&executor).await.unwrap();

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
    async fn start_background_loop_recovers_then_stops() {
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
        backdate_saga(&url, saga_id).await;
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
            check_interval: Duration::from_millis(150),
            ..RecoveryConfig::default()
        };
        let manager = Arc::new(SagaRecoveryManager::new(Arc::clone(&store), config));

        Arc::clone(&manager).start_background_loop(Arc::new(executor)).await.unwrap();
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
    async fn start_background_loop_rejects_double_start() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let config = RecoveryConfig {
            check_interval: Duration::from_millis(150),
            ..RecoveryConfig::default()
        };
        let manager = Arc::new(SagaRecoveryManager::new(Arc::clone(&store), config));
        let executor = Arc::new(executor);

        Arc::clone(&manager).start_background_loop(Arc::clone(&executor)).await.unwrap();

        // A second start while running must fail loud, not spawn a duplicate loop.
        let second = Arc::clone(&manager).start_background_loop(Arc::clone(&executor)).await;
        assert!(second.is_err(), "a double start must be rejected: {second:?}");

        // Clean up: stop the single running loop.
        manager.stop_background_loop().await.unwrap();
        assert!(!manager.is_running());
    }

    // ── Concurrency-safe recovery: SKIP LOCKED claim + lease (#429 hardening P04) ─

    /// Two recovery workers claiming stuck sagas at the same time get **disjoint**
    /// sets — the `FOR UPDATE SKIP LOCKED` guarantee — so no saga is ever
    /// double-driven. Each seeded saga is claimed by exactly one worker (a live
    /// lease blocks the other, whether the claims run truly concurrently or serialise
    /// on the pool).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn claim_stuck_sagas_two_workers_get_disjoint_sets() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // Seed 6 stuck (Executing) sagas with known ids.
        let ids: Vec<Uuid> = (0..6).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            store.save_saga(&stuck_saga(*id)).await.unwrap();
        }

        // Two workers claim concurrently with a generous limit + lease.
        let (a, b) = tokio::join!(
            store.claim_stuck_sagas(Uuid::new_v4(), 300, 1000, 0),
            store.claim_stuck_sagas(Uuid::new_v4(), 300, 1000, 0),
        );
        let set_a: std::collections::HashSet<Uuid> = a.unwrap().into_iter().map(|s| s.id).collect();
        let set_b: std::collections::HashSet<Uuid> = b.unwrap().into_iter().map(|s| s.id).collect();

        assert!(set_a.is_disjoint(&set_b), "two workers must claim disjoint sets");
        for id in &ids {
            let in_a = set_a.contains(id);
            let in_b = set_b.contains(id);
            assert!(
                in_a ^ in_b,
                "seeded saga {id} must be claimed by exactly one worker (a={in_a}, b={in_b})"
            );
        }

        for id in &ids {
            store.delete_saga(*id).await.unwrap();
        }
    }

    /// A live lease blocks re-claim; an expired lease makes the saga claimable again
    /// (so a crashed recovery worker's claims are automatically reclaimable).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn claim_respects_live_lease_and_reclaims_expired() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let id = Uuid::new_v4();
        store.save_saga(&stuck_saga(id)).await.unwrap();

        // Worker A claims it under a long lease.
        let claimed_a = store.claim_stuck_sagas(Uuid::new_v4(), 300, 1000, 0).await.unwrap();
        assert!(claimed_a.iter().any(|s| s.id == id), "worker A claims the stuck saga");

        // Worker B cannot re-claim it while A's lease is live.
        let claimed_b = store.claim_stuck_sagas(Uuid::new_v4(), 300, 1000, 0).await.unwrap();
        assert!(!claimed_b.iter().any(|s| s.id == id), "a live lease blocks re-claim");

        // Expire the lease directly; worker B can then reclaim it.
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        adapter
            .execute_raw_query(&format!(
                "UPDATE tb_federation_sagas SET recovery_lease_expires_at = NOW() - INTERVAL '1 hour' WHERE id = '{id}'"
            ))
            .await
            .unwrap();
        let claimed_c = store.claim_stuck_sagas(Uuid::new_v4(), 300, 1000, 0).await.unwrap();
        assert!(claimed_c.iter().any(|s| s.id == id), "an expired lease is reclaimable");

        store.delete_saga(id).await.unwrap();
    }

    /// Two recovery managers each running one iteration concurrently drive every
    /// stuck saga to a terminal state **exactly once**. Each saga's step is a
    /// unique-id `Create`, so a saga driven twice would re-run its `Create` →
    /// duplicate-PK failure → `Failed`; asserting all reach `Completed` therefore
    /// proves the `SKIP LOCKED` claim prevented any double-processing.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn two_recovery_managers_drive_stuck_sagas_without_double_processing() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // Seed 4 stuck sagas, each with a single Create step keyed by a unique id.
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        for (i, id) in ids.iter().enumerate() {
            store.save_saga(&stuck_saga(*id)).await.unwrap();
            backdate_saga(&url, *id).await;
            store
                .save_saga_step(&pending_step(
                    *id,
                    0,
                    MutationType::Create,
                    &typename,
                    json!({"id": format!("rec-{i}-{id}"), "total": "1"}),
                ))
                .await
                .unwrap();
        }

        // RecoveryConfig is Copy, so one value seeds both managers. A generous
        // per-iteration cap lets each manager claim all seeded stuck sagas in one tick.
        let cfg = RecoveryConfig {
            max_sagas_per_iteration: 100,
            ..RecoveryConfig::default()
        };
        let m1 = SagaRecoveryManager::new(Arc::clone(&store), cfg);
        let m2 = SagaRecoveryManager::new(Arc::clone(&store), cfg);
        let (r1, r2) = tokio::join!(m1.run_iteration(&executor), m2.run_iteration(&executor));
        r1.unwrap();
        r2.unwrap();

        for id in &ids {
            let saga = store.load_saga(*id).await.unwrap().unwrap();
            assert_eq!(
                saga.state,
                SagaState::Completed,
                "saga {id} must be driven exactly once → Completed (double-drive would Fail it)"
            );
        }

        for id in &ids {
            store.delete_saga(*id).await.unwrap();
        }
    }
}

// ── Wired coordinator end-to-end against real PostgreSQL (saga) ──────
//
// Ignored by default — these require a live PostgreSQL reachable via
// `DATABASE_URL` (the saga store is Postgres-only). The CI integration leg runs
// them with `--features saga --include-ignored` against the bound
// service. They exercise the additive `SagaCoordinator`, which ties forward
// execution + compensation into one handle; the loud-fail `SagaCoordinator`
// entry points are unchanged and covered by its own unit tests.

#[cfg(feature = "saga")]
mod coordinator_pg {
    use std::sync::Arc;

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        CompensationStrategy, FederatedType, FederationMetadata, FederationMutationExecutor,
        KeyDirective, PostgresSagaStore, SagaCoordinator, SagaCoordinatorStep, SagaState,
        SagaStoreError, StepState,
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
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

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
        let expected_name = format!("create{typename}");
        assert_eq!(
            loaded[0].mutation_name.as_deref(),
            Some(expected_name.as_str()),
            "the full mutation_name round-trips through create_saga (not just the verb)"
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
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

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
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

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
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

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
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

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

    /// Phase 04 (Cycle 2): `with_subgraph` validates the peer URL at registration —
    /// an `https` peer is accepted, a plain-`http` peer is rejected (SSRF
    /// fail-loud-at-setup), before any saga runs.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn with_subgraph_validates_peer_url_at_registration() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // An https peer is accepted.
        let accepted = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic)
            .with_subgraph(
                "peer",
                reqwest::Url::parse("https://peer.example.com/graphql").unwrap(),
            );
        assert!(accepted.is_ok(), "an https peer URL must be accepted: {:?}", accepted.err());

        // A plain-http peer is rejected at registration, not at dispatch.
        let rejected = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic)
            .with_subgraph("peer", reqwest::Url::parse("http://peer.example.com/graphql").unwrap())
            .err();
        assert!(rejected.is_some(), "a plain-http peer URL must be rejected: {rejected:?}");
    }
}

// ── Wired remote step dispatch end-to-end (saga + test-utils) ─────────
//
// Ignored by default — these require a live PostgreSQL (`DATABASE_URL`) plus a
// loopback `wiremock` subgraph. They need the `test-utils` feature: the SSRF
// guard blocks a loopback/http peer, so the coordinator's `*_for_test` /
// `_unchecked` builders (which bypass it) are only compiled under `test-utils`.
// The CI integration leg runs them with `--features saga,test-utils
// --include-ignored`.

#[cfg(all(feature = "saga", feature = "test-utils"))]
mod remote_dispatch_pg {
    use std::sync::Arc;

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        CompensationStrategy, FederatedType, FederationMetadata, FederationMutationExecutor,
        HttpMutationConfig, KeyDirective, PostgresSagaStore, SagaCoordinator, SagaCoordinatorStep,
        SagaState, StepState,
    };
    use serde_json::json;
    use uuid::Uuid;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, method, path},
    };

    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    fn unique_typename() -> String {
        format!("sagaremote{}", Uuid::new_v4().simple())
    }

    /// A GraphQL response `{"data": {"<op>": {"__typename": <typename>, "id": <id>}}}`
    /// with a runtime `op` key (`json!` treats a bare identifier as a literal key, so
    /// the dynamic key is inserted via `serde_json::Map`).
    fn op_response(op: &str, typename: &str, id: &str) -> serde_json::Value {
        let mut entity = serde_json::Map::new();
        entity.insert("__typename".to_string(), json!(typename));
        entity.insert("id".to_string(), json!(id));
        let mut data = serde_json::Map::new();
        data.insert(op.to_string(), serde_json::Value::Object(entity));
        json!({ "data": serde_json::Value::Object(data) })
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

    /// Cycle 4: a saga with one local step and one remote step completes — the
    /// remote step is dispatched over HTTP to the registered peer (exactly once),
    /// the local step runs against the SQL adapter, and each step persists its own
    /// transport's response.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store) + loopback wiremock"]
    async fn execute_saga_routes_local_and_remote_steps() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // A mock peer subgraph returning a create response; must be hit exactly once
        // (only the remote step, never the local one). The response is keyed by the
        // full mutation name the step now sends (`create{typename}`) — the store
        // persists the real operation name, not the coarse verb.
        let server = MockServer::start().await;
        let remote_id = format!("remote-{}", Uuid::new_v4());
        let remote_op = format!("create{typename}");
        let mut entity = serde_json::Map::new();
        entity.insert("__typename".to_string(), json!(typename));
        entity.insert("id".to_string(), json!(remote_id));
        let mut data = serde_json::Map::new();
        data.insert(remote_op, serde_json::Value::Object(entity));
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "data": serde_json::Value::Object(data) })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let peer_url = reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic)
            .with_http_client_for_test(HttpMutationConfig::default())
            .unwrap()
            .with_subgraph_unchecked("remote-subgraph", peer_url);

        // Step 1: local (subgraph "orders" is not registered → SQL path).
        // Step 2: remote (subgraph "remote-subgraph" resolves to the mock).
        let local_id = format!("local-{}", Uuid::new_v4());
        let step_local = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": local_id, "total": "10"}),
            String::new(),
            json!({}),
        );
        let step_remote = SagaCoordinatorStep::new(
            2,
            "remote-subgraph",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": remote_id, "total": "20"}),
            String::new(),
            json!({}),
        );
        let saga_id = coordinator.create_saga(vec![step_local, step_remote]).await.unwrap();

        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();
        assert_eq!(result.state, SagaState::Completed, "both steps complete: {result:?}");
        assert_eq!(result.completed_steps, 2, "two steps completed: {result:?}");
        assert!(!result.compensated, "no compensation on success: {result:?}");

        // The mock received exactly one POST — only the remote step went over HTTP.
        let requests = server.received_requests().await.unwrap();
        let posts = requests.iter().filter(|r| r.url.path() == "/graphql").count();
        assert_eq!(posts, 1, "only the remote step hits the peer subgraph: {posts}");

        // Each step persisted its own transport's response: the local step's stored
        // result carries the read-back row (`total`), the remote step's carries the
        // mock entity (`id` = remote_id).
        let mut steps = store.load_saga_steps(saga_id).await.unwrap();
        steps.sort_by_key(|s| s.order);
        assert!(
            steps.iter().all(|s| s.state == StepState::Completed),
            "both Completed: {steps:?}"
        );
        let local_result = steps[0].result.as_ref().expect("local step result persisted");
        assert_eq!(
            local_result["total"], "10",
            "local step carries the DB read-back: {local_result}"
        );
        let remote_result = steps[1].result.as_ref().expect("remote step result persisted");
        assert_eq!(
            remote_result["id"], remote_id,
            "remote step carries the mock response entity: {remote_result}"
        );

        // The local row landed in the table; the remote id never touched it.
        let table = typename.to_lowercase();
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        let local_rows = adapter
            .execute_raw_query(&format!("SELECT id FROM \"{table}\" WHERE id = '{local_id}'"))
            .await
            .unwrap();
        assert_eq!(local_rows.len(), 1, "the local step wrote its row to the DB");
        let remote_rows = adapter
            .execute_raw_query(&format!("SELECT id FROM \"{table}\" WHERE id = '{remote_id}'"))
            .await
            .unwrap();
        assert!(remote_rows.is_empty(), "the remote step never wrote to the local DB");

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Phase 02: compensation routes over the same transport the forward step used.
    /// A saga runs a local step and a remote step, then fails at a third step;
    /// Automatic compensation rolls the remote step back over HTTPS (a
    /// `delete{typename}` POST to the peer) and the local step back against the SQL
    /// adapter. The saga reaches `Compensated` and the peer sees exactly one delete.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store) + loopback wiremock"]
    async fn compensation_rolls_remote_step_back_over_http() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let create_op = format!("create{typename}");
        let delete_op = format!("delete{typename}");
        let remote_id = format!("remote-{}", Uuid::new_v4());

        // The peer answers both the forward create and the compensation delete, each
        // keyed by the full op name the step sends and hit exactly once.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains(create_op.clone()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(op_response(&create_op, &typename, &remote_id)),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains(delete_op.clone()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(op_response(&delete_op, &typename, &remote_id)),
            )
            .expect(1)
            .mount(&server)
            .await;

        let peer_url = reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic)
            .with_http_client_for_test(HttpMutationConfig::default())
            .unwrap()
            .with_subgraph_unchecked("remote-subgraph", peer_url);

        // Step 1 local (creates a row), step 2 remote (create via peer), step 3 local
        // update on a missing row → forward fails after two completed steps, so
        // Automatic compensation rolls both back (remote step over HTTP, local via SQL).
        let local_id = format!("local-{}", Uuid::new_v4());
        let step1 = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            create_op.clone(),
            json!({"id": local_id, "total": "1"}),
            delete_op.clone(),
            json!({"id": local_id}),
        );
        let step2 = SagaCoordinatorStep::new(
            2,
            "remote-subgraph",
            typename.as_str(),
            create_op.clone(),
            json!({"id": remote_id, "total": "2"}),
            delete_op.clone(),
            json!({"id": remote_id}),
        );
        let step3 = SagaCoordinatorStep::new(
            3,
            "orders",
            typename.as_str(),
            format!("update{typename}"),
            json!({"id": "missing", "total": "9"}),
            delete_op.clone(),
            json!({"id": "missing"}),
        );
        let saga_id = coordinator.create_saga(vec![step1, step2, step3]).await.unwrap();

        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();
        assert_eq!(
            result.state,
            SagaState::Failed,
            "a failed step leaves the result Failed: {result:?}"
        );
        assert!(
            result.compensated,
            "Automatic strategy compensated the completed steps: {result:?}"
        );

        // Both completed steps rolled back → saga persisted Compensated.
        assert_eq!(
            store.load_saga(saga_id).await.unwrap().unwrap().state,
            SagaState::Compensated,
            "all completed steps rolled back → Compensated"
        );
        let mut steps = store.load_saga_steps(saga_id).await.unwrap();
        steps.sort_by_key(|s| s.order);
        assert_eq!(steps[0].state, StepState::Compensated, "local step rolled back: {steps:?}");
        assert_eq!(steps[1].state, StepState::Compensated, "remote step rolled back: {steps:?}");
        assert_eq!(steps[2].state, StepState::Failed, "the failing step is untouched: {steps:?}");

        // The peer was rolled back exactly once, over HTTP (a delete POST).
        let requests = server.received_requests().await.unwrap();
        let deletes = requests
            .iter()
            .filter(|r| String::from_utf8_lossy(&r.body).contains(delete_op.as_str()))
            .count();
        assert_eq!(deletes, 1, "the remote step was rolled back over HTTP exactly once");

        // The local step's compensation deleted its row from the DB.
        let table = typename.to_lowercase();
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        let rows = adapter
            .execute_raw_query(&format!("SELECT id FROM \"{table}\" WHERE id = '{local_id}'"))
            .await
            .unwrap();
        assert!(rows.is_empty(), "the local step's compensation deleted its row");

        store.delete_saga(saga_id).await.unwrap();
    }
}

// ── @requires cross-subgraph pre-fetch end-to-end (saga + test-utils) ─
//
// Ignored by default — these require a live PostgreSQL (`DATABASE_URL`) plus a
// loopback `wiremock` `_entities` subgraph. They need the `test-utils` feature so
// the coordinator's `*_for_test` / `_unchecked` builders (which bypass the SSRF
// guard) are compiled. The CI integration leg runs them with
// `--features saga,test-utils --include-ignored`.

#[cfg(all(feature = "saga", feature = "test-utils"))]
mod prefetch_pg {
    use std::sync::Arc;

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        CompensationStrategy, FederatedType, FederationMetadata, FederationMutationExecutor,
        HttpClientConfig, KeyDirective, PostgresSagaStore, RequiredField, SagaCoordinator,
        SagaCoordinatorStep, SagaState, SagaStoreError, StepState,
    };
    use serde_json::json;
    use uuid::Uuid;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    fn unique_typename() -> String {
        format!("sagareq{}", Uuid::new_v4().simple())
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

    /// A `_entities` mock subgraph returning `entities` (the value of the
    /// `_entities` array). Registered under `catalog`.
    async fn catalog_server(entities: serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "_entities": entities }
            })))
            .mount(&server)
            .await;
        server
    }

    /// Cycle 1: `create_saga` rejects a step that `@requires` a field from an
    /// unregistered subgraph — fail-loud-at-setup, nothing is persisted.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn create_saga_rejects_requires_from_unregistered_subgraph() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // No subgraph registered → the @requires spec cannot be resolved.
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);
        let before = store.saga_count().await.unwrap();

        let step = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": "x", "total": "1"}),
            String::new(),
            json!({}),
        )
        .with_required_fields(vec![RequiredField {
            subgraph:   "catalog".to_string(),
            typename:   "Catalog".to_string(),
            key:        json!({"id": "c1"}),
            field_path: "price".to_string(),
            target_var: "total".to_string(),
        }]);

        let err = coordinator
            .create_saga(vec![step])
            .await
            .expect_err("a @requires from an unregistered subgraph must be rejected");
        assert!(
            matches!(err, SagaStoreError::Database(ref m) if m.contains("unregistered subgraph")),
            "the error names the unregistered subgraph: {err:?}"
        );
        assert_eq!(store.saga_count().await.unwrap(), before, "a rejected saga persists nothing");
    }

    /// A step's `@requires` specs round-trip through the store: `create_saga`
    /// persists them and `load_saga_steps` reads them back intact.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn required_fields_round_trip_through_store() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // Register catalog (a bare URL; no server needed — nothing is executed here).
        let catalog_url = reqwest::Url::parse("http://127.0.0.1:1/graphql").unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic)
            .with_subgraph_unchecked("catalog", catalog_url);

        let required = RequiredField {
            subgraph:   "catalog".to_string(),
            typename:   "Catalog".to_string(),
            key:        json!({"id": "c1"}),
            field_path: "price".to_string(),
            target_var: "total".to_string(),
        };
        let step = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": "x"}),
            String::new(),
            json!({}),
        )
        .with_required_fields(vec![required.clone()]);
        let saga_id = coordinator.create_saga(vec![step]).await.unwrap();

        let steps = store.load_saga_steps(saga_id).await.unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].required_fields,
            vec![required],
            "the @requires specs round-trip through the store: {:?}",
            steps[0].required_fields
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Cycle 3: a required field the owning subgraph cannot provide fails the step
    /// **before** dispatch — the mutation never runs (zero rows written).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store) + loopback wiremock"]
    async fn missing_required_field_fails_step_before_dispatch() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // The catalog resolves the entity but WITHOUT the required `price` field.
        let server = catalog_server(json!([{"__typename": "Catalog"}])).await;
        let catalog_url = reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic)
            .with_entity_resolver_for_test(HttpClientConfig::default())
            .unwrap()
            .with_subgraph_unchecked("catalog", catalog_url);

        // The step carries a full `total` in its base variables, so a dispatch WOULD
        // write a row — proving zero-dispatch means the row's absence, not a no-op.
        let local_id = format!("nodispatch-{}", Uuid::new_v4());
        let step = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": local_id, "total": "5"}),
            String::new(),
            json!({}),
        )
        .with_required_fields(vec![RequiredField {
            subgraph:   "catalog".to_string(),
            typename:   "Catalog".to_string(),
            key:        json!({"id": "c1"}),
            field_path: "price".to_string(),
            target_var: "total".to_string(),
        }]);
        let saga_id = coordinator.create_saga(vec![step]).await.unwrap();

        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();
        assert_eq!(result.state, SagaState::Failed, "an unresolved @requires fails the saga");
        assert_eq!(result.completed_steps, 0, "no step completed: {result:?}");

        // The step is persisted Failed and its error names the missing field.
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        assert_eq!(steps[0].state, StepState::Failed, "the step is Failed: {steps:?}");

        // Zero dispatch: the mutation never ran, so no row exists in the table.
        let table = typename.to_lowercase();
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        let rows = adapter
            .execute_raw_query(&format!("SELECT id FROM \"{table}\" WHERE id = '{local_id}'"))
            .await
            .unwrap();
        assert!(rows.is_empty(), "the mutation must never run when @requires is unresolved");

        store.delete_saga(saga_id).await.unwrap();
    }

    /// Cycle 4: a step `@requires` a field owned by a registered subgraph; the
    /// wiremock `_entities` endpoint returns it, and the step's mutation runs with
    /// the fetched value merged into its variables (read back from the DB row).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store) + loopback wiremock"]
    async fn cross_subgraph_prefetch_merges_value_into_mutation() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        // The catalog subgraph owns the `price` field (returned as "99").
        let server = catalog_server(json!([{"__typename": "Catalog", "price": "99"}])).await;
        let catalog_url = reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic)
            .with_entity_resolver_for_test(HttpClientConfig::default())
            .unwrap()
            .with_subgraph_unchecked("catalog", catalog_url);

        // The step runs locally (subgraph "orders") and supplies only `id`; the
        // `total` column is filled by the pre-fetched catalog `price`.
        let local_id = format!("req-{}", Uuid::new_v4());
        let step = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": local_id}),
            String::new(),
            json!({}),
        )
        .with_required_fields(vec![RequiredField {
            subgraph:   "catalog".to_string(),
            typename:   "Catalog".to_string(),
            key:        json!({"id": "c1"}),
            field_path: "price".to_string(),
            target_var: "total".to_string(),
        }]);
        let saga_id = coordinator.create_saga(vec![step]).await.unwrap();

        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();
        assert_eq!(result.state, SagaState::Completed, "the step ran with the fetched value");
        assert_eq!(result.completed_steps, 1, "one step completed: {result:?}");

        // The pre-fetch endpoint was hit exactly once.
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/graphql")
            .count();
        assert_eq!(posts, 1, "the @requires field was fetched exactly once: {posts}");

        // The mutation ran with the merged value: the DB row's `total` is the fetched
        // catalog price, not anything the caller supplied (the caller supplied none).
        let table = typename.to_lowercase();
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        let rows = adapter
            .execute_raw_query(&format!("SELECT total FROM \"{table}\" WHERE id = '{local_id}'"))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "the local mutation wrote its row");
        assert_eq!(
            rows[0].get("total").and_then(|v| v.as_str()),
            Some("99"),
            "the fetched @requires value was merged into the mutation: {:?}",
            rows[0]
        );

        store.delete_saga(saga_id).await.unwrap();
    }
}

// ── Recovery safety (P19: #744 #745 #747 #766) ────
//
// The chaos-shaped contract for crash recovery: replay must be exactly-once for
// committed work, a live saga must never be claimed by the recovery loop, an
// ambiguous (timed-out) dispatch must not duplicate its logical effect, and a
// remote-subgraph step must never be replayed against the local SQL adapter.
// Ignored by default — they need a live PostgreSQL via `DATABASE_URL` plus the
// `test-utils` loopback builders; the CI postgres integration leg runs them with
// `--features saga,test-utils --include-ignored --test-threads=1`.

#[cfg(all(feature = "saga", feature = "test-utils"))]
mod recovery_safety_pg {
    use std::sync::Arc;

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        CompensationStrategy, FederatedType, FederationMetadata, FederationMutationExecutor,
        HttpMutationConfig, KeyDirective, MutationType, PostgresSagaStore, RecoveryConfig,
        RetryPolicy, Saga, SagaCoordinator, SagaCoordinatorStep, SagaRecoveryManager, SagaState,
        SagaStep, StepState,
    };
    use serde_json::json;
    use uuid::Uuid;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    fn unique_typename() -> String {
        format!("sagarecov{}", Uuid::new_v4().simple())
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

    /// Backdate a saga's `updated_at` so it is genuinely stale: a crash-left saga
    /// stopped touching its row, which is what makes it claimable once
    /// `claim_stuck_sagas` enforces a staleness threshold.
    async fn backdate_saga(url: &str, saga_id: Uuid) {
        let adapter = PostgresAdapter::new(url).await.unwrap();
        adapter
            .execute_raw_query(&format!(
                "UPDATE tb_federation_sagas SET updated_at = NOW() - INTERVAL '1 hour' \
                 WHERE id = '{saga_id}'"
            ))
            .await
            .unwrap();
    }

    const fn count_rows(rows: &[std::collections::HashMap<String, serde_json::Value>]) -> usize {
        rows.len()
    }

    /// A GraphQL success payload `{"data": {"<op>": {"__typename", "id"}}}` with a
    /// runtime op key (`json!` treats bare identifiers as literal keys).
    fn op_response(op: &str, typename: &str, id: &str) -> serde_json::Value {
        let mut entity = serde_json::Map::new();
        entity.insert("__typename".to_string(), json!(typename));
        entity.insert("id".to_string(), json!(id));
        let mut data = serde_json::Map::new();
        data.insert(op.to_string(), serde_json::Value::Object(entity));
        json!({ "data": serde_json::Value::Object(data) })
    }

    /// #744 — forward replay executes committed work exactly once. A saga crashed
    /// after steps 1–2 committed (rows in the table, steps `Completed`, saga left
    /// `Executing`); recovery must drive only step 3, leave steps 1–2 untouched,
    /// and finish the saga `Completed`. Today `execute_saga` re-marks every step
    /// `Executing` and re-dispatches it: the re-INSERT hits the primary key, the
    /// step is rewritten `Completed → Failed`, and the saga ends `Failed` with the
    /// committed work re-executed.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn recovery_replay_skips_committed_steps() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let table = typename.to_lowercase();
        let adapter = PostgresAdapter::new(&url).await.unwrap();

        // The saga: 3 create-steps; steps 0 and 1 committed before the crash.
        let saga_id = Uuid::new_v4();
        store
            .save_saga(&Saga {
                id:           saga_id,
                state:        SagaState::Pending,
                created_at:   chrono::Utc::now(),
                completed_at: None,
                metadata:     None,
            })
            .await
            .unwrap();

        let mut step_ids = Vec::new();
        for (order, id_val) in [(0usize, "a1"), (1, "a2"), (2, "a3")] {
            let step = SagaStep {
                id: Uuid::new_v4(),
                saga_id,
                order,
                subgraph: "orders".to_string(),
                remote: false,
                mutation_type: MutationType::Create,
                mutation_name: Some(format!("create{typename}")),
                typename: typename.clone(),
                variables: json!({"id": id_val, "total": "10"}),
                state: StepState::Pending,
                result: None,
                started_at: None,
                completed_at: None,
                compensation_mutation: None,
                compensation_variables: None,
                required_fields: Vec::new(),
                compensation_error: None,
            };
            store.save_saga_step(&step).await.unwrap();
            step_ids.push(step.id);
        }

        // Simulate the crash: steps 1–2 committed their rows and were persisted
        // Completed (result included, via the legal Pending → Executing →
        // Completed chain); the saga row was left Executing.
        for (i, id_val) in [(0usize, "a1"), (1, "a2")] {
            adapter
                .execute_raw_query(&format!(
                    "INSERT INTO \"{table}\" (id, total) VALUES ('{id_val}', '10')"
                ))
                .await
                .unwrap();
            store
                .update_saga_step_result(step_ids[i], &json!({"id": id_val, "total": "10"}))
                .await
                .unwrap();
            store.update_saga_step_state(step_ids[i], &StepState::Executing).await.unwrap();
            store.update_saga_step_state(step_ids[i], &StepState::Completed).await.unwrap();
        }
        store.update_saga_state(saga_id, &SagaState::Executing).await.unwrap();
        backdate_saga(&url, saga_id).await;

        let manager = SagaRecoveryManager::new(Arc::clone(&store), RecoveryConfig::default());
        manager.run_iteration(&executor).await.unwrap();

        // Exactly-once: each committed entity still exists exactly once…
        for id_val in ["a1", "a2"] {
            let rows = adapter
                .execute_raw_query(&format!("SELECT id FROM \"{table}\" WHERE id = '{id_val}'"))
                .await
                .unwrap();
            assert_eq!(
                count_rows(&rows),
                1,
                "committed step '{id_val}' must not re-execute on recovery"
            );
        }
        // …its step row never left Completed…
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        assert_eq!(
            steps[0].state,
            StepState::Completed,
            "a Completed step must stay Completed through replay: {steps:?}"
        );
        assert_eq!(
            steps[1].state,
            StepState::Completed,
            "a Completed step must stay Completed through replay: {steps:?}"
        );
        // …the pending step 3 ran…
        let rows = adapter
            .execute_raw_query(&format!("SELECT id FROM \"{table}\" WHERE id = 'a3'"))
            .await
            .unwrap();
        assert_eq!(count_rows(&rows), 1, "the interrupted step must be driven to completion");
        assert_eq!(steps[2].state, StepState::Completed, "step 3 completes: {steps:?}");
        // …and the saga finished Completed, not Failed-on-duplicate.
        let saga = store.load_saga(saga_id).await.unwrap().unwrap();
        assert_eq!(
            saga.state,
            SagaState::Completed,
            "recovery of a half-committed saga must complete it exactly once"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #745 — the recovery loop must not claim a saga that is actively executing.
    /// A coordinator drives a saga whose remote step is slow (in flight on a mock
    /// peer); a recovery tick fires mid-flight. `claim_stuck_sagas` has no
    /// staleness threshold and the forward path holds no lease, so today the tick
    /// claims the live saga and re-drives it concurrently — with an empty subgraph
    /// registry, i.e. against the local database.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires DATABASE_URL (Postgres saga store) + loopback wiremock"]
    async fn recovery_does_not_claim_actively_executing_saga() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let table = typename.to_lowercase();

        // The peer answers the create after 2s — long enough for a recovery tick
        // to fire while the step is genuinely in flight.
        let create_op = format!("create{typename}");
        let remote_id = format!("live-{}", Uuid::new_v4());
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(op_response(&create_op, &typename, &remote_id))
                    .set_delay(std::time::Duration::from_secs(2)),
            )
            .mount(&server)
            .await;

        let peer_url = reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Manual)
            .with_http_client_for_test(HttpMutationConfig {
                timeout_ms: 10_000,
                ..HttpMutationConfig::default()
            })
            .unwrap()
            .with_subgraph_unchecked("payments", peer_url);

        let step = SagaCoordinatorStep::new(
            1,
            "payments",
            typename.as_str(),
            create_op.clone(),
            json!({"id": remote_id, "total": "42"}),
            String::new(),
            json!({}),
        );
        let saga_id = coordinator.create_saga(vec![step]).await.unwrap();

        // Drive the saga on a background task; tick recovery while it is in flight.
        let coordinator = Arc::new(coordinator);
        let drive = {
            let coordinator = Arc::clone(&coordinator);
            let executor = executor.clone();
            tokio::spawn(async move { coordinator.execute_saga(saga_id, &executor).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let manager = SagaRecoveryManager::new(Arc::clone(&store), RecoveryConfig::default());
        manager.run_iteration(&executor).await.unwrap();
        let stats = manager.get_stats();

        let result = drive.await.unwrap().unwrap();
        assert_eq!(result.state, SagaState::Completed, "the live drive completes: {result:?}");

        // The recovery tick must have left the live saga alone…
        assert_eq!(
            stats.executing_sagas_found, 0,
            "a saga that is actively executing must not be claimed as stuck"
        );
        // …the peer saw exactly one dispatch…
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/graphql")
            .count();
        assert_eq!(posts, 1, "one live drive → exactly one dispatch, no concurrent re-drive");
        // …and nothing was ever written to the local database (the recovery
        // replay would have dispatched the remote step against the local table).
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        let rows = adapter.execute_raw_query(&format!("SELECT id FROM \"{table}\"")).await.unwrap();
        assert_eq!(count_rows(&rows), 0, "no concurrent local re-drive of the remote step");

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #747 — an ambiguous (timed-out) dispatch must not duplicate the logical
    /// effect. The first attempt is received by the peer but answers slowly; the
    /// step times out, retries, and the second attempt succeeds. Every attempt must
    /// carry the same idempotency key so the receiver can deduplicate: the number
    /// of distinct logical effects — attempts not sharing a prior attempt's key —
    /// must be exactly one. Today no key is sent at all, so the two byte-identical
    /// requests are two indistinguishable charges.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires DATABASE_URL (Postgres saga store) + loopback wiremock"]
    async fn timed_out_dispatch_retries_with_same_idempotency_key() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let create_op = format!("create{typename}");
        let remote_id = format!("chg-{}", Uuid::new_v4());
        let server = MockServer::start().await;
        // First attempt: the peer receives the mutation (i.e. may well commit it)
        // but answers after the step timeout. Second attempt: immediate success.
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(op_response(&create_op, &typename, &remote_id))
                    .set_delay(std::time::Duration::from_millis(1500)),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(op_response(&create_op, &typename, &remote_id)),
            )
            .mount(&server)
            .await;

        let peer_url = reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Manual)
            .with_retry_policy(RetryPolicy {
                max_retries:     1,
                base_delay_ms:   0,
                step_timeout_ms: Some(500),
            })
            .with_http_client_for_test(HttpMutationConfig::default())
            .unwrap()
            .with_subgraph_unchecked("payments", peer_url);

        let step = SagaCoordinatorStep::new(
            1,
            "payments",
            typename.as_str(),
            create_op.clone(),
            json!({"id": remote_id, "total": "99"}),
            String::new(),
            json!({}),
        );
        let saga_id = coordinator.create_saga(vec![step]).await.unwrap();
        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();
        assert_eq!(result.state, SagaState::Completed, "the retry succeeds: {result:?}");

        // Both attempts reached the peer…
        let requests = server.received_requests().await.unwrap();
        let posts: Vec<_> = requests.iter().filter(|r| r.url.path() == "/graphql").collect();
        assert_eq!(posts.len(), 2, "timeout then retry → two attempts on the wire");

        // …and each carried the same per-step idempotency key, so a receiver can
        // recognise attempt 2 as a retry of attempt 1 (one logical effect).
        let keys: Vec<Option<String>> = posts
            .iter()
            .map(|r| {
                r.headers
                    .get("idempotency-key")
                    .map(|v| v.to_str().unwrap_or_default().to_string())
            })
            .collect();
        assert!(
            keys.iter().all(|k| k.as_deref().is_some_and(|k| !k.is_empty())),
            "every dispatch attempt must carry an Idempotency-Key header; got {keys:?}"
        );
        let distinct: std::collections::HashSet<_> = keys.iter().flatten().collect();
        assert_eq!(
            distinct.len(),
            1,
            "retries of one step must reuse one idempotency key (one logical effect); \
             got {keys:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #766 — recovery must never replay a remote-subgraph step against the local
    /// SQL adapter. A saga whose only step targets the registered remote subgraph
    /// "shipping" crashes before execution; the recovery loop (which carries no
    /// subgraph registry) claims it. Today the step silently falls through to
    /// `execute_local_mutation`: the shipment is `INSERT`ed into the local table,
    /// the saga is marked Completed, and the real shipping service never hears of
    /// it. Recovery must instead fail loud and write nothing.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires DATABASE_URL (Postgres saga store) + loopback wiremock"]
    async fn recovery_never_replays_remote_step_locally() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);
        let table = typename.to_lowercase();

        // The remote peer exists (and must receive nothing during this recovery).
        let server = MockServer::start().await;
        let peer_url = reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Manual)
            .with_http_client_for_test(HttpMutationConfig::default())
            .unwrap()
            .with_subgraph_unchecked("shipping", peer_url);

        let remote_id = format!("ship-{}", Uuid::new_v4());
        let step = SagaCoordinatorStep::new(
            1,
            "shipping",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": remote_id, "total": "7"}),
            String::new(),
            json!({}),
        );
        let saga_id = coordinator.create_saga(vec![step]).await.unwrap();

        // Crash before execution: the saga is left Executing with its remote step
        // still Pending, and goes stale.
        store.update_saga_state(saga_id, &SagaState::Executing).await.unwrap();
        backdate_saga(&url, saga_id).await;

        // The recovery loop has no registry/client for "shipping".
        let manager = SagaRecoveryManager::new(Arc::clone(&store), RecoveryConfig::default());
        manager.run_iteration(&executor).await.unwrap();

        // The remote peer received nothing (no transport was configured)…
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/graphql")
            .count();
        assert_eq!(posts, 0, "recovery without a transport cannot reach the peer");
        // …the shipment was NOT written to the local database…
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        let rows = adapter.execute_raw_query(&format!("SELECT id FROM \"{table}\"")).await.unwrap();
        assert_eq!(
            count_rows(&rows),
            0,
            "a remote-subgraph step must never be replayed against the local database"
        );
        // …and the saga was not fabricated to a happy terminal state: it stays
        // in-doubt (still Executing, leased) for an operator or a routed recovery.
        let saga = store.load_saga(saga_id).await.unwrap().unwrap();
        assert_ne!(
            saga.state,
            SagaState::Completed,
            "an unreplayable remote step must not produce a Completed saga"
        );
        let steps = store.load_saga_steps(saga_id).await.unwrap();
        assert_ne!(
            steps[0].state,
            StepState::Completed,
            "the remote step never ran, so it must not be Completed: {steps:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #744 (backstop) — the store itself rejects the transition that made
    /// recovery double-execution possible: `Completed -> Executing` (and any
    /// transition out of `Compensated`) is refused atomically in the UPDATE,
    /// no matter which code path attempts it.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn store_rejects_reexecuting_a_completed_step() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store
            .save_saga(&Saga {
                id:           saga_id,
                state:        SagaState::Pending,
                created_at:   chrono::Utc::now(),
                completed_at: None,
                metadata:     None,
            })
            .await
            .unwrap();
        let step = SagaStep {
            id: Uuid::new_v4(),
            saga_id,
            order: 0,
            subgraph: "orders".to_string(),
            remote: false,
            mutation_type: MutationType::Create,
            mutation_name: None,
            typename: typename.clone(),
            variables: serde_json::json!({"id": "g1"}),
            state: StepState::Pending,
            result: None,
            started_at: None,
            completed_at: None,
            compensation_mutation: None,
            compensation_variables: None,
            required_fields: Vec::new(),
            compensation_error: None,
        };
        store.save_saga_step(&step).await.unwrap();

        // Legal chain up to Completed…
        store.update_saga_step_state(step.id, &StepState::Executing).await.unwrap();
        store.update_saga_step_state(step.id, &StepState::Completed).await.unwrap();

        // …then the forbidden re-execution write must be refused by the store.
        let err = store
            .update_saga_step_state(step.id, &StepState::Executing)
            .await
            .expect_err("Completed -> Executing is the double-execution write; it must be refused");
        assert!(
            matches!(err, fraiseql_federation::SagaStoreError::InvalidStateTransition { .. }),
            "the refusal is a typed InvalidStateTransition: {err:?}"
        );
        // The step is untouched.
        let reloaded = store.load_saga_step(step.id).await.unwrap().unwrap();
        assert_eq!(reloaded.state, StepState::Completed);

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #785 — recovery attempts are genuinely counted. Every recovery record
    /// used to be inserted with `attempt_count = 0`, so the audit trail always
    /// read "attempt 0" and no retry cap was enforceable.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn recovery_attempts_are_really_counted() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store
            .save_saga(&Saga {
                id:           saga_id,
                state:        SagaState::Executing,
                created_at:   chrono::Utc::now(),
                completed_at: None,
                metadata:     None,
            })
            .await
            .unwrap();

        assert_eq!(store.get_recovery_attempts(saga_id).await.unwrap(), 0);
        store.mark_saga_for_recovery(saga_id, "auto-recovery").await.unwrap();
        assert_eq!(
            store.get_recovery_attempts(saga_id).await.unwrap(),
            1,
            "the first recovery attempt must be recorded as attempt 1, not 0"
        );
        store.mark_saga_for_recovery(saga_id, "auto-recovery").await.unwrap();
        assert_eq!(
            store.get_recovery_attempts(saga_id).await.unwrap(),
            2,
            "attempts must accumulate — this is what makes a retry cap enforceable"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #785 — a poison saga is parked after `max_recovery_attempts` instead of
    /// being re-claimed and retried forever while its recovery rows grow
    /// without bound. Parking leaves the saga's state untouched (in-doubt,
    /// never fabricated terminal) and pushes its lease out of automation's
    /// reach.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn poison_saga_is_parked_after_max_attempts() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store
            .save_saga(&Saga {
                id:           saga_id,
                state:        SagaState::Executing,
                created_at:   chrono::Utc::now(),
                completed_at: None,
                metadata:     None,
            })
            .await
            .unwrap();
        // The saga already burned the whole default attempt budget.
        for _ in 0..5 {
            store.mark_saga_for_recovery(saga_id, "auto-recovery").await.unwrap();
        }
        backdate_saga(&url, saga_id).await;

        let manager = SagaRecoveryManager::new(Arc::clone(&store), RecoveryConfig::default());
        manager.run_iteration(&executor).await.unwrap();

        // Not driven (state untouched, no sixth attempt recorded), and the
        // parking is an error the operator can see in stats.
        let saga = store.load_saga(saga_id).await.unwrap().unwrap();
        assert_eq!(saga.state, SagaState::Executing, "parking must not fabricate a terminal state");
        assert_eq!(
            store.get_recovery_attempts(saga_id).await.unwrap(),
            5,
            "a parked saga is not retried — no further attempt is recorded"
        );
        assert!(manager.get_stats().errors >= 1, "parking is surfaced in stats.errors");

        // Parked = out of automation's reach: another tick claims nothing.
        let manager2 = SagaRecoveryManager::new(Arc::clone(&store), RecoveryConfig::default());
        manager2.run_iteration(&executor).await.unwrap();
        assert_eq!(
            store.get_recovery_attempts(saga_id).await.unwrap(),
            5,
            "a parked saga must not be re-claimed by later ticks"
        );

        store.delete_saga(saga_id).await.unwrap();
    }
}

// ── Compensation honesty (P19: #746 #767) ────
//
// The compensator itself reports honestly; these tests pin the layers ABOVE it:
// the coordinator must not discard the CompensationResult (#746), and the
// status API must read recorded state rather than fabricate one (#767).

#[cfg(feature = "saga")]
mod compensation_honesty_pg {
    use std::sync::Arc;

    use fraiseql_db::{PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_federation::{
        CompensationStatus, CompensationStrategy, FederatedType, FederationMetadata,
        FederationMutationExecutor, KeyDirective, MutationType, PostgresSagaStore, Saga,
        SagaCompensator, SagaCoordinator, SagaCoordinatorStep, SagaExecutor, SagaState, SagaStep,
        StepState,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL").ok()
    }

    fn unique_typename() -> String {
        format!("sagacomp{}", Uuid::new_v4().simple())
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

    /// #746 (cancel path) — cancelling a saga whose completed step cannot be
    /// rolled back must say so: `compensated: false`, no terminal `Cancelled`
    /// write over un-compensated work, and the failed step surfaced. Today
    /// `cancel_saga` discards the `CompensationResult`, hardcodes
    /// `compensated: true`, and stamps `Cancelled` over the `Failed` state the
    /// compensator just recorded — permanently orphaning the un-rolled-back
    /// mutation (recovery ignores `Cancelled`).
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn cancel_saga_with_unrollbackable_step_reports_honestly() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

        // Step 1 has NO registered compensation (empty string → None); step 2
        // never runs. The saga is cancelled mid-flight with step 1 committed.
        let id_a = format!("a-{}", Uuid::new_v4());
        let step1 = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": id_a, "total": "10"}),
            String::new(),
            json!({}),
        );
        let step2 = SagaCoordinatorStep::new(
            2,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": format!("b-{}", Uuid::new_v4()), "total": "20"}),
            String::new(),
            json!({}),
        );
        let step1_id = step1.id;
        let saga_id = coordinator.create_saga(vec![step1, step2]).await.unwrap();

        // Drive the saga mid-flight: saga Executing, step 1 committed.
        let table = typename.to_lowercase();
        let adapter = PostgresAdapter::new(&url).await.unwrap();
        adapter
            .execute_raw_query(&format!(
                "INSERT INTO \"{table}\" (id, total) VALUES ('{id_a}', '10')"
            ))
            .await
            .unwrap();
        store.update_saga_state(saga_id, &SagaState::Executing).await.unwrap();
        store.update_saga_step_state(step1_id, &StepState::Executing).await.unwrap();
        store.update_saga_step_state(step1_id, &StepState::Completed).await.unwrap();

        let result = coordinator.cancel_saga(saga_id, &executor).await.unwrap();

        // The rollback did not happen — the result must say so…
        assert!(
            !result.compensated,
            "step 1 has no compensation; reporting compensated=true fabricates a rollback: \
             {result:?}"
        );
        assert_ne!(
            result.state,
            SagaState::Cancelled,
            "Cancelled asserts 'completed steps were rolled back' — which is false here: \
             {result:?}"
        );
        assert!(
            result.error.as_deref().is_some_and(|e| e.contains('1')),
            "the un-rolled-back step must be surfaced in the error: {result:?}"
        );
        // …and the persisted state must keep the saga reachable for manual
        // repair (Cancelled is terminal and recovery ignores it).
        let saga = store.load_saga(saga_id).await.unwrap().unwrap();
        assert_ne!(
            saga.state,
            SagaState::Cancelled,
            "un-compensated work must never be sealed under Cancelled"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #746 (execute path) — a failed saga whose automatic compensation only
    /// partially succeeded must report `compensated: false`. Today the
    /// `Automatic` arm discards the `CompensationResult` and hardcodes `true`,
    /// so the caller is told the rollback happened even when zero steps rolled
    /// back.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn execute_saga_automatic_reports_partial_compensation() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Automatic);

        // Step 1 completes but is NOT compensatable; step 2 fails (update of a
        // missing row) and triggers automatic compensation.
        let id_a = format!("a-{}", Uuid::new_v4());
        let step1 = SagaCoordinatorStep::new(
            1,
            "orders",
            typename.as_str(),
            format!("create{typename}"),
            json!({"id": id_a, "total": "10"}),
            String::new(),
            json!({}),
        );
        let step2 = SagaCoordinatorStep::new(
            2,
            "orders",
            typename.as_str(),
            format!("update{typename}"),
            json!({"id": "missing-row", "total": "20"}),
            String::new(),
            json!({}),
        );
        let saga_id = coordinator.create_saga(vec![step1, step2]).await.unwrap();

        let result = coordinator.execute_saga(saga_id, &executor).await.unwrap();

        assert_eq!(result.state, SagaState::Failed, "step 2 fails the saga: {result:?}");
        assert!(
            !result.compensated,
            "step 1 was not rolled back (no compensation registered); compensated=true is a \
             fabricated rollback: {result:?}"
        );
        assert!(
            result.error.as_deref().is_some_and(|e| e.contains("compensat")),
            "the incomplete rollback must be surfaced in the error: {result:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #767 — the status API reads *recorded* compensation state. After a real
    /// partial rollback (steps 1–2 compensated, step 3's inverse failed), the
    /// status must be `PartiallyCompensated` with `failed_steps = [3]` and
    /// 1-indexed step numbers. Today `failed_steps` is hardcoded empty (the
    /// `PartiallyCompensated` branch is unreachable), the answer is inferred by
    /// sniffing forward payloads for magic keys, and step numbers are 0-indexed.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn get_compensation_status_reads_recorded_state() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let coordinator = SagaCoordinator::new(Arc::clone(&store), CompensationStrategy::Manual);

        // Steps 1–2 carry a working delete-compensation; step 3 has none, so
        // its rollback fails while 1–2 succeed → a genuine partial rollback.
        let ids: Vec<String> = (0..3).map(|i| format!("s{i}-{}", Uuid::new_v4())).collect();
        let mut steps = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            let compensation = if i < 2 {
                format!("delete{typename}")
            } else {
                String::new()
            };
            let comp_vars = if i < 2 { json!({"id": id}) } else { json!({}) };
            steps.push(SagaCoordinatorStep::new(
                u32::try_from(i + 1).unwrap(),
                "orders",
                typename.as_str(),
                format!("create{typename}"),
                json!({"id": id, "total": "1"}),
                compensation,
                comp_vars,
            ));
        }
        let saga_id = coordinator.create_saga(steps).await.unwrap();

        // Forward-drive all three to Completed, then roll back.
        SagaExecutor::with_store(Arc::clone(&store))
            .execute_saga(saga_id, &executor, &std::collections::HashMap::new(), None, None)
            .await
            .unwrap();
        let compensator = SagaCompensator::with_store(Arc::clone(&store));
        let comp = compensator
            .compensate_saga(saga_id, &executor, &std::collections::HashMap::new(), None)
            .await
            .unwrap();
        assert_eq!(
            comp.status,
            CompensationStatus::PartiallyCompensated,
            "precondition: the rollback is genuinely partial: {comp:?}"
        );

        // The status API must report the same recorded reality.
        let status = compensator
            .get_compensation_status(saga_id)
            .await
            .unwrap()
            .expect("a saga with recorded compensation state must report it");
        assert_eq!(
            status.status,
            CompensationStatus::PartiallyCompensated,
            "recorded state says partial; the API must not collapse it: {status:?}"
        );
        assert_eq!(
            status.failed_steps,
            vec![3],
            "step 3's inverse failed and must be named (1-indexed): {status:?}"
        );
        let succeeded: Vec<u32> = status
            .step_results
            .iter()
            .filter(|r| r.success)
            .map(|r| r.step_number)
            .collect();
        assert_eq!(
            succeeded,
            vec![1, 2],
            "steps 1 and 2 rolled back (1-indexed step numbers): {status:?}"
        );
        let failed_result = status
            .step_results
            .iter()
            .find(|r| r.step_number == 3)
            .expect("step 3's failed rollback appears in step_results");
        assert!(!failed_result.success, "step 3's rollback failed: {status:?}");
        assert!(
            failed_result.error.is_some(),
            "the failed rollback carries its recorded error: {status:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }

    /// #767 (magic keys) — a forward payload that happens to contain a
    /// `deleted` column (e.g. a soft-delete flag from `RETURNING *`) must not
    /// be reported as a successful compensation. A failed saga with **no**
    /// compensation evidence was never in a compensation phase → `None`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL (Postgres saga store)"]
    async fn get_compensation_status_does_not_sniff_forward_payloads() {
        let Some(url) = database_url() else {
            eprintln!("DATABASE_URL not set; skipping");
            return;
        };
        let typename = unique_typename();
        let (store, _executor) = setup(&url, &typename).await;
        let store = Arc::new(store);

        let saga_id = Uuid::new_v4();
        store
            .save_saga(&Saga {
                id:           saga_id,
                state:        SagaState::Pending,
                created_at:   chrono::Utc::now(),
                completed_at: None,
                metadata:     None,
            })
            .await
            .unwrap();
        let step = SagaStep {
            id: Uuid::new_v4(),
            saga_id,
            order: 0,
            subgraph: "orders".to_string(),
            remote: false,
            mutation_type: MutationType::Create,
            mutation_name: None,
            typename: typename.clone(),
            variables: json!({"id": "x1"}),
            state: StepState::Pending,
            result: None,
            started_at: None,
            completed_at: None,
            compensation_mutation: None,
            compensation_variables: None,
            required_fields: Vec::new(),
            compensation_error: None,
        };
        store.save_saga_step(&step).await.unwrap();

        // The forward payload legitimately contains a `deleted` soft-delete
        // flag — bait for the magic-key sniffer.
        store
            .update_saga_step_result(step.id, &json!({"id": "x1", "deleted": false}))
            .await
            .unwrap();
        store.update_saga_step_state(step.id, &StepState::Executing).await.unwrap();
        store.update_saga_step_state(step.id, &StepState::Completed).await.unwrap();
        store.update_saga_state(saga_id, &SagaState::Failed).await.unwrap();

        let status = SagaCompensator::with_store(Arc::clone(&store))
            .get_compensation_status(saga_id)
            .await
            .unwrap();
        assert!(
            status.is_none(),
            "no compensation ever ran — reporting anything (let alone a success inferred \
             from a forward payload's 'deleted' key) fabricates a rollback: {status:?}"
        );

        store.delete_saga(saga_id).await.unwrap();
    }
}
