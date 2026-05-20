//! JSONB field handling strategy selection
//!
//! Determines whether to project fields at database level or stream full JSONB

use serde::Deserialize;

/// Strategy for JSONB field handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum JsonbStrategy {
    /// Extract only requested fields using `jsonb_build_object/JSON_OBJECT`
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
            other => {
                Err(format!("Invalid JSONB strategy '{}', must be 'project' or 'stream'", other))
            },
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
            default_strategy: JsonbStrategy::Project,
            auto_threshold_percent: 80,
        }
    }
}

impl JsonbOptimizationOptions {
    /// Choose strategy based on field count and configuration
    #[must_use]
    pub fn choose_strategy(&self, requested_fields: usize, total_fields: usize) -> JsonbStrategy {
        if total_fields == 0 {
            return self.default_strategy;
        }

        #[allow(clippy::cast_precision_loss)]
        // Reason: field counts are small integers; f64 precision loss on usize is not material
        let percent = (requested_fields as f64 / total_fields as f64) * 100.0;

        if percent >= f64::from(self.auto_threshold_percent) {
            JsonbStrategy::Stream
        } else {
            self.default_strategy
        }
    }
}
