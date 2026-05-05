//! Action execution with tracing integration examples
//!
//! This module demonstrates how to integrate action tracing with actual action execution.

use super::action_tracing::{WebhookTracer, EmailTracer, SlackTracer, ActionSpan};
use super::TraceContext;
use std::collections::HashMap;

/// Example: Traced webhook execution with context propagation
///
/// Shows how to inject trace context headers into webhook requests
pub fn webhook_execution_example(
    trace_context: &TraceContext,
    webhook_url: &str,
) -> HashMap<String, String> {
    let tracer = WebhookTracer::new(webhook_url.to_string());

    // Record execution start
    tracer.record_start();

    // Generate trace context headers for HTTP request
    let headers = trace_context.to_headers();

    // Record header injection
    tracer.record_trace_context_injection(headers.len());

    // Return headers to be included in HTTP request
    headers
}

/// Example: Traced email execution with batch handling
///
/// Shows how to track email execution including batch sends
pub fn email_execution_example(recipients: &[&str]) -> Vec<EmailTracer> {
    let tracers: Vec<EmailTracer> = recipients
        .iter()
        .map(|recipient| EmailTracer::new(recipient.to_string()))
        .collect();

    // Record batch operation
    if !tracers.is_empty() {
        tracers[0].record_batch_send(tracers.len());
    }

    // Record individual email starts
    for tracer in &tracers {
        tracer.record_start("order_confirmation");
    }

    tracers
}

/// Example: Traced Slack execution with thread handling
///
/// Shows how to track Slack operations including thread creation
pub fn slack_execution_example(channel: &str) -> SlackTracer {
    let tracer = SlackTracer::new(channel.to_string());

    tracer.record_start();

    // Later, after thread is created:
    tracer.record_thread_created("ts-1234567890.123456");

    // Track reactions
    tracer.record_reaction("👍");

    tracer
}

/// Example: Generic action span for coordinated action tracking
///
/// Shows how to use ActionSpan for tracking multiple related actions
pub struct ActionBatchExecutor {
    pub(crate) actions: Vec<ActionSpan>,
}

impl ActionBatchExecutor {
    /// Create a new batch executor for multiple actions
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Add an action to the batch
    pub fn add_action(&mut self, action_type: &str, action_name: &str) {
        self.actions
            .push(ActionSpan::new(action_type.to_string(), action_name.to_string()));
    }

    /// Execute all actions with tracing
    pub fn execute_batch(&self, results: &[(bool, f64)]) {
        for (action, (success, duration_ms)) in self.actions.iter().zip(results.iter()) {
            action.record_start_span();
            action.record_result_span(*success, *duration_ms);
        }
    }

    /// Track action errors in batch
    pub fn record_batch_errors(&self, errors: &[(&str, &str)]) {
        for (action_name, error) in errors {
            if let Some(action) = self.actions.iter().find(|a| a.action_name == *action_name) {
                action.record_span_error(error);
            }
        }
    }
}

impl Default for ActionBatchExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Example: Trace context propagation through action chain
///
/// Shows how to propagate trace context across multiple action executions
pub struct ActionChain {
    trace_context: TraceContext,
    actions: Vec<String>,
}

impl ActionChain {
    /// Create a new action chain with trace context
    pub fn new(trace_context: TraceContext) -> Self {
        Self {
            trace_context,
            actions: Vec::new(),
        }
    }

    /// Add action to chain
    pub fn add_action(&mut self, action_name: &str) -> TraceContext {
        self.actions.push(action_name.to_string());

        // Generate child span for this action
        let child_span_id = self.trace_context.child_span_id();
        TraceContext::new(
            self.trace_context.trace_id.clone(),
            child_span_id,
            self.trace_context.trace_flags,
        )
    }

    /// Execute action chain with trace context
    pub fn execute_action_chain(&self) -> Vec<HashMap<String, String>> {
        self.actions
            .iter()
            .map(|_action| self.trace_context.to_headers())
            .collect()
    }
}
