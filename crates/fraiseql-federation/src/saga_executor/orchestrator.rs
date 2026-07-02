//! Saga orchestration: multi-step execution and state queries.

use std::collections::HashMap;

use fraiseql_db::traits::DatabaseAdapter;
use reqwest::Url;
use uuid::Uuid;

use super::{ExecutionState, SagaExecutor, StepExecutionResult, forward, prefetch};
use crate::{
    http_resolver::HttpEntityResolver,
    mutation_executor::FederationMutationExecutor,
    mutation_http_client::HttpMutationClient,
    saga_store::{Result as SagaStoreResult, SagaState, SagaStoreError, StepState},
};

impl SagaExecutor {
    /// Execute all steps in a saga sequentially during the forward phase.
    ///
    /// Marks the saga `Executing`, then drives each step (ordered by
    /// `SagaStep::order`) through the internal `dispatch_step`: each step is marked
    /// `Executing`, its real mutation is dispatched, a successful result is
    /// persisted, and the step is marked `Completed` or `Failed`. Execution **stops
    /// at the first failed step** (compensation is decided separately); the saga is
    /// then marked `Completed` (all steps succeeded) or `Failed`.
    ///
    /// Step/saga state is never fabricated — a mutation failure persists a real
    /// `Failed` transition rather than a `Completed` one (audit H32).
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga to execute
    /// * `mutation_executor` - Local mutation transport for the steps' subgraph
    /// * `subgraph_urls` - Registered remote peers (subgraph name → base URL); a step whose
    ///   `subgraph` matches an entry is dispatched over HTTPS, and a step's `@requires` fields are
    ///   pre-fetched from their owning subgraph's URL here
    /// * `http_client` - HTTP client for remote dispatch; `None` = local-only
    /// * `entity_resolver` - HTTP entity resolver for `@requires` pre-fetch; `None` disables
    ///   pre-fetch, so a step that declares `@requires` fields fails loud before dispatch
    ///
    /// A step is dispatched remotely only when **both** an `http_client` is present
    /// **and** its `subgraph` resolves to a registered URL; otherwise it falls
    /// through to the local SQL adapter, so mixed local/remote sagas exercise both
    /// paths in one run. Before each step's mutation runs, any `@requires` field it
    /// declares is resolved from its owning subgraph and merged into the mutation
    /// variables; an unresolved field fails the step **before** dispatch (a real
    /// `Failed` step that stops the saga and triggers compensation, never a mutation
    /// with missing inputs — #429).
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::Database`] if no saga store is configured,
    /// [`SagaStoreError::SagaNotFound`] if the saga does not exist, or any store
    /// error encountered while loading steps or persisting state.
    pub async fn execute_saga<A: DatabaseAdapter>(
        &self,
        saga_id: Uuid,
        mutation_executor: &FederationMutationExecutor<A>,
        subgraph_urls: &HashMap<String, Url>,
        http_client: Option<&HttpMutationClient>,
        entity_resolver: Option<&HttpEntityResolver>,
    ) -> SagaStoreResult<Vec<StepExecutionResult>> {
        let store = self.store.as_ref().ok_or_else(|| {
            SagaStoreError::Database(
                "forward saga execution requires a configured saga store".to_string(),
            )
        })?;

        // Mark the saga Executing first: a missing saga surfaces as SagaNotFound
        // from the store's row-count check rather than silently executing zero steps.
        store.update_saga_state(saga_id, &SagaState::Executing).await?;

        let mut steps = store.load_saga_steps(saga_id).await?;
        steps.sort_by_key(|step| step.order);

        let mut results = Vec::with_capacity(steps.len());
        for step in &steps {
            store.update_saga_step_state(step.id, &StepState::Executing).await?;

            // Route to the remote HTTP client only when a client is configured
            // and the step's subgraph names a registered peer; otherwise local.
            let remote = crate::mutation_http_client::resolve_remote(
                &step.subgraph,
                http_client,
                subgraph_urls,
            );

            // Pre-fetch @requires fields (if any) and dispatch with the merged
            // variables. An unresolved required field fails the step BEFORE its
            // mutation runs — a real Failed step (never a mutation with missing
            // inputs, #429) that stops the saga and triggers compensation like any
            // other failure. Retry/timeout policy (default: one attempt) wraps the
            // dispatch so a transient step failure is retried first.
            let (result, state) = if step.required_fields.is_empty() {
                self.dispatch_step_with_retry(mutation_executor, step, remote).await
            } else {
                match prefetch::resolve_required_fields(
                    &step.required_fields,
                    &step.variables,
                    entity_resolver,
                    subgraph_urls,
                )
                .await
                {
                    Ok(merged_variables) => {
                        let mut merged = step.clone();
                        merged.variables = merged_variables;
                        self.dispatch_step_with_retry(mutation_executor, &merged, remote).await
                    },
                    Err(error) => {
                        let step_number =
                            u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1);
                        prefetch::prefetch_failure(step_number, &error)
                    },
                }
            };

            // Persist the real post-mutation entity only on success; a failed step
            // is marked Failed and carries no fabricated result payload.
            if let Some(data) = result.data.as_ref() {
                store.update_saga_step_result(step.id, data).await?;
            }
            store.update_saga_step_state(step.id, &state).await?;

            let stop = !result.success;
            results.push(result);
            if stop {
                break;
            }
        }

        let saga_state = forward::saga_state_for(&results);
        store.update_saga_state(saga_id, &saga_state).await?;
        Ok(results)
    }

    /// Get the current execution state of a saga, derived from persisted step states.
    ///
    /// Read-only: counts completed steps, reports the first `Executing` step as the
    /// current one, and flags the saga failed if any step is `Failed`.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::Database`] if no saga store is configured, or any
    /// store error encountered while loading steps.
    pub async fn execution_state(&self, saga_id: Uuid) -> SagaStoreResult<ExecutionState> {
        let store = self.store.as_ref().ok_or_else(|| {
            SagaStoreError::Database(
                "forward saga execution state requires a configured saga store".to_string(),
            )
        })?;

        let mut steps = store.load_saga_steps(saga_id).await?;
        steps.sort_by_key(|step| step.order);

        let total_steps = u32::try_from(steps.len()).unwrap_or(u32::MAX);
        let completed_steps =
            u32::try_from(steps.iter().filter(|step| step.state == StepState::Completed).count())
                .unwrap_or(u32::MAX);
        let failed = steps.iter().any(|step| step.state == StepState::Failed);
        let current_step = steps
            .iter()
            .find(|step| step.state == StepState::Executing)
            .map(|step| u32::try_from(step.order).unwrap_or(u32::MAX).saturating_add(1));

        Ok(ExecutionState {
            saga_id,
            total_steps,
            completed_steps,
            current_step,
            failed,
            failure_reason: None,
        })
    }
}
