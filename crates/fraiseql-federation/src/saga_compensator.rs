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

use std::{collections::HashMap, sync::Arc};

use ::tracing::debug;
use fraiseql_db::traits::DatabaseAdapter;
use reqwest::Url;
use uuid::Uuid;

use crate::{
    mutation_executor::FederationMutationExecutor,
    mutation_http_client::{HttpMutationClient, resolve_remote},
    saga_store::{
        PostgresSagaStore, Result as SagaStoreResult, SagaState, SagaStoreError, StepState,
    },
};

/// Pure compensation-phase decision helpers.
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
    /// The compensation phase is still running (the saga is `Compensating`);
    /// the reported step results are the evidence recorded so far, not a final
    /// verdict (#767).
    InProgress,
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

    /// Get compensation status for a saga, **read from recorded state** (#767).
    ///
    /// Every entry is persisted evidence, never an inference:
    /// - a step in [`StepState::Compensated`] is a recorded successful rollback;
    /// - a step with a recorded `compensation_error` is a recorded failed rollback (it appears in
    ///   `failed_steps`, 1-indexed like every other step-number API);
    /// - a saga with **no** recorded compensation evidence was never in a compensation phase →
    ///   `Ok(None)`. In particular, a forward payload that happens to contain keys like `deleted`
    ///   is *forward* data and is never interpreted as a rollback.
    ///
    /// The status derives from the evidence: all recorded attempts succeeded →
    /// [`CompensationStatus::Compensated`]; some succeeded and some failed →
    /// [`CompensationStatus::PartiallyCompensated`]; all failed →
    /// [`CompensationStatus::CompensationFailed`]. A saga still
    /// [`SagaState::Compensating`] reports the evidence recorded so far.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga
    ///
    /// # Errors
    ///
    /// Returns any store error encountered while loading the saga or its steps.
    pub async fn get_compensation_status(
        &self,
        saga_id: Uuid,
    ) -> SagaStoreResult<Option<CompensationResult>> {
        debug!(saga_id = %saga_id, "Compensation status queried");

        let Some(store) = &self.store else {
            debug!(saga_id = %saga_id, "No saga store available - returning None");
            return Ok(None);
        };

        let Some(saga) = store.load_saga(saga_id).await? else {
            return Ok(None);
        };

        let mut steps = store.load_saga_steps(saga_id).await?;
        steps.sort_by_key(|step| step.order);

        let mut step_results = Vec::new();
        let mut failed_steps = Vec::new();
        for step in &steps {
            let step_number = u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1);
            if step.state == StepState::Compensated {
                step_results.push(CompensationStepResult {
                    step_number,
                    success: true,
                    // No inverse-mutation payload is persisted; the recorded
                    // Compensated transition is the evidence. The forward
                    // payload is NOT echoed here — it is not rollback data.
                    data: None,
                    error: None,
                    duration_ms: 0,
                });
            } else if let Some(error) = &step.compensation_error {
                failed_steps.push(step_number);
                step_results.push(CompensationStepResult {
                    step_number,
                    success: false,
                    data: None,
                    error: Some(error.clone()),
                    duration_ms: 0,
                });
            }
        }

        // No recorded evidence and not mid-compensation → the saga was never in
        // a compensation phase; there is no status to report.
        if step_results.is_empty() && saga.state != SagaState::Compensating {
            return Ok(None);
        }

        let succeeded = step_results.iter().any(|r| r.success);
        let status = if saga.state == SagaState::Compensating {
            // Mid-flight: whatever is recorded so far is not a final verdict.
            CompensationStatus::InProgress
        } else if failed_steps.is_empty() {
            CompensationStatus::Compensated
        } else if succeeded {
            CompensationStatus::PartiallyCompensated
        } else {
            CompensationStatus::CompensationFailed
        };

        let error = if failed_steps.is_empty() {
            None
        } else {
            Some(format!("{} step(s) could not be compensated", failed_steps.len()))
        };

        let result = CompensationResult {
            saga_id,
            status,
            step_results,
            failed_steps,
            total_duration_ms: 0,
            error,
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

/// Rollback (compensation) execution: each completed step is compensated on the same
/// transport its forward step used — the local SQL adapter, or over HTTPS to a
/// registered peer subgraph.
impl SagaCompensator {
    /// Dispatch a single step's compensation (inverse) mutation and map the
    /// outcome to a [`CompensationStepResult`] — the compensation analog of
    /// `SagaExecutor::dispatch_step`. Pure dispatch with **no persistence**:
    /// [`Self::compensate_step`] / [`Self::compensate_saga`] own the
    /// step-state write. Routing mirrors forward execution:
    /// - `remote = None` → the inverse runs against the local SQL adapter via
    ///   [`FederationMutationExecutor::execute_local_mutation`].
    /// - `remote = Some((client, url))` → the inverse is propagated over HTTPS to the peer subgraph
    ///   via [`HttpMutationClient::execute_mutation`], so a step that executed remotely is rolled
    ///   back on the same transport.
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

        // The compensation is its own logical mutation, distinct from the forward
        // step, so it carries its own stable idempotency key derived from the
        // step id (#747): a retried or crash-replayed rollback deduplicates,
        // and can never collide with the forward dispatch's key.
        let idempotency_key = format!("{}:compensate", step.id);
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
                        Some(&idempotency_key),
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
    pub async fn compensate_step<A: DatabaseAdapter>(
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

        // Persist the rollback outcome either way (#767): a successful
        // compensation transitions the step Compensated (and clears any earlier
        // recorded failure); a failed one leaves the step Completed for a later
        // best-effort retry and records WHY, so the status API reports recorded
        // reality rather than inferring one.
        if result.success {
            store.update_saga_step_state(step.id, &StepState::Compensated).await?;
            store.record_step_compensation_error(step.id, None).await?;
        } else {
            store
                .record_step_compensation_error(
                    step.id,
                    Some(result.error.as_deref().unwrap_or("compensation failed")),
                )
                .await?;
        }

        Ok(result)
    }

    /// Execute the compensation phase for a saga: roll back every completed
    /// step in strict reverse execution order.
    ///
    /// Marks the saga `Compensating`, then for each completed step (most-recent
    /// first) dispatches [`Self::compensate_step`], continuing past
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
    ///   resolves to a registered URL, so a mixed local/remote saga rolls back each step on its own
    ///   transport.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::Database`] if no saga store is configured,
    /// [`SagaStoreError::SagaNotFound`] if the saga does not exist, or any store
    /// error encountered while loading steps or persisting state.
    pub async fn compensate_saga<A: DatabaseAdapter>(
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
            let result = self.compensate_step(mutation_executor, step, remote).await?;
            if !result.success {
                failed_steps.push(result.step_number);
            }
            step_results.push(result);
        }

        let total_duration_ms = u64::try_from(overall.elapsed().as_millis()).unwrap_or(u64::MAX);

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

#[cfg(test)]
mod tests;
