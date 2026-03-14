//! Adaptive connection pool auto-tuner.
//!
//! Monitors connection pool health via [`PoolMetrics`] and either resizes the
//! pool or emits a recommended size when the queue depth or idle ratio crosses
//! configured thresholds.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use fraiseql_core::db::{traits::DatabaseAdapter, types::PoolMetrics};

use crate::config::pool_tuning::PoolPressureMonitorConfig;

/// Recommendation produced by [`PoolSizingAdvisor::evaluate`].
///
/// `RecommendScaleUp` and `RecommendScaleDown` are *recommendations*, not executed actions.
/// Whether they are applied depends on whether a `resize_fn` was configured in
/// [`PoolSizingAdvisor::start`]. Without a `resize_fn`, decisions are logged at INFO level
/// only — no actual pool resize occurs.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PoolSizingRecommendation {
    /// Pool size is appropriate. No action needed.
    Stable,
    /// Pool should be grown to `new_size` connections.
    ///
    /// Applied only if a `resize_fn` was configured; otherwise logged as an advisory.
    RecommendScaleUp {
        /// New target pool size.
        new_size: u32,
        /// Human-readable reason for the recommendation.
        reason:   String,
    },
    /// Pool should be shrunk to `new_size` connections.
    ///
    /// Applied only if a `resize_fn` was configured; otherwise logged as an advisory.
    RecommendScaleDown {
        /// New target pool size.
        new_size: u32,
        /// Human-readable reason for the recommendation.
        reason:   String,
    },
}

/// Connection pool pressure monitor with scaling recommendations.
///
/// Call [`PoolSizingAdvisor::evaluate`] with current [`PoolMetrics`] to get a
/// [`PoolSizingRecommendation`], or call [`PoolSizingAdvisor::start`] to launch a
/// background task that polls the adapter automatically.
///
/// **Note**: This monitor operates in recommendation mode only. The pool is not
/// resized at runtime — act on `fraiseql_pool_tuning_*` events by adjusting
/// `max_connections` in `fraiseql.toml` and restarting the server.
pub struct PoolSizingAdvisor {
    /// Pressure monitoring configuration.
    pub(crate) config:  PoolPressureMonitorConfig,
    /// Consecutive samples with high queue depth.
    high_queue_samples: AtomicU32,
    /// Consecutive samples with high idle ratio.
    low_idle_samples:   AtomicU32,
    /// Total resize operations applied or recommended.
    adjustments_total:  AtomicU64,
    /// Current recommended/actual target pool size (0 = not yet sampled).
    current_target:     AtomicU32,
}

impl PoolSizingAdvisor {
    /// Create a new pool pressure monitor with the given configuration.
    pub const fn new(config: PoolPressureMonitorConfig) -> Self {
        Self {
            config,
            high_queue_samples: AtomicU32::new(0),
            low_idle_samples: AtomicU32::new(0),
            adjustments_total: AtomicU64::new(0),
            current_target: AtomicU32::new(0),
        }
    }

    /// Evaluate current pool metrics and return a scaling decision.
    ///
    /// This method is pure computation — no I/O, no async.  It updates internal
    /// sample counters so consecutive calls with the same condition accumulate
    /// toward `samples_before_action`.
    pub fn evaluate(&self, metrics: &PoolMetrics) -> PoolSizingRecommendation {
        let current = self.current_size(metrics);
        let min = self.config.min_pool_size;
        let max = self.config.max_pool_size;

        // ── Scale-up check ──────────────────────────────────────────────────
        if metrics.waiting_requests > self.config.target_queue_depth {
            let count = self.high_queue_samples.fetch_add(1, Ordering::Relaxed) + 1;
            self.low_idle_samples.store(0, Ordering::Relaxed);

            if count >= self.config.samples_before_action {
                let desired = (current + self.config.scale_up_step).min(max);
                if desired > current {
                    self.high_queue_samples.store(0, Ordering::Relaxed);
                    self.adjustments_total.fetch_add(1, Ordering::Relaxed);
                    self.current_target.store(desired, Ordering::Relaxed);
                    return PoolSizingRecommendation::RecommendScaleUp {
                        new_size: desired,
                        reason:   format!(
                            "{} requests waiting (threshold {}); grown by {}",
                            metrics.waiting_requests,
                            self.config.target_queue_depth,
                            self.config.scale_up_step,
                        ),
                    };
                }
                // Already at max — reset and stay stable
                self.high_queue_samples.store(0, Ordering::Relaxed);
            }
            return PoolSizingRecommendation::Stable;
        }

        self.high_queue_samples.store(0, Ordering::Relaxed);

        // ── Scale-down check ─────────────────────────────────────────────────
        if current > min && metrics.total_connections > 0 {
            let idle_ratio =
                f64::from(metrics.idle_connections) / f64::from(metrics.total_connections);

            if idle_ratio > self.config.scale_down_idle_ratio && metrics.waiting_requests == 0 {
                let count = self.low_idle_samples.fetch_add(1, Ordering::Relaxed) + 1;

                if count >= self.config.samples_before_action {
                    let desired = current.saturating_sub(self.config.scale_down_step).max(min);
                    self.low_idle_samples.store(0, Ordering::Relaxed);
                    self.adjustments_total.fetch_add(1, Ordering::Relaxed);
                    self.current_target.store(desired, Ordering::Relaxed);
                    return PoolSizingRecommendation::RecommendScaleDown {
                        new_size: desired,
                        reason:   format!(
                            "idle ratio {:.0}% > {:.0}% threshold; shrunk by {}",
                            idle_ratio * 100.0,
                            self.config.scale_down_idle_ratio * 100.0,
                            self.config.scale_down_step,
                        ),
                    };
                }
                return PoolSizingRecommendation::Stable;
            }
        }

        self.low_idle_samples.store(0, Ordering::Relaxed);
        PoolSizingRecommendation::Stable
    }

