//! Saga Compensation Phase Executor
//!
//! Executes compensation mutations during the rollback phase, implementing
//! the inverse operations needed to undo completed saga steps when later steps fail.
//!
//! # Architecture
//!
//! The compensation phase executor:
//! - Loads sagas from persistent storage
//! - Executes compensation steps in strict REVERSE order (N → N-1 → 1)
//! - Continues compensation even if individual steps fail (resilience)
//! - Captures and persists compensation results
//! - Tracks compensation state for monitoring and recovery
//! - Provides comprehensive observability and audit trails
//!
//! # Execution Flow
//!
//! ```text
//! Load Failed Saga from Store
//!    ↓
//! Identify Completed Steps (1..N-1)
//!    ↓
//! For Each Step in Reverse (N-1..1):
//!    ├─ Transition step to Compensating
//!    ├─ Execute compensation mutation via MutationExecutor
//!    ├─ Capture compensation result
//!    ├─ Persist compensation result to store
//!    ├─ On success: Transition to Compensated
//!    └─ On failure: Record error but continue with next step
//!
//! Update Saga State:
//!    ├─ If all compensated: Saga → Compensated
//!    └─ If any compensation failed: Saga → CompensationFailed
//! ```
//!
//! # Key Properties
//!
//! The compensation phase maintains several critical properties:
//!
//! 1. **Deterministic Order**: Always reverse (N-1, N-2, ..., 1)
//! 2. **Error Resilience**: Continues even if individual steps fail
//! 3. **Idempotency**: Safe to retry without side effects
//! 4. **Atomicity**: All-or-nothing state transitions (Compensating → final state)
//! 5. **Observability**: Full audit trail with metrics and tracing
//!
//! # Compensation Result Tracking
//!
//! Each compensation step is tracked with:
//! - Success/failure status
//! - Compensation result data (confirmation of rollback)
//! - Error details if failed
//! - Execution duration in milliseconds
//! - Timestamp (tracked by `saga_store`)
//!
//! Results are persisted for:
//! - **Audit trails**: What was compensated and when
//! - **Recovery analysis**: Which steps failed and why
//! - **Observability**: Metrics and distributed tracing
//! - **Compliance**: Records for regulatory requirements
//!
//! # Compensation State Machine
//!
//! ```text
//! Forward Phase Failure
//!         ↓
//! Load Saga (state: Failed)
//!         ↓
//! Transition to: Compensating
//!         ↓
//! For Each Step in Reverse (N-1..1):
//!    ├─ Execute compensation mutation
//!    ├─ Record result (success/failure)
//!    └─ Continue regardless of outcome
//!         ↓
//! Determine Final Status:
//!    ├─ All success → Compensated
//!    ├─ Some fail → PartiallyCompensated
//!    └─ All fail → CompensationFailed
//!         ↓
//! Update Saga State & Persist Results
//! ```
//!
//! # Example
//!
//! ```text
//! // Requires: distributed saga infrastructure (PostgreSQL + message broker).
//! // See: tests/integration/ for runnable examples.
//! let compensator = SagaCompensator::new();
//!
//! // Execute compensation for a failed saga
//! let result = compensator.compensate_saga(saga_id).await?;
//!
//! match result.status {
//!     CompensationStatus::Compensated => {
//!         println!("All steps rolled back successfully");
//!     }
//!     CompensationStatus::PartiallyCompensated => {
//!         println!("Some compensations failed: {:?}", result.failed_steps);
//!     }
//!     CompensationStatus::CompensationFailed => {
//!         eprintln!("All compensations failed - manual intervention required");
//!     }
//! }
//! ```

use std::sync::Arc;

use ::tracing::{debug, info};
use uuid::Uuid;

use crate::saga_store::{PostgresSagaStore, Result as SagaStoreResult, SagaState, StepState};

/// Pure compensation-phase decision helpers (always compiled; see the module docs
/// for why the logic lives outside the feature gate).
mod compensation;

