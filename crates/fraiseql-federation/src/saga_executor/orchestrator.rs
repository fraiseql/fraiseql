//! Saga orchestration: multi-step execution and state queries.

use ::tracing::info;
use uuid::Uuid;

use super::{ExecutionState, SagaExecutor, StepExecutionResult};
use crate::saga_store::{Result as SagaStoreResult, SagaStoreError};

impl SagaExecutor {
    /// Execute all steps in a saga sequentially.
    ///
    /// # Status
    ///
    /// **Not implemented.** The forward-phase driver previously transitioned the
    /// saga to `Executing`, invoked the fabricating [`Self::execute_step`], and
    /// persisted a `Completed` saga state without performing any real mutation
    /// (audit H32). It now fails loud instead of persisting fabricated progress.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga to execute
    ///
    /// # Errors
    ///
    /// Always returns [`SagaStoreError::NotImplemented`].
    pub async fn execute_saga(&self, saga_id: Uuid) -> SagaStoreResult<Vec<StepExecutionResult>> {
        info!(
            saga_id = %saga_id,
            "Saga forward phase requested but distributed saga execution is unwired"
        );

        Err(SagaStoreError::NotImplemented {
            operation: "SagaExecutor::execute_saga".to_string(),
        })
    }

    /// Get current execution state of saga.
    ///
    /// # Status
    ///
    /// **Not implemented.** The reported `ExecutionState` was derived from saga
    /// step states that are only ever fabricated by the unwired forward phase, so
    /// any value returned here would misrepresent real progress. It fails loud
    /// until distributed saga execution is wired.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga
    ///
    /// # Errors
    ///
    /// Always returns [`SagaStoreError::NotImplemented`].
    pub async fn get_execution_state(&self, saga_id: Uuid) -> SagaStoreResult<ExecutionState> {
        info!(
            saga_id = %saga_id,
            "Saga execution-state query requested but distributed saga execution is unwired"
        );

        Err(SagaStoreError::NotImplemented {
            operation: "SagaExecutor::get_execution_state".to_string(),
        })
    }
}
