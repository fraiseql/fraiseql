//! Authentication monitoring and observability.
//!
//! Provides [`AuthEvent`] for structured event logging, [`AuthMetrics`] for
//! in-process counters, and [`OperationTimer`] for latency measurement.
use std::time::Instant;

use serde::Serialize;
use tracing::{Level, info, span, warn};

/// A structured log record for a single authentication event.
///
/// Constructed with [`AuthEvent::new`] and populated via builder methods.
/// Call [`AuthEvent::log`] to emit the record through `tracing`.
///
/// # Example
///
/// ```rust
/// use fraiseql_auth::AuthEvent;
/// let event = AuthEvent::new("login")
///     .with_user_id("user123".to_string())
///     .with_provider("google".to_string())
///     .success(42.5);
/// event.log();
/// ```
#[derive(Debug, Serialize)]
pub struct AuthEvent {
    /// Name of the authentication event (e.g., `"login"`, `"token_refresh"`).
    pub event:       String,
    /// Optional authenticated user ID associated with this event.
    pub user_id:     Option<String>,
    /// OAuth provider name (e.g., `"google"`, `"okta"`).
    pub provider:    Option<String>,
    /// Outcome: `"started"`, `"success"`, or `"error"`.
    pub status:      String,
    /// Duration of the operation in milliseconds.
    pub duration_ms: f64,
    /// Error message if the operation failed.
    pub error:       Option<String>,
    /// RFC 3339 timestamp of when this event was created.
    pub timestamp:   String,
    /// Optional correlation ID for tracing a request across services.
    pub request_id:  Option<String>,
}

impl AuthEvent {
    /// Create a new event record in the `"started"` state.
    pub fn new(event: &str) -> Self {
        Self {
            event:       event.to_string(),
            user_id:     None,
            provider:    None,
            status:      "started".to_string(),
            duration_ms: 0.0,
            error:       None,
            timestamp:   chrono::Utc::now().to_rfc3339(),
            request_id:  None,
        }
    }

    /// Set the user ID associated with this event.
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set the OAuth provider name for this event.
    pub fn with_provider(mut self, provider: String) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Set the request correlation ID for distributed tracing.
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Mark the event as successful and record its duration.
    pub fn success(mut self, duration_ms: f64) -> Self {
        self.status = "success".to_string();
        self.duration_ms = duration_ms;
        self
    }

    /// Mark the event as failed, recording the error and duration.
    pub fn error(mut self, error: String, duration_ms: f64) -> Self {
        self.status = "error".to_string();
        self.error = Some(error);
        self.duration_ms = duration_ms;
        self
    }

    /// Emit this event through `tracing` at the appropriate level.
    ///
    /// Successful events are logged at `INFO`; errors at `WARN`.
    /// Events in the `"started"` state are silently dropped.
    pub fn log(&self) {
        match self.status.as_str() {
            "success" => {
                info!(
                    event = %self.event,
                    user_id = ?self.user_id,
                    provider = ?self.provider,
                    duration_ms = self.duration_ms,
                    "Authentication event",
                );
            },
            "error" => {
                warn!(
                    event = %self.event,
                    error = ?self.error,
                    duration_ms = self.duration_ms,
                    "Authentication error",
                );
            },
            _ => {},
        }
    }
}

/// In-process counters for authentication operations.
///
/// These counters are for lightweight observability within a single process.
/// For production monitoring, export these values to a metrics system such as
/// Prometheus.  All fields are plain `u64`; thread-safe mutation requires an
/// outer `Mutex` or `RwLock`.
#[derive(Debug, Clone)]
pub struct AuthMetrics {
    /// Total number of authentication attempts (successful + failed).
    pub total_auth_attempts:        u64,
    /// Number of authentication attempts that succeeded.
    pub successful_authentications: u64,
    /// Number of authentication attempts that failed.
    pub failed_authentications:     u64,
    /// Number of access tokens issued since startup.
    pub tokens_issued:              u64,
    /// Number of access tokens refreshed since startup.
    pub tokens_refreshed:           u64,
    /// Number of sessions explicitly revoked since startup.
    pub sessions_revoked:           u64,
}

