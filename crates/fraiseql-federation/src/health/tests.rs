#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_register_subgraph() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.register("orders", "http://orders:4001/graphql");

        assert_eq!(agg.subgraph_count(), 2);
    }

    #[test]
    fn test_healthy_report() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.report_healthy("users", Duration::from_millis(10));

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Healthy);
        assert_eq!(report.healthy_count, 1);
        assert_eq!(report.unhealthy_count, 0);
    }

    #[test]
    fn test_degraded_report() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("slow_service", "http://slow:4000/graphql");
        agg.report_healthy("slow_service", Duration::from_secs(10));

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Degraded);
        assert_eq!(report.degraded_count, 1);
    }

    #[test]
    fn test_unhealthy_report() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.report_unhealthy("users");

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Unhealthy);
        assert_eq!(report.unhealthy_count, 1);

        let user_report = &report.subgraphs[0];
        assert_eq!(user_report.consecutive_failures, 1);
    }

    #[test]
    fn test_mixed_health_worst_wins() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");
        agg.register("orders", "http://orders:4001/graphql");

        agg.report_healthy("users", Duration::from_millis(10));
        agg.report_unhealthy("orders");

        let report = agg.aggregate();
        assert_eq!(
            report.overall_status,
            SubgraphHealthStatus::Unhealthy,
            "worst status should win"
        );
        assert_eq!(report.healthy_count, 1);
        assert_eq!(report.unhealthy_count, 1);
    }

    #[test]
    fn test_recovery_resets_failures() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");

        agg.report_unhealthy("users");
        agg.report_unhealthy("users");

        let report = agg.aggregate();
        assert_eq!(report.subgraphs[0].consecutive_failures, 2);

        agg.report_healthy("users", Duration::from_millis(5));

        let report = agg.aggregate();
        assert_eq!(report.subgraphs[0].consecutive_failures, 0);
        assert_eq!(report.overall_status, SubgraphHealthStatus::Healthy);
    }

    #[test]
    fn test_unknown_initial_status() {
        let agg = SubgraphHealthAggregator::new();
        agg.register("users", "http://users:4000/graphql");

        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Unknown);
        assert_eq!(report.subgraphs[0].status, SubgraphHealthStatus::Unknown);
    }

    #[test]
    fn test_empty_aggregator() {
        let agg = SubgraphHealthAggregator::new();
        let report = agg.aggregate();
        assert_eq!(report.overall_status, SubgraphHealthStatus::Unknown);
        assert!(report.subgraphs.is_empty());
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(SubgraphHealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(SubgraphHealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(SubgraphHealthStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(SubgraphHealthStatus::Unknown.to_string(), "unknown");
    }
