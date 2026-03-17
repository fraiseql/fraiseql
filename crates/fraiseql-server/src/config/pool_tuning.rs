//! Connection pool pressure monitoring configuration.

use serde::{Deserialize, Serialize};

/// Configuration for connection pool pressure monitoring with scaling recommendations.
///
/// This monitor samples `PoolMetrics` at a configurable interval and emits
/// scaling recommendations via `fraiseql_pool_tuning_*` Prometheus metrics and
/// log lines. **It does not resize the pool at runtime** — the underlying
/// `deadpool-postgres` library does not expose a `resize()` API.
///
/// To act on recommendations: adjust `max_connections` in `fraiseql.toml` and
/// restart the server. Active pool resizing is tracked as future work (migration
/// to `bb8` with `resize()` support).
///
/// # Recommendation mode
///
/// All scaling decisions are advisory. When a recommendation fires, the monitor:
/// - Updates `fraiseql_pool_tuning_adjustments_total` (Prometheus counter)
/// - Logs the recommendation at `WARN` level
/// - Updates `recommended_size()` for external inspection
///
/// To suppress the `WARN` noise in environments that already tune the pool
/// manually, set `enabled = false` in `[pool_tuning]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolPressureMonitorConfig {
    /// Enable adaptive pool sizing.  Default: `false`.
    #[serde(default)]
    pub enabled: bool,

    /// Minimum pool size.  The tuner never shrinks below this value.  Default: 5.
    #[serde(default = "default_min_pool_size")]
    pub min_pool_size: u32,

    /// Maximum pool size.  The tuner never grows above this value.  Default: 50.
    #[serde(default = "default_max_pool_size")]
    pub max_pool_size: u32,

    /// Maximum acceptable queue depth before scaling up.  Default: 3.
    #[serde(default = "default_target_queue_depth")]
    pub target_queue_depth: u32,

    /// Connections to add per scale-up step.  Default: 5.
    #[serde(default = "default_scale_up_step")]
    pub scale_up_step: u32,

    /// Connections to remove per scale-down step.  Default: 2.
    #[serde(default = "default_scale_down_step")]
    pub scale_down_step: u32,

    /// Minimum idle ratio (idle / total) before considering a scale-down.
    /// Default: 0.5 (50% idle connections triggers potential shrink).
    #[serde(default = "default_scale_down_idle_ratio")]
    pub scale_down_idle_ratio: f64,

    /// Polling interval in milliseconds.  Default: 30 000 (30 s).
    #[serde(default = "default_tuning_interval_ms")]
    pub tuning_interval_ms: u64,

    /// Consecutive samples above threshold required before acting.  Default: 3.
    #[serde(default = "default_samples_before_action")]
    pub samples_before_action: u32,
}

const fn default_min_pool_size() -> u32 {
    5
}
const fn default_max_pool_size() -> u32 {
    50
}
const fn default_target_queue_depth() -> u32 {
    3
}
const fn default_scale_up_step() -> u32 {
    5
}
const fn default_scale_down_step() -> u32 {
    2
}
const fn default_scale_down_idle_ratio() -> f64 {
    0.5
}
const fn default_tuning_interval_ms() -> u64 {
    30_000
}
const fn default_samples_before_action() -> u32 {
    3
}

impl Default for PoolPressureMonitorConfig {
    fn default() -> Self {
        Self {
            enabled:               false,
            min_pool_size:         default_min_pool_size(),
            max_pool_size:         default_max_pool_size(),
            target_queue_depth:    default_target_queue_depth(),
            scale_up_step:         default_scale_up_step(),
            scale_down_step:       default_scale_down_step(),
            scale_down_idle_ratio: default_scale_down_idle_ratio(),
            tuning_interval_ms:    default_tuning_interval_ms(),
            samples_before_action: default_samples_before_action(),
        }
    }
}

/// Deprecated alias for [`PoolPressureMonitorConfig`].
///
/// This type was renamed in v2.0.1 to clarify that pool monitoring operates in
/// recommendation mode only — the pool is not resized at runtime.
/// Use [`PoolPressureMonitorConfig`] in new code.
#[deprecated(since = "2.0.1", note = "Use PoolPressureMonitorConfig")]
pub type PoolTuningConfig = PoolPressureMonitorConfig;

