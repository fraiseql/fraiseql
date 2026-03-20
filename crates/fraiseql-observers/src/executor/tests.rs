#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::{
    config::{ActionConfig, BackoffStrategy, FailurePolicy, RetryConfig},
    error::ObserverError,
    event::{EntityEvent, EventKind},
    matcher::EventMatcher,
    testing::mocks::MockDeadLetterQueue,
    traits::ActionResult,
};

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
        max_attempts:     5,
        initial_delay_ms: 100,
        max_delay_ms:     5000,
        backoff_strategy: BackoffStrategy::Exponential,
    };

    // Jitter: ±25% of base delay — check inclusive range.
    let d = executor.calculate_backoff(1, &config).as_millis();
    assert!((75..=125).contains(&d), "attempt 1: expected ~100 ms (±25%), got {d}");
    let d = executor.calculate_backoff(2, &config).as_millis();
    assert!((150..=250).contains(&d), "attempt 2: expected ~200 ms (±25%), got {d}");
    let d = executor.calculate_backoff(3, &config).as_millis();
    assert!((300..=500).contains(&d), "attempt 3: expected ~400 ms (±25%), got {d}");
    let d = executor.calculate_backoff(4, &config).as_millis();
    assert!((600..=1000).contains(&d), "attempt 4: expected ~800 ms (±25%), got {d}");
    let d = executor.calculate_backoff(5, &config).as_millis();
    assert!((1200..=2000).contains(&d), "attempt 5: expected ~1600 ms (±25%), got {d}");
}

#[test]
fn test_backoff_linear() {
    let executor = create_test_executor();
    let config = RetryConfig {
        max_attempts:     5,
        initial_delay_ms: 100,
        max_delay_ms:     5000,
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
        max_attempts:     5,
        initial_delay_ms: 100,
        max_delay_ms:     5000,
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
        max_attempts:     10,
        initial_delay_ms: 100,
        max_delay_ms:     1000,
        backoff_strategy: BackoffStrategy::Exponential,
    };

    // Cap is at 1000; jitter ±25% gives [750, 1250]
    let d = executor.calculate_backoff(10, &config).as_millis();
    assert!((750..=1250).contains(&d), "capped attempt: expected ~1000 ms (±25%), got {d}");
}

#[test]
fn test_execution_summary_success() {
    let summary = ExecutionSummary {
        successful_actions: 5,
        failed_actions:     0,
        conditions_skipped: 0,
        total_duration_ms:  50.0,
        dlq_errors:         0,
        errors:             vec![],
        duplicate_skipped:  false,
        tenant_rejected:    false,
        cache_hits:         0,
        cache_misses:       0,
    };

    assert!(summary.is_success());
    assert_eq!(summary.total_actions(), 5);
}

