mod auto_tuner_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use fraiseql_core::db::{
        WhereClause,
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
    };
    use fraiseql_error::Result as FraiseQLResult;

    use super::super::auto_tuner::{PoolSizingAdvisor, PoolSizingRecommendation};
    use crate::config::pool_tuning::PoolPressureMonitorConfig;

    // Minimal mock adapter for tests — no database required.
    struct MockAdapter {
        metrics: PoolMetrics,
    }

    impl MockAdapter {
        fn with_metrics(metrics: PoolMetrics) -> Self {
            Self { metrics }
        }
    }

    // Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
    // its transformed method signatures to satisfy the trait contract
    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl fraiseql_core::db::traits::DatabaseAdapter for MockAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            self.metrics
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn make_tuner(min: u32, max: u32, target_queue: u32) -> PoolSizingAdvisor {
        PoolSizingAdvisor::new(PoolPressureMonitorConfig {
            enabled: true,
            min_pool_size: min,
            max_pool_size: max,
            target_queue_depth: target_queue,
            samples_before_action: 1,
            ..Default::default()
        })
    }

    fn metrics(total: u32, idle: u32, waiting: u32) -> PoolMetrics {
        PoolMetrics {
            total_connections:  total,
            idle_connections:   idle,
            active_connections: total.saturating_sub(idle),
            waiting_requests:   waiting,
        }
    }

    #[test]
    fn test_evaluate_stable_when_queue_low_and_idle_at_threshold() {
        // 10/20 = 50% idle — exactly at threshold, NOT above it → Stable
        let tuner = make_tuner(5, 50, 3);
        assert_eq!(tuner.evaluate(&metrics(20, 10, 0)), PoolSizingRecommendation::Stable);
    }

    #[test]
    fn test_evaluate_scale_up_when_queue_exceeds_target() {
        let tuner = make_tuner(5, 50, 3);
        let decision = tuner.evaluate(&metrics(20, 2, 8));
        assert!(
            matches!(&decision, PoolSizingRecommendation::RecommendScaleUp { new_size, .. } if *new_size == 25),
            "expected ScaleUp to 25, got {decision:?}"
        );
    }

    #[test]
    fn test_evaluate_scale_down_when_idle_ratio_high() {
        let tuner = make_tuner(5, 50, 3);
        // 18/20 = 0.9 idle ratio > 0.5 threshold
        let decision = tuner.evaluate(&metrics(20, 18, 0));
        assert!(
            matches!(&decision, PoolSizingRecommendation::RecommendScaleDown { new_size, .. } if *new_size == 18),
            "expected ScaleDown to 18, got {decision:?}"
        );
    }

    #[test]
    fn test_evaluate_never_below_min() {
        use std::sync::atomic::Ordering;
        let tuner = make_tuner(10, 50, 3);
        // Force initial current_target to 12
        tuner.current_target.store(12, Ordering::Relaxed);
        let decision = tuner.evaluate(&metrics(12, 12, 0));
        assert!(
            matches!(&decision, PoolSizingRecommendation::RecommendScaleDown { new_size, .. } if *new_size >= 10),
            "must not go below min=10, got {decision:?}"
        );
    }

    #[test]
    fn test_evaluate_never_above_max() {
        let tuner = make_tuner(5, 25, 3);
        // Pool already at max with high queue
        let decision = tuner.evaluate(&metrics(25, 0, 20));
        assert_eq!(decision, PoolSizingRecommendation::Stable, "cannot scale above max");
    }

    #[test]
    fn test_consecutive_samples_required_before_action() {
        let tuner = PoolSizingAdvisor::new(PoolPressureMonitorConfig {
            enabled: true,
            min_pool_size: 5,
            max_pool_size: 50,
            target_queue_depth: 3,
            scale_up_step: 5,
            samples_before_action: 3,
            ..Default::default()
        });
        let high_queue = metrics(20, 2, 8);
        assert_eq!(tuner.evaluate(&high_queue), PoolSizingRecommendation::Stable);
        assert_eq!(tuner.evaluate(&high_queue), PoolSizingRecommendation::Stable);
        assert!(matches!(
            tuner.evaluate(&high_queue),
            PoolSizingRecommendation::RecommendScaleUp { .. }
        ));
    }

    #[test]
    fn test_auto_tuner_recommended_size_initialises_to_zero() {
        let tuner = PoolSizingAdvisor::new(PoolPressureMonitorConfig::default());
        assert_eq!(tuner.recommended_size(), 0);
    }

    #[test]
    fn test_auto_tuner_adjustments_counter_starts_at_zero() {
        let tuner = PoolSizingAdvisor::new(PoolPressureMonitorConfig::default());
        assert_eq!(tuner.adjustments_total(), 0);
    }

    #[test]
    fn test_adjustments_counter_increments_on_scale_up() {
        let tuner = make_tuner(5, 50, 3);
        tuner.evaluate(&metrics(20, 2, 8));
        assert_eq!(tuner.adjustments_total(), 1);
    }

    #[test]
    fn test_adjustments_counter_increments_on_scale_down() {
        let tuner = make_tuner(5, 50, 3);
        tuner.evaluate(&metrics(20, 18, 0));
        assert_eq!(tuner.adjustments_total(), 1);
    }

    #[test]
    fn test_recommended_size_updated_after_scale_up() {
        let tuner = make_tuner(5, 50, 3);
        tuner.evaluate(&metrics(20, 2, 8));
        assert_eq!(tuner.recommended_size(), 25);
    }

    #[tokio::test]
    async fn test_start_task_samples_at_interval() {
        let config = PoolPressureMonitorConfig {
            enabled: true,
            tuning_interval_ms: 10,
            samples_before_action: 100, // never actually act
            ..Default::default()
        };
        let tuner = Arc::new(PoolSizingAdvisor::new(config));
        let adapter = Arc::new(MockAdapter::with_metrics(metrics(10, 8, 0)));
        let handle = PoolSizingAdvisor::start(tuner.clone(), adapter, None);
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Not crashing and handle is alive = success
        assert!(!handle.is_finished());
        handle.abort();
    }
}