    /// Total number of resize operations applied or recommended.
    pub fn adjustments_total(&self) -> u64 {
        self.adjustments_total.load(Ordering::Relaxed)
    }

    /// Current recommended pool size (0 = not yet sampled).
    pub fn recommended_size(&self) -> u32 {
        self.current_target.load(Ordering::Relaxed)
    }

    /// Start a background polling task.
    ///
    /// The task samples `adapter.pool_metrics()` every `tuning_interval_ms`
    /// milliseconds and calls [`Self::evaluate`].  If `resize_fn` is
    /// provided, it is called with the new pool size whenever a resize is
    /// decided.  If `resize_fn` is `None`, the tuner operates in
    /// **recommendation mode**: it updates `recommended_size` and logs a
    /// warning without modifying the pool.
    ///
    /// Returns a [`tokio::task::JoinHandle`] that can be aborted for shutdown.
    pub fn start<A: DatabaseAdapter + 'static>(
        self: Arc<Self>,
        adapter: Arc<A>,
        resize_fn: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    ) -> tokio::task::JoinHandle<()> {
        let interval_ms = self.config.tuning_interval_ms;

        tokio::spawn(async move {
            tracing::debug!(
                "Pool pressure monitoring enabled (recommendation mode). \
                 The pool cannot be resized at runtime; act on \
                 fraiseql_pool_scaling_recommended events by adjusting \
                 max_connections and restarting."
            );
            if resize_fn.is_none() {
                tracing::debug!(
                    "No resize_fn configured — pool pressure monitor is in \
                     pure advisory mode. Recommendations will appear in \
                     fraiseql_pool_tuning_* metrics and WARN log lines."
                );
            }

            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms.max(1)));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;
                let metrics = adapter.pool_metrics();

                match self.evaluate(&metrics) {
                    PoolSizingRecommendation::Stable => {},
                    PoolSizingRecommendation::RecommendScaleUp {
                        new_size,
                        ref reason,
                    } => {
                        if let Some(ref f) = resize_fn {
                            tracing::info!(
                                new_size,
                                reason = reason.as_str(),
                                "Pool auto-tuner: scaling up"
                            );
                            f(new_size as usize);
                        } else {
                            tracing::warn!(
                                new_size,
                                reason = reason.as_str(),
                                "Pool auto-tuner recommends scaling up \
                                 (resize not available — configure resize_fn)"
                            );
                        }
                    },
                    PoolSizingRecommendation::RecommendScaleDown {
                        new_size,
                        ref reason,
                    } => {
                        if let Some(ref f) = resize_fn {
                            tracing::info!(
                                new_size,
                                reason = reason.as_str(),
                                "Pool auto-tuner: scaling down"
                            );
                            f(new_size as usize);
                        } else {
                            tracing::warn!(
                                new_size,
                                reason = reason.as_str(),
                                "Pool auto-tuner recommends scaling down \
                                 (resize not available — configure resize_fn)"
                            );
                        }
                    },
                }
            }
        })
    }

    /// Current pool size from metrics, falling back to `min_pool_size`.
    fn current_size(&self, metrics: &PoolMetrics) -> u32 {
        let recorded = self.current_target.load(Ordering::Relaxed);
        if recorded > 0 {
            recorded
        } else if metrics.total_connections > 0 {
            metrics.total_connections
        } else {
            self.config.min_pool_size
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use async_trait::async_trait;
    use fraiseql_core::db::{
        WhereClause,
        types::{DatabaseType, JsonbValue},
    };
    use fraiseql_error::Result as FraiseQLResult;

    use super::*;

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
    impl DatabaseAdapter for MockAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
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
