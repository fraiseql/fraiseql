//! Single-step execution for saga forward phase.

use ::tracing::info;
use uuid::Uuid;

use super::{SagaExecutor, StepExecutionResult};
use crate::saga_store::{Result as SagaStoreResult, SagaStoreError};

impl SagaExecutor {
    /// Execute a single saga step.
    ///
    /// # Status
    ///
    /// **Not implemented.** Distributed saga step execution has no real
    /// mutation transport wired: there is no `MutationExecutor` dispatch and no
    /// `@requires` cross-subgraph resolution behind this entry point. The
    /// previous body fabricated a result document, persisted a `Completed`
    /// transition, and returned `success: true` without performing any work
    /// (audit H32). Rather than persist fabricated success it now fails loud.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga being executed
    /// * `step_number` - Step number to execute (1-indexed, 1 = first step)
    /// * `mutation_name` - GraphQL mutation operation name
    /// * `variables` - Input variables for the mutation (JSON value)
    /// * `subgraph` - Target subgraph name (must exist in federation)
    ///
    /// # Errors
    ///
    /// Always returns [`SagaStoreError::NotImplemented`]; it must never
    /// transition step state or persist a result.
    pub async fn execute_step(
        &self,
        saga_id: Uuid,
        step_number: u32,
        mutation_name: &str,
        _variables: &serde_json::Value,
        subgraph: &str,
    ) -> SagaStoreResult<StepExecutionResult> {
        info!(
            saga_id = %saga_id,
            step = step_number,
            mutation = mutation_name,
            subgraph = subgraph,
            "Step execution requested but distributed saga execution is unwired"
        );

        Err(SagaStoreError::NotImplemented {
            operation: "SagaExecutor::execute_step".to_string(),
        })
    }
}
