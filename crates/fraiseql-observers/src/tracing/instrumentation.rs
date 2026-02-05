//! Core instrumentation for distributed tracing
//!
//! This module provides decorators and wrappers for core components
//! to add tracing without modifying existing code.

use tracing::{info, warn, debug, Level};

/// Trace a listener operation
///
/// Records timing and status of listener processing
pub struct ListenerTracer {
    pub listener_id: String,
}

impl ListenerTracer {
    /// Create a new listener tracer
    pub fn new(listener_id: String) -> Self {
        Self { listener_id }
    }

    /// Record listener startup
    pub fn record_startup(&self) {
        debug!(
            listener_id = %self.listener_id,
            "Listener starting"
        );
    }

    /// Record listener health check
    pub fn record_health_check(&self, healthy: bool) {
        let level = if healthy { Level::DEBUG } else { Level::WARN };
        let message = if healthy { "Listener healthy" } else { "Listener unhealthy" };

        if healthy {
            debug!(listener_id = %self.listener_id, message);
        } else {
            warn!(listener_id = %self.listener_id, message);
        }
    }

    /// Record event batch processing
    pub fn record_batch_start(&self, batch_size: usize, checkpoint_offset: u64) {
        debug!(
            listener_id = %self.listener_id,
            batch_size = batch_size,
            checkpoint_offset = checkpoint_offset,
            "Processing event batch"
        );
    }

    /// Record batch completion
    pub fn record_batch_complete(&self, events_processed: usize, errors: usize) {
        let level = if errors > 0 { Level::WARN } else { Level::DEBUG };
        if errors > 0 {
            warn!(
                listener_id = %self.listener_id,
                events_processed = events_processed,
                errors = errors,
                "Batch complete with errors"
            );
        } else {
            debug!(
                listener_id = %self.listener_id,
                events_processed = events_processed,
                "Batch complete"
            );
        }
    }
}

/// Trace an executor operation
pub struct ExecutorTracer {
    pub executor_id: String,
}

impl ExecutorTracer {
    /// Create a new executor tracer
    pub fn new(executor_id: String) -> Self {
        Self { executor_id }
    }

    /// Record action execution start
    pub fn record_action_start(&self, action_type: &str, action_name: &str) {
        debug!(
            executor_id = %self.executor_id,
            action_type = %action_type,
            action_name = %action_name,
            "Executing action"
        );
    }

    /// Record action execution success
    pub fn record_action_success(&self, action_type: &str, duration_ms: u128) {
        debug!(
            executor_id = %self.executor_id,
            action_type = %action_type,
            duration_ms = duration_ms,
            "Action succeeded"
        );
    }

    /// Record action execution failure
    pub fn record_action_failure(&self, action_type: &str, error: &str, duration_ms: u128) {
        warn!(
            executor_id = %self.executor_id,
            action_type = %action_type,
            error = %error,
            duration_ms = duration_ms,
            "Action failed"
        );
    }

    /// Record action retry
    pub fn record_action_retry(&self, action_type: &str, retry_count: u32, reason: &str) {
        info!(
            executor_id = %self.executor_id,
            action_type = %action_type,
            retry_count = retry_count,
            reason = %reason,
            "Retrying action"
        );
    }
}

/// Trace condition evaluation
pub struct ConditionTracer {
    pub observer_name: String,
}

impl ConditionTracer {
    /// Create a new condition tracer
    pub fn new(observer_name: String) -> Self {
        Self { observer_name }
    }

    /// Record condition evaluation start
    pub fn record_evaluation_start(&self) {
        debug!(
            observer_name = %self.observer_name,
            "Evaluating condition"
        );
    }

    /// Record condition evaluation result
    pub fn record_evaluation_result(&self, matched: bool, duration_ms: u128) {
        debug!(
            observer_name = %self.observer_name,
            matched = matched,
            duration_ms = duration_ms,
            "Condition evaluation complete"
        );
    }

    /// Record condition evaluation error
    pub fn record_evaluation_error(&self, error: &str) {
        warn!(
            observer_name = %self.observer_name,
            error = %error,
            "Condition evaluation failed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listener_tracer_creation() {
        let tracer = ListenerTracer::new("listener-1".to_string());
        assert_eq!(tracer.listener_id, "listener-1");
    }

    #[test]
    fn test_executor_tracer_creation() {
        let tracer = ExecutorTracer::new("executor-1".to_string());
        assert_eq!(tracer.executor_id, "executor-1");
    }

    #[test]
    fn test_condition_tracer_creation() {
        let tracer = ConditionTracer::new("observer-1".to_string());
        assert_eq!(tracer.observer_name, "observer-1");
    }

    #[test]
    fn test_listener_tracer_methods() {
        let tracer = ListenerTracer::new("listener-1".to_string());
        tracer.record_startup();
        tracer.record_health_check(true);
        tracer.record_batch_start(10, 100);
        tracer.record_batch_complete(10, 0);
    }

    #[test]
    fn test_executor_tracer_methods() {
        let tracer = ExecutorTracer::new("executor-1".to_string());
        tracer.record_action_start("webhook", "notify");
        tracer.record_action_success("webhook", 50);
        tracer.record_action_failure("webhook", "timeout", 5000);
        tracer.record_action_retry("webhook", 1, "temporary failure");
    }

    #[test]
    fn test_condition_tracer_methods() {
        let tracer = ConditionTracer::new("observer-1".to_string());
        tracer.record_evaluation_start();
        tracer.record_evaluation_result(true, 10);
        tracer.record_evaluation_error("invalid condition");
    }
}