/// Represents the result of a compensation step execution
///
/// Contains the outcome of executing a single compensation mutation, including:
/// - Step number being compensated
/// - Success/failure status
/// - Compensation result data if successful (confirmation of rollback)
/// - Error details if failed
/// - Execution metrics (duration)
///
/// # Key Differences from Forward Execution
///
/// Compensation results differ from `StepExecutionResult` in important ways:
/// - **Focus**: Forward = "what data did we create?" → Compensation = "did we delete/undo it?"
/// - **Data**: Forward = business entity data → Compensation = confirmation flags (deleted,
///   `rolled_back`, etc.)
/// - **Error Tolerance**: Forward = stop on first error → Compensation = continue despite failures
/// - **Idempotency**: Compensation must be idempotent (safe to retry)
///
/// # Example Success Data
///
/// ```json
/// {
///   "deleted": true,
///   "confirmation_id": "comp-1-uuid",
///   "timestamp": "2026-01-28T10:30:45Z"
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CompensationStepResult {
    /// Original step number being compensated (1-indexed)
    pub step_number: u32,
    /// Whether compensation succeeded
    pub success:     bool,
    /// Confirmation data from compensation mutation if successful
    ///
    /// May contain:
    /// - `deleted`: true/false (for delete compensations)
    /// - `rolled_back`: true/false (for update compensations)
    /// - `restored`: true/false (for create compensations)
    /// - `confirmation_id`: ID or reference to rollback operation
    pub data:        Option<serde_json::Value>,
    /// Error message if compensation failed
    ///
    /// Includes:
    /// - Error type (network, timeout, mutation failed, etc.)
    /// - Subgraph context
    /// - Suggestion for manual recovery
    pub error:       Option<String>,
    /// Execution duration in milliseconds
    ///
    /// Measured from compensation start to completion (or failure)
    /// Useful for performance monitoring
    pub duration_ms: u64,
}

/// Overall status of compensation phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompensationStatus {
    /// All compensation steps completed successfully
    Compensated,
    /// Some compensation steps succeeded, but at least one failed
    PartiallyCompensated,
    /// Compensation phase failed completely (manual intervention may be needed)
    CompensationFailed,
}

/// Complete compensation result for a saga
///
/// Provides comprehensive tracking of the compensation phase execution,
/// including results for each compensated step and overall status.
/// Used for observability, recovery, and audit trails.
///
/// # Fields
/// - `saga_id`: Unique identifier for the saga being compensated
/// - `status`: Overall compensation outcome
/// - `step_results`: Detailed results for each step (in reverse execution order)
/// - `failed_steps`: List of step numbers where compensation failed (for quick lookup)
/// - `total_duration_ms`: Total time spent in compensation phase
/// - `error`: High-level error message if compensation failed completely
#[derive(Debug, Clone)]
pub struct CompensationResult {
    /// Saga ID that was compensated
    pub saga_id:           Uuid,
    /// Overall compensation status
    pub status:            CompensationStatus,
    /// Results for each compensated step (in reverse order: N-1..1)
    pub step_results:      Vec<CompensationStepResult>,
    /// Steps that failed compensation (step numbers)
    pub failed_steps:      Vec<u32>,
    /// Total compensation duration in milliseconds
    pub total_duration_ms: u64,
    /// Error message if status is `CompensationFailed`
    pub error:             Option<String>,
}

/// Saga compensation phase executor
///
/// Orchestrates the rollback of completed saga steps when a later step fails.
/// Executes compensation mutations in reverse order and provides resilience
/// through error tolerance and recovery capabilities.
pub struct SagaCompensator {
    /// Saga store for loading/saving compensation state
    /// Optional to support testing without database
    store: Option<Arc<PostgresSagaStore>>,
}

impl SagaCompensator {
    /// Create a new saga compensator without a saga store
    ///
    /// This is suitable for testing. For production, use `with_store()`.
    #[must_use]
    pub const fn new() -> Self {
        Self { store: None }
    }

    /// Create a new saga compensator with a saga store
    ///
    /// This enables persistence of compensation state and recovery from failures.
    #[must_use]
    pub const fn with_store(store: Arc<PostgresSagaStore>) -> Self {
        Self { store: Some(store) }
    }

    /// Check if compensator has a saga store configured
    #[must_use]
    pub const fn has_store(&self) -> bool {
        self.store.is_some()
    }

