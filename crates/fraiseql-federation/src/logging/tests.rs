use super::*;

    #[test]
    fn test_federation_log_context_creation() {
        let ctx = FederationLogContext::new(
            FederationOperationType::EntityResolution,
            "query-123".to_string(),
            10,
        );

        assert_eq!(ctx.entity_count, 10);
        assert_eq!(ctx.query_id, "query-123");
        assert!(ctx.typename.is_none());
        assert!(ctx.error_message.is_none());
    }

    #[test]
    fn test_federation_log_context_builder() {
        let ctx = FederationLogContext::new(
            FederationOperationType::ResolveDb,
            "query-456".to_string(),
            20,
        )
        .with_strategy(ResolutionStrategy::Db)
        .with_typename("User".to_string())
        .with_entity_count_unique(15)
        .with_resolved_count(15)
        .complete(25.5);

        assert_eq!(ctx.entity_count, 20);
        assert_eq!(ctx.entity_count_unique, Some(15));
        assert_eq!(ctx.resolved_count, Some(15));
        assert!((ctx.duration_ms - 25.5_f64).abs() < f64::EPSILON);
        assert!(matches!(ctx.status, OperationStatus::Success));
    }

    #[test]
    fn test_federation_log_context_error() {
        let ctx = FederationLogContext::new(
            FederationOperationType::ResolveHttp,
            "query-789".to_string(),
            5,
        )
        .fail(15.2, "Connection refused".to_string());

        assert!(matches!(ctx.status, OperationStatus::Error));
        assert_eq!(ctx.error_message, Some("Connection refused".to_string()));
        assert!((ctx.duration_ms - 15.2_f64).abs() < f64::EPSILON);
    }

    #[test]
    fn test_log_timer_elapsed() {
        let timer = LogTimer::new();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10.0);
        assert!(elapsed < 100.0); // Should be much less than 100ms
    }

    #[test]
    fn test_federation_log_context_serialization() {
        let ctx = FederationLogContext::new(
            FederationOperationType::EntityResolution,
            "query-123".to_string(),
            10,
        )
        .with_strategy(ResolutionStrategy::Db)
        .with_typename("User".to_string())
        .complete(25.5);

        let json = serde_json::to_string(&ctx).expect("JSON serialization failed");
        assert!(json.contains("\"entity_count\":10"));
        assert!(json.contains("\"duration_ms\":25.5"));
        assert!(json.contains("\"status\":\"success\""));
    }
