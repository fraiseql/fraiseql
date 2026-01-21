//! Main observer executor engine with retry logic and orchestration.
//!
//! This module implements the core execution engine that:
//! 1. Receives events from the listener
//! 2. Matches events to observers using the matcher
//! 3. Evaluates conditions for each observer
//! 4. Executes actions with retry logic
//! 5. Handles failures via Dead Letter Queue

use crate::actions::{EmailAction, SlackAction, WebhookAction};
use crate::config::{ActionConfig, BackoffStrategy, FailurePolicy, RetryConfig};
use crate::condition::ConditionParser;
use crate::error::{ObserverError, Result};
use crate::event::EntityEvent;
use crate::matcher::EventMatcher;
use crate::traits::{ActionResult, DeadLetterQueue};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Main observer executor engine
pub struct ObserverExecutor {
    /// Event-to-observer matcher
    matcher: Arc<EventMatcher>,
    /// Condition parser and evaluator
    condition_parser: Arc<ConditionParser>,
    /// Webhook action executor
    webhook_action: Arc<WebhookAction>,
    /// Slack action executor
    slack_action: Arc<SlackAction>,
    /// Email action executor
    email_action: Arc<EmailAction>,
    /// Dead letter queue for failed actions
    dlq: Arc<dyn DeadLetterQueue>,
}

impl ObserverExecutor {
    /// Create a new executor
    pub fn new(
        matcher: EventMatcher,
        dlq: Arc<dyn DeadLetterQueue>,
    ) -> Self {
        Self {
            matcher: Arc::new(matcher),
            condition_parser: Arc::new(ConditionParser::new()),
            webhook_action: Arc::new(WebhookAction::new()),
            slack_action: Arc::new(SlackAction::new()),
            email_action: Arc::new(EmailAction::new()),
            dlq,
        }
    }

    /// Process an event through all matching observers
    ///
    /// This is the main entry point. For each matching observer:
    /// 1. Evaluate condition (if present)
    /// 2. Execute actions with retry logic
    /// 3. Handle failures via DLQ
    pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
        let mut summary = ExecutionSummary::new();
        let matching_observers = self.matcher.find_matches(event);

        debug!(
            "Processing event {} with {} matching observers",
            event.id,
            matching_observers.len()
        );

        for observer in matching_observers {
            // Skip if condition is not met
            if let Some(condition) = &observer.condition {
                match self.condition_parser.parse_and_evaluate(condition, event) {
                    Ok(true) => {
                        debug!("Condition passed for observer");
                    }
                    Ok(false) => {
                        debug!("Condition failed, skipping observer");
                        summary.conditions_skipped += 1;
                        continue;
                    }
                    Err(e) => {
                        error!("Condition evaluation error: {}", e);
                        summary.errors.push(e.to_string());
                        continue;
                    }
                }
            }

            // Execute actions for this observer
            for action in &observer.actions {
                self.execute_action_with_retry(action, event, &observer.retry, &observer.on_failure, &mut summary)
                    .await;
            }
        }

