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

#[cfg(test)]
mod tests;
