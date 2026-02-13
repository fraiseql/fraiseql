//! Configuration validation
//!
//! Validates server configuration at startup

use serde::{Deserialize, Serialize};

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server port
    pub port:                 u16,
    /// Server host
    pub host:                 String,
    /// Database URL
    pub database_url:         String,
    /// Log level
    pub log_level:            String,
    /// Request timeout in seconds
    pub request_timeout_secs: u32,
}

/// Configuration validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Is valid
    pub valid:  bool,
    /// Errors
    pub errors: Vec<String>,
}

impl ValidationResult {
    /// Create valid result
    pub fn valid() -> Self {
        Self {
            valid:  true,
            errors: Vec::new(),
        }
    }

    /// Create invalid result with errors
    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
        }
    }

    /// Add an error
    pub fn with_error(mut self, error: String) -> Self {
        self.valid = false;
        self.errors.push(error);
        self
    }
}

/// Validate server configuration
pub fn validate_config(config: &ServerConfig) -> ValidationResult {
    let mut errors = Vec::new();

    // Validate port
    if config.port == 0 {
        errors.push("Port must not be zero".to_string());
    }

    // Validate host
    if config.host.is_empty() {
        errors.push("Host must not be empty".to_string());
    }

    // Validate database URL
    if config.database_url.is_empty() {
        errors.push("Database URL must not be empty".to_string());
    }

    // Validate log level
    let valid_levels = ["debug", "info", "warn", "error"];
    if !valid_levels.contains(&config.log_level.as_str()) {
        errors.push(format!(
            "Invalid log level: {}. Must be one of: {:?}",
            config.log_level, valid_levels
        ));
    }

    // Validate request timeout
    if config.request_timeout_secs == 0 {
        errors.push("Request timeout must be greater than zero".to_string());
    }

    if errors.is_empty() {
        ValidationResult::valid()
    } else {
        ValidationResult::invalid(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config() {
        let config = ServerConfig {
            port:                 8080,
            host:                 "0.0.0.0".to_string(),
            database_url:         "postgres://localhost/db".to_string(),
            log_level:            "info".to_string(),
            request_timeout_secs: 30,
        };

        let result = validate_config(&config);
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_invalid_config() {
        let config = ServerConfig {
            port:                 0,
            host:                 "".to_string(),
            database_url:         "".to_string(),
            log_level:            "invalid".to_string(),
            request_timeout_secs: 0,
        };

        let result = validate_config(&config);
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }
}
