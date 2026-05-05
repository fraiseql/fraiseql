//! Action-level tracing instrumentation
//!
//! Provides decorator wrappers for action execution with tracing support.
//! Wraps webhook, email, and Slack actions to record execution metrics and errors.

use tracing::{debug, warn, Instrument};

/// Webhook action tracer
///
/// Wraps webhook execution with tracing instrumentation
pub struct WebhookTracer {
    pub url: String,
}

impl WebhookTracer {
    /// Create a new webhook tracer
    pub fn new(url: String) -> Self {
        Self { url }
    }

    /// Record webhook execution start
    pub fn record_start(&self) {
        debug!(
            url = %self.url,
            "Webhook action starting"
        );
    }

    /// Record webhook success
    pub fn record_success(&self, status_code: u16, duration_ms: f64) {
        debug!(
            url = %self.url,
            status_code = status_code,
            duration_ms = duration_ms,
            "Webhook action succeeded"
        );
    }

    /// Record webhook failure
    pub fn record_failure(&self, error: &str, duration_ms: f64) {
        warn!(
            url = %self.url,
            error = %error,
            duration_ms = duration_ms,
            "Webhook action failed"
        );
    }

    /// Record webhook retry
    pub fn record_retry(&self, attempt: u32, reason: &str) {
        debug!(
            url = %self.url,
            attempt = attempt,
            reason = %reason,
            "Retrying webhook action"
        );
    }

    /// Record request header injection
    pub fn record_trace_context_injection(&self, header_count: usize) {
        debug!(
            url = %self.url,
            header_count = header_count,
            "Injected trace context headers"
        );
    }
}

/// Email action tracer
///
/// Wraps email execution with tracing instrumentation
pub struct EmailTracer {
    pub recipient: String,
}

impl EmailTracer {
    /// Create a new email tracer
    pub fn new(recipient: String) -> Self {
        Self { recipient }
    }

    /// Record email execution start
    pub fn record_start(&self, subject: &str) {
        debug!(
            recipient = %self.recipient,
            subject = %subject,
            "Email action starting"
        );
    }

    /// Record email success
    pub fn record_success(&self, message_id: Option<&str>, duration_ms: f64) {
        debug!(
            recipient = %self.recipient,
            message_id = ?message_id,
            duration_ms = duration_ms,
            "Email action succeeded"
        );
    }

    /// Record email failure
    pub fn record_failure(&self, error: &str, duration_ms: f64) {
        warn!(
            recipient = %self.recipient,
            error = %error,
            duration_ms = duration_ms,
            "Email action failed"
        );
    }

    /// Record email retry
    pub fn record_retry(&self, attempt: u32, reason: &str) {
        debug!(
            recipient = %self.recipient,
            attempt = attempt,
            reason = %reason,
            "Retrying email action"
        );
    }

    /// Record batch email send
    pub fn record_batch_send(&self, recipient_count: usize) {
        debug!(
            recipient_count = recipient_count,
            "Sending batch email"
        );
    }
}

/// Slack action tracer
///
/// Wraps Slack execution with tracing instrumentation
pub struct SlackTracer {
    pub channel: String,
}

impl SlackTracer {
    /// Create a new Slack tracer
    pub fn new(channel: String) -> Self {
        Self { channel }
    }

    /// Record Slack execution start
    pub fn record_start(&self) {
        debug!(
            channel = %self.channel,
            "Slack action starting"
        );
    }

    /// Record Slack success
    pub fn record_success(&self, status_code: u16, duration_ms: f64) {
        debug!(
            channel = %self.channel,
            status_code = status_code,
            duration_ms = duration_ms,
            "Slack action succeeded"
        );
    }

    /// Record Slack failure
    pub fn record_failure(&self, error: &str, duration_ms: f64) {
        warn!(
            channel = %self.channel,
            error = %error,
            duration_ms = duration_ms,
            "Slack action failed"
        );
    }

    /// Record Slack retry
    pub fn record_retry(&self, attempt: u32, reason: &str) {
        debug!(
            channel = %self.channel,
            attempt = attempt,
            reason = %reason,
            "Retrying Slack action"
        );
    }

    /// Record thread creation
    pub fn record_thread_created(&self, thread_id: &str) {
        debug!(
            channel = %self.channel,
            thread_id = %thread_id,
            "Created Slack thread"
        );
    }

    /// Record message reaction
    pub fn record_reaction(&self, emoji: &str) {
        debug!(
            channel = %self.channel,
            emoji = %emoji,
            "Added reaction to Slack message"
        );
    }
}

/// Generic action span context
///
/// Tracks a span for any action execution
pub struct ActionSpan {
    pub action_type: String,
    pub action_name: String,
}

impl ActionSpan {
    /// Create a new action span
    pub fn new(action_type: String, action_name: String) -> Self {
        Self {
            action_type,
            action_name,
        }
    }

    /// Record action execution start with span
    pub fn record_start_span(&self) {
        debug!(
            action_type = %self.action_type,
            action_name = %self.action_name,
            "Action span starting"
        );
    }

    /// Record action result with span
    pub fn record_result_span(&self, success: bool, duration_ms: f64) {
        if success {
            debug!(
                action_type = %self.action_type,
                action_name = %self.action_name,
                duration_ms = duration_ms,
                "Action span completed successfully"
            );
        } else {
            warn!(
                action_type = %self.action_type,
                action_name = %self.action_name,
                duration_ms = duration_ms,
                "Action span failed"
            );
        }
    }

    /// Record action span error
    pub fn record_span_error(&self, error: &str) {
        warn!(
            action_type = %self.action_type,
            action_name = %self.action_name,
            error = %error,
            "Action span error"
        );
    }
}
