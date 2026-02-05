//! Observer management configuration.

use serde::{Deserialize, Serialize};

/// Configuration for observer management endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverManagementConfig {
    /// Whether observer management endpoints are enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Base path for observer management endpoints (default: "/api/observers")
    #[serde(default = "default_base_path")]
    pub base_path: String,

    /// Maximum page size for list queries
    #[serde(default = "default_max_page_size")]
    pub max_page_size: i64,

    /// Whether to include request/response payloads in logs
    #[serde(default)]
    pub log_payloads: bool,

    /// Retention period for observer logs in days (0 = keep forever)
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: i64,

    /// Whether to require authentication for management endpoints
    #[serde(default = "default_require_auth")]
    pub require_auth: bool,
}

impl Default for ObserverManagementConfig {
    fn default() -> Self {
        Self {
            enabled:            true,
            base_path:          "/api/observers".to_string(),
            max_page_size:      100,
            log_payloads:       false,
            log_retention_days: 30,
            require_auth:       true,
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_base_path() -> String {
    "/api/observers".to_string()
}

fn default_max_page_size() -> i64 {
    100
}

fn default_log_retention_days() -> i64 {
    30
}

fn default_require_auth() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ObserverManagementConfig::default();
        assert!(config.enabled);
        assert_eq!(config.base_path, "/api/observers");
        assert_eq!(config.max_page_size, 100);
        assert!(!config.log_payloads);
        assert_eq!(config.log_retention_days, 30);
        assert!(config.require_auth);
    }
}
