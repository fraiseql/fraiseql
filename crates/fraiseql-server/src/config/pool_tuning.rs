//! Connection pool auto-tuning configuration.

use serde::{Deserialize, Serialize};

/// Configuration for adaptive connection pool sizing.
///
/// The auto-tuner samples `PoolMetrics` at a
/// configurable interval and adjusts the pool size (or emits a recommended size)
/// based on queue depth and idle ratio.
///
/// # Runtime resize
///
/// If a `resize_fn` is supplied to `PoolAutoTuner::start`, the tuner will call it
/// to apply resizes in real time.  If no resize function is available (e.g. the
/// database adapter does not expose pool mutation), the tuner operates in
/// **recommendation mode**: it still emits `fraiseql_pool_recommended_size` and
/// logs a warning, but does not change the actual pool size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolTuningConfig {
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

fn default_min_pool_size() -> u32 { 5 }
fn default_max_pool_size() -> u32 { 50 }
fn default_target_queue_depth() -> u32 { 3 }
fn default_scale_up_step() -> u32 { 5 }
fn default_scale_down_step() -> u32 { 2 }
fn default_scale_down_idle_ratio() -> f64 { 0.5 }
fn default_tuning_interval_ms() -> u64 { 30_000 }
fn default_samples_before_action() -> u32 { 3 }

impl Default for PoolTuningConfig {
    fn default() -> Self {
        Self {
            enabled:                false,
            min_pool_size:          default_min_pool_size(),
            max_pool_size:          default_max_pool_size(),
            target_queue_depth:     default_target_queue_depth(),
            scale_up_step:          default_scale_up_step(),
            scale_down_step:        default_scale_down_step(),
            scale_down_idle_ratio:  default_scale_down_idle_ratio(),
            tuning_interval_ms:     default_tuning_interval_ms(),
            samples_before_action:  default_samples_before_action(),
        }
    }
}

impl PoolTuningConfig {
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
    use super::*;

    #[test]
    fn test_default_config_is_disabled() {
        let cfg = PoolTuningConfig::default();
        assert!(!cfg.enabled, "auto-tuning should be off by default");
    }

    #[test]
    fn test_default_bounds_are_sensible() {
        let cfg = PoolTuningConfig::default();
        assert!(cfg.min_pool_size < cfg.max_pool_size);
        assert!(cfg.scale_up_step > 0);
        assert!(cfg.scale_down_step > 0);
        assert!(cfg.tuning_interval_ms >= 1000);
    }

    #[test]
    fn test_validate_passes_for_defaults() {
        assert!(PoolTuningConfig::default().validate().is_ok(), "default pool tuning config should pass validation");
    }

    #[test]
    fn test_validate_min_lt_max() {
        let cfg = PoolTuningConfig { min_pool_size: 10, max_pool_size: 5, ..Default::default() };
        assert!(cfg.validate().is_err(), "min >= max should be invalid");
    }

    #[test]
    fn test_validate_min_equals_max_is_invalid() {
        let cfg = PoolTuningConfig { min_pool_size: 10, max_pool_size: 10, ..Default::default() };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_idle_ratio_above_one() {
        let cfg = PoolTuningConfig { scale_down_idle_ratio: 1.5, ..Default::default() };
        assert!(cfg.validate().is_err(), "idle ratio > 1.0 should be invalid");
    }

    #[test]
    fn test_validate_idle_ratio_negative() {
        let cfg = PoolTuningConfig { scale_down_idle_ratio: -0.1, ..Default::default() };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_zero_scale_up_step() {
        let cfg = PoolTuningConfig { scale_up_step: 0, ..Default::default() };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_zero_scale_down_step() {
        let cfg = PoolTuningConfig { scale_down_step: 0, ..Default::default() };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_interval_too_short() {
        let cfg = PoolTuningConfig { tuning_interval_ms: 50, ..Default::default() };
        assert!(cfg.validate().is_err());
    }
}