impl AuthMetrics {
    /// Create a new `AuthMetrics` with all counters initialized to zero.
    pub const fn new() -> Self {
        Self {
            total_auth_attempts:        0,
            successful_authentications: 0,
            failed_authentications:     0,
            tokens_issued:              0,
            tokens_refreshed:           0,
            sessions_revoked:           0,
        }
    }

    /// Increment the total authentication attempts counter.
    pub const fn record_attempt(&mut self) {
        self.total_auth_attempts += 1;
    }

    /// Increment the successful authentications counter.
    pub const fn record_success(&mut self) {
        self.successful_authentications += 1;
    }

    /// Increment the failed authentications counter.
    pub const fn record_failure(&mut self) {
        self.failed_authentications += 1;
    }

    /// Increment the tokens issued counter.
    pub const fn record_token_issued(&mut self) {
        self.tokens_issued += 1;
    }

    /// Increment the tokens refreshed counter.
    pub const fn record_token_refreshed(&mut self) {
        self.tokens_refreshed += 1;
    }

    /// Increment the sessions revoked counter.
    pub const fn record_session_revoked(&mut self) {
        self.sessions_revoked += 1;
    }

    /// Return the success rate as a percentage (0–100).
    ///
    /// Returns `0.0` when no attempts have been recorded yet.
    pub fn success_rate(&self) -> f64 {
        if self.total_auth_attempts == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)] // Reason: acceptable precision for metrics/timing
            let result = (self.successful_authentications as f64)
                / (self.total_auth_attempts as f64)
                * 100.0;
            result
        }
    }
}

impl Default for AuthMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// A wall-clock timer for measuring the duration of authentication operations.
///
/// The timer starts immediately on construction via [`OperationTimer::start`].
/// Call [`OperationTimer::finish`] to log the elapsed time and discard the timer,
/// or read [`OperationTimer::elapsed_ms`] to sample without consuming.
pub struct OperationTimer {
    start:     Instant,
    operation: String,
}

impl OperationTimer {
    /// Start timing `operation` and open a tracing span at `DEBUG` level.
    pub fn start(operation: &str) -> Self {
        let span = span!(Level::DEBUG, "operation", %operation);
        let _guard = span.enter();

        Self {
            start:     Instant::now(),
            operation: operation.to_string(),
        }
    }

    /// Return the elapsed time in milliseconds since this timer was started.
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    /// Log the completed operation at `INFO` level with its elapsed duration.
    pub fn finish(self) {
        let elapsed = self.elapsed_ms();
        info!(
            operation = %self.operation,
            duration_ms = elapsed,
            "Operation completed",
        );
    }
}

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test module — wildcard keeps test boilerplate minimal
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)] // Reason: acceptable precision for metrics/timing — values set directly from literals
    fn test_auth_event_builder() {
        let event = AuthEvent::new("login")
            .with_user_id("user123".to_string())
            .with_provider("google".to_string())
            .success(50.0);

        assert_eq!(event.event, "login");
        assert_eq!(event.user_id, Some("user123".to_string()));
        assert_eq!(event.provider, Some("google".to_string()));
        assert_eq!(event.status, "success");
        assert_eq!(event.duration_ms, 50.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: acceptable precision for metrics/timing — values set directly from literals
    fn test_auth_metrics() {
        let mut metrics = AuthMetrics::new();

        metrics.record_attempt();
        metrics.record_attempt();
        metrics.record_success();
        metrics.record_failure();

        assert_eq!(metrics.total_auth_attempts, 2);
        assert_eq!(metrics.successful_authentications, 1);
        assert_eq!(metrics.failed_authentications, 1);
        assert_eq!(metrics.success_rate(), 50.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: acceptable precision for metrics/timing — values set directly from literals
    fn test_auth_metrics_success_rate() {
        let mut metrics = AuthMetrics::new();

        // 100% success rate
        for _ in 0..10 {
            metrics.record_attempt();
            metrics.record_success();
        }

        assert_eq!(metrics.success_rate(), 100.0);

        // Drop to 50%
        metrics.record_attempt();
        metrics.record_failure();

        assert!((metrics.success_rate() - 90.91).abs() < 0.1); // ~90.91%
    }

    #[test]
    fn test_operation_timer() {
        let timer = OperationTimer::start("test_op");
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 0.0);
        assert!(elapsed < 1000.0);
    }
}
