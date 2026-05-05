#[cfg(feature = "metrics")]
mod handler_tests {
    use super::super::handler::*;

    #[tokio::test]
    async fn test_metrics_handler_returns_text() {
        let (headers, body) = metrics_handler().await;
        assert_eq!(headers[0].0, "content-type", "Should return content-type header");
        assert!(body.contains("fraiseql_observer"), "Should contain observer metrics");
    }
}

#[cfg(feature = "metrics")]
mod registry_tests {
    use super::super::registry::*;

    #[test]
    fn test_global_metrics_registry_initialization() {
        // Get the global metrics registry (initializes on first call)
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Verify it was initialized properly (metrics persist across tests due to global registry)
        let cache_hits = metrics.cache_hits_total.get();
        let cache_misses = metrics.cache_misses_total.get();

        // Just verify we can retrieve values without panicking (they may be non-zero from other
        // tests)
        let _ = cache_hits;
        let _ = cache_misses;
    }

    #[test]
    fn test_event_metrics_recording() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record some events
        metrics.event_processed();
        metrics.event_processed();

        // Verify they were recorded (will be non-zero if this test ran)
        assert!(metrics.events_processed_total.get() >= 2);
    }

    #[test]
    fn test_cache_hit_rate_calculation() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Call cache_hit and cache_miss to test the calculation logic
        let initial_hits = metrics.cache_hits_total.get();
        let initial_misses = metrics.cache_misses_total.get();

        metrics.cache_hit();
        metrics.cache_hit();
        metrics.cache_miss();

        // Verify the new values
        assert_eq!(metrics.cache_hits_total.get(), initial_hits + 2);
        assert_eq!(metrics.cache_misses_total.get(), initial_misses + 1);

        // Cache hit rate should be 2/3 = 66.67%
        let hit_rate = metrics.cache_hit_rate();
        assert!(hit_rate > 60.0 && hit_rate < 100.0);
    }

    #[test]
    fn test_action_execution_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record some action executions
        metrics.action_executed("webhook", 0.5);
        metrics.action_executed("slack", 0.1);

        // Verify they were recorded
        let webhook_count = metrics.action_executed_total.with_label_values(&["webhook"]).get();
        let slack_count = metrics.action_executed_total.with_label_values(&["slack"]).get();

        assert!(webhook_count >= 1);
        assert!(slack_count >= 1);
    }

    #[test]
    fn test_backlog_gauge_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Set backlog size
        metrics.set_backlog_size(42);
        assert_eq!(metrics.backlog_size.get(), 42);

        // Update it
        metrics.set_backlog_size(100);
        assert_eq!(metrics.backlog_size.get(), 100);
    }

    #[test]
    fn test_job_queue_metrics_recording() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record job queueing
        metrics.job_queued();
        metrics.job_queued();

        // Verify jobs were queued (will be non-zero if this test ran)
        assert!(metrics.job_queued_total.get() >= 2);
    }

    #[test]
    fn test_job_execution_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record job executions
        metrics.job_executed("webhook", 0.5);
        metrics.job_executed("email", 1.2);

        // Verify they were recorded
        let webhook_count = metrics.job_executed_total.with_label_values(&["webhook"]).get();
        let email_count = metrics.job_executed_total.with_label_values(&["email"]).get();

        assert!(webhook_count >= 1);
        assert!(email_count >= 1);
    }

    #[test]
    fn test_job_failure_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record job failures
        metrics.job_failed("webhook", "timeout");
        metrics.job_failed("webhook", "connection_error");
        metrics.job_failed("email", "authentication_failed");

        // Verify they were recorded
        let webhook_timeout =
            metrics.job_failed_total.with_label_values(&["webhook", "timeout"]).get();
        let webhook_connection = metrics
            .job_failed_total
            .with_label_values(&["webhook", "connection_error"])
            .get();
        let email_auth = metrics
            .job_failed_total
            .with_label_values(&["email", "authentication_failed"])
            .get();

        assert!(webhook_timeout >= 1);
        assert!(webhook_connection >= 1);
        assert!(email_auth >= 1);
    }

    #[test]
    fn test_job_retry_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Record retry attempts
        metrics.job_retry_attempt("webhook");
        metrics.job_retry_attempt("webhook");
        metrics.job_retry_attempt("slack");

        // Verify they were recorded
        let webhook_retries = metrics.job_retry_attempts.with_label_values(&["webhook"]).get();
        let slack_retries = metrics.job_retry_attempts.with_label_values(&["slack"]).get();

        assert!(webhook_retries >= 2);
        assert!(slack_retries >= 1);
    }

    #[test]
    fn test_job_queue_depth_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Set job queue depth
        metrics.set_job_queue_depth(42);
        assert_eq!(metrics.job_queue_depth.get(), 42);

        // Update it
        metrics.set_job_queue_depth(100);
        assert_eq!(metrics.job_queue_depth.get(), 100);
    }

    #[test]
    fn test_job_dlq_items_tracking() {
        let metrics = MetricsRegistry::global().expect("Failed to get global metrics");

        // Set job DLQ items
        metrics.set_job_dlq_items(5);
        assert_eq!(metrics.job_dlq_items.get(), 5);

        // Update it
        metrics.set_job_dlq_items(15);
        assert_eq!(metrics.job_dlq_items.get(), 15);
    }
}