#[test]
fn test_execution_summary_failure() {
    let summary = ExecutionSummary {
        successful_actions: 3,
        failed_actions:     1,
        conditions_skipped: 1,
        total_duration_ms:  75.0,
        dlq_errors:         0,
        errors:             vec![],
        duplicate_skipped:  false,
        tenant_rejected:    false,
        cache_hits:         0,
        cache_misses:       0,
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

// Listener integration tests ()

#[tokio::test]
async fn test_run_listener_loop_empty_batch() {
    use sqlx::postgres::PgPool;

    use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

    let executor = create_test_executor();
    let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
    let config = ChangeLogListenerConfig::new(pool);
    let mut listener = ChangeLogListener::new(config);

    // Run for 1 iteration - should handle empty batch gracefully
    let result = executor.run_listener_loop(&mut listener, Some(1)).await;

    // Should succeed despite no entries
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_checkpoint_tracking() {
    use sqlx::postgres::PgPool;

    use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

    let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
    let config = ChangeLogListenerConfig::new(pool);
    let mut listener = ChangeLogListener::new(config);

    // Initial checkpoint should be 0
    assert_eq!(listener.checkpoint(), 0);

    // Update checkpoint
    listener.set_checkpoint(100);
    assert_eq!(listener.checkpoint(), 100);

    // Checkpoint persists
    assert_eq!(listener.checkpoint(), 100);
}

#[tokio::test]
async fn test_listener_config_builder() {
    use sqlx::postgres::PgPool;

    use crate::listener::ChangeLogListenerConfig;

    let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
    let config = ChangeLogListenerConfig::new(pool)
        .with_poll_interval(250)
        .with_batch_size(200)
        .with_resume_from(500);

    assert_eq!(config.poll_interval_ms, 250);
    assert_eq!(config.batch_size, 200);
    assert_eq!(config.resume_from_id, Some(500));
}

// Error handling and resilience tests ()

#[tokio::test]
async fn test_run_listener_loop_with_iteration_limit() {
    use sqlx::postgres::PgPool;

    use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

    let executor = create_test_executor();
    let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
    let config = ChangeLogListenerConfig::new(pool);
    let mut listener = ChangeLogListener::new(config);

    // Should complete successfully with iteration limit
    let result = executor.run_listener_loop(&mut listener, Some(3)).await;
    assert!(result.is_ok());
}

#[test]
fn test_exponential_backoff_calculation() {
    let executor = create_test_executor();
    let config = RetryConfig {
        max_attempts:     5,
        initial_delay_ms: 100,
        max_delay_ms:     5000,
        backoff_strategy: BackoffStrategy::Exponential,
    };

    // Exponential backoff should double each time
    let delay1 = executor.calculate_backoff(1, &config);
    let delay2 = executor.calculate_backoff(2, &config);
    let delay3 = executor.calculate_backoff(3, &config);

    // Jitter ±25%: 100 ms → [75, 125], 200 ms → [150, 250], 400 ms → [300, 500]
    assert!(
        delay1.as_millis() >= 75 && delay1.as_millis() <= 125,
        "delay1 expected ~100 ms (±25%), got {}",
        delay1.as_millis()
    );
    assert!(
        delay2.as_millis() >= 150 && delay2.as_millis() <= 250,
        "delay2 expected ~200 ms (±25%), got {}",
        delay2.as_millis()
    );
    assert!(
        delay3.as_millis() >= 300 && delay3.as_millis() <= 500,
        "delay3 expected ~400 ms (±25%), got {}",
        delay3.as_millis()
    );
}

#[test]
fn test_exponential_backoff_cap() {
    let executor = create_test_executor();
    let config = RetryConfig {
        max_attempts:     10,
        initial_delay_ms: 100,
        max_delay_ms:     1000,
        backoff_strategy: BackoffStrategy::Exponential,
    };

    // Should cap at max_delay_ms
    let delay8 = executor.calculate_backoff(8, &config);
    let delay9 = executor.calculate_backoff(9, &config);

    // Both should be near max (1000); jitter ±25% gives [750, 1250]
    assert!(
        delay8.as_millis() >= 750 && delay8.as_millis() <= 1250,
        "delay8 expected ~1000 ms (±25%), got {}",
        delay8.as_millis()
    );
    assert!(
        delay9.as_millis() >= 750 && delay9.as_millis() <= 1250,
        "delay9 expected ~1000 ms (±25%), got {}",
        delay9.as_millis()
    );
}

#[tokio::test]
async fn test_run_listener_loop_zero_iterations() {
    use sqlx::postgres::PgPool;

    use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

    let executor = create_test_executor();
    let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
    let config = ChangeLogListenerConfig::new(pool);
    let mut listener = ChangeLogListener::new(config);

    // Should handle zero iterations
    let result = executor.run_listener_loop(&mut listener, Some(0)).await;
    assert!(result.is_ok());
}

// =========================================================================
// Helper: build an executor backed by a MockActionDispatcher
// =========================================================================

fn make_mock_executor(
    dispatcher: Arc<crate::testing::mocks::MockActionDispatcher>,
    dlq: Arc<crate::testing::mocks::MockDeadLetterQueue>,
) -> ObserverExecutor {
    ObserverExecutor::with_dispatcher(EventMatcher::new(), dlq, dispatcher)
}

fn make_mock_executor_with_matcher(
    matcher: EventMatcher,
    dispatcher: Arc<crate::testing::mocks::MockActionDispatcher>,
    dlq: Arc<crate::testing::mocks::MockDeadLetterQueue>,
) -> ObserverExecutor {
    ObserverExecutor::with_dispatcher(matcher, dlq, dispatcher)
}

fn webhook_action() -> ActionConfig {
    ActionConfig::Webhook {
        url:           Some("https://example.com/hook".to_string()),
        url_env:       None,
        headers:       std::collections::HashMap::new(),
        body_template: None,
    }
}

fn test_event() -> crate::event::EntityEvent {
    crate::event::EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        uuid::Uuid::new_v4(),
        json!({"id": 42}),
    )
}

fn make_retry(max_attempts: u32, initial_delay_ms: u64) -> RetryConfig {
    RetryConfig {
        max_attempts,
        initial_delay_ms,
        max_delay_ms: 5000,
        backoff_strategy: BackoffStrategy::Fixed,
    }
}

// =========================================================================
// execute_action_with_retry — happy path
// =========================================================================

#[tokio::test]
async fn test_retry_happy_path_succeeds_first_attempt() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 5.0);
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), Arc::clone(&dlq));

    let action = webhook_action();
    let event = test_event();
    let retry = make_retry(3, 0);
    let failure_policy = FailurePolicy::Log;
    let mut summary = ExecutionSummary::new();

    executor
        .execute_action_with_retry(&action, &event, &retry, &failure_policy, &mut summary)
        .await;

    assert_eq!(summary.successful_actions, 1, "expected 1 success");
    assert_eq!(summary.failed_actions, 0);
    assert_eq!(dispatcher.call_count(), 1, "dispatched exactly once");
}

