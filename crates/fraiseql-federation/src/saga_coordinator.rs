//! Federation Saga Coordinator
//!
//! Coordinates distributed transaction execution across multiple federation subgraphs
//! using the saga pattern. Implements forward phase (execution) and compensation phase
//! (rollback) for transactional consistency.
//!
//! # Architecture
//!
//! The saga coordinator manages:
//! - Sequential step execution across subgraphs
//! - Automatic compensation on failure
//! - Persistent saga state (via `SagaStore`)
//! - Recovery from interruptions
//! - Structured logging and observability
//!
//! # State Machine
//!
//! ```text
//! Pending → Executing → Completed (all steps ok)
//!           ↓
//!       Failed → Compensating → Compensated (rolled back)
//! ```
//!
//! # Saga Execution Flow
//!
//! 1. **Creation**: User provides ordered steps with forward and compensation mutations
//! 2. **Validation**: Verify all steps are valid and subgraphs exist
//! 3. **Forward Phase**: Execute steps 1..N in order
//!    - Each step: Load data → Check @requires → Execute mutation → Persist result
//!    - On success: Move to next step
//!    - On failure: Transition to compensation phase
//! 4. **Compensation Phase** (on failure):
//!    - Execute compensation in reverse order (N..1)
//!    - Use original variables to build inverse operations
//!    - Continue even if compensation fails (collect all errors)
//! 5. **Completion**: Return final state with all step results
//!
//! # Example
//!
//! ```text
//! // Requires: distributed saga infrastructure (PostgreSQL + message broker).
//! // See: tests/integration/ for runnable examples.
//! // Create saga coordinator
//! let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
//!
//! // Define steps for multi-service transaction
//! let steps = vec![
//!     SagaStep::new(
//!         1, "orders-service", "Order", "createOrder",
//!         json!({"id": "order-1", "total": 100.0}),
//!         "deleteOrder", json!({"id": "order-1"})
//!     ),
//!     SagaStep::new(
//!         2, "inventory-service", "Inventory", "reserveInventory",
//!         json!({"orderId": "order-1", "items": [...]}),
//!         "releaseInventory", json!({"orderId": "order-1"})
//!     ),
//! ];
//!
//! // Create and execute saga
//! let saga_id = coordinator.create_saga(steps).await?;
//! let result = coordinator.execute_saga(saga_id).await?;
//!
//! match result.state {
//!     SagaState::Completed => println!("Success!"),
//!     SagaState::Compensated => println!("Failed and rolled back"),
//!     _ => println!("Unknown state"),
//! }
//! ```
//!
//! # Observability
//!
//! The coordinator integrates with the tracing system for comprehensive observability:
//! - `info!()` for saga lifecycle events (created, completed, failed)
//! - `debug!()` for step-level details (step started, compensation triggered)
//! - `warn!()` for failures and recoveries
//! - Structured logging with `saga_id`, `step_number`, subgraph context

use ::tracing::{debug, info, warn};
use uuid::Uuid;

use crate::saga_store::{RequiredField, Result as SagaStoreResult, SagaState};

/// Pure coordinator decision helpers (always compiled; see the module docs for why
/// the logic lives outside the feature gate).
mod coordination;

/// Wired coordinator that ties forward execution, compensation, and recovery into
/// a single handle (the `unstable-saga` feature). Additive: the loud-fail
/// [`SagaCoordinator`] below keeps its signatures and behaviour in every build.
#[cfg(feature = "unstable-saga")]
mod wired;
#[cfg(feature = "unstable-saga")]
pub use wired::WiredSagaCoordinator;

/// Represents a saga step mutation to execute
///
/// Each step defines:
/// - A forward mutation (executed during forward phase)
/// - A compensation mutation (executed during rollback if needed)
/// - Variables for both operations
///
/// Steps must be ordered 1..N and will execute sequentially.
/// If any step fails, compensation is triggered automatically
/// (depending on strategy) in reverse order N..1.
#[derive(Debug, Clone)]
pub struct SagaStep {
    /// Unique step identifier
    pub id:                     Uuid,
    /// Position in saga (1-indexed, must be sequential)
    pub number:                 u32,
    /// Target subgraph for this step's execution
    pub subgraph:               String,
    /// Entity type being mutated (e.g., "Order", "Payment")
    pub typename:               String,
    /// Forward mutation operation name (e.g., "createOrder", "recordPayment")
    pub mutation_name:          String,
    /// Variables for forward mutation
    /// Must include all input fields for the mutation
    pub variables:              serde_json::Value,
    /// Compensation mutation operation name
    /// Usually inverse of `mutation_name` (e.g., `deleteOrder`, `reversePayment`)
    pub compensation_mutation:  String,
    /// Variables for compensation mutation
    /// Must be able to identify and reverse the forward mutation
    pub compensation_variables: serde_json::Value,
    /// Cross-subgraph `@requires` fields to pre-fetch before this step's mutation
    /// runs (empty = none). Set with [`SagaStep::with_required_fields`]. Each is
    /// resolved from its owning subgraph and merged into `variables` before
    /// dispatch; an unresolved field fails the step before its mutation runs.
    pub required_fields:        Vec<RequiredField>,
}

