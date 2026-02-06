//! JSONB field handling strategy selection
//!
//! Determines whether to project fields at database level or stream full JSONB

use serde::Deserialize;

/// Strategy for JSONB field handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonbStrategy {
    /// Extract only requested fields using jsonb_build_object/JSON_OBJECT
    #[default]
    Project,
    /// Stream full JSONB column, filter in application
    Stream,
}

impl std::str::FromStr for JsonbStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "project" => Ok(JsonbStrategy::Project),
            "stream" => Ok(JsonbStrategy::Stream),
            other => Err(format!(
                "Invalid JSONB strategy '{}', must be 'project' or 'stream'",
                other
            )),
        }
    }
}

impl<'de> Deserialize<'de> for JsonbStrategy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        std::str::FromStr::from_str(&s).map_err(serde::de::Error::custom)
    }
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

        if percent >= f64::from(self.auto_threshold_percent) {
            JsonbStrategy::Stream
        } else {
            self.default_strategy
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Strategy Parsing Tests
    // ========================================================================

    #[test]
    fn test_jsonb_strategy_from_str_project() {
        let strategy: JsonbStrategy = "project".parse().unwrap();
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_jsonb_strategy_from_str_stream() {
        let strategy: JsonbStrategy = "stream".parse().unwrap();
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_jsonb_strategy_from_str_case_insensitive() {
        assert_eq!("PROJECT".parse::<JsonbStrategy>().unwrap(), JsonbStrategy::Project);
        assert_eq!("Stream".parse::<JsonbStrategy>().unwrap(), JsonbStrategy::Stream);
        assert_eq!("pRoJeCt".parse::<JsonbStrategy>().unwrap(), JsonbStrategy::Project);
    }

    #[test]
    fn test_jsonb_strategy_from_str_invalid() {
        let result = "invalid".parse::<JsonbStrategy>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSONB strategy"));
    }

    #[test]
    fn test_jsonb_strategy_deserialize() {
        let json = r#""project""#;
        let strategy: JsonbStrategy = serde_json::from_str(json).unwrap();
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_jsonb_strategy_deserialize_stream() {
        let json = r#""stream""#;
        let strategy: JsonbStrategy = serde_json::from_str(json).unwrap();
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    // ========================================================================
    // Strategy Selection Tests
    // ========================================================================

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
