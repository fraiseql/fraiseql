//! TOML-based database configuration parsing and management
//!
//! This module provides runtime configuration loading for database connections,
//! feature flags, and runtime settings from TOML files.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Database configuration for a single database
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabaseConnectionConfig {
    /// Database URL/connection string
    pub url: String,
    /// Connection pool size
    pub pool_size: u32,
    /// SSL mode (disable, allow, prefer, require)
    pub ssl_mode: String,
    /// Connection timeout in seconds
    pub timeout_seconds: u32,
}

impl Default for DatabaseConnectionConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            pool_size: 10,
            ssl_mode: "prefer".to_string(),
            timeout_seconds: 30,
        }
    }
}

/// Multiple database configurations (e.g., primary, replica, secondary)
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabasesConfig {
    /// Database configurations by name
    #[serde(flatten)]
    pub databases: BTreeMap<String, DatabaseConnectionConfig>,
}

impl DatabasesConfig {
    /// Get a database configuration by name
    #[allow(dead_code)]
    pub fn get(&self, name: &str) -> Option<&DatabaseConnectionConfig> {
        self.databases.get(name)
    }

    /// Get the primary database (first one, or one named "primary")
    #[allow(dead_code)]
    pub fn primary(&self) -> Option<&DatabaseConnectionConfig> {
        self.databases.get("primary").or_else(|| self.databases.values().next())
    }

    /// List all database names
    #[allow(dead_code)]
    pub fn names(&self) -> Vec<&str> {
        self.databases.keys().map(String::as_str).collect()
    }

    /// Check if any databases are configured
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.databases.is_empty()
    }

    /// Validate all database configurations
    #[allow(dead_code)]
    pub fn validate(&self) -> Result<()> {
        if self.is_empty() {
            anyhow::bail!("No databases configured");
        }

        for (name, config) in &self.databases {
            if config.url.is_empty() {
                anyhow::bail!("Database '{name}' has empty URL");
            }
            if config.pool_size == 0 {
                anyhow::bail!("Database '{name}' pool_size must be > 0");
            }
            if config.timeout_seconds == 0 {
                anyhow::bail!("Database '{name}' timeout_seconds must be > 0");
            }
        }

        Ok(())
    }
}

/// Feature flags for runtime features
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FeaturesConfig {
    /// Enable Arrow format for subscriptions
    pub enable_arrow: bool,
    /// Enable result caching
    pub enable_caching: bool,
    /// Enable subscriptions
    pub enable_subscriptions: bool,
    /// Enable federation
    pub enable_federation: bool,
    /// Enable observers/event system
    pub enable_observers: bool,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            enable_arrow: false,
            enable_caching: true,
            enable_subscriptions: false,
            enable_federation: false,
            enable_observers: false,
        }
    }
}

/// Runtime settings for query execution
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct RuntimeSettingsConfig {
    /// Query timeout in milliseconds
    pub query_timeout_ms: u64,
    /// Maximum batch size for bulk operations
    pub max_batch_size: u32,
    /// Maximum query depth (for complexity limiting)
    pub max_query_depth: u32,
    /// Enable query cost analysis
    pub enable_cost_analysis: bool,
}

impl Default for RuntimeSettingsConfig {
    fn default() -> Self {
        Self {
            query_timeout_ms: 30000,
            max_batch_size: 1000,
            max_query_depth: 10,
            enable_cost_analysis: false,
        }
    }
}

/// Complete TOML configuration for runtime
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct TomlConfig {
    /// Database configurations
    #[serde(rename = "database")]
    pub databases: DatabasesConfig,

    /// Feature flags
    #[serde(rename = "features")]
    pub features: FeaturesConfig,

    /// Runtime settings
    #[serde(rename = "runtime")]
    pub runtime: RuntimeSettingsConfig,
}

impl TomlConfig {
    /// Load configuration from TOML file
    #[allow(dead_code)]
    pub fn from_file(path: &str) -> Result<Self> {
        let path = Path::new(path);
        if !path.exists() {
            anyhow::bail!("Configuration file not found: {}", path.display());
        }

        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read TOML file: {}", path.display()))?;

        Self::parse_toml(&content)
    }

    /// Parse configuration from TOML string
    #[allow(dead_code)]
    pub fn parse_toml(content: &str) -> Result<Self> {
        toml::from_str(content).context("Failed to parse TOML configuration")
    }