        Ok(summary)
    }

    /// Execute a single action with retry logic
    async fn execute_action_with_retry(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
        retry_config: &RetryConfig,
        failure_policy: &FailurePolicy,
        summary: &mut ExecutionSummary,
    ) {
        let mut attempt = 0;

        loop {
            attempt += 1;
            debug!(
                "Action {} execution attempt {}/{}",
                action.action_type(),
                attempt,
                retry_config.max_attempts
            );

            match self.execute_action_internal(action, event).await {
                Ok(result) => {
                    info!(
                        "Action {} succeeded in {}ms",
                        action.action_type(),
                        result.duration_ms
                    );
                    summary.successful_actions += 1;
                    summary.total_duration_ms += result.duration_ms;
                    return;
                }
                Err(e) => {
                    let is_transient = e.is_transient();

                    if !is_transient {
                        // Permanent error, don't retry
                        warn!(
                            "Permanent error in action {}: {}",
                            action.action_type(),
                            e
                        );
                        self.handle_action_failure(action, event, &e, failure_policy, summary)
                            .await;
                        return;
                    }

                    if attempt >= retry_config.max_attempts {
                        // Retries exhausted
                        error!(
                            "Action {} failed after {} attempts",
                            action.action_type(),
                            attempt
                        );
                        self.handle_action_failure(action, event, &e, failure_policy, summary)
                            .await;
                        return;
                    }

                    // Calculate backoff and retry
                    let delay = self.calculate_backoff(attempt, retry_config);
                    warn!(
                        "Action {} attempt {} failed: {}. Retrying in {:?}",
                        action.action_type(),
                        attempt,
                        e,
                        delay
                    );

                    sleep(delay).await;
                }
            }
        }
    }

    /// Execute action and return result
    async fn execute_action_internal(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
    ) -> Result<ActionResult> {
        match action {
            ActionConfig::Webhook {
                url,
                url_env,
                headers,
                body_template,
            } => {
                let webhook_url = if let Some(u) = url {
                    u.clone()
                } else if let Some(var_name) = url_env {
                    std::env::var(var_name).map_err(|_| ObserverError::InvalidActionConfig {
                        reason: format!("Webhook URL env var {} not found", var_name),
                    })?
                } else {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook URL not provided".to_string(),
                    });
                };

                match self
                    .webhook_action
                    .execute(&webhook_url, headers, body_template.as_deref(), event)
                    .await
                {
                    Ok(response) => Ok(ActionResult {
                        action_type: "webhook".to_string(),
                        success: true,
                        message: format!("HTTP {}", response.status_code),
                        duration_ms: response.duration_ms,
                    }),
                    Err(e) => Err(e),
                }
            }
            ActionConfig::Slack {
                webhook_url,
                webhook_url_env,
                channel,
                message_template,
            } => {
                let slack_url = if let Some(u) = webhook_url {
                    u.clone()
                } else if let Some(var_name) = webhook_url_env {
                    std::env::var(var_name).map_err(|_| ObserverError::InvalidActionConfig {
                        reason: format!("Slack webhook URL env var {} not found", var_name),
                    })?
                } else {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Slack webhook URL not provided".to_string(),
                    });
                };

                match self
                    .slack_action
                    .execute(&slack_url, channel.as_deref(), message_template.as_deref(), event)
                    .await
                {
                    Ok(response) => Ok(ActionResult {
                        action_type: "slack".to_string(),
                        success: true,
                        message: format!("HTTP {}", response.status_code),
                        duration_ms: response.duration_ms,
                    }),
                    Err(e) => Err(e),
                }
            }
            ActionConfig::Email {
                to,
                to_template: _,
                subject,
                subject_template: _,
                body_template,
                reply_to: _,
            } => {
                let email_to = to.as_ref().ok_or(ObserverError::InvalidActionConfig {
                    reason: "Email 'to' not provided".to_string(),
                })?;

                let email_subject = subject.as_ref().ok_or(ObserverError::InvalidActionConfig {
                    reason: "Email 'subject' not provided".to_string(),
                })?;

                match self
                    .email_action
                    .execute(email_to, email_subject, body_template.as_deref(), event)
                    .await
                {
                    Ok(response) => Ok(ActionResult {
                        action_type: "email".to_string(),
                        success: response.success,
                        message: response
                            .message_id
                            .unwrap_or_else(|| "queued".to_string()),
                        duration_ms: response.duration_ms,
                    }),
                    Err(e) => Err(e),
                }
            }
            ActionConfig::Sms { .. }
            | ActionConfig::Push { .. }
            | ActionConfig::Search { .. }
            | ActionConfig::Cache { .. } => {
                Err(ObserverError::UnsupportedActionType {
                    action_type: action.action_type().to_string(),
                })
            }
        }
    }

    /// Handle action failure based on failure policy
    async fn handle_action_failure(
        &self,
        action: &ActionConfig,
        event: &EntityEvent,
        error: &ObserverError,
        failure_policy: &FailurePolicy,
        summary: &mut ExecutionSummary,
    ) {
        match failure_policy {
            FailurePolicy::Log => {
                error!(
                    "Action {} failed for event {}: {}",
                    action.action_type(),
                    event.id,
                    error
                );
                summary.failed_actions += 1;
            }
            FailurePolicy::Alert => {
                error!(
                    "ALERT: Action {} failed for event {}: {}",
                    action.action_type(),
                    event.id,
                    error
                );
                summary.failed_actions += 1;
                // Phase 6.7: Implement alerting via separate observer
            }
            FailurePolicy::Dlq => {
                info!(
                    "Moving failed action {} to DLQ for event {}",
                    action.action_type(),
                    event.id
                );
                if let Err(e) = self
                    .dlq
                    .push(event.clone(), action.clone(), error.to_string())
                    .await
                {
                    error!("Failed to push to DLQ: {}", e);
                    summary.dlq_errors += 1;
                }
                summary.failed_actions += 1;
            }
        }
    }

    /// Calculate backoff delay based on attempt number and strategy
    fn calculate_backoff(&self, attempt: u32, config: &RetryConfig) -> Duration {
        let delay_ms = match config.backoff_strategy {
            BackoffStrategy::Exponential => {
                // 2^(attempt-1) * initial_delay, capped at max_delay
                let exponent = (attempt - 1) as u32;
                let base_delay = config.initial_delay_ms * (2_u64.pow(exponent));
                base_delay.min(config.max_delay_ms)
            }
            BackoffStrategy::Linear => {
                // attempt * initial_delay, capped at max_delay
                let base_delay = config.initial_delay_ms * (attempt as u64);
                base_delay.min(config.max_delay_ms)
            }
            BackoffStrategy::Fixed => {
                // Always use initial_delay
                config.initial_delay_ms
            }
        };

        Duration::from_millis(delay_ms)
    }
}

