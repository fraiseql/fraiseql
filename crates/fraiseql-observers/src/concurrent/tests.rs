#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod concurrent_tests {
    use std::time::Duration;

    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        concurrent::*,
        config::ActionConfig,
        error::Result,
        event::EntityEvent,
        traits::{ActionExecutor, ActionResult},
    };

    // Create a simple mock for testing
    #[derive(Clone)]
    struct TestExecutor;

    impl ActionExecutor for TestExecutor {
        async fn execute(
            &self,
            _event: &EntityEvent,
            _action: &ActionConfig,
        ) -> Result<ActionResult> {
            Ok(ActionResult {
                action_type: "test".to_string(),
                success:     true,
                message:     "Test success".to_string(),
                duration_ms: 10.0,
                status_code: None,
            })
        }
    }

    #[tokio::test]
    async fn test_concurrent_execution_empty() {
        let executor = TestExecutor;
        let concurrent = ConcurrentActionExecutor::new(executor, 1000);
        let event = EntityEvent::new(
            crate::event::EventKind::Created,
            "Test".to_string(),
            Uuid::new_v4(),
            json!({}),
        );

        let results = concurrent.execute_all(&event, &[]).await;
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_concurrent_execution_single_action() {
        let executor = TestExecutor;
        let concurrent = ConcurrentActionExecutor::new(executor, 5000);
        let event = EntityEvent::new(
            crate::event::EventKind::Created,
            "Test".to_string(),
            Uuid::new_v4(),
            json!({"email": "test@example.com"}),
        );

        let action = ActionConfig::Email {
            to:               Some("test@example.com".to_string()),
            to_template:      None,
            subject:          Some("Test".to_string()),
            subject_template: None,
            body_template:    Some("Test body".to_string()),
            reply_to:         None,
        };

        let results = concurrent.execute_all(&event, &[action]).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[test]
    fn test_concurrent_timeout_configuration() {
        let executor = TestExecutor;
        let concurrent = ConcurrentActionExecutor::new(executor, 5000);

        assert_eq!(concurrent.action_timeout_ms(), 5000);
    }

    #[test]
    fn test_concurrent_executor_clone() {
        // Ensure ConcurrentActionExecutor is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<ConcurrentActionExecutor<TestExecutor>>();
    }

    // --- S11-2: failure and timeout tests ---

    #[derive(Clone)]
    struct FailingExecutor;

    impl ActionExecutor for FailingExecutor {
        async fn execute(
            &self,
            _event: &EntityEvent,
            _action: &ActionConfig,
        ) -> Result<ActionResult> {
            Err(crate::error::ObserverError::ActionExecutionFailed {
                reason: "injected failure".to_string(),
            })
        }
    }

    #[derive(Clone)]
    struct SlowExecutor {
        delay_ms: u64,
    }

    impl ActionExecutor for SlowExecutor {
        async fn execute(
            &self,
            _event: &EntityEvent,
            _action: &ActionConfig,
        ) -> Result<ActionResult> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            #[allow(clippy::cast_precision_loss)]
            // Reason: f64 precision is acceptable for metrics counters
            let duration_ms = self.delay_ms as f64;
            Ok(ActionResult {
                action_type: "slow".to_string(),
                success: true,
                message: "Completed after delay".to_string(),
                duration_ms,
                status_code: None,
            })
        }
    }

    fn make_event() -> EntityEvent {
        EntityEvent::new(
            crate::event::EventKind::Created,
            "Test".to_string(),
            Uuid::new_v4(),
            json!({}),
        )
    }

    fn email_action() -> ActionConfig {
        ActionConfig::Email {
            to:               Some("test@example.com".to_string()),
            to_template:      None,
            subject:          Some("Subject".to_string()),
            subject_template: None,
            body_template:    Some("Body".to_string()),
            reply_to:         None,
        }
    }

    #[tokio::test]
    async fn test_concurrent_failing_executor_returns_failure_result() {
        let executor = FailingExecutor;
        let concurrent = ConcurrentActionExecutor::new(executor, 5000);
        let results = concurrent.execute_all(&make_event(), &[email_action()]).await;

        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "failing executor should produce success=false");
        assert!(
            results[0].message.contains("injected failure"),
            "error message should propagate; got: {}",
            results[0].message
        );
    }

    #[tokio::test]
    async fn test_concurrent_timeout_produces_timeout_result() {
        // 1 ms timeout, 500 ms delay — always times out.
        let executor = SlowExecutor { delay_ms: 500 };
        let concurrent = ConcurrentActionExecutor::new(executor, 1);
        let results = concurrent.execute_all(&make_event(), &[email_action()]).await;

        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "timed-out action should produce success=false");
        assert!(
            results[0].message.contains("timeout"),
            "message should mention timeout; got: {}",
            results[0].message
        );
    }

    #[tokio::test]
    async fn test_concurrent_multiple_actions_returns_all_results() {
        // 3 actions — one email succeeds (TestExecutor always Ok), two more too.
        let executor = TestExecutor;
        let concurrent = ConcurrentActionExecutor::new(executor, 5000);
        let actions = vec![email_action(), email_action(), email_action()];
        let results = concurrent.execute_all(&make_event(), &actions).await;

        assert_eq!(results.len(), 3, "all 3 actions should produce a result");
        assert!(results.iter().all(|r| r.success), "all should succeed with TestExecutor");
    }

    #[tokio::test]
    async fn test_concurrent_set_timeout() {
        let executor = TestExecutor;
        let mut concurrent = ConcurrentActionExecutor::new(executor, 1000);
        concurrent.set_action_timeout_ms(2500);
        assert_eq!(concurrent.action_timeout_ms(), 2500);
    }
}
