//! Wired saga coordinator (the `unstable-saga` feature).
//!
//! Ties the three already-wired subsystems — forward execution
//! ([`SagaExecutor::execute_saga_local`]), compensation
//! ([`SagaCompensator::compensate_saga_local`]), and the store — into a single
//! public handle. A caller can create a saga with compensation metadata, execute
//! it end-to-end (forward + automatic rollback on failure), cancel it, and query
//! its status through one type.
//!
//! Additive by design: the loud-fail [`SagaCoordinator`](super::SagaCoordinator)
//! and its contract tests are untouched. This type carries the `store`, `executor`,
//! and `compensator` the loud-fail coordinator never had, so its methods are the
//! real thing — no stub shares their names. Remote (HTTP) step dispatch is Phase 04;
//! every mutation here runs against the local SQL adapter.

use std::sync::Arc;

use ::tracing::{info, warn};
use fraiseql_db::traits::DatabaseAdapter;
use uuid::Uuid;

use super::{
    CompensationStrategy, SagaResult, SagaStatus, SagaStep, coordination, validate_step_sequence,
};
use crate::{
    mutation_executor::FederationMutationExecutor,
    saga_compensator::SagaCompensator,
    saga_executor::SagaExecutor,
    saga_store::{
        PostgresSagaStore, Result as SagaStoreResult, Saga, SagaState, SagaStep as StoreSagaStep,
        SagaStoreError, StepState,
    },
};

/// A fully-wired saga coordinator over a Postgres saga store.
///
/// Owns the store plus a [`SagaExecutor`] and [`SagaCompensator`] built over the
/// same store, and delegates each lifecycle operation to them. Unlike the loud-fail
/// [`SagaCoordinator`](super::SagaCoordinator), every method here performs real work
/// and persists real state.
pub struct WiredSagaCoordinator {
    /// How a failed saga is rolled back.
    strategy:    CompensationStrategy,
    /// Persistent saga store (shared with `executor`/`compensator`).
    store:       Arc<PostgresSagaStore>,
    /// Forward-phase executor over `store`.
    executor:    SagaExecutor,
    /// Compensation-phase executor over `store`.
    compensator: SagaCompensator,
}

impl WiredSagaCoordinator {
    /// Create a coordinator over `store` with the given compensation `strategy`.
    ///
    /// The forward executor and compensator are constructed over clones of the same
    /// store handle, so all three share one connection pool.
    #[must_use]
    pub fn new(store: Arc<PostgresSagaStore>, strategy: CompensationStrategy) -> Self {
        let executor = SagaExecutor::with_store(Arc::clone(&store));
        let compensator = SagaCompensator::with_store(Arc::clone(&store));
        Self {
            strategy,
            store,
            executor,
            compensator,
        }
    }

    /// The compensation strategy this coordinator applies on failure.
    #[must_use]
    pub const fn strategy(&self) -> CompensationStrategy {
        self.strategy
    }

    /// Create and persist a new saga from ordered `steps`, in state `Pending`.
    ///
    /// Validates the steps (present, sequentially numbered 1..N) before any write,
    /// then persists the saga and each step — including its compensation mutation +
    /// variables so a later rollback can find them. Each step's forward
    /// `mutation_name` is mapped to the persisted mutation kind
    /// (`coordination::mutation_type_for`); a name with no recognised verb is
    /// rejected rather than defaulted, so a saga whose steps cannot be dispatched is
    /// never created.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::Database`] if the steps are empty, out of order, or
    /// carry a forward mutation name whose kind cannot be determined; or any store
    /// error encountered while persisting the saga or its steps.
    pub async fn create_saga(&self, steps: Vec<SagaStep>) -> SagaStoreResult<Uuid> {
        validate_step_sequence(&steps)?;

        let saga_id = Uuid::new_v4();
        let saga = Saga {
            id:           saga_id,
            state:        SagaState::Pending,
            created_at:   chrono::Utc::now(),
            completed_at: None,
            metadata:     None,
        };
        self.store.save_saga(&saga).await?;

        for step in &steps {
            let mutation_type =
                coordination::mutation_type_for(&step.mutation_name).ok_or_else(|| {
                    SagaStoreError::Database(format!(
                        "cannot determine mutation type for step {} operation '{}': expected a \
                         name beginning with create/add, update/modify, or delete/remove",
                        step.number, step.mutation_name
                    ))
                })?;

            // An empty compensation mutation means the step has no registered
            // rollback → store None so the compensator skips it (best-effort, #429).
            let (compensation_mutation, compensation_variables) =
                if step.compensation_mutation.is_empty() {
                    (None, None)
                } else {
                    (
                        Some(step.compensation_mutation.clone()),
                        Some(step.compensation_variables.clone()),
                    )
                };

            let store_step = StoreSagaStep {
                id: step.id,
                saga_id,
                // Coordinator steps are 1-indexed (validated); the store is 0-based.
                order: (step.number as usize).saturating_sub(1),
                subgraph: step.subgraph.clone(),
                mutation_type,
                typename: step.typename.clone(),
                variables: step.variables.clone(),
                state: StepState::Pending,
                result: None,
                started_at: None,
                completed_at: None,
                compensation_mutation,
                compensation_variables,
            };
            self.store.save_saga_step(&store_step).await?;
        }

        info!(saga_id = %saga_id, steps = steps.len(), "Saga created and persisted (Pending)");
        Ok(saga_id)
    }

