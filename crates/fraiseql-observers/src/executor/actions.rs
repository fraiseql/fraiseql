//! Per-action execution and failure handling.

use std::{sync::atomic::Ordering, time::Duration};

use tracing::{error, info, warn};

use super::{ExecutionSummary, ObserverExecutor};
use crate::{
    config::{ActionConfig, FailurePolicy},
    error::{ObserverError, Result},
    event::EntityEvent,
    traits::ActionResult,
};

impl ObserverExecutor {
    /// Execute action and return result
    pub(crate) async fn execute_action_internal(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
    ) -> Result<ActionResult> {
        // Try cache first (skip for CacheAction itself)
        #[cfg(feature = "caching")]
        if !matches!(action, ActionConfig::Cache { .. }) {
            if let Some(cached) = self.try_cache_get(event, action).await {
                return Ok(cached);
            }
        }

        let result = if let Some(timeout_ms) = self.action_timeout_ms {
            tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                self.dispatcher.dispatch(action, event),
            )
            .await
            .unwrap_or_else(|_| {
                Err(crate::error::ObserverError::ActionExecutionFailed {
                    reason: format!(
                        "action '{}' timed out after {timeout_ms} ms",
                        action.action_type()
                    ),
                })
            })
        } else {
            self.dispatcher.dispatch(action, event).await
        };

        // Cache successful results before returning
        #[cfg(feature = "caching")]
        if let Ok(ref res) = result {
            if !matches!(action, ActionConfig::Cache { .. }) {
                self.cache_store(event, action, res).await;
            }
        }

        result
    }

    /// Handle action failure based on failure policy
    #[allow(clippy::cognitive_complexity)] // Reason: failure policy dispatch with per-policy logging and DLQ routing
    pub(crate) async fn handle_action_failure(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
        error: &ObserverError,
        failure_policy: &FailurePolicy,
        summary: &mut ExecutionSummary,
    ) {
        // Record error metrics
        #[cfg(feature = "metrics")]
        {
            let error_type = match error.code() {
                crate::error::ObserverErrorCode::ActionExecutionFailed => "execution_failed",
                crate::error::ObserverErrorCode::ActionPermanentlyFailed => "permanently_failed",
                crate::error::ObserverErrorCode::InvalidActionConfig => "invalid_config",
                crate::error::ObserverErrorCode::TemplateRenderingFailed => {
                    "template_rendering_failed"
                },
                crate::error::ObserverErrorCode::DatabaseError => "database_error",
                crate::error::ObserverErrorCode::CircuitBreakerOpen => "circuit_breaker_open",
                _ => "other_error",
            };
            self.metrics.action_error(action.action_type(), error_type);
        }

        match failure_policy {
            FailurePolicy::Log => {
                error!("Action {} failed for event {}: {}", action.action_type(), event.id, error);
                summary.failed_actions += 1;
            },
            FailurePolicy::Alert => {
                error!(
                    "ALERT: Action {} failed for event {}: {}",
                    action.action_type(),
                    event.id,
                    error
                );
                summary.failed_actions += 1;
            },
            FailurePolicy::Dlq => {
                // Check DLQ size cap before pushing (drop-newest strategy).
                if let Some(max) = self.max_dlq_size {
                    let current = self.dlq_push_count.load(Ordering::Relaxed);
                    if current >= max {
                        warn!(
                            max_dlq_size = max,
                            action_type = action.action_type(),
                            event_id = %event.id,
                            "DLQ full; dropping failed action entry"
                        );
                        #[cfg(feature = "metrics")]
                        self.metrics.dlq_overflow();
                        summary.failed_actions += 1;
                        return;
                    }
                }

                info!(
                    "Moving failed action {} to DLQ for event {}",
                    action.action_type(),
                    event.id
                );
                if let Err(e) =
                    self.dlq.push(event.clone(), action.clone(), error.to_string()).await
                {
                    error!("Failed to push to DLQ: {}", e);
                    summary.dlq_errors += 1;
                } else {
                    // Increment push counter only on successful DLQ push.
                    self.dlq_push_count.fetch_add(1, Ordering::Relaxed);
                }
                summary.failed_actions += 1;
            },
        }
    }
}
