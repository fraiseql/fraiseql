//! Single-step execution for saga forward phase.

use ::tracing::warn;
use fraiseql_db::traits::DatabaseAdapter;
use reqwest::Url;

use super::{SagaExecutor, StepExecutionResult, forward};
use crate::{
    mutation_executor::FederationMutationExecutor,
    mutation_http_client::HttpMutationClient,
    saga_store::{SagaStep, StepState},
};

impl SagaExecutor {
    /// Dispatch a single step's mutation and map the outcome to a
    /// [`StepExecutionResult`] plus the [`StepState`] to persist.
    ///
    /// Pure dispatch with no persistence — [`Self::execute_saga`] owns step/saga
    /// state writes. Routing:
    /// - `remote = None` → the step runs against the local SQL adapter via
    ///   [`FederationMutationExecutor::execute_local_mutation`].
    /// - `remote = Some((client, url))` → the step is propagated over HTTPS to the peer subgraph
    ///   via [`HttpMutationClient::execute_mutation`].
    ///
    /// The operation name sent is the step's full persisted `mutation_name`
    /// (e.g. `createOrder`), so a remote subgraph receives the real mutation;
    /// a pre-migration row with no name falls back to the mutation-kind verb
    /// (`create`/`update`/`delete`), which the name-driven `determine_mutation_type`
    /// also resolves for the local path. A mutation `Err` (local or remote) becomes a
    /// real `success: false` step, never fabricated success (audit H32).
    pub(crate) async fn dispatch_step<A: DatabaseAdapter>(
        mutation_executor: &FederationMutationExecutor<A>,
        step: &SagaStep,
        remote: Option<(&HttpMutationClient, &Url)>,
    ) -> (StepExecutionResult, StepState) {
        let step_number = u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1);
        // Prefer the full persisted operation name (e.g. `createOrder`) so a
        // remote subgraph receives the real mutation; pre-migration rows with
        // no name fall back to the mutation-kind verb (`create`), which the
        // name-driven `determine_mutation_type` also resolves for the local path.
        let op_name = step.mutation_name.as_deref().unwrap_or_else(|| step.mutation_type.as_str());
        let started = std::time::Instant::now();
        let outcome = match remote {
            None => {
                mutation_executor
                    .execute_local_mutation(&step.typename, op_name, &step.variables)
                    .await
            },
            Some((client, url)) => {
                client
                    .execute_mutation(
                        url.as_str(),
                        &step.typename,
                        op_name,
                        &step.variables,
                        mutation_executor.metadata(),
                    )
                    .await
            },
        };
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        forward::step_result_from(step_number, &outcome, duration_ms)
    }

    /// Dispatch a step under the executor's [`crate::saga_executor::RetryPolicy`]:
    /// retry a failed dispatch up to `max_retries` times with exponential backoff,
    /// applying the optional per-attempt timeout.
    ///
    /// A successful attempt returns immediately. An attempt that fails or times out
    /// is a real failed attempt — never a fabricated success (audit H32) — and once
    /// retries are exhausted the last failed result is returned so the saga's
    /// compensation strategy acts on a genuine failure. With the default
    /// [`crate::saga_executor::RetryPolicy::none`] this is exactly one attempt,
    /// identical to [`Self::dispatch_step`].
    pub(crate) async fn dispatch_step_with_retry<A: DatabaseAdapter>(
        &self,
        mutation_executor: &FederationMutationExecutor<A>,
        step: &SagaStep,
        remote: Option<(&HttpMutationClient, &Url)>,
    ) -> (StepExecutionResult, StepState) {
        let policy = self.retry;
        let step_number = u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1);
        let mut attempt: u32 = 0;
        loop {
            let (result, state) = match policy.step_timeout_ms {
                Some(ms) => {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(ms),
                        Self::dispatch_step(mutation_executor, step, remote),
                    )
                    .await
                    {
                        Ok(outcome) => outcome,
                        Err(_) => (
                            StepExecutionResult {
                                step_number,
                                success: false,
                                data: None,
                                error: Some(format!("step dispatch timed out after {ms}ms")),
                                duration_ms: ms,
                            },
                            StepState::Failed,
                        ),
                    }
                },
                None => Self::dispatch_step(mutation_executor, step, remote).await,
            };

            // Success, or no retries left → report the genuine outcome.
            if result.success || attempt >= policy.max_retries {
                return (result, state);
            }

            warn!(
                saga_id = %step.saga_id,
                step = step_number,
                attempt = attempt + 1,
                max_retries = policy.max_retries,
                error = result.error.as_deref().unwrap_or("<none>"),
                "saga step failed; retrying with backoff"
            );

            // Exponential backoff: base * 2^attempt, saturating (0 base = no wait).
            let factor = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
            let backoff_ms = policy.base_delay_ms.saturating_mul(factor);
            if backoff_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            }
            attempt += 1;
        }
    }

    /// Execute (dispatch) a single saga step's real local mutation and report the
    /// outcome.
    ///
    /// Performs the real mutation via
    /// [`FederationMutationExecutor::execute_local_mutation`] and maps the result,
    /// but does not persist — [`Self::execute_saga`] owns step and saga state. A
    /// mutation failure is reported as `success: false` with the error captured
    /// (never fabricated success, audit H32), not an `Err`.
    ///
    /// # Arguments
    ///
    /// * `mutation_executor` - Local mutation transport for the step's subgraph
    /// * `step` - The persisted step definition (typename, mutation type, input)
    pub async fn execute_step<A: DatabaseAdapter>(
        &self,
        mutation_executor: &FederationMutationExecutor<A>,
        step: &SagaStep,
    ) -> StepExecutionResult {
        // Direct single-step dispatch is always local; the remote-routing registry
        // lives on the coordinator's `execute_saga` path. The executor's
        // retry/timeout policy still applies.
        self.dispatch_step_with_retry(mutation_executor, step, None).await.0
    }
}