    /// Execute a saga end-to-end: run the forward phase, then — on any step failure
    /// under the `Automatic` strategy — roll back the completed steps.
    ///
    /// On full success the saga is `Completed` and `compensated` is false. On a step
    /// failure the returned result is `Failed`; under
    /// [`CompensationStrategy::Automatic`] the completed steps are compensated and
    /// `compensated` is true, while under [`CompensationStrategy::Manual`] the saga
    /// is left `Failed` for an operator to compensate explicitly (`compensated` is
    /// false). `completed_steps` counts the steps whose forward mutation succeeded.
    ///
    /// # Errors
    ///
    /// Returns any store error from the forward or compensation phase — e.g.
    /// [`SagaStoreError::SagaNotFound`] if `saga_id` does not exist.
    pub async fn execute_saga<A: DatabaseAdapter>(
        &self,
        saga_id: Uuid,
        mutation_executor: &FederationMutationExecutor<A>,
    ) -> SagaStoreResult<SagaResult> {
        let results = self.executor.execute_saga_local(saga_id, mutation_executor).await?;

        let total_steps =
            u32::try_from(self.store.load_saga_steps(saga_id).await?.len()).unwrap_or(u32::MAX);
        let completed_steps =
            u32::try_from(results.iter().filter(|r| r.success).count()).unwrap_or(u32::MAX);

        if results.iter().all(|r| r.success) {
            info!(saga_id = %saga_id, "Saga completed: every step succeeded");
            return Ok(SagaResult {
                saga_id,
                state: SagaState::Completed,
                completed_steps,
                total_steps,
                error: None,
                compensated: false,
            });
        }

        let error = results.iter().find(|r| !r.success).and_then(|r| r.error.clone());

        // The strategy governs rollback: Automatic compensates the completed steps
        // immediately; Manual leaves the saga Failed for an operator to trigger
        // compensation explicitly. A rollback that did not run is never reported.
        let compensated = match self.strategy {
            CompensationStrategy::Automatic => {
                warn!(saga_id = %saga_id, "Saga failed; compensating completed steps");
                self.compensator.compensate_saga_local(saga_id, mutation_executor).await?;
                true
            },
            CompensationStrategy::Manual => {
                warn!(
                    saga_id = %saga_id,
                    "Saga failed; Manual strategy leaves compensation to an operator"
                );
                false
            },
        };

        Ok(SagaResult {
            saga_id,
            state: SagaState::Failed,
            completed_steps,
            total_steps,
            error,
            compensated,
        })
    }

    /// Query the live status of a saga.
    ///
    /// Reports the persisted state, total and completed step counts, the currently
    /// executing step (if any, as a 1-indexed number), and progress as a percentage
    /// of completed steps.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::SagaNotFound`] if `saga_id` does not exist, or any
    /// store error while loading the saga or its steps.
    pub async fn get_saga_status(&self, saga_id: Uuid) -> SagaStoreResult<SagaStatus> {
        let saga = self
            .store
            .load_saga(saga_id)
            .await?
            .ok_or(SagaStoreError::SagaNotFound(saga_id))?;
        let steps = self.store.load_saga_steps(saga_id).await?;

        let step_count = u32::try_from(steps.len()).unwrap_or(u32::MAX);
        let completed_steps =
            u32::try_from(steps.iter().filter(|s| s.state == StepState::Completed).count())
                .unwrap_or(u32::MAX);
        let current_step = steps
            .iter()
            .find(|s| s.state == StepState::Executing)
            .map(|s| u32::try_from(s.order).unwrap_or(u32::MAX).saturating_add(1));
        let progress_percentage = coordination::progress_percentage(completed_steps, step_count);

        Ok(SagaStatus {
            saga_id,
            state: saga.state,
            step_count,
            completed_steps,
            current_step,
            progress_percentage,
        })
    }

