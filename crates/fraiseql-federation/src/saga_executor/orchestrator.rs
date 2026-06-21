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

/// Wired forward-phase orchestration (the `unstable-saga` feature).
///
/// Additive: the fail-loud `execute_saga` / `get_execution_state` above keep their
/// signatures and behaviour in every build (the `#429` acceptance spec exercises
/// them). The real orchestration + state query are the `*_local` / `execution_state`
/// methods, gated behind `unstable-saga` until proven.
#[cfg(feature = "unstable-saga")]
mod wired {
    use fraiseql_db::traits::DatabaseAdapter;
    use uuid::Uuid;

    use super::super::{ExecutionState, SagaExecutor, StepExecutionResult, forward};
    use crate::{
        mutation_executor::FederationMutationExecutor,
        saga_store::{Result as SagaStoreResult, SagaState, SagaStoreError, StepState},
    };

    impl SagaExecutor {
        /// Execute all steps in a saga sequentially during the forward phase.
        ///
        /// Marks the saga `Executing`, then drives each step (ordered by
        /// `SagaStep::order`) through the internal `dispatch_step`: each step is
        /// marked `Executing`, its real mutation is dispatched, a successful result
        /// is persisted, and the step is marked `Completed` or `Failed`. Execution
        /// **stops at the first failed step** (compensation is decided separately);
        /// the saga is then marked `Completed` (all steps succeeded) or `Failed`.
        ///
        /// Step/saga state is never fabricated — a mutation failure persists a real
        /// `Failed` transition rather than a `Completed` one (audit H32).
        ///
        /// # Arguments
        ///
        /// * `saga_id` - ID of saga to execute
        /// * `mutation_executor` - Local mutation transport for the steps' subgraph
        ///
        /// # Errors
        ///
        /// Returns [`SagaStoreError::Database`] if no saga store is configured,
        /// [`SagaStoreError::SagaNotFound`] if the saga does not exist, or any
        /// store error encountered while loading steps or persisting state.
        pub async fn execute_saga_local<A: DatabaseAdapter>(
            &self,
            saga_id: Uuid,
            mutation_executor: &FederationMutationExecutor<A>,
        ) -> SagaStoreResult<Vec<StepExecutionResult>> {
            let store = self.store.as_ref().ok_or_else(|| {
                SagaStoreError::Database(
                    "forward saga execution requires a configured saga store".to_string(),
                )
            })?;

            // Mark the saga Executing first: a missing saga surfaces as
            // SagaNotFound from the store's row-count check rather than silently
            // executing zero steps.
            store.update_saga_state(saga_id, &SagaState::Executing).await?;

            let mut steps = store.load_saga_steps(saga_id).await?;
            steps.sort_by_key(|step| step.order);

            let mut results = Vec::with_capacity(steps.len());
            for step in &steps {
                store.update_saga_step_state(step.id, &StepState::Executing).await?;

                let (result, state) = Self::dispatch_step(mutation_executor, step).await;

                // Persist the real post-mutation entity only on success; a failed
                // step is marked Failed and carries no fabricated result payload.
                if let Some(data) = result.data.as_ref() {
                    store.update_saga_step_result(step.id, data).await?;
                }
                store.update_saga_step_state(step.id, &state).await?;

                let stop = !result.success;
                results.push(result);
                if stop {
                    break;
                }
            }

            let saga_state = forward::saga_state_for(&results);
            store.update_saga_state(saga_id, &saga_state).await?;
            Ok(results)
        }

        /// Get the current execution state of a saga, derived from persisted step
        /// states.
        ///
        /// Read-only: counts completed steps, reports the first `Executing` step as
        /// the current one, and flags the saga failed if any step is `Failed`.
        ///
        /// # Arguments
        ///
        /// * `saga_id` - ID of saga
        ///
        /// # Errors
        ///
        /// Returns [`SagaStoreError::Database`] if no saga store is configured, or
        /// any store error encountered while loading steps.
        pub async fn execution_state(&self, saga_id: Uuid) -> SagaStoreResult<ExecutionState> {
            let store = self.store.as_ref().ok_or_else(|| {
                SagaStoreError::Database(
                    "forward saga execution state requires a configured saga store".to_string(),
                )
            })?;

            let mut steps = store.load_saga_steps(saga_id).await?;
            steps.sort_by_key(|step| step.order);

            let total_steps = u32::try_from(steps.len()).unwrap_or(u32::MAX);
            let completed_steps = u32::try_from(
                steps.iter().filter(|step| step.state == StepState::Completed).count(),
            )
            .unwrap_or(u32::MAX);
            let failed = steps.iter().any(|step| step.state == StepState::Failed);
            let current_step = steps
                .iter()
                .find(|step| step.state == StepState::Executing)
                .map(|step| u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1));

            Ok(ExecutionState {
                saga_id,
                total_steps,
                completed_steps,
                current_step,
                failed,
                failure_reason: None,
            })
        }
    }
}