    /// Execute compensation for a failed saga.
    ///
    /// # Status
    ///
    /// **Not implemented.** The compensation driver previously transitioned the
    /// saga to `Compensating`, invoked the fabricating [`Self::compensate_step`]
    /// for each completed step, and persisted a `Compensated` state without
    /// performing any real rollback mutation (audit H33 / M-saga-coordinator).
    /// It now fails loud instead of persisting fabricated compensation progress.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of the saga to compensate
    ///
    /// # Errors
    ///
    /// Always returns
    /// [`SagaStoreError::NotImplemented`](crate::saga_store::SagaStoreError::NotImplemented).
    pub async fn compensate_saga(&self, saga_id: Uuid) -> SagaStoreResult<CompensationResult> {
        info!(
            saga_id = %saga_id,
            "Saga compensation requested but distributed saga compensation is unwired"
        );

        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCompensator::compensate_saga".to_string(),
        })
    }

    /// Compensate a single step.
    ///
    /// # Status
    ///
    /// **Not implemented.** This path previously simulated a successful
    /// compensation: it built a fake `{"deleted": true, ...}` confirmation
    /// document, persisted it over the forward result, and returned
    /// `success: true` without dispatching any compensation mutation (audit
    /// H33). It now fails loud and persists nothing.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga being compensated
    /// * `step_number` - Step number to compensate (1-indexed)
    /// * `compensation_mutation` - Name of compensation mutation
    /// * `original_result_data` - Result data from original forward step
    /// * `subgraph` - Target subgraph for compensation mutation
    ///
    /// # Errors
    ///
    /// Always returns
    /// [`SagaStoreError::NotImplemented`](crate::saga_store::SagaStoreError::NotImplemented);
    /// it must never persist a compensation result.
    pub async fn compensate_step(
        &self,
        saga_id: Uuid,
        step_number: u32,
        compensation_mutation: &str,
        _original_result_data: &serde_json::Value,
        subgraph: &str,
    ) -> SagaStoreResult<CompensationStepResult> {
        info!(
            saga_id = %saga_id,
            step = step_number,
            compensation_mutation = compensation_mutation,
            subgraph = subgraph,
            "Step compensation requested but distributed saga compensation is unwired"
        );

        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCompensator::compensate_step".to_string(),
        })
    }

    /// Get compensation status for a saga
    ///
    /// Retrieves the current compensation state without triggering new compensation.
    /// Useful for monitoring and recovery operations.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga
    ///
    /// # Returns
    ///
    /// Current `CompensationResult` if saga is or was in compensation phase
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError` if saga not found
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// let result = compensator.get_compensation_status(saga_id).await?;
    /// println!("Compensation status: {:?}", result.status);
    /// ```
    pub async fn get_compensation_status(
        &self,
        saga_id: Uuid,
    ) -> SagaStoreResult<Option<CompensationResult>> {
        debug!(saga_id = %saga_id, "Compensation status queried");

        // Load saga and steps to build compensation status

        // If no store available, return None
        let Some(store) = &self.store else {
            debug!(saga_id = %saga_id, "No saga store available - returning None");
            return Ok(None);
        };

        // Load saga to check if it's in compensation-related state
        let saga = store.load_saga(saga_id).await.map_err(|e| {
            debug!(saga_id = %saga_id, error = ?e, "Failed to load saga for compensation status");
            e
        })?;

        let Some(saga_data) = saga else {
            return Ok(None);
        };

        // Only return compensation results for sagas that have been compensated
        if saga_data.state != SagaState::Compensated
            && saga_data.state != SagaState::Compensating
            && saga_data.state != SagaState::Failed
        {
            return Ok(None);
        }

        // Load all steps to build compensation results
        let steps = store.load_saga_steps(saga_id).await.map_err(|e| {
            debug!(saga_id = %saga_id, error = ?e, "Failed to load saga steps for compensation status");
            e
        })?;

        // Build results for completed steps (which have compensation data in their results)
        let mut step_results = vec![];
        let failed_steps = vec![];

        for step in steps.iter().filter(|s| s.state == StepState::Completed) {
            // Check if the result contains compensation data (has "deleted" or "confirmation_id")
            let has_compensation = step
                .result
                .as_ref()
                .is_some_and(|r| r.get("deleted").is_some() || r.get("confirmation_id").is_some());

            if has_compensation {
                let success = true;
                #[allow(clippy::cast_possible_truncation)]
                // Reason: step count is bounded well below u32::MAX
                let step_number = step.order as u32;
                step_results.push(CompensationStepResult {
                    step_number,
                    success,
                    data: step.result.clone(),
                    error: None,
                    duration_ms: 0,
                });
            }
        }

        // Determine status based on saga state and failed steps
        let status = if saga_data.state == SagaState::Compensated {
            CompensationStatus::Compensated
        } else if !failed_steps.is_empty() {
            CompensationStatus::PartiallyCompensated
        } else {
            CompensationStatus::CompensationFailed
        };

        let result = CompensationResult {
            saga_id,
            status,
            step_results,
            failed_steps,
            total_duration_ms: 0,
            error: None,
        };

        debug!(saga_id = %saga_id, status = ?result.status, "Compensation status retrieved");
        Ok(Some(result))
    }
}

impl Default for SagaCompensator {
    fn default() -> Self {
        Self::new()
    }
}

