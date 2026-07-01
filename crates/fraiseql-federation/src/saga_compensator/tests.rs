#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;
use crate::saga_store::SagaStoreError;

#[test]
fn test_saga_compensator_creation() {
    let compensator = SagaCompensator::new();
    drop(compensator);
}

#[test]
fn test_saga_compensator_default() {
    let _compensator = SagaCompensator::default();
    // Default should work
}

#[test]
fn test_saga_compensator_with_store() {
    // Test that we can create a compensator; full store testing requires a database.
    let compensator = SagaCompensator::new();
    assert!(!compensator.has_store());
}

#[test]
fn test_saga_compensator_has_store_method() {
    // Verify has_store() correctly reports status
    let compensator = SagaCompensator::new();
    assert!(!compensator.has_store());
}

/// H33: `compensate_step` must fail loud — it previously simulated a successful
/// compensation and persisted a fabricated `{"deleted": true}` document.
#[tokio::test]
async fn compensate_step_fails_loud() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator
        .compensate_step(saga_id, 1, "testCompensation", &serde_json::json!({}), "test-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "compensate_step must fail loud, got: {result:?}"
    );
}

/// H33: the compensation driver must fail loud rather than reporting a
/// fabricated `Compensated` status.
#[tokio::test]
async fn compensate_saga_fails_loud() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator.compensate_saga(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "compensate_saga must fail loud, got: {result:?}"
    );
}

/// H33: every reverse-order compensation must fail loud (no fabricated success).
#[tokio::test]
async fn compensate_step_reverse_order_all_fail_loud() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    for step_num in (1..=3).rev() {
        let result = compensator
            .compensate_step(saga_id, step_num, "deleteEntity", &serde_json::json!({}), "svc")
            .await;
        assert!(
            matches!(result, Err(SagaStoreError::NotImplemented { .. })),
            "compensate_step {step_num} must fail loud, got: {result:?}"
        );
    }
}

/// H33: the `operation` string must identify the failing entry point.
#[tokio::test]
async fn compensate_step_operation_is_descriptive() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator
        .compensate_step(saga_id, 1, "deleteEntity", &serde_json::json!({}), "svc")
        .await;

    match result {
        Err(SagaStoreError::NotImplemented { operation }) => {
            assert_eq!(operation, "SagaCompensator::compensate_step");
        },
        other => panic!("expected NotImplemented, got: {other:?}"),
    }
}

/// `get_compensation_status` is a read-only status query that never persists
/// state; without a store it honestly reports `None`.
#[tokio::test]
async fn get_compensation_status_without_store_is_none() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let status = compensator
        .get_compensation_status(saga_id)
        .await
        .unwrap_or_else(|e| panic!("expected Ok from get_compensation_status: {e}"));

    assert!(status.is_none(), "no store should yield no status");
}

/// Wired compensation (`unstable-saga`). The store-backed rollback paths are proven
/// end-to-end against real PostgreSQL in `tests/saga_integration.rs`; here we pin
/// the store-absent contract without any external service — a compensator with no
/// store fails loud (never a silent no-op) before touching the executor. The
/// executor is an in-memory SQLite one purely to satisfy the type; it is never
/// reached.
#[cfg(feature = "unstable-saga")]
mod wired {
    use std::sync::Arc;

    use fraiseql_db::sqlite::SqliteAdapter;
    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        mutation_executor::FederationMutationExecutor,
        saga_compensator::SagaCompensator,
        saga_store::{MutationType, SagaStep, SagaStoreError, StepState},
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

    async fn order_executor() -> FederationMutationExecutor<SqliteAdapter> {
        let adapter =
            Arc::new(SqliteAdapter::with_pool_config("sqlite::memory:", 1, 1).await.unwrap());
        FederationMutationExecutor::new(adapter, order_metadata(), false)
    }

    fn completed_step() -> SagaStep {
        SagaStep {
            id:                     Uuid::new_v4(),
            saga_id:                Uuid::new_v4(),
            order:                  0,
            subgraph:               "orders".to_string(),
            mutation_type:          MutationType::Create,
            mutation_name:          Some("createOrder".to_string()),
            typename:               "Order".to_string(),
            variables:              json!({"id": "o1"}),
            state:                  StepState::Completed,
            result:                 None,
            started_at:             None,
            completed_at:           None,
            compensation_mutation:  Some("deleteOrder".to_string()),
            compensation_variables: Some(json!({"id": "o1"})),
        }
    }

    #[tokio::test]
    async fn compensate_step_local_without_store_fails_loud() {
        let compensator = SagaCompensator::new();
        let executor = order_executor().await;
        let result = compensator.compensate_step_local(&executor, &completed_step()).await;
        assert!(
            matches!(result, Err(SagaStoreError::Database(_))),
            "compensate_step_local without a store must fail loud, never no-op: {result:?}"
        );
    }

    #[tokio::test]
    async fn compensate_saga_local_without_store_fails_loud() {
        let compensator = SagaCompensator::new();
        let executor = order_executor().await;
        let result = compensator.compensate_saga_local(Uuid::new_v4(), &executor).await;
        assert!(
            matches!(result, Err(SagaStoreError::Database(_))),
            "compensate_saga_local without a store must fail loud, never no-op: {result:?}"
        );
    }
}
