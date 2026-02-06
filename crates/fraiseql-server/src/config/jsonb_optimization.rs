//! JSONB optimization configuration
//!
//! Configurable strategy for handling JSONB field projection:
//! - "project": Extract only requested fields at database level (reduces payload)
//! - "stream": Return full JSONB column and filter in Rust (reduces CPU)

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

/// JSONB optimization configuration
#[derive(Debug, Clone, Deserialize)]
pub struct JsonbOptimizationConfig {
    /// Default strategy: "project" | "stream"
    #[serde(default = "default_strategy")]
    pub default_strategy: JsonbStrategy,

    /// Auto-switch threshold: if requesting >= this % of fields, use stream instead
    #[serde(default = "default_auto_threshold_percent")]
    pub auto_threshold_percent: u32,

    /// Allow per-query override via GraphQL directive
    #[serde(default = "default_allow_query_hint")]
    pub allow_query_hint: bool,
}

fn default_strategy() -> JsonbStrategy {
    JsonbStrategy::Project
}

fn default_auto_threshold_percent() -> u32 {
    80
}

fn default_allow_query_hint() -> bool {
    true
}

impl Default for JsonbOptimizationConfig {
    fn default() -> Self {
        Self {
            default_strategy:       default_strategy(),
            auto_threshold_percent: default_auto_threshold_percent(),
            allow_query_hint:       default_allow_query_hint(),
        }
    }
}

impl JsonbOptimizationConfig {
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

    // ========================================================================
    // Phase 2, Cycle 1: Configuration Loading and Parsing
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
    }

    #[test]
    fn test_jsonb_strategy_from_str_invalid() {
        let result = "invalid".parse::<JsonbStrategy>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSONB strategy"));
    }

    #[test]
    fn test_jsonb_strategy_default() {
        assert_eq!(JsonbStrategy::default(), JsonbStrategy::Project);
    }

    #[test]
    fn test_jsonb_optimization_config_default() {
        let config = JsonbOptimizationConfig::default();
        assert_eq!(config.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.auto_threshold_percent, 80);
        assert_eq!(config.allow_query_hint, true);
    }

    // ========================================================================
    // Phase 2, Cycle 2: Strategy Selection Logic
    // ========================================================================

    #[test]
    fn test_choose_strategy_uses_default_when_below_threshold() {
        let config = JsonbOptimizationConfig {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
            allow_query_hint:       true,
        };

        // Requesting 50% of fields, below 80% threshold
        let strategy = config.choose_strategy(5, 10);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_strategy_uses_stream_when_above_threshold() {
        let config = JsonbOptimizationConfig {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
            allow_query_hint:       true,
        };

        // Requesting 85% of fields, above 80% threshold
        let strategy = config.choose_strategy(17, 20);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_at_exact_threshold() {
        let config = JsonbOptimizationConfig {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
            allow_query_hint:       true,
        };

        // Requesting exactly 80% of fields (at threshold)
        let strategy = config.choose_strategy(8, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_single_field() {
        let config = JsonbOptimizationConfig::default();

        // Requesting 1 of 10 fields (10% < 80%)
        let strategy = config.choose_strategy(1, 10);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_strategy_all_fields() {
        let config = JsonbOptimizationConfig {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
            allow_query_hint:       true,
        };

        // Requesting all 10 fields (100% > 80%)
        let strategy = config.choose_strategy(10, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_respects_default_strategy() {
        let config = JsonbOptimizationConfig {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 80,
            allow_query_hint:       true,
        };

        // Requesting 50% of fields, below threshold
        // Should use Stream as default_strategy
        let strategy = config.choose_strategy(5, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_zero_total_fields() {
        let config = JsonbOptimizationConfig::default();

        // Edge case: 0 total fields
        let strategy = config.choose_strategy(0, 0);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    // ========================================================================
    // Phase 2, Cycle 3: Deserialization from TOML
    // ========================================================================

    #[test]
    fn test_jsonb_optimization_config_deserialize() {
        let toml_str = r#"
default_strategy = "project"
auto_threshold_percent = 75
allow_query_hint = true
"#;
        let config: JsonbOptimizationConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.auto_threshold_percent, 75);
        assert_eq!(config.allow_query_hint, true);
    }

    #[test]
    fn test_jsonb_optimization_config_deserialize_stream() {
        let toml_str = r#"
default_strategy = "stream"
auto_threshold_percent = 50
allow_query_hint = false
"#;
        let config: JsonbOptimizationConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_strategy, JsonbStrategy::Stream);
        assert_eq!(config.auto_threshold_percent, 50);
        assert_eq!(config.allow_query_hint, false);
    }

    #[test]
    fn test_jsonb_optimization_config_deserialize_with_defaults() {
        let toml_str = r#"
default_strategy = "project"
"#;
        let config: JsonbOptimizationConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.auto_threshold_percent, 80); // default
        assert_eq!(config.allow_query_hint, true); // default
    }

    #[test]
    fn test_jsonb_optimization_config_deserialize_empty() {
        let toml_str = "";
        let config: JsonbOptimizationConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.auto_threshold_percent, 80);
        assert_eq!(config.allow_query_hint, true);
    }
}