impl PoolPressureMonitorConfig {
    /// Validate configuration invariants.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - `min_pool_size >= max_pool_size`
    /// - `scale_up_step == 0` or `scale_down_step == 0`
    /// - `scale_down_idle_ratio` is outside `[0.0, 1.0]`
    /// - `tuning_interval_ms < 100`
    pub fn validate(&self) -> Result<(), String> {
        if self.min_pool_size >= self.max_pool_size {
            return Err(format!(
                "pool_tuning: min_pool_size ({}) must be less than max_pool_size ({})",
                self.min_pool_size, self.max_pool_size
            ));
        }
        if self.scale_up_step == 0 {
            return Err("pool_tuning: scale_up_step must be > 0".to_string());
        }
        if self.scale_down_step == 0 {
            return Err("pool_tuning: scale_down_step must be > 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.scale_down_idle_ratio) {
            return Err(format!(
                "pool_tuning: scale_down_idle_ratio ({}) must be in [0.0, 1.0]",
                self.scale_down_idle_ratio
            ));
        }
        if self.tuning_interval_ms < 100 {
            return Err(format!(
                "pool_tuning: tuning_interval_ms ({}) must be >= 100",
                self.tuning_interval_ms
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]  // Reason: test module wildcard import; brings all items into test scope
    use super::*;

    #[test]
    fn test_default_config_is_disabled() {
        let cfg = PoolPressureMonitorConfig::default();
        assert!(!cfg.enabled, "pool pressure monitoring should be off by default");
    }

    #[test]
    fn test_default_bounds_are_sensible() {
        let cfg = PoolPressureMonitorConfig::default();
        assert!(cfg.min_pool_size < cfg.max_pool_size);
        assert!(cfg.scale_up_step > 0);
        assert!(cfg.scale_down_step > 0);
        assert!(cfg.tuning_interval_ms >= 1000);
    }

    #[test]
    fn test_validate_passes_for_defaults() {
        PoolPressureMonitorConfig::default()
            .validate()
            .unwrap_or_else(|e| panic!("default pool monitor config should pass validation: {e}"));
    }

    #[test]
    fn test_validate_min_lt_max() {
        let cfg = PoolPressureMonitorConfig {
            min_pool_size: 10,
            max_pool_size: 5,
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "min >= max should be invalid, got: {:?}", cfg.validate());
    }

    #[test]
    fn test_validate_min_equals_max_is_invalid() {
        let cfg = PoolPressureMonitorConfig {
            min_pool_size: 10,
            max_pool_size: 10,
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "min == max should be invalid, got: {:?}", cfg.validate());
    }

    #[test]
    fn test_validate_idle_ratio_above_one() {
        let cfg = PoolPressureMonitorConfig {
            scale_down_idle_ratio: 1.5,
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "idle ratio > 1.0 should be invalid, got: {:?}", cfg.validate());
    }

    #[test]
    fn test_validate_idle_ratio_negative() {
        let cfg = PoolPressureMonitorConfig {
            scale_down_idle_ratio: -0.1,
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "idle ratio < 0.0 should be invalid, got: {:?}", cfg.validate());
    }

    #[test]
    fn test_validate_zero_scale_up_step() {
        let cfg = PoolPressureMonitorConfig {
            scale_up_step: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "scale_up_step == 0 should be invalid, got: {:?}", cfg.validate());
    }

    #[test]
    fn test_validate_zero_scale_down_step() {
        let cfg = PoolPressureMonitorConfig {
            scale_down_step: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "scale_down_step == 0 should be invalid, got: {:?}", cfg.validate());
    }

    #[test]
    #[allow(deprecated)]  // Reason: re-exporting deprecated alias for backward compatibility
    fn test_pool_tuning_config_alias_works() {
        // PoolTuningConfig is a deprecated alias for PoolPressureMonitorConfig
        let _cfg: PoolTuningConfig = PoolTuningConfig::default();
    }

    #[test]
    fn test_validate_interval_too_short() {
        let cfg = PoolPressureMonitorConfig {
            tuning_interval_ms: 50,
            ..Default::default()
        };
        assert!(cfg.validate().is_err(), "tuning_interval_ms < 100 should be invalid, got: {:?}", cfg.validate());
    }
}