#[tokio::test]
async fn test_retry_total_duration_accumulated_on_success() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 42.0);
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let action = webhook_action();
    let event = test_event();
    let retry = make_retry(3, 0);
    let mut summary = ExecutionSummary::new();

    executor
        .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
        .await;

    assert!((summary.total_duration_ms - 42.0).abs() < f64::EPSILON);
}

// =========================================================================
// execute_action_with_retry — transient errors + retries
// =========================================================================

#[tokio::test]
async fn test_retry_transient_then_success() {
    use std::sync::atomic::{AtomicU32, Ordering};

    // Custom dispatcher that fails the first call, succeeds the second.
    struct TransientThenOkDispatcher {
        attempts: AtomicU32,
    }

    impl ActionDispatcher for TransientThenOkDispatcher {
        fn dispatch<'a>(
            &'a self,
            action: &'a ActionConfig,
            _event: &'a EntityEvent,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionResult>> + Send + 'a>>
        {
            let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
            let action_type = action.action_type().to_string();
            Box::pin(async move {
                if attempt == 1 {
                    Err(ObserverError::ActionExecutionFailed {
                        reason: "transient".to_string(),
                    })
                } else {
                    Ok(ActionResult {
                        action_type,
                        success: true,
                        message: "ok".to_string(),
                        duration_ms: 1.0,
                    })
                }
            })
        }
    }

    let dispatcher = Arc::new(TransientThenOkDispatcher {
        attempts: AtomicU32::new(0),
    });
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = ObserverExecutor::with_dispatcher(EventMatcher::new(), dlq, dispatcher);

    let action = webhook_action();
    let event = test_event();
    let retry = make_retry(3, 0);
    let mut summary = ExecutionSummary::new();

    executor
        .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
        .await;

    assert_eq!(summary.successful_actions, 1);
    assert_eq!(summary.failed_actions, 0);
}

#[tokio::test]
async fn test_retry_permanent_error_no_retry() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    // ActionPermanentlyFailed is NOT transient — should not retry
    dispatcher.expect_err(
        "webhook",
        ObserverError::ActionPermanentlyFailed {
            reason: "bad config".to_string(),
        },
    );
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let action = webhook_action();
    let event = test_event();
    let retry = make_retry(5, 0);
    let mut summary = ExecutionSummary::new();

    executor
        .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
        .await;

    // Called exactly once — no retry for permanent errors
    assert_eq!(dispatcher.call_count(), 1);
    assert_eq!(summary.successful_actions, 0);
    assert_eq!(summary.failed_actions, 1);
}

#[tokio::test]
async fn test_retry_exhausted_after_max_attempts() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    // ActionExecutionFailed IS transient — will retry until exhaustion
    dispatcher.expect_err(
        "webhook",
        ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        },
    );
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let action = webhook_action();
    let event = test_event();
    let retry = make_retry(3, 0);
    let mut summary = ExecutionSummary::new();

    executor
        .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
        .await;

    // Should have been called max_attempts times (3) then failed
    assert_eq!(dispatcher.call_count(), 3);
    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.successful_actions, 0);
}

#[tokio::test]
async fn test_retry_single_attempt_max_no_retry() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    dispatcher.expect_err(
        "webhook",
        ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        },
    );
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let action = webhook_action();
    let event = test_event();
    // max_attempts=1 → no retries at all
    let retry = make_retry(1, 0);
    let mut summary = ExecutionSummary::new();

    executor
        .execute_action_with_retry(&action, &event, &retry, &FailurePolicy::Log, &mut summary)
        .await;

    assert_eq!(dispatcher.call_count(), 1);
    assert_eq!(summary.failed_actions, 1);
}

#[tokio::test]
async fn test_retry_two_transient_then_success() {
    use std::sync::atomic::{AtomicU32, Ordering};

    struct FailTwiceThenOk {
        count: AtomicU32,
    }

    impl ActionDispatcher for FailTwiceThenOk {
        fn dispatch<'a>(
            &'a self,
            action: &'a ActionConfig,
            _event: &'a EntityEvent,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionResult>> + Send + 'a>>
        {
            let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
            let at = action.action_type().to_string();
            Box::pin(async move {
                if n <= 2 {
                    Err(ObserverError::ActionExecutionFailed {
                        reason: format!("transient attempt {n}"),
                    })
                } else {
                    Ok(ActionResult {
                        action_type: at,
                        success:     true,
                        message:     "ok".to_string(),
                        duration_ms: 1.0,
                    })
                }
            })
        }
    }

    let dispatcher = Arc::new(FailTwiceThenOk {
        count: AtomicU32::new(0),
    });
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = ObserverExecutor::with_dispatcher(EventMatcher::new(), dlq, dispatcher);

    let mut summary = ExecutionSummary::new();
    executor
        .execute_action_with_retry(
            &webhook_action(),
            &test_event(),
            &make_retry(5, 0),
            &FailurePolicy::Log,
            &mut summary,
        )
        .await;

    assert_eq!(summary.successful_actions, 1);
    assert_eq!(summary.failed_actions, 0);
}

