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
//! // Requires: a live PostgreSQL saga store + a FederationMutationExecutor.
//! // See: tests/saga_integration.rs for runnable examples.
//! // Create a coordinator over the saga store.
//! let coordinator = SagaCoordinator::new(Arc::new(store), CompensationStrategy::Automatic);
//!
//! // Define steps for a multi-service transaction.
//! let steps = vec![
//!     SagaCoordinatorStep::new(
//!         1, "orders-service", "Order", "createOrder",
//!         json!({"id": "order-1", "total": 100.0}),
//!         "deleteOrder", json!({"id": "order-1"})
//!     ),
//!     SagaCoordinatorStep::new(
//!         2, "inventory-service", "Inventory", "reserveInventory",
//!         json!({"orderId": "order-1", "items": [...]}),
//!         "releaseInventory", json!({"orderId": "order-1"})
//!     ),
//! ];
//!
//! // Create and execute the saga.
//! let saga_id = coordinator.create_saga(steps).await?;
//! let result = coordinator.execute_saga(saga_id, &mutation_executor).await?;
//!
//! match result.state {
//!     SagaState::Completed => println!("Success!"),
//!     SagaState::Failed => println!("Failed and rolled back"),
//!     _ => println!("Other state"),
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

use ::tracing::warn;
use uuid::Uuid;

use crate::saga_store::{RequiredField, Result as SagaStoreResult, SagaState};

/// Pure coordinator decision helpers.
mod coordination;

/// The [`SagaCoordinator`] handle that ties forward execution, compensation, and the
/// store into one type.
mod coordinator;
pub use coordinator::SagaCoordinator;

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
    /// ([`SagaCoordinator::with_subgraph`](crate::saga_coordinator::SagaCoordinator::with_subgraph))
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
/// Used by [`SagaCoordinator::create_saga`](crate::saga_coordinator::SagaCoordinator::create_saga):
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
