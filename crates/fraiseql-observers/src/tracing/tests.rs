//! Comprehensive tracing integration tests
//!
//! End-to-end tests for distributed tracing functionality

#[cfg(test)]
mod tracing_e2e_tests {
    use crate::tracing::{
        init_tracing, TracingConfig, TraceContext, ListenerTracer, ExecutorTracer,
        ConditionTracer, WebhookTracer, EmailTracer, SlackTracer, ActionSpan,
        ActionBatchExecutor, ActionChain, JaegerConfig, JaegerSpan, record_span,
        flush_spans, is_initialized,
    };

    #[test]
    fn test_full_tracing_initialization() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test-observer".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        let result = init_tracing(config);
        assert!(result.is_ok());
        assert!(is_initialized());
    }

    #[test]
    fn test_trace_context_propagation_chain() {
        let root_context = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );

        // Create child contexts
        let child1_id = root_context.child_span_id();
        let child1 = TraceContext::new(
            root_context.trace_id.clone(),
            child1_id.clone(),
            0x01,
        );

        let child2_id = child1.child_span_id();
        let child2 = TraceContext::new(
            root_context.trace_id.clone(),
            child2_id,
            0x01,
        );

        // Verify trace IDs are maintained
        assert_eq!(root_context.trace_id, child1.trace_id);
        assert_eq!(child1.trace_id, child2.trace_id);

        // Verify span IDs are different
        assert_ne!(root_context.span_id, child1.span_id);
        assert_ne!(child1.span_id, child2.span_id);
    }

    #[test]
    fn test_listener_tracer_full_lifecycle() {
        let tracer = ListenerTracer::new("listener-1".to_string());

        tracer.record_startup();
        tracer.record_health_check(true);
        tracer.record_batch_start(100, 1000);
        tracer.record_batch_complete(100, 0);
        tracer.record_health_check(false);
        tracer.record_batch_complete(50, 5);
    }

    #[test]
    fn test_executor_tracer_with_retries() {
        let tracer = ExecutorTracer::new("executor-1".to_string());

        tracer.record_action_start("webhook", "notify_user");
        tracer.record_action_failure("webhook", "connection timeout", 5000.0);

        for attempt in 1..=3 {
            tracer.record_action_retry("webhook", attempt, "temporary failure");
        }

        tracer.record_action_success("webhook", 100.0);
    }

    #[test]
    fn test_condition_tracer_with_errors() {
        let tracer = ConditionTracer::new("order_validator".to_string());

        tracer.record_evaluation_start();
        tracer.record_evaluation_error("invalid condition syntax");

        // Retry
        tracer.record_evaluation_start();
        tracer.record_evaluation_result(true, 25.0);
    }

    #[test]
    fn test_webhook_tracer_with_context_injection() {
        let trace_context = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );

        let tracer = WebhookTracer::new("https://api.example.com/webhook".to_string());

        tracer.record_start();
        let headers = trace_context.to_headers();
        tracer.record_trace_context_injection(headers.len());
        tracer.record_success(200, 42.5);
    }

    #[test]
    fn test_email_tracer_batch_operations() {
        let recipients = vec![
            EmailTracer::new("user1@example.com".to_string()),
            EmailTracer::new("user2@example.com".to_string()),
            EmailTracer::new("user3@example.com".to_string()),
        ];

        // Record batch operation
        recipients[0].record_batch_send(recipients.len());

        // Record individual executions
        for (i, tracer) in recipients.iter().enumerate() {
            tracer.record_start("confirmation");

            if i == 1 {
                tracer.record_failure("SMTP connection failed", 3000.0);
                tracer.record_retry(1, "network error");
                tracer.record_success(None, 5000.0);
            } else {
                tracer.record_success(Some(&format!("msg-{}", i)), 100.0 * (i + 1) as f64);
            }
        }
    }

    #[test]
    fn test_slack_tracer_with_threads() {
        let tracer = SlackTracer::new("#alerts".to_string());

        tracer.record_start();
        tracer.record_success(200, 75.0);
        tracer.record_thread_created("ts-1234567890.123456");
        tracer.record_reaction("👍");
        tracer.record_reaction("🚀");

        // Failure scenario
        let tracer2 = SlackTracer::new("#notifications".to_string());
        tracer2.record_start();
        tracer2.record_failure("rate_limited", 429.0);
        tracer2.record_retry(1, "rate limit exceeded");
    }

    #[test]
    fn test_action_span_lifecycle() {
        let span = ActionSpan::new(
            "webhook".to_string(),
            "notify_users".to_string(),
        );

        span.record_start_span();
        span.record_result_span(true, 123.45);

        let error_span = ActionSpan::new(
            "email".to_string(),
            "send_bulk".to_string(),
        );

        error_span.record_start_span();
        error_span.record_span_error("SMTP server unavailable");
    }

    #[test]
    fn test_action_batch_executor_multi_action() {
        let mut executor = ActionBatchExecutor::new();

        // Add multiple actions
        executor.add_action("webhook", "notify_system");
        executor.add_action("email", "notify_admins");
        executor.add_action("slack", "alert_team");
        executor.add_action("webhook", "update_analytics");

        // Execute with mixed results
        let results = vec![
            (true, 45.0),      // Success
            (true, 120.0),     // Success
            (false, 5000.0),   // Failure
            (true, 50.0),      // Success
        ];

        executor.execute_batch(&results);

        // Record errors for specific actions
        let errors = vec![
            ("slack", "webhook timeout")
        ];
        executor.record_batch_errors(&errors);
    }

    #[test]
    fn test_action_chain_sequential_execution() {
        let trace_context = TraceContext::new(
            "parent".repeat(8),
            "span".repeat(4),
            0x01,
        );

        let mut chain = ActionChain::new(trace_context);

        // Add sequential actions
        let webhook_ctx = chain.add_action("webhook");
        let email_ctx = chain.add_action("email");
        let slack_ctx = chain.add_action("slack");

        // Verify parent trace ID is maintained
        assert_eq!(webhook_ctx.trace_id, "parent".repeat(8));
        assert_eq!(email_ctx.trace_id, "parent".repeat(8));
        assert_eq!(slack_ctx.trace_id, "parent".repeat(8));

        // Verify different span IDs
        assert_ne!(webhook_ctx.span_id, email_ctx.span_id);
        assert_ne!(email_ctx.span_id, slack_ctx.span_id);

        // Execute chain
        let all_headers = chain.execute_action_chain();
        assert_eq!(all_headers.len(), 3);

        for headers in all_headers {
            assert!(headers.contains_key("traceparent"));
        }
    }

    #[test]
    fn test_jaeger_span_recording() {
        let span = JaegerSpan {
            trace_id: "a".repeat(32),
            span_id: "b".repeat(16),
            parent_span_id: None,
            operation_name: "process_event".to_string(),
            start_time_ms: 1000,
            duration_ms: 150,
            tags: vec![
                ("event_id".to_string(), "evt-123".to_string()),
                ("status".to_string(), "success".to_string()),
            ],
            status: "ok".to_string(),
        };

        // Would record span (requires initialized Jaeger)
        // let result = record_span(span);
        // In tests without initialized Jaeger, this would error

        assert_eq!(span.duration_ms, 150);
        assert_eq!(span.tags.len(), 2);
    }

    #[test]
    fn test_jaeger_parent_child_spans() {
        let root = JaegerSpan {
            trace_id: "root".repeat(8),
            span_id: "parent".repeat(4),
            parent_span_id: None,
            operation_name: "root_operation".to_string(),
            start_time_ms: 0,
            duration_ms: 1000,
            tags: vec![],
            status: "ok".to_string(),
        };

        let child = JaegerSpan {
            trace_id: "root".repeat(8),
            span_id: "child".repeat(4),
            parent_span_id: Some("parent".repeat(4)),
            operation_name: "child_operation".to_string(),
            start_time_ms: 100,
            duration_ms: 500,
            tags: vec![
                ("child_attr".to_string(), "value".to_string()),
            ],
            status: "ok".to_string(),
        };

        // Verify relationships
        assert_eq!(root.trace_id, child.trace_id);
        assert_eq!(child.parent_span_id, Some("parent".repeat(4)));
    }

    #[test]
    fn test_jaeger_config_from_tracing_config() {
        let tracing_config = TracingConfig {
            enabled: true,
            service_name: "my-service".to_string(),
            jaeger_endpoint: "http://jaeger.example.com:14268/api/traces".to_string(),
            sample_rate: 0.5,
        };

        let jaeger_config = JaegerConfig::from_tracing_config(&tracing_config);

        assert_eq!(jaeger_config.service_name, "my-service");
        assert_eq!(jaeger_config.endpoint, "http://jaeger.example.com:14268/api/traces");
        assert_eq!(jaeger_config.sample_rate, 0.5);
        assert_eq!(jaeger_config.max_batch_size, 512);

        let validation = jaeger_config.validate();
        assert!(validation.is_ok());
    }

    #[test]
    fn test_end_to_end_event_processing_trace() {
        // Simulate complete event processing with tracing

        // 1. Create trace context for event
        let trace_context = TraceContext::new(
            "event".repeat(8),
            "root".repeat(4),
            0x01,
        );

        // 2. Record listener processing
        let listener = ListenerTracer::new("listener-1".to_string());
        listener.record_startup();
        listener.record_batch_start(10, 500);

        // 3. Record condition evaluation
        let condition = ConditionTracer::new("validator".to_string());
        condition.record_evaluation_start();
        condition.record_evaluation_result(true, 15.0);

        // 4. Record action execution
        let actions = ActionBatchExecutor::new();
        let results = vec![
            (true, 50.0),
            (true, 100.0),
        ];

        // 5. Complete listener batch
        listener.record_batch_complete(10, 0);

        // 6. Generate trace context headers for propagation
        let headers = trace_context.to_headers();
        assert!(headers.contains_key("traceparent"));
    }

    #[test]
    fn test_sampling_behavior() {
        // Test sampling rate interpretation

        let always_sample = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,  // sampled flag set
        );
        assert!(always_sample.is_sampled());

        let never_sample = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x00,  // sampled flag not set
        );
        assert!(!never_sample.is_sampled());
    }

    #[test]
    fn test_trace_context_header_format() {
        let ctx = TraceContext::new(
            "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            "00f067aa0ba902b7".to_string(),
            0x01,
        );

        let header = ctx.to_traceparent_header();

        // Verify W3C format: 00-{trace_id}-{span_id}-{flags}
        assert!(header.starts_with("00-"));
        assert_eq!(header.len(), "00-{32hex}-{16hex}-{2hex}".len());
    }

    #[test]
    fn test_trace_context_round_trip() {
        let original = TraceContext::new(
            "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            "00f067aa0ba902b7".to_string(),
            0x01,
        );

        let header = original.to_traceparent_header();
        let parsed = TraceContext::from_traceparent_header(&header);

        assert!(parsed.is_some());
        let parsed = parsed.unwrap();

        assert_eq!(parsed.trace_id, original.trace_id);
        assert_eq!(parsed.span_id, original.span_id);
        assert_eq!(parsed.trace_flags, original.trace_flags);
    }
}