// =========================================================================
// handle_action_failure — FailurePolicy branches
// =========================================================================

#[tokio::test]
async fn test_failure_policy_log_increments_failed_actions() {
    let executor = create_test_executor();
    let action = webhook_action();
    let event = test_event();
    let error = ObserverError::ActionExecutionFailed {
        reason: "SMTP timeout".to_string(),
    };
    let mut summary = ExecutionSummary::new();

    executor
        .handle_action_failure(&action, &event, &error, &FailurePolicy::Log, &mut summary)
        .await;

    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.dlq_errors, 0);
}

#[tokio::test]
async fn test_failure_policy_alert_increments_failed_actions() {
    let executor = create_test_executor();
    let action = webhook_action();
    let event = test_event();
    let error = ObserverError::ActionExecutionFailed {
        reason: "network error".to_string(),
    };
    let mut summary = ExecutionSummary::new();

    executor
        .handle_action_failure(&action, &event, &error, &FailurePolicy::Alert, &mut summary)
        .await;

    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.dlq_errors, 0);
}

#[tokio::test]
async fn test_failure_policy_dlq_success_no_dlq_error() {
    // MockDeadLetterQueue.push() always succeeds → dlq_errors should remain 0
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), Arc::clone(&dlq));
    let action = webhook_action();
    let event = test_event();
    let error = ObserverError::ActionExecutionFailed {
        reason: "timeout".to_string(),
    };
    let mut summary = ExecutionSummary::new();

    executor
        .handle_action_failure(&action, &event, &error, &FailurePolicy::Dlq, &mut summary)
        .await;

    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.dlq_errors, 0);
    // Item was pushed to the DLQ
    assert_eq!(dlq.item_count(), 1);
}

#[tokio::test]
async fn test_failure_policy_dlq_error_counted() {
    use uuid::Uuid;

    use crate::traits::{DeadLetterQueue, DlqItem};

    /// A DLQ that always returns an error from push().
    struct AlwaysFailDlq;

    #[async_trait::async_trait]
    impl DeadLetterQueue for AlwaysFailDlq {
        async fn push(
            &self,
            _event: EntityEvent,
            _action: ActionConfig,
            _error: String,
        ) -> Result<Uuid> {
            Err(ObserverError::DlqError {
                reason: "redis unavailable".to_string(),
            })
        }

        async fn get_pending(&self, _limit: i64) -> Result<Vec<DlqItem>> {
            Ok(vec![])
        }

        async fn mark_success(&self, _id: Uuid) -> Result<()> {
            Ok(())
        }

        async fn mark_retry_failed(&self, _id: Uuid, _error: &str) -> Result<()> {
            Ok(())
        }
    }

    let failing_dlq = Arc::new(AlwaysFailDlq);
    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    let executor = ObserverExecutor::with_dispatcher(EventMatcher::new(), failing_dlq, dispatcher);
    let action = webhook_action();
    let event = test_event();
    let error = ObserverError::ActionExecutionFailed {
        reason: "timeout".to_string(),
    };
    let mut summary = ExecutionSummary::new();

    executor
        .handle_action_failure(&action, &event, &error, &FailurePolicy::Dlq, &mut summary)
        .await;

    assert_eq!(summary.dlq_errors, 1);
    assert_eq!(summary.failed_actions, 1);
}

#[tokio::test]
async fn test_failure_policy_log_does_not_touch_dlq() {
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), Arc::clone(&dlq));

    let mut summary = ExecutionSummary::new();
    executor
        .handle_action_failure(
            &webhook_action(),
            &test_event(),
            &ObserverError::ActionExecutionFailed {
                reason: "err".to_string(),
            },
            &FailurePolicy::Log,
            &mut summary,
        )
        .await;

    // DLQ must have received no pushes
    assert_eq!(dlq.item_count(), 0);
}

// =========================================================================
// execute_action_internal — dispatch-level tests
// =========================================================================