impl SagaStep {
    /// Create a new saga step
    ///
    /// # Arguments
    ///
    /// * `number` - Step position in saga (1 for first, 2 for second, etc.)
    /// * `subgraph` - Target subgraph name (e.g., "orders-service")
    /// * `typename` - Entity type being mutated
    /// * `mutation_name` - Forward mutation operation name
    /// * `variables` - Input variables for forward mutation
    /// * `compensation_mutation` - Compensation operation name (usually inverse)
    /// * `compensation_variables` - Input for compensation mutation
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// let step = SagaStep::new(
    ///     1,
    ///     "orders-service",
    ///     "Order",
    ///     "createOrder",
    ///     json!({"id": "order-1", "amount": 100.0}),
    ///     "deleteOrder",
    ///     json!({"id": "order-1"})
    /// );
    /// ```
    pub fn new(
        number: u32,
        subgraph: impl Into<String>,
        typename: impl Into<String>,
        mutation_name: impl Into<String>,
        variables: serde_json::Value,
        compensation_mutation: impl Into<String>,
        compensation_variables: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            number,
            subgraph: subgraph.into(),
            typename: typename.into(),
            mutation_name: mutation_name.into(),
            variables,
            compensation_mutation: compensation_mutation.into(),
            compensation_variables,
            required_fields: Vec::new(),
        }
    }

    /// Declare the cross-subgraph `@requires` fields this step depends on (builder).
    ///
    /// Each [`RequiredField`] is fetched from its owning subgraph's `_entities`
    /// endpoint and merged into this step's mutation `variables` before dispatch.
    /// The owning subgraphs must be registered on the coordinator
    /// ([`WiredSagaCoordinator::with_subgraph`](crate::saga_coordinator::WiredSagaCoordinator::with_subgraph))
    /// or `create_saga` rejects the saga at setup.
    #[must_use]
    pub fn with_required_fields(mut self, required_fields: Vec<RequiredField>) -> Self {
        self.required_fields = required_fields;
        self
    }
}

/// Compensation strategy for saga failures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum CompensationStrategy {
    /// Automatically compensate on any failure
    #[default]
    Automatic,
    /// Wait for manual compensation trigger
    Manual,
}

/// Saga execution result
#[derive(Debug, Clone)]
pub struct SagaResult {
    /// Saga identifier
    pub saga_id:         Uuid,
    /// Final state of saga
    pub state:           SagaState,
    /// Number of successfully executed steps
    pub completed_steps: u32,
    /// Total number of steps
    pub total_steps:     u32,
    /// Error message if failed
    pub error:           Option<String>,
    /// Compensation performed
    pub compensated:     bool,
}

/// Validate that a saga's steps are present and sequentially ordered (1..N).
///
/// Shared by the loud-fail [`SagaCoordinator::create_saga`] and the wired
/// [`WiredSagaCoordinator`](crate::saga_coordinator::WiredSagaCoordinator)::`create_saga`:
/// a saga must have at least one step and its steps must be numbered 1, 2, 3, … in
/// order before any persistence is attempted.
///
/// # Errors
///
/// Returns [`SagaStoreError::Database`](crate::saga_store::SagaStoreError::Database)
/// if the steps are empty or not in sequential order.
fn validate_step_sequence(steps: &[SagaStep]) -> SagaStoreResult<()> {
    // Validate at least one step
    if steps.is_empty() {
        warn!("Saga creation failed: saga must have at least one step");
        return Err(crate::saga_store::SagaStoreError::Database(
            "saga must have at least one step".to_string(),
        ));
    }

    // Validate step order
    for (i, step) in steps.iter().enumerate() {
        if step.number as usize != i + 1 {
            warn!(
                expected = i + 1,
                actual = step.number,
                "Saga creation failed: steps must be in sequential order"
            );
            return Err(crate::saga_store::SagaStoreError::Database(
                "steps must be in sequential order".to_string(),
            ));
        }
    }

    Ok(())
}

/// Saga coordinator for distributed transaction orchestration.
///
/// **Not implemented.** Every operational method fails loud with
/// [`SagaStoreError::NotImplemented`](crate::saga_store::SagaStoreError::NotImplemented)
/// — distributed saga coordination is unwired (see
/// [#429](https://github.com/fraiseql/fraiseql/issues/429)). The previous
/// `with_executor`/`with_compensator` builders accepted `Arc<dyn Any>` values
/// that were stored as `Arc::new(())` and never downcast or used (M-saga-coordinator);
/// they were removed rather than advertise wiring that did nothing.
pub struct SagaCoordinator {
    /// Compensation strategy
    strategy: CompensationStrategy,
}

impl SagaCoordinator {
    /// Create a new saga coordinator
    ///
    /// # Arguments
    ///
    /// * `strategy` - How to handle compensation on failure
    #[must_use]
    pub const fn new(strategy: CompensationStrategy) -> Self {
        Self { strategy }
    }

