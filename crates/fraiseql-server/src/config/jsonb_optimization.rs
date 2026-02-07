//! JSONB optimization configuration
//!
//! Configurable strategy for handling JSONB field projection:
//! - "project": Extract only requested fields at database level (reduces payload)
//! - "stream": Return full JSONB column and filter in Rust (reduces CPU)
//!
//! This module re-exports core JSONB types for use in server-level configuration.
//! The actual strategy logic is implemented in fraiseql-core::runtime::jsonb_strategy.

// Re-export core types - single source of truth
pub use fraiseql_core::runtime::{JsonbOptimizationOptions as CoreJsonbOptions, JsonbStrategy};
use serde::Deserialize;

/// Server-level JSONB optimization configuration
///
/// Extends the core JsonbOptimizationOptions with server-specific concerns like
/// query directives and caching hints.
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
    JsonbStrategy::default()
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
    /// Convert to core options for use in runtime
    pub fn to_core_options(&self) -> CoreJsonbOptions {
        CoreJsonbOptions {
            default_strategy:       self.default_strategy,
            auto_threshold_percent: self.auto_threshold_percent,
        }
    }

    /// Choose strategy based on field count and configuration
    pub fn choose_strategy(&self, requested_fields: usize, total_fields: usize) -> JsonbStrategy {
        self.to_core_options().choose_strategy(requested_fields, total_fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonb_optimization_config_default() {
        let config = JsonbOptimizationConfig::default();
        assert_eq!(config.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.auto_threshold_percent, 80);
        assert_eq!(config.allow_query_hint, true);
    }

    #[test]
    fn test_jsonb_optimization_config_to_core_options() {
        let config = JsonbOptimizationConfig {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 75,
            allow_query_hint:       false,
        };

        let core_opts = config.to_core_options();
        assert_eq!(core_opts.default_strategy, JsonbStrategy::Stream);
        assert_eq!(core_opts.auto_threshold_percent, 75);
    }

    #[test]
    fn test_jsonb_optimization_config_choose_strategy() {
        let config = JsonbOptimizationConfig {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
            allow_query_hint:       true,
        };

        // Below threshold
        assert_eq!(config.choose_strategy(5, 10), JsonbStrategy::Project);
        // Above threshold
        assert_eq!(config.choose_strategy(8, 10), JsonbStrategy::Stream);
    }

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
        assert_eq!(config.auto_threshold_percent, 80);
        assert_eq!(config.allow_query_hint, true);
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