#[tokio::test]
async fn test_dispatch_webhook_missing_url_returns_invalid_config() {
    // DefaultActionDispatcher handles the missing-URL case directly.
    let executor = create_test_executor();
    let action = ActionConfig::Webhook {
        url:           None,
        url_env:       None,
        headers:       std::collections::HashMap::new(),
        body_template: None,
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ObserverError::InvalidActionConfig { .. }));
}

#[tokio::test]
async fn test_dispatch_webhook_url_env_var_missing_returns_error_with_var_name() {
    let executor = create_test_executor();
    let action = ActionConfig::Webhook {
        url:           None,
        url_env:       Some("FRAISEQL_TEST_WEBHOOK_URL_DEFINITELY_NOT_SET".to_string()),
        headers:       std::collections::HashMap::new(),
        body_template: None,
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    if let ObserverError::InvalidActionConfig { reason } = err {
        assert!(
            reason.contains("FRAISEQL_TEST_WEBHOOK_URL_DEFINITELY_NOT_SET"),
            "error should mention the missing env var name, got: {reason}"
        );
    } else {
        panic!("expected InvalidActionConfig, got {err:?}");
    }
}

#[tokio::test]
async fn test_dispatch_slack_missing_webhook_url_returns_invalid_config() {
    let executor = create_test_executor();
    let action = ActionConfig::Slack {
        webhook_url:      None,
        webhook_url_env:  None,
        channel:          None,
        message_template: None,
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
}

#[tokio::test]
async fn test_dispatch_email_missing_to_returns_invalid_config() {
    let executor = create_test_executor();
    let action = ActionConfig::Email {
        to:               None,
        to_template:      None,
        subject:          Some("Hello".to_string()),
        subject_template: None,
        body_template:    Some("body".to_string()),
        reply_to:         None,
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
}

#[tokio::test]
async fn test_dispatch_email_missing_subject_returns_invalid_config() {
    let executor = create_test_executor();
    let action = ActionConfig::Email {
        to:               Some("user@example.com".to_string()),
        to_template:      None,
        subject:          None,
        subject_template: None,
        body_template:    Some("body".to_string()),
        reply_to:         None,
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
}

#[tokio::test]
async fn test_dispatch_sms_missing_phone_returns_invalid_config() {
    let executor = create_test_executor();
    let action = ActionConfig::Sms {
        phone:            None,
        phone_template:   None,
        message_template: Some("Hi".to_string()),
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
}

#[tokio::test]
async fn test_dispatch_push_missing_device_token_returns_invalid_config() {
    let executor = create_test_executor();
    let action = ActionConfig::Push {
        device_token:   None,
        title_template: Some("title".to_string()),
        body_template:  Some("body".to_string()),
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ObserverError::InvalidActionConfig { .. }));
}

#[tokio::test]
async fn test_dispatch_slack_url_env_var_missing_error() {
    let executor = create_test_executor();
    let action = ActionConfig::Slack {
        webhook_url:      None,
        webhook_url_env:  Some("FRAISEQL_TEST_SLACK_URL_MISSING_VAR".to_string()),
        channel:          None,
        message_template: None,
    };
    let event = test_event();

    let result = executor.execute_action_internal(&action, &event).await;

    assert!(result.is_err());
    if let ObserverError::InvalidActionConfig { reason } = result.unwrap_err() {
        assert!(
            reason.contains("FRAISEQL_TEST_SLACK_URL_MISSING_VAR"),
            "reason should mention var name, got: {reason}"
        );
    } else {
        panic!("expected InvalidActionConfig");
    }
}

// =========================================================================
// process_event integration tests with MockActionDispatcher
// =========================================================================

#[tokio::test]
async fn test_process_event_no_matching_observers_returns_empty_summary() {
    // Empty matcher → no observers match → summary zeroed
    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let event = test_event();
    let summary = executor.process_event(&event).await.unwrap();

    assert_eq!(summary.successful_actions, 0);
    assert_eq!(summary.failed_actions, 0);
    assert_eq!(dispatcher.call_count(), 0);
}

#[tokio::test]
async fn test_process_event_with_mock_dispatcher_success() {
    use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 10.0);

    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  None,
        actions:    vec![webhook_action()],
        retry:      RetryConfig {
            max_attempts: 1,
            initial_delay_ms: 0,
            ..RetryConfig::default()
        },
        on_failure:  FP::Log,
        synchronous: false,
    };
    let mut observers = std::collections::HashMap::new();
    observers.insert("obs".to_string(), observer);
    let matcher = EventMatcher::build(observers).unwrap();

    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

    let event = test_event();
    let summary = executor.process_event(&event).await.unwrap();

    assert_eq!(summary.successful_actions, 1);
    assert_eq!(summary.failed_actions, 0);
    assert_eq!(dispatcher.call_count(), 1);
}

#[tokio::test]
async fn test_process_event_mock_dispatcher_failure_goes_to_log_policy() {
    use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    dispatcher.expect_err(
        "webhook",
        ObserverError::ActionPermanentlyFailed {
            reason: "stub failure".to_string(),
        },
    );

    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  None,
        actions:    vec![webhook_action()],
        retry:      RetryConfig {
            max_attempts: 1,
            initial_delay_ms: 0,
            ..RetryConfig::default()
        },
        on_failure:  FP::Log,
        synchronous: false,
    };
    let mut observers = std::collections::HashMap::new();
    observers.insert("obs".to_string(), observer);
    let matcher = EventMatcher::build(observers).unwrap();

    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

    let summary = executor.process_event(&test_event()).await.unwrap();

    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.successful_actions, 0);
}

