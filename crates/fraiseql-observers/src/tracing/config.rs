//! Tracing configuration

use serde::{Deserialize, Serialize};
use std::env;

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable/disable tracing
    pub enabled: bool,

    /// Service name for traces
    pub service_name: String,

    /// Jaeger endpoint for trace export
    pub jaeger_endpoint: String,

    /// Sample rate (0.0 - 1.0)
    /// - 1.0 = trace all events (100%)
    /// - 0.1 = trace 10% of events
    /// - 0.0 = disabled
    pub sample_rate: f64,
}

impl TracingConfig {
    /// Create config from environment variables
    ///
    /// Reads the following environment variables:
    /// - `TRACING_ENABLED`: Enable tracing (default: false)
    /// - `TRACING_SERVICE_NAME`: Service name (default: "observer-service")
    /// - `JAEGER_ENDPOINT`: Jaeger endpoint (default: http://localhost:14268/api/traces)
    /// - `JAEGER_SAMPLE_RATE`: Sample rate 0.0-1.0 (default: 1.0)
    pub fn from_env() -> Self {
        Self {
            enabled: env::var("TRACING_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),

            service_name: env::var("TRACING_SERVICE_NAME")
                .unwrap_or_else(|_| "observer-service".to_string()),

            jaeger_endpoint: env::var("JAEGER_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:14268/api/traces".to_string()),

            sample_rate: env::var("JAEGER_SAMPLE_RATE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
        }
    }

    /// Validate configuration
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - service_name is empty
    /// - sample_rate is not between 0.0 and 1.0
    /// - jaeger_endpoint is invalid
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.service_name.is_empty() {
            return Err(crate::error::Error::Tracing(
                "service_name cannot be empty".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.sample_rate) {
            return Err(crate::error::Error::Tracing(
                format!(
                    "sample_rate must be between 0.0 and 1.0, got {}",
                    self.sample_rate
                ),
            ));
        }

        // Validate endpoint URL format
        if self.enabled && !self.jaeger_endpoint.starts_with("http://")
            && !self.jaeger_endpoint.starts_with("https://")
        {
            return Err(crate::error::Error::Tracing(
                "jaeger_endpoint must be a valid HTTP(S) URL".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: "observer-service".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = TracingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.service_name, "observer-service");
        assert_eq!(config.sample_rate, 1.0);
    }

    #[test]
    fn test_config_validate_empty_service_name() {
        let config = TracingConfig {
            enabled: true,
            service_name: String::new(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_sample_rate() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.5,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_endpoint() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "localhost:14268".to_string(),
            sample_rate: 1.0,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_success() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test-service".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        };

        assert!(config.validate().is_ok());
    }
}
