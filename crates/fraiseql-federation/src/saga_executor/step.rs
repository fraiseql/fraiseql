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

/// Wired forward-phase execution (the `unstable-saga` feature).
///
/// These are additive: the fail-loud `execute_step` above keeps its signature and
/// behaviour in every build (it is the published placeholder contract and the
/// `#429` acceptance spec exercises it). The real local-mutation transport is
/// exposed as the `*_local` methods, gated behind `unstable-saga` until proven.
#[cfg(feature = "unstable-saga")]
mod wired {
    use fraiseql_db::traits::DatabaseAdapter;
    use reqwest::Url;

    use super::super::{SagaExecutor, StepExecutionResult, forward};
    use crate::{
        mutation_executor::FederationMutationExecutor,
        mutation_http_client::HttpMutationClient,
        saga_store::{SagaStep, StepState},
    };

    impl SagaExecutor {
        /// Dispatch a single step's mutation and map the outcome to a
        /// [`StepExecutionResult`] plus the [`StepState`] to persist.
        ///
        /// Pure dispatch with no persistence — [`Self::execute_saga_local`] owns
        /// step/saga state writes. Routing:
        /// - `remote = None` → the step runs against the local SQL adapter via
        ///   [`FederationMutationExecutor::execute_local_mutation`].
        /// - `remote = Some((client, url))` → the step is propagated over HTTPS to the peer
        ///   subgraph via [`HttpMutationClient::execute_mutation`].
        ///
        /// Either way the persisted [`crate::saga_store::MutationType`] is rendered
        /// to its canonical verb (`create`/`update`/`delete`) as the operation
        /// name — the store carries only the mutation *kind*, not the full remote
        /// mutation name (the local path already dispatches by verb; carrying the
        /// full remote name is a future store-schema extension). A mutation `Err`
        /// (local or remote) becomes a real `success: false` step, never fabricated
        /// success (audit H32).
        pub(crate) async fn dispatch_step<A: DatabaseAdapter>(
            mutation_executor: &FederationMutationExecutor<A>,
            step: &SagaStep,
            remote: Option<(&HttpMutationClient, &Url)>,
        ) -> (StepExecutionResult, StepState) {
            let step_number = u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1);
            let started = std::time::Instant::now();
            let outcome = match remote {
                None => {
                    mutation_executor
                        .execute_local_mutation(
                            &step.typename,
                            step.mutation_type.as_str(),
                            &step.variables,
                        )
                        .await
                },
                Some((client, url)) => {
                    client
                        .execute_mutation(
                            url.as_str(),
                            &step.typename,
                            step.mutation_type.as_str(),
                            &step.variables,
                            mutation_executor.metadata(),
                        )
                        .await
                },
            };
            let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            forward::step_result_from(step_number, &outcome, duration_ms)
        }

        /// Execute (dispatch) a single saga step's real local mutation and report
        /// the outcome.
        ///
        /// Performs the real mutation via
        /// [`FederationMutationExecutor::execute_local_mutation`] and maps the
        /// result, but does not persist — [`Self::execute_saga_local`] owns step and
        /// saga state. A mutation failure is reported as `success: false` with the
        /// error captured (never fabricated success, audit H32), not an `Err`.
        ///
        /// # Arguments
        ///
        /// * `mutation_executor` - Local mutation transport for the step's subgraph
        /// * `step` - The persisted step definition (typename, mutation type, input)
        pub async fn execute_step_local<A: DatabaseAdapter>(
            &self,
            mutation_executor: &FederationMutationExecutor<A>,
            step: &SagaStep,
        ) -> StepExecutionResult {
            // Direct single-step dispatch is always local; the remote-routing
            // registry lives on the coordinator's `execute_saga_local` path.
            Self::dispatch_step(mutation_executor, step, None).await.0
        }
    }
}
