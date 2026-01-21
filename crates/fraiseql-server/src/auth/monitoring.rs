// Authentication monitoring and observability
use std::time::Instant;
use tracing::{info, warn, error, span, Level};
use serde::Serialize;

/// Structured log for authentication events
#[derive(Debug, Serialize)]
pub struct AuthEvent {
    pub event: String,
    pub user_id: Option<String>,
    pub provider: Option<String>,
    pub status: String,
    pub duration_ms: f64,
    pub error: Option<String>,
    pub timestamp: String,
    pub request_id: Option<String>,
}

impl AuthEvent {
    pub fn new(event: &str) -> Self {
        Self {
            event: event.to_string(),
            user_id: None,
            provider: None,
            status: "started".to_string(),
            duration_ms: 0.0,
            error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id: None,
        }
    }

    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_provider(mut self, provider: String) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn success(mut self, duration_ms: f64) -> Self {
        self.status = "success".to_string();
        self.duration_ms = duration_ms;
        self
    }

    pub fn error(mut self, error: String, duration_ms: f64) -> Self {
        self.status = "error".to_string();
        self.error = Some(error);
        self.duration_ms = duration_ms;
        self
    }

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
            }
            "error" => {
                warn!(
                    event = %self.event,
                    error = ?self.error,
                    duration_ms = self.duration_ms,
                    "Authentication error",
                );
            }
            _ => {}
        }
    }
}

/// Metrics for authentication operations
#[derive(Debug, Clone)]
pub struct AuthMetrics {
    pub total_auth_attempts: u64,
    pub successful_authentications: u64,
    pub failed_authentications: u64,
    pub tokens_issued: u64,
    pub tokens_refreshed: u64,
    pub sessions_revoked: u64,
}

impl AuthMetrics {
    pub fn new() -> Self {
        Self {
            total_auth_attempts: 0,
            successful_authentications: 0,
            failed_authentications: 0,
            tokens_issued: 0,
            tokens_refreshed: 0,
            sessions_revoked: 0,
        }
    }

    pub fn record_attempt(&mut self) {
        self.total_auth_attempts += 1;
    }

    pub fn record_success(&mut self) {
        self.successful_authentications += 1;
    }

    pub fn record_failure(&mut self) {
        self.failed_authentications += 1;
    }

    pub fn record_token_issued(&mut self) {
        self.tokens_issued += 1;
    }

    pub fn record_token_refreshed(&mut self) {
        self.tokens_refreshed += 1;
    }

    pub fn record_session_revoked(&mut self) {
        self.sessions_revoked += 1;
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_auth_attempts == 0 {
            0.0
        } else {
            (self.successful_authentications as f64) / (self.total_auth_attempts as f64) * 100.0
        }
    }
}

impl Default for AuthMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer for measuring operation duration
pub struct OperationTimer {
    start: Instant,
    operation: String,
}

impl OperationTimer {
    pub fn start(operation: &str) -> Self {
        let span = span!(Level::DEBUG, "operation", %operation);
        let _guard = span.enter();

        Self {
            start: Instant::now(),
            operation: operation.to_string(),
        }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

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
    use super::*;

    #[test]
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
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();

        assert!(elapsed >= 10.0);
        assert!(elapsed < 100.0); // Should be quick
    }
}
