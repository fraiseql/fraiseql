#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use uuid::Uuid;

use super::SagaExecutor;
use crate::saga_store::SagaStoreError;

#[test]
fn test_saga_executor_creation() {
    let executor = SagaExecutor::new();
    drop(executor);
}

#[test]
fn test_saga_executor_default() {
    let _executor = SagaExecutor::default();
    // Default should work
}

#[test]
fn test_saga_executor_with_store() {
    // Test that we can create an executor; full store testing requires a
    // database (integration tests).
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

#[test]
fn test_saga_executor_has_store_method() {
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

/// H32: `execute_step` must fail loud — distributed saga execution is unwired,
/// so it must never fabricate a result or persist a step transition.
#[tokio::test]
async fn execute_step_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "testMutation", &serde_json::json!({}), "test-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step must fail loud, got: {result:?}"
    );
}

/// H32: the no-store path must also fail loud (it previously returned a
/// fabricated placeholder success).
#[tokio::test]
async fn execute_step_without_store_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "createOrder", &serde_json::json!({}), "orders-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step without store must fail loud, got: {result:?}"
    );
}

/// H32: the forward-phase driver must fail loud rather than reporting empty or
/// fabricated step results.
#[tokio::test]
async fn execute_saga_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor.execute_saga(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_saga must fail loud, got: {result:?}"
    );
}

/// H32: `get_execution_state` derived its values from fabricated step states; it
/// must now fail loud.
#[tokio::test]
async fn get_execution_state_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor.get_execution_state(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "get_execution_state must fail loud, got: {result:?}"
    );
}

/// H32: the `operation` string must identify the failing entry point so callers
/// and logs can tell the unwired paths apart.
#[tokio::test]
async fn execute_step_operation_is_descriptive() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "mutation", &serde_json::json!({}), "service")
        .await;

    match result {
        Err(SagaStoreError::NotImplemented { operation }) => {
            assert_eq!(operation, "SagaExecutor::execute_step");
        },
        other => panic!("expected NotImplemented, got: {other:?}"),
    }
}

/// Wired forward-phase execution (`unstable-saga`). `execute_step_local` dispatches
/// the step's real local mutation through a `DatabaseAdapter` and reports the
/// outcome without fabricating success. Proven here against an in-memory SQLite
/// adapter (single connection so the schema is shared) — no external service.
#[cfg(feature = "unstable-saga")]
mod wired {
    use std::sync::Arc;

    use fraiseql_db::sqlite::SqliteAdapter;
    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        mutation_executor::FederationMutationExecutor,
        saga_executor::SagaExecutor,
        saga_store::{MutationType, SagaStep, StepState},
        types::{FederatedType, FederationMetadata, KeyDirective},
    };

    fn order_metadata() -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types: vec![FederatedType {
                name:                "Order".to_string(),
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

    fn order_step(mutation_type: MutationType, variables: serde_json::Value) -> SagaStep {
        SagaStep {
            id: Uuid::new_v4(),
            saga_id: Uuid::new_v4(),
            order: 0,
            subgraph: "orders".to_string(),
            mutation_type,
            typename: "Order".to_string(),
            variables,
            state: StepState::Pending,
            result: None,
            started_at: None,
            completed_at: None,
            compensation_mutation: None,
            compensation_variables: None,
        }
    }

    /// Single-connection in-memory SQLite with an `"order"` table — the
    /// `lowercase(typename)` table the federation mutation builder targets.
    /// Returns the wired executor plus a handle to the same adapter so a test can
    /// read the database back directly.
    async fn order_table_executor()
    -> (FederationMutationExecutor<SqliteAdapter>, Arc<SqliteAdapter>) {
        use fraiseql_db::traits::DatabaseAdapter;

        // A single connection keeps the schema visible across queries: each
        // `sqlite::memory:` connection is otherwise a separate database.
        let adapter =
            Arc::new(SqliteAdapter::with_pool_config("sqlite::memory:", 1, 1).await.unwrap());
        adapter
            .execute_raw_query("CREATE TABLE \"order\" (id TEXT PRIMARY KEY, total TEXT)")
            .await
            .unwrap();
        let executor =
            FederationMutationExecutor::new(Arc::clone(&adapter), order_metadata(), false);
        (executor, adapter)
    }

    #[tokio::test]
    async fn execute_step_local_dispatches_real_create() {
        use fraiseql_db::traits::DatabaseAdapter;

        let executor = SagaExecutor::new();
        let (mutation_executor, adapter) = order_table_executor().await;
        let step = order_step(MutationType::Create, json!({"id": "o1", "total": "100"}));

        let result = executor.execute_step_local(&mutation_executor, &step).await;

        assert!(result.success, "a successful create must report success: {result:?}");
        assert_eq!(result.step_number, 1, "0-based order maps to 1-indexed step number");
        let data = result.data.expect("a successful step must carry the read-back entity");
        assert_eq!(data["id"], "o1", "result must reflect the real inserted row: {data}");
        assert_eq!(data["__typename"], "Order");

        // The row really landed in the database — not a fabricated response.
        let rows = adapter
            .execute_raw_query("SELECT id FROM \"order\" WHERE id = 'o1'")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "the create must have persisted a real row");
    }

    #[tokio::test]
    async fn execute_step_local_failed_mutation_reports_failure_not_fabricated_success() {
        let executor = SagaExecutor::new();
        let (mutation_executor, _adapter) = order_table_executor().await;
        // UPDATE targeting an id that does not exist → 0 rows → NotFound. The
        // step must report failure, never a fabricated Completed (audit H32).
        let step = order_step(MutationType::Update, json!({"id": "missing", "total": "5"}));

        let result = executor.execute_step_local(&mutation_executor, &step).await;

        assert!(!result.success, "a 0-row update must report failure: {result:?}");
        assert!(result.data.is_none(), "a failed step must not fabricate result data");
        assert!(result.error.is_some(), "a failed step must carry the error: {result:?}");
    }
}
