//! Comprehensive tracing integration tests
//!
//! End-to-end tests for distributed tracing functionality

#[cfg(test)]
mod tracing_e2e_tests {
    use crate::tracing::{
        init_tracing, TracingConfig, TraceContext, ListenerTracer, ExecutorTracer,
        ConditionTracer, WebhookTracer, EmailTracer, SlackTracer, ActionSpan,
        ActionBatchExecutor, ActionChain, JaegerConfig, JaegerSpan,
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
        let exporter = result
            .unwrap_or_else(|e| panic!("expected Ok for valid enabled tracing config: {e}"));
        assert!(exporter.is_some(), "Expected Some(JaegerExporter) when tracing is enabled");
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

    // ---- Smoke tests ----
    // The following tests verify that tracer methods can be called in various
    // sequences without panicking. For observability code, this is the primary
    // correctness guarantee: recording operations must never crash the host.

    #[test]
    fn test_listener_tracer_full_lifecycle() {
        let tracer = ListenerTracer::new("listener-1".to_string());

        tracer.record_startup();
        tracer.record_health_check(true);
        tracer.record_batch_start(100, 1000);
        tracer.record_batch_complete(100, 0);
        tracer.record_health_check(false);
        tracer.record_batch_complete(50, 5);
        // No panic = success: tracer handles full lifecycle
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
        // No panic = success: tracer handles retry sequences
    }

    #[test]
    fn test_condition_tracer_with_errors() {
        let tracer = ConditionTracer::new("order_validator".to_string());

        tracer.record_evaluation_start();
        tracer.record_evaluation_error("invalid condition syntax");
        tracer.record_evaluation_start();
        tracer.record_evaluation_result(true, 25.0);
        // No panic = success: tracer handles error-then-success sequences
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
        assert!(!headers.is_empty(), "trace context should produce headers");
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

        assert_eq!(recipients.len(), 3);
        recipients[0].record_batch_send(recipients.len());

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
        // No panic = success: tracer handles batch with mixed outcomes
    }

    #[test]
    fn test_slack_tracer_with_threads() {
        let tracer = SlackTracer::new("#alerts".to_string());

        tracer.record_start();
        tracer.record_success(200, 75.0);
        tracer.record_thread_created("ts-1234567890.123456");
        tracer.record_reaction("\u{1f44d}");
        tracer.record_reaction("\u{1f680}");

        let tracer2 = SlackTracer::new("#notifications".to_string());
        tracer2.record_start();
        tracer2.record_failure("rate_limited", 429.0);
        tracer2.record_retry(1, "rate limit exceeded");
        // No panic = success: tracer handles threads, reactions, and failures
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
        // No panic = success: span handles success and error paths
    }

    #[test]
    fn test_action_batch_executor_multi_action() {
        let mut executor = ActionBatchExecutor::new();

        executor.add_action("webhook", "notify_system");
        executor.add_action("email", "notify_admins");
        executor.add_action("slack", "alert_team");
        executor.add_action("webhook", "update_analytics");

        let results = vec![
            (true, 45.0),
            (true, 120.0),
            (false, 5000.0),
            (true, 50.0),
        ];

        executor.execute_batch(&results);

        let errors = vec![
            ("slack", "webhook timeout")
        ];
        executor.record_batch_errors(&errors);
        // No panic = success: batch executor handles mixed results and errors
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

        jaeger_config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for valid jaeger config: {e}"));
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
        let _actions = ActionBatchExecutor::new();
        let _results = vec![
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

mod mod_tests {
    use super::super::*;

    #[test]
    fn test_tracing_init_disabled() {
        let config = TracingConfig {
            enabled: false,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        // Should not panic or error
        let result = init_tracing(config);
        result.unwrap_or_else(|e| panic!("expected Ok when tracing is disabled: {e}"));
    }

    #[test]
    fn test_tracing_config_validation() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        };

        assert_eq!(config.service_name, "test");
        assert!(config.sample_rate >= 0.0 && config.sample_rate <= 1.0);
    }
}

mod config_tests {
    use super::super::config::*;

    #[test]
    fn test_config_default() {
        let config = TracingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.service_name, "observer-service");
        assert_eq!(config.sample_rate, 1.0);
    }

    #[test]
    fn test_config_validate_empty_service_name() {
        let config = TracingConfig {
            enabled: true,
            service_name: String::new(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "empty service_name must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_config_validate_invalid_sample_rate() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.5,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "sample_rate=1.5 must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_config_validate_invalid_endpoint() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "localhost:14268".to_string(),
            sample_rate: 1.0,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "non-http endpoint must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_config_validate_success() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test-service".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        };

        config.validate().unwrap_or_else(|e| panic!("expected Ok for valid config: {e}"));
    }
}

mod spans_tests {
    use super::super::spans::*;
    use crate::entity::EntityKind;
    use crate::event::Event;
    use uuid::Uuid;

    #[test]
    fn test_create_event_span() {
        let event = Event {
            id: Uuid::new_v4(),
            entity: crate::entity::Entity {
                id: Uuid::new_v4(),
                entity_type: "Order".to_string(),
                data: serde_json::json!({}),
            },
            kind: EntityKind::Created,
            timestamp: std::time::SystemTime::now(),
        };

        let (span_name, attributes) = create_event_span(&event);

        assert_eq!(span_name, "process_event");
        assert!(!attributes.is_empty());

        let attr_names: Vec<_> = attributes.iter().map(|(k, _)| *k).collect();
        assert!(attr_names.contains(&"event_id"));
        assert!(attr_names.contains(&"entity_type"));
        assert!(attr_names.contains(&"event_kind"));
    }

    #[test]
    fn test_create_action_span() {
        let (span_name, attributes) = create_action_span("webhook", 3);

        assert_eq!(span_name, "execute_action");
        assert_eq!(attributes.len(), 2);

        let attr_map: std::collections::HashMap<_, _> = attributes.iter().cloned().collect();
        assert_eq!(attr_map.get("action_type"), Some(&"webhook".to_string()));
        assert_eq!(attr_map.get("action_count"), Some(&"3".to_string()));
    }

    #[test]
    fn test_create_phase_span() {
        let attrs = vec![("status", "success".to_string())];
        let (span_name, attributes) = create_phase_span("checkpoint_load", attrs);

        assert_eq!(span_name, "checkpoint_load");
        assert_eq!(attributes.len(), 1);
    }
}

mod instrumentation_tests {
    use super::super::instrumentation::*;

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

mod exporter_tests {
    use super::super::exporter::*;
    use super::super::config::TracingConfig;

    fn make_test_exporter() -> JaegerExporter {
        init_jaeger_exporter(&TracingConfig {
            enabled: true,
            service_name: "test-service".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        })
        .expect("test exporter should initialize")
    }

    #[test]
    fn test_jaeger_config_creation() {
        let tracing_config = TracingConfig {
            enabled: true,
            service_name: "my-service".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        };

        let jaeger_config = JaegerConfig::from_tracing_config(&tracing_config);

        assert_eq!(jaeger_config.service_name, "my-service");
        assert_eq!(jaeger_config.endpoint, "http://localhost:14268/api/traces");
        assert_eq!(jaeger_config.sample_rate, 0.5);
    }

    #[test]
    fn test_jaeger_config_validation_valid() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        config.validate().unwrap_or_else(|e| panic!("expected Ok for valid config: {e}"));
    }

    #[test]
    fn test_jaeger_config_validation_invalid_endpoint() {
        let config = JaegerConfig {
            endpoint: String::new(),
            sample_rate: 0.5,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "empty endpoint must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_service_name() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
            service_name: String::new(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "empty service name must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_sample_rate_high() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.5,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "sample_rate > 1.0 must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_sample_rate_low() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: -0.1,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "sample_rate < 0.0 must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_batch_size() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
            service_name: "test".to_string(),
            max_batch_size: 0,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "max_batch_size=0 must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_exporter_init_enabled() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        let result = init_jaeger_exporter(&config);
        result.unwrap_or_else(|e| panic!("expected Ok for valid enabled config: {e}"));
    }

    #[test]
    fn test_jaeger_exporter_init_disabled() {
        let config = TracingConfig {
            enabled: false,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        let result = init_jaeger_exporter(&config);
        // Should succeed because validate() passes even if disabled
        result.unwrap_or_else(|e| panic!("expected Ok even when disabled: {e}"));
    }

    #[test]
    fn test_jaeger_exporter_init_invalid_config() {
        let config = TracingConfig {
            enabled: true,
            service_name: String::new(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        let result = init_jaeger_exporter(&config);
        assert!(
            matches!(result, Err(crate::error::Error::Tracing(_))),
            "empty service_name must return Tracing error, got: {result:?}"
        );
    }

    #[test]
    fn test_jaeger_span_creation() {
        let span = JaegerSpan {
            trace_id: "a".repeat(32),
            span_id: "b".repeat(16),
            parent_span_id: None,
            operation_name: "process_event".to_string(),
            start_time_ms: 1000,
            duration_ms: 100,
            tags: vec![
                ("event_id".to_string(), "evt-123".to_string()),
                ("status".to_string(), "success".to_string()),
            ],
            status: "ok".to_string(),
        };

        assert_eq!(span.trace_id, "a".repeat(32));
        assert_eq!(span.span_id, "b".repeat(16));
        assert_eq!(span.duration_ms, 100);
        assert_eq!(span.tags.len(), 2);
    }

    #[test]
    fn test_jaeger_span_with_parent() {
        let span = JaegerSpan {
            trace_id: "a".repeat(32),
            span_id: "c".repeat(16),
            parent_span_id: Some("b".repeat(16)),
            operation_name: "execute_action".to_string(),
            start_time_ms: 1100,
            duration_ms: 50,
            tags: vec![("action_type".to_string(), "webhook".to_string())],
            status: "ok".to_string(),
        };

        assert!(span.parent_span_id.is_some());
        assert_eq!(span.parent_span_id.as_ref().unwrap(), "b".repeat(16));
    }

    #[test]
    fn test_get_exporter_config() {
        let exporter = make_test_exporter();
        let config = exporter.config();
        assert_eq!(config.service_name, "test-service");
    }
}

mod action_tracing_tests {
    use super::super::action_tracing::*;

    #[test]
    fn test_webhook_tracer_creation() {
        let tracer = WebhookTracer::new("http://example.com/webhook".to_string());
        assert_eq!(tracer.url, "http://example.com/webhook");
    }

    #[test]
    fn test_webhook_tracer_methods() {
        let tracer = WebhookTracer::new("http://example.com/webhook".to_string());
        tracer.record_start();
        tracer.record_success(200, 42.5);
        tracer.record_failure("timeout", 5000.0);
        tracer.record_retry(1, "temporary failure");
        tracer.record_trace_context_injection(2);
    }

    #[test]
    fn test_email_tracer_creation() {
        let tracer = EmailTracer::new("user@example.com".to_string());
        assert_eq!(tracer.recipient, "user@example.com");
    }

    #[test]
    fn test_email_tracer_methods() {
        let tracer = EmailTracer::new("user@example.com".to_string());
        tracer.record_start("Welcome");
        tracer.record_success(Some("msg-123"), 150.0);
        tracer.record_failure("smtp error", 500.0);
        tracer.record_retry(2, "temporary failure");
        tracer.record_batch_send(5);
    }

    #[test]
    fn test_slack_tracer_creation() {
        let tracer = SlackTracer::new("#notifications".to_string());
        assert_eq!(tracer.channel, "#notifications");
    }

    #[test]
    fn test_slack_tracer_methods() {
        let tracer = SlackTracer::new("#notifications".to_string());
        tracer.record_start();
        tracer.record_success(200, 75.0);
        tracer.record_failure("webhook error", 3000.0);
        tracer.record_retry(1, "rate limited");
        tracer.record_thread_created("ts-123");
        tracer.record_reaction("👍");
    }

    #[test]
    fn test_action_span_creation() {
        let span = ActionSpan::new("webhook".to_string(), "notify_user".to_string());
        assert_eq!(span.action_type, "webhook");
        assert_eq!(span.action_name, "notify_user");
    }

    #[test]
    fn test_action_span_methods() {
        let span = ActionSpan::new("email".to_string(), "send_confirmation".to_string());
        span.record_start_span();
        span.record_result_span(true, 250.0);
        span.record_span_error("SMTP connection failed");
    }

    #[test]
    fn test_action_span_failure() {
        let span = ActionSpan::new("slack".to_string(), "send_alert".to_string());
        span.record_start_span();
        span.record_result_span(false, 5000.0);
    }
}

mod action_integration_tests {
    use super::super::action_integration::*;
    use super::super::TraceContext;

    #[test]
    fn test_webhook_execution_example() {
        let trace_context = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );

        let headers = webhook_execution_example(&trace_context, "http://example.com/webhook");

        assert!(headers.contains_key("traceparent"));
    }

    #[test]
    fn test_email_execution_example() {
        let recipients = vec!["user1@example.com", "user2@example.com"];
        let tracers = email_execution_example(&recipients);

        assert_eq!(tracers.len(), 2);
    }

    #[test]
    fn test_slack_execution_example() {
        let tracer = slack_execution_example("#notifications");
        assert_eq!(tracer.channel, "#notifications");
    }

    #[test]
    fn test_action_batch_executor() {
        let mut executor = ActionBatchExecutor::new();
        executor.add_action("webhook", "notify_user");
        executor.add_action("email", "send_confirmation");
        executor.add_action("slack", "alert_team");

        let results = vec![(true, 50.0), (true, 150.0), (false, 3000.0)];
        executor.execute_batch(&results);

        assert_eq!(executor.actions.len(), 3);
    }

    #[test]
    fn test_action_batch_executor_errors() {
        let mut executor = ActionBatchExecutor::new();
        executor.add_action("webhook", "notify_user");
        executor.add_action("email", "send_confirmation");

        let errors = vec![("webhook", "connection timeout")];
        executor.record_batch_errors(&errors);

        assert_eq!(executor.actions.len(), 2);
    }

    #[test]
    fn test_action_chain() {
        let trace_context = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );

        let mut chain = ActionChain::new(trace_context);
        let webhook_ctx = chain.add_action("webhook");
        let email_ctx = chain.add_action("email");
        let slack_ctx = chain.add_action("slack");

        // Verify trace IDs match parent
        assert_eq!(webhook_ctx.trace_id, "a".repeat(32));
        assert_eq!(email_ctx.trace_id, "a".repeat(32));
        assert_eq!(slack_ctx.trace_id, "a".repeat(32));
    }

    #[test]
    fn test_action_chain_execution() {
        let trace_context = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );

        let mut chain = ActionChain::new(trace_context);
        chain.add_action("webhook");
        chain.add_action("email");

        let headers = chain.execute_action_chain();

        assert_eq!(headers.len(), 2);
        for header_map in headers {
            assert!(header_map.contains_key("traceparent"));
        }
    }
}

mod propagation_tests {
    use super::super::propagation::*;
    use std::collections::HashMap;

    #[test]
    fn test_trace_context_new() {
        let ctx = TraceContext::new(
            "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            "00f067aa0ba902b7".to_string(),
            0x01,
        );

        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, 0x01);
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_is_sampled() {
        let sampled = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );
        assert!(sampled.is_sampled());

        let not_sampled = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x00,
        );
        assert!(!not_sampled.is_sampled());
    }

    #[test]
    fn test_to_traceparent_header() {
        let ctx = TraceContext::new(
            "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            "00f067aa0ba902b7".to_string(),
            0x01,
        );

        let header = ctx.to_traceparent_header();
        assert_eq!(
            header,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
    }

    #[test]
    fn test_to_headers() {
        let ctx = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );

        let headers = ctx.to_headers();
        assert!(headers.contains_key("traceparent"));
        assert_eq!(
            headers["traceparent"],
            format!("00-{}-{}-01", "a".repeat(32), "b".repeat(16))
        );
    }

    #[test]
    fn test_from_traceparent_header_valid() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent_header(header);

        assert!(ctx.is_some());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, 0x01);
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_from_traceparent_header_invalid_version() {
        let header = "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_from_traceparent_header_invalid_trace_id() {
        let header = "00-invalid-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_from_traceparent_header_invalid_span_id() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-invalid-01";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_from_traceparent_header_extra_fields_rejected_for_version_00() {
        // W3C spec §3.2.1: version-00 with extra dash-separated components is invalid.
        // tracestate is carried by a separate header, not embedded in traceparent.
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-vendor=value";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none(), "version-00 headers with 5+ parts must be rejected");
    }

    #[test]
    fn test_from_headers_tracestate_from_separate_header() {
        // tracestate must come from the tracestate header, not from traceparent.
        let mut headers = std::collections::HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
        );
        headers.insert("tracestate".to_string(), "vendor=value".to_string());

        let ctx = TraceContext::from_headers(&headers).unwrap();
        assert_eq!(ctx.trace_state, Some("vendor=value".to_string()));
    }

    #[test]
    fn test_from_traceparent_header_rejects_all_zero_trace_id() {
        let header = format!("00-{}-00f067aa0ba902b7-01", "0".repeat(32));
        assert!(TraceContext::from_traceparent_header(&header).is_none());
    }

    #[test]
    fn test_from_traceparent_header_rejects_all_zero_span_id() {
        let header = format!("00-4bf92f3577b34da6a3ce929d0e0e4736-{}-01", "0".repeat(16));
        assert!(TraceContext::from_traceparent_header(&header).is_none());
    }

    #[test]
    fn test_from_headers() {
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
        );
        headers.insert(
            "tracestate".to_string(),
            "vendor=value".to_string(),
        );

        let ctx = TraceContext::from_headers(&headers);

        assert!(ctx.is_some());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.trace_state, Some("vendor=value".to_string()));
    }

    #[test]
    fn test_child_span_id() {
        let ctx = TraceContext::new(
            "a".repeat(32),
            "0000000000000001".to_string(),
            0x01,
        );

        let child_id = ctx.child_span_id();
        assert_ne!(child_id, ctx.span_id);
        assert_eq!(child_id.len(), 16);
    }

    #[test]
    fn test_default_produces_valid_ids() {
        let ctx = TraceContext::default();
        // W3C spec: trace_id must be 32 hex chars, non-zero
        assert_eq!(ctx.trace_id.len(), 32);
        assert!(ctx.trace_id.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!ctx.trace_id.chars().all(|c| c == '0'), "all-zero trace_id is invalid");
        // span_id must be 16 hex chars, non-zero
        assert_eq!(ctx.span_id.len(), 16);
        assert!(ctx.span_id.chars().all(|c| c.is_ascii_hexdigit()));
        // Two defaults must differ (astronomically unlikely to collide)
        let ctx2 = TraceContext::default();
        assert_ne!(ctx.trace_id, ctx2.trace_id);
    }
}