#[tokio::test]
async fn test_process_event_condition_false_skips_action() {
    use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 1.0);

    // Condition always false: id == 99999 won't match json({"id":42})
    // Using a numeric field that exists so eval_comparison returns Ok(false)
    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  Some("id == 99999".to_string()),
        actions:    vec![webhook_action()],
        retry:      RetryConfig::default(),
        on_failure:  FP::Log,
        synchronous: false,
    };
    let mut observers = std::collections::HashMap::new();
    observers.insert("obs".to_string(), observer);
    let matcher = EventMatcher::build(observers).unwrap();

    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

    let summary = executor.process_event(&test_event()).await.unwrap();

    // Condition skipped → no dispatch, no failure
    assert_eq!(summary.conditions_skipped, 1);
    assert_eq!(summary.successful_actions, 0);
    assert_eq!(dispatcher.call_count(), 0);
}

#[tokio::test]
async fn test_process_event_multiple_observers_all_succeed() {
    use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 5.0);

    // Build three observers with unique names so the matcher registers all three.
    let mut observers_map = std::collections::HashMap::new();
    for i in 0..3usize {
        let observer = ObserverDefinition {
            event_type: "INSERT".to_string(),
            entity:     "Order".to_string(),
            condition:  None,
            actions:    vec![webhook_action()],
            retry:      RetryConfig {
                max_attempts: 1,
                initial_delay_ms: 0,
                ..RetryConfig::default()
            },
            on_failure:  FP::Log,
            synchronous: false,
        };
        observers_map.insert(format!("obs_{i}"), observer);
    }
    let matcher = EventMatcher::build(observers_map).unwrap();

    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

    let summary = executor.process_event(&test_event()).await.unwrap();

    assert_eq!(summary.successful_actions, 3);
    assert_eq!(dispatcher.call_count(), 3);
}

#[tokio::test]
async fn test_process_event_multiple_actions_in_one_observer() {
    use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 5.0);
    dispatcher.expect_ok("cache", 2.0);

    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  None,
        actions:    vec![
            webhook_action(),
            ActionConfig::Cache {
                key_pattern: "orders:*".to_string(),
                action:      "invalidate".to_string(),
            },
        ],
        retry:      RetryConfig {
            max_attempts: 1,
            initial_delay_ms: 0,
            ..RetryConfig::default()
        },
        on_failure:  FP::Log,
        synchronous: false,
    };
    let mut observers = std::collections::HashMap::new();
    observers.insert("obs".to_string(), observer);
    let matcher = EventMatcher::build(observers).unwrap();

    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), dlq);

    let summary = executor.process_event(&test_event()).await.unwrap();

    assert_eq!(summary.successful_actions, 2);
    assert_eq!(dispatcher.call_count(), 2);
}

#[tokio::test]
async fn test_process_event_dlq_policy_pushes_on_failure() {
    use crate::config::{FailurePolicy as FP, ObserverDefinition, RetryConfig};

    let dispatcher = Arc::new(crate::testing::mocks::MockActionDispatcher::new());
    dispatcher.expect_err(
        "webhook",
        ObserverError::ActionPermanentlyFailed {
            reason: "permanent".to_string(),
        },
    );

    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  None,
        actions:    vec![webhook_action()],
        retry:      RetryConfig {
            max_attempts: 1,
            initial_delay_ms: 0,
            ..RetryConfig::default()
        },
        on_failure:  FP::Dlq,
        synchronous: false,
    };
    let mut observers = std::collections::HashMap::new();
    observers.insert("obs".to_string(), observer);
    let matcher = EventMatcher::build(observers).unwrap();

    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor =
        make_mock_executor_with_matcher(matcher, Arc::clone(&dispatcher), Arc::clone(&dlq));

    let summary = executor.process_event(&test_event()).await.unwrap();

    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.dlq_errors, 0);
    assert_eq!(dlq.item_count(), 1);
}