    /// Cancel an in-flight saga, rolling back any completed steps first.
    ///
    /// Refuses to cancel a saga already in a terminal state
    /// (`coordination::saga_state_is_terminal`). Any completed steps are
    /// compensated *before* the saga is marked `Cancelled`, so the `Cancelled`
    /// transition is the last, authoritative write; `compensated` reflects whether a
    /// rollback actually ran (a saga with nothing completed reports false, never a
    /// fabricated rollback).
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::SagaNotFound`] if `saga_id` does not exist,
    /// [`SagaStoreError::InvalidStateTransition`] if the saga is already terminal, or
    /// any store/compensation error encountered while cancelling.
    pub async fn cancel_saga<A: DatabaseAdapter>(
        &self,
        saga_id: Uuid,
        mutation_executor: &FederationMutationExecutor<A>,
    ) -> SagaStoreResult<SagaResult> {
        let saga = self
            .store
            .load_saga(saga_id)
            .await?
            .ok_or(SagaStoreError::SagaNotFound(saga_id))?;

        if coordination::saga_state_is_terminal(&saga.state) {
            warn!(
                saga_id = %saga_id,
                state = saga.state.as_str(),
                "Refusing to cancel a saga already in a terminal state"
            );
            return Err(SagaStoreError::InvalidStateTransition {
                from: saga.state.as_str().to_string(),
                to:   SagaState::Cancelled.as_str().to_string(),
            });
        }

        let steps = self.store.load_saga_steps(saga_id).await?;
        let total_steps = u32::try_from(steps.len()).unwrap_or(u32::MAX);
        let completed_steps =
            u32::try_from(steps.iter().filter(|s| s.state == StepState::Completed).count())
                .unwrap_or(u32::MAX);

        // Roll back completed work before the Cancelled transition: the compensator
        // drives the saga through Compensating, so Cancelled must be written last.
        let compensated = if completed_steps > 0 {
            self.compensator.compensate_saga_local(saga_id, mutation_executor).await?;
            true
        } else {
            false
        };

        self.store.update_saga_state(saga_id, &SagaState::Cancelled).await?;
        info!(saga_id = %saga_id, compensated, "Saga cancelled");

        Ok(SagaResult {
            saga_id,
            state: SagaState::Cancelled,
            completed_steps,
            total_steps,
            error: None,
            compensated,
        })
    }

    /// Assemble the final result of a saga from its persisted saga + step rows.
    ///
    /// `compensated` is true when the saga reached `Compensated` or any step was
    /// rolled back to `Compensated`.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::SagaNotFound`] if `saga_id` does not exist, or any
    /// store error while loading the saga or its steps.
    pub async fn get_saga_result(&self, saga_id: Uuid) -> SagaStoreResult<SagaResult> {
        let saga = self
            .store
            .load_saga(saga_id)
            .await?
            .ok_or(SagaStoreError::SagaNotFound(saga_id))?;
        let steps = self.store.load_saga_steps(saga_id).await?;

        let total_steps = u32::try_from(steps.len()).unwrap_or(u32::MAX);
        let completed_steps =
            u32::try_from(steps.iter().filter(|s| s.state == StepState::Completed).count())
                .unwrap_or(u32::MAX);
        let compensated = saga.state == SagaState::Compensated
            || steps.iter().any(|s| s.state == StepState::Compensated);

        Ok(SagaResult {
            saga_id,
            state: saga.state,
            completed_steps,
            total_steps,
            error: None,
            compensated,
        })
    }

    /// List the ids of all in-flight sagas (`Executing` or `Pending`).
    ///
    /// # Errors
    ///
    /// Returns any store error encountered while querying by state.
    pub async fn list_in_flight_sagas(&self) -> SagaStoreResult<Vec<Uuid>> {
        let mut ids = Vec::new();
        for state in [SagaState::Executing, SagaState::Pending] {
            let sagas = self.store.load_sagas_by_state(&state).await?;
            ids.extend(sagas.into_iter().map(|saga| saga.id));
        }
        Ok(ids)
    }
}