/// Summary of event processing results
#[derive(Debug, Clone, Default)]
pub struct ExecutionSummary {
    /// Number of successful action executions
    pub successful_actions: usize,
    /// Number of failed action executions
    pub failed_actions: usize,
    /// Number of observers skipped due to condition
    pub conditions_skipped: usize,
    /// Total execution time in milliseconds
    pub total_duration_ms: f64,
    /// DLQ push errors
    pub dlq_errors: usize,
    /// Other errors encountered
    pub errors: Vec<String>,
}

impl ExecutionSummary {
    /// Create a new empty summary
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        self.failed_actions == 0 && self.dlq_errors == 0 && self.errors.is_empty()
    }

    /// Get total actions processed
    pub fn total_actions(&self) -> usize {
        self.successful_actions + self.failed_actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use crate::testing::mocks::MockDeadLetterQueue;
    use serde_json::json;

    fn create_test_matcher() -> EventMatcher {
        EventMatcher::new()
    }

    fn create_test_executor() -> ObserverExecutor {
        let matcher = create_test_matcher();
        let dlq = Arc::new(MockDeadLetterQueue::new());
        ObserverExecutor::new(matcher, dlq)
    }

    #[test]
    fn test_executor_creation() {
        let executor = create_test_executor();
        let _ = executor;
    }

    #[test]
    fn test_backoff_exponential() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts: 5,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_strategy: BackoffStrategy::Exponential,
        };

        assert_eq!(executor.calculate_backoff(1, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(2, &config).as_millis(), 200);
        assert_eq!(executor.calculate_backoff(3, &config).as_millis(), 400);
        assert_eq!(executor.calculate_backoff(4, &config).as_millis(), 800);
        assert_eq!(executor.calculate_backoff(5, &config).as_millis(), 1600);
    }

    #[test]
    fn test_backoff_linear() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts: 5,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_strategy: BackoffStrategy::Linear,
        };

        assert_eq!(executor.calculate_backoff(1, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(2, &config).as_millis(), 200);
        assert_eq!(executor.calculate_backoff(3, &config).as_millis(), 300);
        assert_eq!(executor.calculate_backoff(4, &config).as_millis(), 400);
        assert_eq!(executor.calculate_backoff(5, &config).as_millis(), 500);
    }

    #[test]
    fn test_backoff_fixed() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts: 5,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_strategy: BackoffStrategy::Fixed,
        };

        assert_eq!(executor.calculate_backoff(1, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(2, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(3, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(4, &config).as_millis(), 100);
        assert_eq!(executor.calculate_backoff(5, &config).as_millis(), 100);
    }

    #[test]
    fn test_backoff_exponential_cap() {
        let executor = create_test_executor();
        let config = RetryConfig {
            max_attempts: 10,
            initial_delay_ms: 100,
            max_delay_ms: 1000,
            backoff_strategy: BackoffStrategy::Exponential,
        };

        // Should be capped at 1000
        assert_eq!(executor.calculate_backoff(10, &config).as_millis(), 1000);
    }

    #[test]
    fn test_execution_summary_success() {
        let summary = ExecutionSummary {
            successful_actions: 5,
            failed_actions: 0,
            conditions_skipped: 0,
            total_duration_ms: 50.0,
            dlq_errors: 0,
            errors: vec![],
        };

        assert!(summary.is_success());
        assert_eq!(summary.total_actions(), 5);
    }

    #[test]
    fn test_execution_summary_failure() {
        let summary = ExecutionSummary {
            successful_actions: 3,
            failed_actions: 1,
            conditions_skipped: 1,
            total_duration_ms: 75.0,
            dlq_errors: 0,
            errors: vec![],
        };

        assert!(!summary.is_success());
        assert_eq!(summary.total_actions(), 4);
    }

    #[tokio::test]
    async fn test_process_event_no_matching_observers() {
        let executor = create_test_executor();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({"total": 100}),
        );

        let summary = executor.process_event(&event).await.unwrap();

        assert!(summary.is_success());
        assert_eq!(summary.successful_actions, 0);
        assert_eq!(summary.failed_actions, 0);
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 30000);
    }
}