#[tokio::test]
async fn test_failure_policy_log_does_not_increment_dlq_errors() {
    let executor = create_test_executor();
    let mut summary = ExecutionSummary::new();

    executor
        .handle_action_failure(
            &webhook_action(),
            &test_event(),
            &ObserverError::ActionExecutionFailed {
                reason: "timeout".to_string(),
            },
            &FailurePolicy::Log,
            &mut summary,
        )
        .await;

    assert_eq!(summary.dlq_errors, 0);
    assert_eq!(summary.failed_actions, 1);
}

#[tokio::test]
async fn test_failure_policy_alert_does_not_increment_dlq_errors() {
    let executor = create_test_executor();
    let mut summary = ExecutionSummary::new();

    executor
        .handle_action_failure(
            &webhook_action(),
            &test_event(),
            &ObserverError::ActionExecutionFailed {
                reason: "alert test".to_string(),
            },
            &FailurePolicy::Alert,
            &mut summary,
        )
        .await;

    assert_eq!(summary.dlq_errors, 0);
    assert_eq!(summary.failed_actions, 1);
}

#[tokio::test]
async fn test_mock_dispatcher_call_log_records_action_type() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 5.0);
    dispatcher.expect_ok("cache", 2.0);

    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let actions = [
        webhook_action(),
        ActionConfig::Cache {
            key_pattern: "k:*".to_string(),
            action:      "invalidate".to_string(),
        },
    ];

    for action in &actions {
        let mut s = ExecutionSummary::new();
        executor
            .execute_action_with_retry(
                action,
                &test_event(),
                &make_retry(1, 0),
                &FailurePolicy::Log,
                &mut s,
            )
            .await;
    }

    let calls = dispatcher.calls();
    assert_eq!(calls, vec!["webhook", "cache"]);
}

#[tokio::test]
async fn test_execution_summary_is_success_with_mock() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    dispatcher.expect_ok("webhook", 1.0);
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let mut summary = ExecutionSummary::new();
    executor
        .execute_action_with_retry(
            &webhook_action(),
            &test_event(),
            &make_retry(1, 0),
            &FailurePolicy::Log,
            &mut summary,
        )
        .await;

    assert!(summary.is_success());
    assert_eq!(summary.total_actions(), 1);
}

#[tokio::test]
async fn test_execution_summary_not_success_on_failure() {
    use crate::testing::mocks::MockActionDispatcher;

    let dispatcher = Arc::new(MockActionDispatcher::new());
    dispatcher.expect_err(
        "webhook",
        ObserverError::ActionPermanentlyFailed {
            reason: "permanent".to_string(),
        },
    );
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let executor = make_mock_executor(Arc::clone(&dispatcher), dlq);

    let mut summary = ExecutionSummary::new();
    executor
        .execute_action_with_retry(
            &webhook_action(),
            &test_event(),
            &make_retry(1, 0),
            &FailurePolicy::Log,
            &mut summary,
        )
        .await;

    assert!(!summary.is_success());
    assert_eq!(summary.total_actions(), 1);
}

// =========================================================================
// max_dlq_size — DLQ overflow / drop-newest tests
// =========================================================================

#[tokio::test]
async fn test_dlq_size_limit_allows_push_when_under_cap() {
    // With max_dlq_size = 2 and one push, the entry should reach the DLQ.
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let dlq_dyn: Arc<dyn crate::traits::DeadLetterQueue> = Arc::clone(&dlq) as _;
    let executor = ObserverExecutor::new_with_dlq_limit(EventMatcher::new(), dlq_dyn, 2);

    let error = ObserverError::ActionExecutionFailed {
        reason: "test".to_string(),
    };
    let mut summary = ExecutionSummary::new();

    executor
        .handle_action_failure(
            &webhook_action(),
            &test_event(),
            &error,
            &FailurePolicy::Dlq,
            &mut summary,
        )
        .await;

    assert_eq!(dlq.item_count(), 1, "entry should be in the DLQ");
    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.dlq_errors, 0);
}

#[tokio::test]
async fn test_dlq_size_limit_drops_entry_when_at_cap() {
    // Fill DLQ to the cap (2 pushes), then a third push should be dropped.
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let dlq_dyn: Arc<dyn crate::traits::DeadLetterQueue> = Arc::clone(&dlq) as _;
    let executor = ObserverExecutor::new_with_dlq_limit(EventMatcher::new(), dlq_dyn, 2);

    let error = ObserverError::ActionExecutionFailed {
        reason: "test".to_string(),
    };

    // First two pushes fill the cap.
    for _ in 0..2 {
        let mut summary = ExecutionSummary::new();
        executor
            .handle_action_failure(
                &webhook_action(),
                &test_event(),
                &error,
                &FailurePolicy::Dlq,
                &mut summary,
            )
            .await;
    }

    assert_eq!(dlq.item_count(), 2, "two entries should be in the DLQ");

    // Third push should be silently dropped.
    let mut summary = ExecutionSummary::new();
    executor
        .handle_action_failure(
            &webhook_action(),
            &test_event(),
            &error,
            &FailurePolicy::Dlq,
            &mut summary,
        )
        .await;

    assert_eq!(dlq.item_count(), 2, "DLQ must not grow past cap");
    // Even dropped entries are counted as failed.
    assert_eq!(summary.failed_actions, 1);
    assert_eq!(summary.dlq_errors, 0);
}

