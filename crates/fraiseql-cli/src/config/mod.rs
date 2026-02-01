//! Configuration loading and management
//!
//! This module handles loading configuration from fraiseql.toml files,
//! including security settings, project metadata, and compilation options.

pub mod security;

pub use security::SecurityConfig;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Project configuration from fraiseql.toml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct FraiseQLConfig {
    #[serde(rename = "project")]
    pub project: ProjectConfig,

    #[serde(rename = "fraiseql")]
    pub fraiseql: FraiseQLSettings,
}

/// Project metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "my-fraiseql-app".to_string(),
            version: "1.0.0".to_string(),
            description: None,
        }
    }
}

/// FraiseQL-specific settings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FraiseQLSettings {
    pub schema_file: String,
    pub output_file: String,
    #[serde(rename = "security")]
    pub security: SecurityConfig,
}

impl Default for FraiseQLSettings {
    fn default() -> Self {
        Self {
            schema_file: "schema.json".to_string(),
            output_file: "schema.compiled.json".to_string(),
            security: SecurityConfig::default(),
        }
    }
}

impl FraiseQLConfig {
    /// Load configuration from fraiseql.toml file
    pub fn from_file(path: &str) -> Result<Self> {
        info!("Loading configuration from {path}");

        let path = Path::new(path);
        if !path.exists() {
            anyhow::bail!("Configuration file not found: {path:?}");
        }

        let toml_content = std::fs::read_to_string(path)
            .context("Failed to read fraiseql.toml")?;

        let config: FraiseQLConfig = toml::from_str(&toml_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse fraiseql.toml: {e}"))?;

        Ok(config)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        info!("Validating configuration");
        self.fraiseql.security.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FraiseQLConfig::default();
        assert_eq!(config.project.name, "my-fraiseql-app");
        assert_eq!(config.fraiseql.schema_file, "schema.json");
    }

    #[test]
    fn test_default_security_config() {
        let config = FraiseQLConfig::default();
        assert!(config.fraiseql.security.audit_logging.enabled);
        assert!(config.fraiseql.security.rate_limiting.enabled);
    }

    #[test]
    fn test_validation() {
        let config = FraiseQLConfig::default();
        assert!(config.validate().is_ok());
    }
}