    /// Create a new saga with given steps
    ///
    /// Validates all steps are present and in correct order, then generates
    /// saga ID and persists initial state to the saga store.
    ///
    /// # Arguments
    ///
    /// * `steps` - Ordered list of mutations to execute (1..N in sequence)
    ///
    /// # Returns
    ///
    /// Saga ID for use in `execute_saga` and other operations
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Steps vector is empty
    /// - Steps are not in sequential order (1, 2, 3, ...)
    /// - Subgraphs don't exist (in full implementation)
    /// - Mutations don't exist (in full implementation)
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    /// let steps = vec![
    ///     SagaStep::new(1, "svc1", "Type1", "mut1", vars, "comp1", comp_vars),
    ///     SagaStep::new(2, "svc2", "Type2", "mut2", vars, "comp2", comp_vars),
    /// ];
    /// let saga_id = coordinator.create_saga(steps).await?;
    /// ```
    pub async fn create_saga(&self, steps: Vec<SagaStep>) -> SagaStoreResult<Uuid> {
        validate_step_sequence(&steps)?;

        // Input validation above is real; persistence is not. The previous body
        // generated a UUID and returned it WITHOUT writing anything to the store
        // (M-saga-coordinator), so callers held an id for a saga that did not
        // exist. Fail loud rather than hand back a fabricated id.
        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCoordinator::create_saga".to_string(),
        })
    }

    /// Execute a saga with all its steps.
    ///
    /// # Status
    ///
    /// **Not implemented.** This entry point previously returned a
    /// `SagaState::Completed` `SagaResult` for any saga id without loading the
    /// saga, executing a single step, or touching the store (audit
    /// M-saga-coordinator). It now fails loud rather than reporting fabricated
    /// completion.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of previously created saga
    ///
    /// # Errors
    ///
    /// Always returns
    /// [`SagaStoreError::NotImplemented`](crate::saga_store::SagaStoreError::NotImplemented).
    pub async fn execute_saga(&self, saga_id: Uuid) -> SagaStoreResult<SagaResult> {
        info!(
            saga_id = %saga_id,
            "Saga execution requested but distributed saga coordination is unwired"
        );

        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCoordinator::execute_saga".to_string(),
        })
    }

    /// Get status of executing saga.
    ///
    /// # Status
    ///
    /// **Not implemented.** This previously fabricated a `SagaState::Pending`
    /// status with zeroed counters for any saga id without consulting the store
    /// (audit M-saga-coordinator). It now fails loud rather than reporting a
    /// fabricated status.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga
    ///
    /// # Errors
    ///
    /// Always returns
    /// [`SagaStoreError::NotImplemented`](crate::saga_store::SagaStoreError::NotImplemented).
    pub async fn get_saga_status(&self, saga_id: Uuid) -> SagaStoreResult<SagaStatus> {
        info!(
            saga_id = %saga_id,
            "Saga status requested but distributed saga coordination is unwired"
        );

        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCoordinator::get_saga_status".to_string(),
        })
    }

    /// Cancel an in-flight saga
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga to cancel
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError` if the saga store is unreachable or cancellation fails.
    pub async fn cancel_saga(&self, saga_id: Uuid) -> SagaStoreResult<SagaResult> {
        info!(saga_id = %saga_id, "Saga cancellation requested but coordination is unwired");

        // Previously returned a fabricated `Failed`/"cancelled by user" result
        // without stopping any step, marking the saga, or triggering compensation
        // (M-saga-coordinator).
        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCoordinator::cancel_saga".to_string(),
        })
    }

    /// Get final result of completed saga
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError` if the saga store is unreachable.
    pub async fn get_saga_result(&self, saga_id: Uuid) -> SagaStoreResult<SagaResult> {
        debug!(saga_id = %saga_id, "Saga result queried but coordination is unwired");

        // Previously returned a fabricated `Completed` result for any id without
        // loading anything from the store (M-saga-coordinator).
        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCoordinator::get_saga_result".to_string(),
        })
    }

    /// List all in-flight sagas (executing or compensating)
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError` if the saga store is unreachable.
    pub async fn list_in_flight_sagas(&self) -> SagaStoreResult<Vec<Uuid>> {
        // Previously returned an empty list without ever querying the store, so a
        // caller could not distinguish "no sagas" from "never checked"
        // (M-saga-coordinator).
        Err(crate::saga_store::SagaStoreError::NotImplemented {
            operation: "SagaCoordinator::list_in_flight_sagas".to_string(),
        })
    }

    /// Get compensation strategy
    #[must_use]
    pub const fn strategy(&self) -> CompensationStrategy {
        self.strategy
    }
}

/// Status of an in-flight saga
#[derive(Debug, Clone)]
pub struct SagaStatus {
    /// Saga identifier
    pub saga_id:             Uuid,
    /// Current state
    pub state:               SagaState,
    /// Total number of steps
    pub step_count:          u32,
    /// Number of completed steps
    pub completed_steps:     u32,
    /// Currently executing step, if any
    pub current_step:        Option<u32>,
    /// Progress as percentage (0.0 - 100.0)
    pub progress_percentage: f64,
}

#[cfg(test)]
mod tests;