#[tokio::test]
async fn test_dlq_size_limit_none_allows_unbounded_pushes() {
    // No limit set → all pushes should reach the DLQ.
    let dlq = Arc::new(crate::testing::mocks::MockDeadLetterQueue::new());
    let dlq_dyn: Arc<dyn crate::traits::DeadLetterQueue> = Arc::clone(&dlq) as _;
    // new() sets max_dlq_size = None
    let executor = ObserverExecutor::new(EventMatcher::new(), dlq_dyn);

    let error = ObserverError::ActionExecutionFailed {
        reason: "test".to_string(),
    };

    for _ in 0..5 {
        let mut summary = ExecutionSummary::new();
        executor
            .handle_action_failure(
                &webhook_action(),
                &test_event(),
                &error,
                &FailurePolicy::Dlq,
                &mut summary,
            )
            .await;
    }

    assert_eq!(dlq.item_count(), 5, "all 5 entries should be in the DLQ");
}

// ── AST cache tests (14-2) ────────────────────────────────────────────────

#[test]
fn test_compile_condition_returns_ast_for_valid_condition() {
    use crate::config::runtime::ObserverDefinition;
    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  Some("total > 100".to_string()),
        actions:    vec![],
        retry:      crate::config::RetryConfig::default(),
        on_failure:  crate::config::FailurePolicy::Log,
        synchronous: false,
    };
    let result = observer.compile_condition();
    assert!(result.is_ok(), "valid condition must compile without error");
    assert!(result.unwrap().is_some(), "compile_condition must return Some(ast)");
}

#[test]
fn test_compile_condition_returns_none_when_no_condition() {
    use crate::config::runtime::ObserverDefinition;
    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  None,
        actions:    vec![],
        retry:      crate::config::RetryConfig::default(),
        on_failure:  crate::config::FailurePolicy::Log,
        synchronous: false,
    };
    let result = observer.compile_condition();
    assert!(result.is_ok());
    assert!(
        result.unwrap().is_none(),
        "compile_condition must return None when no condition"
    );
}

#[test]
fn test_compile_condition_returns_error_for_invalid_dsl() {
    use crate::config::runtime::ObserverDefinition;
    let observer = ObserverDefinition {
        event_type: "INSERT".to_string(),
        entity:     "Order".to_string(),
        condition:  Some("@@@invalid$$$".to_string()),
        actions:    vec![],
        retry:      crate::config::RetryConfig::default(),
        on_failure:  crate::config::FailurePolicy::Log,
        synchronous: false,
    };
    let result = observer.compile_condition();
    assert!(result.is_err(), "invalid DSL must return an error from compile_condition");
}

// ── Action timeout tests (14-5) ───────────────────────────────────────────

#[tokio::test]
async fn test_action_timeout_fires_when_dispatcher_is_slow() {
    use std::time::Duration;

    use crate::{executor::ActionDispatcher, traits::ActionResult};

    struct SlowDispatcher;

    impl ActionDispatcher for SlowDispatcher {
        fn dispatch<'a>(
            &'a self,
            action: &'a ActionConfig,
            _event: &'a EntityEvent,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = crate::error::Result<ActionResult>> + Send + 'a>,
        > {
            let action_type = action.action_type().to_string();
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(ActionResult {
                    action_type,
                    success: true,
                    message: "slow ok".to_string(),
                    duration_ms: 200.0,
                })
            })
        }
    }

    let dlq = Arc::new(MockDeadLetterQueue::new());
    let dlq_dyn: Arc<dyn crate::traits::DeadLetterQueue> = Arc::clone(&dlq) as _;
    let mut executor =
        ObserverExecutor::with_dispatcher(EventMatcher::new(), dlq_dyn, Arc::new(SlowDispatcher));
    // Set a 10 ms timeout — the dispatcher sleeps 200 ms, so it must time out.
    executor.action_timeout_ms = Some(10);

    let action = webhook_action();
    let event = test_event();
    let result = executor.execute_action_internal(&action, &event).await;
    assert!(result.is_err(), "slow action must be interrupted by timeout");
    let err = result.unwrap_err();
    assert!(err.to_string().contains("timed out"), "error must mention timeout, got: {err}");
}