/// Wired compensation phase (the `unstable-saga` feature).
///
/// Additive: the fail-loud [`SagaCompensator::compensate_saga`] /
/// [`SagaCompensator::compensate_step`] above keep their signatures and behaviour
/// in every build (the `#429` H33 acceptance spec exercises them). The real
/// local-SQL rollback is exposed as the `*_local` methods, gated behind
/// `unstable-saga` until proven. Remote (HTTP) compensation is Phase 04.
#[cfg(feature = "unstable-saga")]
mod wired {
    use std::collections::HashMap;

    use fraiseql_db::traits::DatabaseAdapter;
    use reqwest::Url;
    use uuid::Uuid;

    use super::{
        CompensationResult, CompensationStatus, CompensationStepResult, SagaCompensator,
        compensation,
    };
    use crate::{
        mutation_executor::FederationMutationExecutor,
        mutation_http_client::{HttpMutationClient, resolve_remote},
        saga_store::{Result as SagaStoreResult, SagaState, SagaStoreError, StepState},
    };

    impl SagaCompensator {
        /// Dispatch a single step's compensation (inverse) mutation and map the
        /// outcome to a [`CompensationStepResult`] — the compensation analog of
        /// `SagaExecutor::dispatch_step`. Pure dispatch with **no persistence**:
        /// [`Self::compensate_step_local`] / [`Self::compensate_saga_local`] own the
        /// step-state write. Routing mirrors forward execution:
        /// - `remote = None` → the inverse runs against the local SQL adapter via
        ///   [`FederationMutationExecutor::execute_local_mutation`].
        /// - `remote = Some((client, url))` → the inverse is propagated over HTTPS to the peer
        ///   subgraph via [`HttpMutationClient::execute_mutation`], so a step that executed
        ///   remotely is rolled back on the same transport.
        ///
        /// A step with no registered compensation, or whose inverse mutation `Err`s
        /// (local or remote), is reported `success: false` — never a fabricated
        /// rollback (audit H33).
        pub(crate) async fn dispatch_compensation<A: DatabaseAdapter>(
            mutation_executor: &FederationMutationExecutor<A>,
            step: &crate::saga_store::SagaStep,
            remote: Option<(&HttpMutationClient, &Url)>,
        ) -> CompensationStepResult {
            let step_number = u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1);

            // A step with no registered compensation cannot be rolled back — report
            // a best-effort miss rather than fabricating a rollback (H33).
            if !compensation::step_is_compensatable(step) {
                return CompensationStepResult {
                    step_number,
                    success: false,
                    data: None,
                    error: Some("no compensation mutation registered".to_string()),
                    duration_ms: 0,
                };
            }
            // `step_is_compensatable` guaranteed a present, non-empty name.
            let mutation = step.compensation_mutation.as_deref().unwrap_or_default();

            // Compensation variables carry the entity key for the inverse mutation;
            // fall back to the forward variables when none were registered.
            let variables = step.compensation_variables.as_ref().unwrap_or(&step.variables);

            let started = std::time::Instant::now();
            let outcome = match remote {
                None => {
                    mutation_executor
                        .execute_local_mutation(&step.typename, mutation, variables)
                        .await
                },
                Some((client, url)) => {
                    client
                        .execute_mutation(
                            url.as_str(),
                            &step.typename,
                            mutation,
                            variables,
                            mutation_executor.metadata(),
                        )
                        .await
                },
            };
            let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

            compensation::compensation_result_from(step_number, &outcome, duration_ms)
        }

        /// Compensate a single completed step by executing its registered
        /// compensation (inverse) mutation, then persisting the rollback.
        ///
        /// The stored `compensation_mutation` name drives the mutation kind
        /// (`determine_mutation_type`), so a create is undone by a `delete…`
        /// compensation, etc.; the `compensation_variables` (falling back to the
        /// forward `variables`) carry the entity key. `remote = Some((client, url))`
        /// rolls the step back over HTTPS to a peer subgraph; `None` uses the local
        /// SQL adapter. On a successful inverse the step is persisted
        /// [`StepState::Compensated`]; a failed inverse or a step with no registered
        /// compensation leaves the step untouched and is reported `success: false` —
        /// a rollback that did not happen is never fabricated (audit H33).
        ///
        /// # Arguments
        ///
        /// * `mutation_executor` - Local mutation transport for the step's subgraph
        /// * `step` - The persisted (completed) step to roll back
        /// * `remote` - `Some((client, url))` to roll back over HTTPS, else local
        ///
        /// # Errors
        ///
        /// Returns [`SagaStoreError::Database`] if no saga store is configured, or
        /// any store error encountered while persisting the compensated state.
        pub async fn compensate_step_local<A: DatabaseAdapter>(
            &self,
            mutation_executor: &FederationMutationExecutor<A>,
            step: &crate::saga_store::SagaStep,
            remote: Option<(&HttpMutationClient, &Url)>,
        ) -> SagaStoreResult<CompensationStepResult> {
            let store = self.store.as_ref().ok_or_else(|| {
                SagaStoreError::Database(
                    "saga compensation requires a configured saga store".to_string(),
                )
            })?;

            let result = Self::dispatch_compensation(mutation_executor, step, remote).await;

            // Persist the rollback only when the inverse mutation actually ran: a
            // successful compensation transitions the step Compensated; a failed one
            // leaves it Completed for a later best-effort retry.
            if result.success {
                store.update_saga_step_state(step.id, &StepState::Compensated).await?;
            }

            Ok(result)
        }

