#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions; all panics are from mutex poisoning
//! Mock implementations of traits for testing without external dependencies.

#[cfg(any(test, feature = "testing"))]
pub mod mocks {
    //! Mock implementations of traits for use in tests.
    use std::{
        collections::{HashMap, VecDeque},
        sync::Mutex,
    };

    #[cfg(feature = "checkpoint")]
    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::{
        config::ActionConfig,
        error::{ObserverError, Result},
        event::EntityEvent,
        traits::{
            ActionExecutor, ActionResult, ConditionEvaluator, DeadLetterQueue, DlqItem,
            EventSource, TemplateRenderer,
        },
    };

    // =========================================================================
    // MockActionDispatcher
    // =========================================================================

    /// Mock action dispatcher for testing executor logic without network calls.
    ///
    /// Pre-register per-action-type canned responses via [`MockActionDispatcher::expect_ok`]
    /// and [`MockActionDispatcher::expect_err`], then inject the mock through
    /// `ObserverExecutor::with_dispatcher` (crate-internal).
    pub struct MockActionDispatcher {
        /// Canned responses keyed by `action_type` string (e.g. `"webhook"`)
        pub responses: Mutex<HashMap<String, std::result::Result<ActionResult, ObserverError>>>,
        /// Ordered log of every `action_type` that `dispatch` was called with
        pub call_log:  Mutex<Vec<String>>,
    }

