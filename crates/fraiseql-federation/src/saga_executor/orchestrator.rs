//! Saga orchestration: multi-step execution and state queries.

use super::*;

impl SagaExecutor {
    /// Execute all steps in a saga sequentially
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga to execute
    ///
    /// # Returns
    ///
    /// Vector of step results (successful or failed)
    ///
    /// # Errors
    ///
    /// Returns an error if the saga cannot be loaded or step execution fails.
    pub async fn execute_saga(
        &self,
        saga_id: Uuid,
    ) -> SagaStoreResult<Vec<StepExecutionResult>> {
        info!(saga_id = %saga_id, "Saga forward phase started");

        // If no store available, return empty results (for testing)
        let Some(store) = &self.store else {
            debug!("No saga store available - returning empty results");
            return Ok(vec![]);
        };

        // 1. Load saga from store
        let saga = store.load_saga(saga_id).await.map_err(|e| {
            warn!(saga_id = %saga_id, error = ?e, "Failed to load saga");
            e
        })?;

        if saga.is_none() {
            return Err(crate::saga_store::SagaStoreError::SagaNotFound(saga_id));
        }

        // 2. Transition saga from Pending to Executing
        store
            .update_saga_state(saga_id, &crate::saga_store::SagaState::Executing)
            .await
            .map_err(|e| {
                warn!(saga_id = %saga_id, error = ?e, "Failed to transition saga to Executing");
                e
            })?;

        info!(saga_id = %saga_id, "Saga transitioned to Executing");

        // 3. Load all steps for this saga
        let steps = store.load_saga_steps(saga_id).await.map_err(|e| {
            warn!(saga_id = %saga_id, error = ?e, "Failed to load saga steps");
            e
        })?;

        // Sort steps by order to ensure sequential execution
        let mut steps = steps;
        steps.sort_by_key(|s| s.order);

        let mut results: Vec<StepExecutionResult> = vec![];
        let mut saga_failed = false;

        // 4. For each step in order
        for step in steps {
            info!(
                saga_id = %saga_id,
                step = step.order,
                "Executing saga step"
            );

            // Construct mutation name from mutation type and typename
            let mutation_name = format!("{}_{}", step.mutation_type.as_str(), step.typename);

            match self
                .execute_step(
                    saga_id,
                    step.order as u32,
                    &mutation_name,
                    &step.variables,
                    &step.subgraph,
                )
                .await
            {
                Ok(step_result) => {
                    info!(
                        saga_id = %saga_id,
                        step = step.order,
                        "Step executed successfully"
                    );
                    results.push(step_result);
                },
                Err(e) => {
                    warn!(
                        saga_id = %saga_id,
                        step = step.order,
                        error = ?e,
                        "Step execution failed - stopping saga"
                    );

                    // Transition saga to Failed state
                    if let Err(state_err) = store
                        .update_saga_state(
                            saga_id,
                            &crate::saga_store::SagaState::Failed,
                        )
                        .await
                    {
                        warn!(saga_id = %saga_id, error = ?state_err, "Failed to transition saga to Failed state");
                    }

                    saga_failed = true;
                    break;
                },
            }
        }

        // 5. Update final saga state
        if !saga_failed {
            store
                .update_saga_state(saga_id, &crate::saga_store::SagaState::Completed)
                .await
                .map_err(|e| {
                    warn!(saga_id = %saga_id, error = ?e, "Failed to transition saga to Completed");
                    e
                })?;

            info!(
                saga_id = %saga_id,
                steps_completed = results.len(),
                "Saga completed successfully"
            );
        }

        info!(
            saga_id = %saga_id,
            steps_completed = results.len(),
            "Saga forward phase completed"
        );

        Ok(results)
    }

    /// Get current execution state of saga
    ///
    /// # Arguments
    ///
    /// * `saga_id` - ID of saga
    ///
    /// # Returns
    ///
    /// Current execution state including completed steps
    ///
    /// # Errors
    ///
    /// Returns an error if the saga store cannot be queried.
    pub async fn get_execution_state(
        &self,
        saga_id: Uuid,
    ) -> SagaStoreResult<ExecutionState> {
        // If no store available, return minimal state
        let Some(store) = &self.store else {
            debug!(saga_id = %saga_id, "No saga store available - returning empty execution state");
            return Ok(ExecutionState {
                saga_id,
                total_steps: 0,
                completed_steps: 0,
                current_step: None,
                failed: false,
                failure_reason: None,
            });
        };

        // Load saga to get state and failure reason
        let saga = store.load_saga(saga_id).await.map_err(|e| {
            warn!(saga_id = %saga_id, error = ?e, "Failed to load saga for execution state");
            e
        })?;

        let (total_steps, completed_steps, failed, failure_reason, current_step) = match saga {
            Some(saga_data) => {
                let steps = store.load_saga_steps(saga_id).await.map_err(|e| {
                    warn!(saga_id = %saga_id, error = ?e, "Failed to load saga steps for execution state");
                    e
                })?;

                let total = steps.len() as u32;
                let completed =
                    steps.iter().filter(|s| s.state == StepState::Completed).count() as u32;
                let current =
                    steps.iter().find(|s| s.state != StepState::Completed).map(|s| s.order as u32);
                let is_failed = saga_data.state == crate::saga_store::SagaState::Failed;

                (total, completed, is_failed, None, current)
            },
            None => (0, 0, false, None, None),
        };

        let state = ExecutionState {
            saga_id,
            total_steps,
            completed_steps,
            current_step,
            failed,
            failure_reason,
        };

        debug!(
            saga_id = %saga_id,
            total_steps = state.total_steps,
            completed_steps = state.completed_steps,
            failed = state.failed,
            "Execution state queried"
        );

        Ok(state)
    }
}