        /// Execute the compensation phase for a saga: roll back every completed
        /// step in strict reverse execution order.
        ///
        /// Marks the saga `Compensating`, then for each completed step (most-recent
        /// first) dispatches [`Self::compensate_step_local`], continuing past
        /// individual failures (best-effort resilience). If every completed step
        /// rolled back the saga is marked [`SagaState::Compensated`]; if any step
        /// could not be compensated the saga stays [`SagaState::Failed`] and the
        /// result reports [`CompensationStatus::PartiallyCompensated`] — a saga is
        /// never marked `Compensated` having undone only part of its work (H33).
        ///
        /// # Arguments
        ///
        /// * `saga_id` - ID of the saga to compensate
        /// * `mutation_executor` - Local mutation transport for the steps' subgraph
        /// * `subgraph_urls` - Registered remote peers (subgraph name → base URL); a completed step
        ///   whose `subgraph` matches one is rolled back over HTTPS
        /// * `http_client` - HTTP client for remote rollback; `None` = local-only. A step is
        ///   compensated remotely only when **both** a client is present **and** its `subgraph`
        ///   resolves to a registered URL, so a mixed local/remote saga rolls back each step on its
        ///   own transport.
        ///
        /// # Errors
        ///
        /// Returns [`SagaStoreError::Database`] if no saga store is configured,
        /// [`SagaStoreError::SagaNotFound`] if the saga does not exist, or any store
        /// error encountered while loading steps or persisting state.
        pub async fn compensate_saga_local<A: DatabaseAdapter>(
            &self,
            saga_id: Uuid,
            mutation_executor: &FederationMutationExecutor<A>,
            subgraph_urls: &HashMap<String, Url>,
            http_client: Option<&HttpMutationClient>,
        ) -> SagaStoreResult<CompensationResult> {
            let store = self.store.as_ref().ok_or_else(|| {
                SagaStoreError::Database(
                    "saga compensation requires a configured saga store".to_string(),
                )
            })?;

            // Enter the compensation phase first: a missing saga surfaces as
            // SagaNotFound from the store's row-count check rather than silently
            // compensating nothing.
            store.update_saga_state(saga_id, &SagaState::Compensating).await?;

            let steps = store.load_saga_steps(saga_id).await?;
            let order = compensation::compensation_order(&steps);

            let overall = std::time::Instant::now();
            let mut step_results = Vec::with_capacity(order.len());
            let mut failed_steps = Vec::new();

            for step in order {
                // Roll back on the same transport the forward step used: remote when
                // the step's subgraph names a registered peer, otherwise local.
                let remote = resolve_remote(&step.subgraph, http_client, subgraph_urls);
                let result = self.compensate_step_local(mutation_executor, step, remote).await?;
                if !result.success {
                    failed_steps.push(result.step_number);
                }
                step_results.push(result);
            }

            let total_duration_ms =
                u64::try_from(overall.elapsed().as_millis()).unwrap_or(u64::MAX);

            // All completed steps rolled back → Compensated. Any miss (a failed
            // inverse or an unregistered compensation) → the saga stays Failed and is
            // reported PartiallyCompensated; never Compensated having undone part.
            let (status, saga_state, error) = if failed_steps.is_empty() {
                (CompensationStatus::Compensated, SagaState::Compensated, None)
            } else {
                (
                    CompensationStatus::PartiallyCompensated,
                    SagaState::Failed,
                    Some(format!("{} step(s) could not be compensated", failed_steps.len())),
                )
            };

            store.update_saga_state(saga_id, &saga_state).await?;

            Ok(CompensationResult {
                saga_id,
                status,
                step_results,
                failed_steps,
                total_duration_ms,
                error,
            })
        }
    }
}

#[cfg(test)]
mod tests;