    impl MockActionDispatcher {
        /// Create a new mock dispatcher with no pre-registered responses.
        #[must_use]
        pub fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
                call_log:  Mutex::new(Vec::new()),
            }
        }

        /// Register a successful response for the given action type.
        pub fn expect_ok(&self, action_type: &str, duration_ms: f64) {
            let result = ActionResult {
                action_type: action_type.to_string(),
                success: true,
                message: "mock success".to_string(),
                duration_ms,
            };
            self.responses.lock().unwrap().insert(action_type.to_string(), Ok(result));
        }

        /// Register an error response for the given action type.
        pub fn expect_err(&self, action_type: &str, err: ObserverError) {
            self.responses.lock().unwrap().insert(action_type.to_string(), Err(err));
        }

        /// Return the ordered list of action types that were dispatched.
        pub fn calls(&self) -> Vec<String> {
            self.call_log.lock().unwrap().clone()
        }

        /// Return the number of times `dispatch` was called.
        pub fn call_count(&self) -> usize {
            self.call_log.lock().unwrap().len()
        }
    }

    impl Default for MockActionDispatcher {
        fn default() -> Self {
            Self::new()
        }
    }

    impl crate::executor::ActionDispatcher for MockActionDispatcher {
        fn dispatch<'a>(
            &'a self,
            action: &'a ActionConfig,
            _event: &'a EntityEvent,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionResult>> + Send + 'a>>
        {
            let action_type = action.action_type().to_string();
            Box::pin(async move {
                self.call_log.lock().unwrap().push(action_type.clone());
                let response = self.responses.lock().unwrap().get(&action_type).cloned();
                match response {
                    Some(Ok(r)) => Ok(r),
                    Some(Err(e)) => Err(e),
                    None => Ok(ActionResult {
                        action_type: action_type.clone(),
                        success:     true,
                        message:     "mock default ok".to_string(),
                        duration_ms: 1.0,
                    }),
                }
            })
        }
    }

    /// Mock event source that yields predefined events
    pub struct MockEventSource {
        events: Mutex<VecDeque<EntityEvent>>,
    }

    impl MockEventSource {
        /// Create a new mock event source with predefined events
        #[must_use]
        pub fn new(events: Vec<EntityEvent>) -> Self {
            Self {
                events: Mutex::new(events.into()),
            }
        }

        /// Create an empty mock event source
        #[must_use]
        pub const fn empty() -> Self {
            Self {
                events: Mutex::new(VecDeque::new()),
            }
        }

        /// Add an event to the source
        pub fn add_event(&self, event: EntityEvent) {
            self.events.lock().unwrap().push_back(event);
        }
    }

    impl EventSource for MockEventSource {
        async fn next_event(&mut self) -> Option<EntityEvent> {
            self.events.lock().unwrap().pop_front()
        }
    }

    /// Mock action executor that records executions
    pub struct MockActionExecutor {
        /// Track executed actions
        executions:     Mutex<Vec<(String, bool)>>,
        /// Should fail for all actions
        should_fail:    Mutex<bool>,
        /// Failure reason if `should_fail` is true
        failure_reason: Mutex<Option<String>>,
    }

    impl MockActionExecutor {
        /// Create a new mock action executor
        #[must_use]
        pub const fn new() -> Self {
            Self {
                executions:     Mutex::new(Vec::new()),
                should_fail:    Mutex::new(false),
                failure_reason: Mutex::new(None),
            }
        }

        /// Configure to fail all actions
        pub fn set_should_fail(&self, should_fail: bool, reason: Option<String>) {
            *self.should_fail.lock().unwrap() = should_fail;
            *self.failure_reason.lock().unwrap() = reason;
        }

        /// Get recorded executions
        pub fn executions(&self) -> Vec<(String, bool)> {
            self.executions.lock().unwrap().clone()
        }

        /// Get execution count
        pub fn execution_count(&self) -> usize {
            self.executions.lock().unwrap().len()
        }
    }

    impl Default for MockActionExecutor {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ActionExecutor for MockActionExecutor {
        async fn execute(
            &self,
            _event: &EntityEvent,
            action: &ActionConfig,
        ) -> Result<ActionResult> {
            let action_type = action.action_type().to_string();
            let should_fail = *self.should_fail.lock().unwrap();

            if should_fail {
                let reason = self
                    .failure_reason
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| "Mock failure".to_string());
                self.executions.lock().unwrap().push((action_type, false));
                return Err(crate::error::ObserverError::ActionExecutionFailed { reason });
            }

            self.executions.lock().unwrap().push((action_type.clone(), true));

            Ok(ActionResult {
                action_type,
                success: true,
                message: "Mock execution".to_string(),
                duration_ms: 10.0,
            })
        }
    }

    /// Mock dead letter queue with in-memory storage
    pub struct MockDeadLetterQueue {
        items: Mutex<Vec<DlqItem>>,
    }

    impl MockDeadLetterQueue {
        /// Create a new mock DLQ
        #[must_use]
        pub const fn new() -> Self {
            Self {
                items: Mutex::new(Vec::new()),
            }
        }

        /// Get all items in the DLQ
        pub fn items(&self) -> Vec<DlqItem> {
            self.items.lock().unwrap().clone()
        }

        /// Get item count
        pub fn item_count(&self) -> usize {
            self.items.lock().unwrap().len()
        }
    }

    impl Default for MockDeadLetterQueue {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl DeadLetterQueue for MockDeadLetterQueue {
        async fn push(
            &self,
            event: EntityEvent,
            action: ActionConfig,
            error: String,
        ) -> Result<Uuid> {
            let id = Uuid::new_v4();
            let item = DlqItem {
                id,
                event,
                action,
                error_message: error,
                attempts: 0,
            };
            self.items.lock().unwrap().push(item);
            Ok(id)
        }

        async fn get_pending(&self, limit: i64) -> Result<Vec<DlqItem>> {
            let items = self.items.lock().unwrap();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Reason: limit from API is small and non-negative
            Ok(items.iter().take(limit as usize).cloned().collect())
        }

        async fn mark_success(&self, id: Uuid) -> Result<()> {
            self.items.lock().unwrap().retain(|item| item.id != id);
            Ok(())
        }

        async fn mark_retry_failed(&self, id: Uuid, _error: &str) -> Result<()> {
            if let Some(item) = self.items.lock().unwrap().iter_mut().find(|i| i.id == id) {
                item.attempts += 1;
            }
            Ok(())
        }
    }

    /// Mock condition evaluator with configurable results
    pub struct MockConditionEvaluator {
        /// Map of condition → result
        results: Mutex<std::collections::HashMap<String, bool>>,
    }

    impl MockConditionEvaluator {
        /// Create a new mock condition evaluator
        #[must_use]
        pub fn new() -> Self {
            Self {
                results: Mutex::new(std::collections::HashMap::new()),
            }
        }

        /// Set the result for a condition
        pub fn set_result(&self, condition: String, result: bool) {
            self.results.lock().unwrap().insert(condition, result);
        }
    }

    impl Default for MockConditionEvaluator {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ConditionEvaluator for MockConditionEvaluator {
        fn evaluate(&self, condition: &str, _event: &EntityEvent) -> Result<bool> {
            Ok(*self.results.lock().unwrap().get(condition).unwrap_or(&true))
        }
    }

    /// Mock template renderer with simple substitution
    pub struct MockTemplateRenderer {
        /// Map of template → rendered output
        templates: Mutex<std::collections::HashMap<String, String>>,
    }

    impl MockTemplateRenderer {
        /// Create a new mock template renderer
        #[must_use]
        pub fn new() -> Self {
            Self {
                templates: Mutex::new(std::collections::HashMap::new()),
            }
        }

        /// Set the output for a template
        pub fn set_output(&self, template: String, output: String) {
            self.templates.lock().unwrap().insert(template, output);
        }

        /// Simple placeholder substitution ({{ key }} → value)
        #[must_use]
        pub fn simple_substitute(template: &str, data: &serde_json::Value) -> String {
            let mut result = template.to_string();

            // Replace {{ key }} with data[key]
            if let serde_json::Value::Object(map) = data {
                for (key, value) in map {
                    let placeholder = format!("{{{{ {key} }}}}");
                    let value_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => value.to_string(),
                    };
                    result = result.replace(&placeholder, &value_str);
                }
            }

            result
        }
    }

    impl Default for MockTemplateRenderer {
        fn default() -> Self {
            Self::new()
        }
    }

    impl TemplateRenderer for MockTemplateRenderer {
        fn render(&self, template: &str, data: &serde_json::Value) -> Result<String> {
            // First check if we have a pre-set output
            if let Some(output) = self.templates.lock().unwrap().get(template) {
                return Ok(output.clone());
            }

            // Otherwise do simple substitution
            Ok(Self::simple_substitute(template, data))
        }
    }

    /// Mock checkpoint store for testing
    #[cfg(feature = "checkpoint")]
    #[derive(Clone)]
    pub struct MockCheckpointStore {
        checkpoints: std::sync::Arc<
            Mutex<std::collections::HashMap<String, crate::checkpoint::CheckpointState>>,
        >,
    }

    #[cfg(feature = "checkpoint")]
    impl MockCheckpointStore {
        /// Create a new mock checkpoint store
        #[must_use]
        pub fn new() -> Self {
            Self {
                checkpoints: std::sync::Arc::new(Mutex::new(std::collections::HashMap::new())),
            }
        }
    }

    #[cfg(feature = "checkpoint")]
    impl Default for MockCheckpointStore {
        fn default() -> Self {
            Self::new()
        }
    }

    // Reason: CheckpointStore is defined with #[async_trait]; all implementations must match
    // its transformed method signatures to satisfy the trait contract
    #[cfg(feature = "checkpoint")]
    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl crate::checkpoint::CheckpointStore for MockCheckpointStore {
        async fn load(
            &self,
            listener_id: &str,
        ) -> Result<Option<crate::checkpoint::CheckpointState>> {
            Ok(self.checkpoints.lock().unwrap().get(listener_id).cloned())
        }

        async fn save(
            &self,
            listener_id: &str,
            state: &crate::checkpoint::CheckpointState,
        ) -> Result<()> {
            self.checkpoints.lock().unwrap().insert(listener_id.to_string(), state.clone());
            Ok(())
        }

        async fn compare_and_swap(
            &self,
            listener_id: &str,
            expected_id: i64,
            new_id: i64,
        ) -> Result<bool> {
            let mut checkpoints = self.checkpoints.lock().unwrap();
            match checkpoints.get(listener_id) {
                Some(state) if state.last_processed_id == expected_id => {
                    let mut new_state = state.clone();
                    new_state.last_processed_id = new_id;
                    checkpoints.insert(listener_id.to_string(), new_state);
                    Ok(true)
                },
                None if expected_id == 0 => {
                    let new_state = crate::checkpoint::CheckpointState {
                        listener_id:       listener_id.to_string(),
                        last_processed_id: new_id,
                        last_processed_at: chrono::Utc::now(),
                        batch_size:        0,
                        event_count:       0,
                    };
                    checkpoints.insert(listener_id.to_string(), new_state);
                    Ok(true)
                },
                _ => Ok(false),
            }
        }

        async fn delete(&self, listener_id: &str) -> Result<()> {
            self.checkpoints.lock().unwrap().remove(listener_id);
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use serde_json::json;

        use super::*;
        use crate::event::EventKind;

        #[tokio::test]
        async fn test_mock_event_source() {
            let event1 = EntityEvent::new(
                EventKind::Created,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({"id": 1}),
            );
            let event2 = EntityEvent::new(
                EventKind::Updated,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({"id": 2}),
            );

            let mut source = MockEventSource::new(vec![event1.clone(), event2.clone()]);

            let e1 = source.next_event().await;
            assert!(e1.is_some());
            assert_eq!(e1.unwrap().data["id"], 1);

            let e2 = source.next_event().await;
            assert!(e2.is_some());
            assert_eq!(e2.unwrap().data["id"], 2);

            let e3 = source.next_event().await;
            assert!(e3.is_none());
        }

        #[tokio::test]
        async fn test_mock_action_executor_success() {
            let executor = MockActionExecutor::new();

            let event = EntityEvent::new(
                EventKind::Created,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({}),
            );

            let action = ActionConfig::Email {
                to:               Some("user@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    Some("Body".to_string()),
                reply_to:         None,
            };

            let result = executor.execute(&event, &action).await;
            result.unwrap_or_else(|e| panic!("expected Ok from mock executor: {e}"));
            assert_eq!(executor.execution_count(), 1);
        }

        #[tokio::test]
        async fn test_mock_action_executor_failure() {
            let executor = MockActionExecutor::new();
            executor.set_should_fail(true, Some("Test failure".to_string()));

            let event = EntityEvent::new(
                EventKind::Created,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({}),
            );

            let action = ActionConfig::Email {
                to:               Some("user@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    Some("Body".to_string()),
                reply_to:         None,
            };

            let result = executor.execute(&event, &action).await;
            assert!(
                matches!(result, Err(ObserverError::ActionExecutionFailed { .. })),
                "should_fail executor must return ActionExecutionFailed, got: {result:?}"
            );
        }

        #[tokio::test]
        async fn test_mock_dlq() {
            let dlq = MockDeadLetterQueue::new();

            let event = EntityEvent::new(
                EventKind::Created,
                "Order".to_string(),
                Uuid::new_v4(),
                json!({}),
            );

            let action = ActionConfig::Email {
                to:               Some("user@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    Some("Body".to_string()),
                reply_to:         None,
            };

            let id = dlq.push(event, action, "Error".to_string()).await.unwrap();

            assert_eq!(dlq.item_count(), 1);

            let items = dlq.get_pending(10).await.unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].id, id);

            dlq.mark_success(id).await.unwrap();
            assert_eq!(dlq.item_count(), 0);
        }

        #[test]
        fn test_mock_template_renderer_substitution() {
            let data = json!({"name": "John", "total": 100});
            let template = "Hello {{ name }}, your total is {{ total }}";

            let result = MockTemplateRenderer::simple_substitute(template, &data);
            assert_eq!(result, "Hello John, your total is 100");
        }
    }
}
