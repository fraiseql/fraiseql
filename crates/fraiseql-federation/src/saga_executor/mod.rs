//! Saga Forward Phase Executor
//!
//! Executes saga steps sequentially during the forward phase, implementing
//! the core saga pattern for distributed transactions across subgraphs.
//!
//! # Architecture
//!
//! The forward phase executor:
//! - Loads sagas from persistent storage
//! - Executes steps in strict sequential order (1 → 2 → 3)
//! - Pre-fetches @requires fields before each step
//! - Captures and persists results for chaining
//! - Tracks execution state for monitoring and recovery
//! - Terminates on first failure and triggers compensation
//!
//! # Execution Flow
//!
//! ```text
//! Load Saga from Store
//!    ↓
//! For Each Step (1..N):
//!    ├─ Validate step is Pending
//!    ├─ Pre-fetch @requires fields from other subgraphs
//!    ├─ Transition step to Executing
//!    ├─ Execute mutation via MutationExecutor
//!    │  (with augmented entity data)
//!    ├─ Capture result data
//!    ├─ Persist step result to store
//!    ├─ Transition step to Completed
//!    └─ Continue to next step
//!       OR on failure: Break and transition to Failed state
//!
//! Update Saga State:
//!    ├─ If all completed: Saga → Completed
//!    └─ If any failed: Saga → Failed (trigger compensation)
//! ```
//!
//! # @requires Field Fetching
//!
//! Each step may have @requires fields that must be present before mutation execution.
//! These fields are fetched from their owning subgraphs before step execution:
//!
//! ```text
//! Step Definition:
//!   mutation: "updateOrder"
//!   @requires: ["product.price", "user.email"]
//!
//! Pre-Execution:
//!   1. Identify @requires fields
//!   2. Fetch from owning subgraphs
//!   3. Augment entity data with fetched fields
//!   4. Execute mutation with complete entity
//! ```
//!
//! # Example
//!
//! ```no_run
//! // Requires: distributed saga infrastructure (PostgreSQL + message broker).
//! // See: tests/integration/ for runnable examples.
//! let executor = SagaExecutor::new();
//!
//! // Execute a single step
//! let result = executor.execute_step(
//!     saga_id,
//!     1,
//!     "createOrder",
//!     &json!({"customerId": "c123", "total": 100.0}),
//!     "orders-service"
//! ).await?;
//!
//! if result.success {
//!     println!("Step 1 created order: {:?}", result.data);
//! } else {
//!     println!("Step 1 failed: {}", result.error.unwrap());
//! }
//! ```

use std::{sync::Arc, time::Instant};

use ::tracing::{debug, info, warn};
use uuid::Uuid;

use crate::saga_store::{PostgresSagaStore, Result as SagaStoreResult, StepState};

mod orchestrator;
mod requires;
mod step;

#[cfg(test)]
mod tests;

/// Represents a step result from execution
///
/// Contains the outcome of executing a single saga step, including:
/// - Whether execution succeeded or failed
/// - Result data if successful (entity with key fields and updated values)
/// - Error details if failed
/// - Execution metrics (duration)
///
/// The result data is stored and available for subsequent steps to reference
/// via result chaining.
#[derive(Debug, Clone)]
pub struct StepExecutionResult {
    /// Step number that executed (1-indexed)
    pub step_number: u32,
    /// Whether step succeeded (true) or failed (false)
    pub success:     bool,
    /// Result data if successful
    ///
    /// Contains:
    /// - `__typename`: Entity type
    /// - Key fields (id, etc.)
    /// - Mutation output fields
    /// - Timestamps
    pub data:        Option<serde_json::Value>,
    /// Error message if failed
    ///
    /// Includes:
    /// - Error type (timeout, network, mutation failed, etc.)
    /// - Subgraph context
    /// - Suggestion for resolution
    pub error:       Option<String>,
    /// Execution duration in milliseconds
    ///
    /// Measured from step start to completion (or failure)
    /// Useful for performance monitoring
    pub duration_ms: u64,
}

/// Current execution state of a saga
#[derive(Debug, Clone)]
pub struct ExecutionState {
    /// Saga identifier
    pub saga_id:         Uuid,
    /// Total steps in saga
    pub total_steps:     u32,
    /// Number of completed steps
    pub completed_steps: u32,
    /// Currently executing step, if any
    pub current_step:    Option<u32>,
    /// Whether saga has failed
    pub failed:          bool,
    /// Reason for failure, if any
    pub failure_reason:  Option<String>,
}

/// Saga forward phase executor
///
/// Executes saga steps sequentially during the forward phase.
/// Coordinates with saga store to persist state and handle failures.
pub struct SagaExecutor {
    /// Saga store for loading/saving saga state
    /// Optional to support testing without database
    pub(super) store: Option<Arc<PostgresSagaStore>>,
}

impl SagaExecutor {
    /// Create a new saga executor without a saga store
    ///
    /// This is suitable for testing. For production, use `with_store()`.
    pub const fn new() -> Self {
        Self { store: None }
    }

    /// Create a new saga executor with a saga store
    ///
    /// This enables persistence of saga state and recovery from failures.
    #[must_use]
    pub const fn with_store(store: Arc<PostgresSagaStore>) -> Self {
        Self { store: Some(store) }
    }

    /// Check if executor has a saga store configured
    #[must_use]
    pub const fn has_store(&self) -> bool {
        self.store.is_some()
    }
}

impl Default for SagaExecutor {
    fn default() -> Self {
        Self::new()
    }
}