    /// Validate all configuration sections
    #[allow(dead_code)]
    pub fn validate(&self) -> Result<()> {
        self.databases.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cycle 1: Parse Database Configuration

    #[test]
    fn test_parse_single_database_config() {
        let toml = r#"
[database.primary]
url = "postgresql://user:pass@localhost/fraiseql"
pool_size = 20
ssl_mode = "require"
timeout_seconds = 30
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(!config.databases.is_empty());

        let db = config.databases.get("primary").expect("primary database not found");
        assert_eq!(db.url, "postgresql://user:pass@localhost/fraiseql");
        assert_eq!(db.pool_size, 20);
        assert_eq!(db.ssl_mode, "require");
        assert_eq!(db.timeout_seconds, 30);
    }

    #[test]
    fn test_parse_multiple_databases() {
        let toml = r#"
[database.primary]
url = "postgresql://primary-host/db"
pool_size = 20

[database.replica]
url = "postgresql://replica-host/db"
pool_size = 10
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert_eq!(config.databases.names().len(), 2);
        assert!(config.databases.get("primary").is_some());
        assert!(config.databases.get("replica").is_some());
    }

    #[test]
    fn test_database_config_defaults() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        let db = config.databases.primary().expect("No primary database");

        assert_eq!(db.pool_size, 10);  // default
        assert_eq!(db.ssl_mode, "prefer");  // default
        assert_eq!(db.timeout_seconds, 30);  // default
    }

    #[test]
    fn test_database_validation_empty_url() {
        let toml = r#"
[database.primary]
url = ""
pool_size = 10
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty URL");
    }

    #[test]
    fn test_database_validation_missing_database() {
        let config = TomlConfig::default();
        assert!(config.validate().is_err(), "Should fail with no databases");
    }

    #[test]
    fn test_database_primary_lookup() {
        let toml = r#"
[database.primary]
url = "postgresql://primary/db"

[database.replica]
url = "postgresql://replica/db"
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        let primary = config.databases.primary().expect("No primary");
        assert!(primary.url.contains("primary"));
    }

    // Cycle 2: Parse Feature Flags and Runtime Settings

    #[test]
    fn test_parse_feature_flags() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"

[features]
enable_arrow = true
enable_caching = true
enable_subscriptions = true
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(config.features.enable_arrow);
        assert!(config.features.enable_caching);
        assert!(config.features.enable_subscriptions);
        assert!(!config.features.enable_federation);  // default is false
    }

    #[test]
    fn test_parse_runtime_settings() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"

[runtime]
query_timeout_ms = 60000
max_batch_size = 5000
max_query_depth = 20
enable_cost_analysis = true
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert_eq!(config.runtime.query_timeout_ms, 60000);
        assert_eq!(config.runtime.max_batch_size, 5000);
        assert_eq!(config.runtime.max_query_depth, 20);
        assert!(config.runtime.enable_cost_analysis);
    }

    #[test]
    fn test_default_feature_flags() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(!config.features.enable_arrow);  // default false
        assert!(config.features.enable_caching);  // default true
        assert!(!config.features.enable_subscriptions);  // default false
    }

    #[test]
    fn test_default_runtime_settings() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert_eq!(config.runtime.query_timeout_ms, 30000);  // default
        assert_eq!(config.runtime.max_batch_size, 1000);  // default
        assert_eq!(config.runtime.max_query_depth, 10);  // default
    }

    // Cycle 3: Configuration Validation and Testing

    #[test]
    fn test_complete_configuration_validation() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"
pool_size = 20

[features]
enable_caching = true

[runtime]
query_timeout_ms = 30000
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(config.validate().is_ok(), "Valid config should pass");
    }

    #[test]
    fn test_database_pool_size_validation() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"
pool_size = 0
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with zero pool_size");
    }

    #[test]
    fn test_database_timeout_validation() {
        let toml = r#"
[database.primary]
url = "postgresql://localhost/db"
timeout_seconds = 0
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with zero timeout");
    }

    #[test]
    fn test_multiple_database_validation() {
        let toml = r#"
[database.primary]
url = "postgresql://primary/db"

[database.replica]
url = ""

[database.secondary]
url = "postgresql://secondary/db"
"#;

        let config = TomlConfig::parse_toml(toml).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty replica URL");
    }
}
