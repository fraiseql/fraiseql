//! JSONB field handling strategy selection
//!
//! Determines whether to project fields at database level or stream full JSONB

/// Strategy for JSONB field handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonbStrategy {
    /// Extract only requested fields using jsonb_build_object/JSON_OBJECT
    #[default]
    Project,
    /// Stream full JSONB column, filter in application
    Stream,
}

/// Configuration for JSONB optimization strategy
#[derive(Debug, Clone)]
pub struct JsonbOptimizationOptions {
    /// Default strategy to use
    pub default_strategy: JsonbStrategy,

    /// Auto-switch threshold: if requesting >= this % of fields, use stream
    pub auto_threshold_percent: u32,
}

impl Default for JsonbOptimizationOptions {
    fn default() -> Self {
        Self {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        }
    }
}

impl JsonbOptimizationOptions {
    /// Choose strategy based on field count and configuration
    pub fn choose_strategy(&self, requested_fields: usize, total_fields: usize) -> JsonbStrategy {
        if total_fields == 0 {
            return self.default_strategy;
        }

        let percent = (requested_fields as f64 / total_fields as f64) * 100.0;

        if percent >= self.auto_threshold_percent as f64 {
            JsonbStrategy::Stream
        } else {
            self.default_strategy
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choose_strategy_below_threshold() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(5, 10);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_strategy_at_threshold() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(8, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_above_threshold() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(9, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_respects_default() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(2, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_zero_total() {
        let opts = JsonbOptimizationOptions::default();
        let strategy = opts.choose_strategy(0, 0);
        assert_eq!(strategy, JsonbStrategy::Project);
    }
}
