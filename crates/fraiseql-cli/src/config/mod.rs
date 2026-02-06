//! Configuration loading and management
//!
//! This module handles loading configuration from fraiseql.toml files,
//! including security settings, project metadata, and compilation options.

pub mod security;
pub mod toml_schema;

use std::path::Path;

use anyhow::{Context, Result};
pub use security::SecurityConfig;
use serde::{Deserialize, Serialize};
pub use toml_schema::TomlSchema;
use tracing::info;

/// Project configuration from fraiseql.toml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct FraiseQLConfig {
    /// Project metadata (name, version, description)
    #[serde(rename = "project")]
    pub project: ProjectConfig,

    /// FraiseQL-specific settings
    #[serde(rename = "fraiseql")]
    pub fraiseql: FraiseQLSettings,
}

/// Project metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ProjectConfig {
    /// Project name
    pub name:        String,
    /// Project version
    pub version:     String,
    /// Optional project description
    pub description: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name:        "my-fraiseql-app".to_string(),
            version:     "1.0.0".to_string(),
            description: None,
        }
    }
}

/// FraiseQL-specific settings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FraiseQLSettings {
    /// Path to the GraphQL schema file
    pub schema_file: String,
    /// Path to the output compiled schema file
    pub output_file: String,
    /// Security configuration
    #[serde(rename = "security")]
    pub security:    SecurityConfig,
}

impl Default for FraiseQLSettings {
    fn default() -> Self {
        Self {
            schema_file: "schema.json".to_string(),
            output_file: "schema.compiled.json".to_string(),
            security:    SecurityConfig::default(),
        }
    }
}

impl FraiseQLConfig {
    /// Load configuration from fraiseql.toml file
    pub fn from_file(path: &str) -> Result<Self> {
        info!("Loading configuration from {path}");

        let path = Path::new(path);
        if !path.exists() {
            anyhow::bail!("Configuration file not found: {}", path.display());
        }

        let toml_content = std::fs::read_to_string(path).context("Failed to read fraiseql.toml")?;

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

    #[test]
    fn test_role_definitions_default() {
        let config = FraiseQLConfig::default();
        assert!(config.fraiseql.security.role_definitions.is_empty());
        assert!(config.fraiseql.security.default_role.is_none());
    }

    #[test]
    fn test_parse_role_definitions_from_toml() {
        let toml_str = r#"
[project]
name = "test-app"

[fraiseql]
schema_file = "schema.json"

[[fraiseql.security.role_definitions]]
name = "viewer"
description = "Read-only access"
scopes = ["read:*"]

[[fraiseql.security.role_definitions]]
name = "admin"
description = "Full access"
scopes = ["admin:*"]

[fraiseql.security]
default_role = "viewer"
"#;

        let config: FraiseQLConfig = toml::from_str(toml_str).expect("Failed to parse TOML");

        assert_eq!(config.fraiseql.security.role_definitions.len(), 2);
        assert_eq!(config.fraiseql.security.role_definitions[0].name, "viewer");
        assert_eq!(config.fraiseql.security.role_definitions[0].scopes[0], "read:*");
        assert_eq!(config.fraiseql.security.role_definitions[1].name, "admin");
        assert_eq!(config.fraiseql.security.default_role, Some("viewer".to_string()));
    }

    #[test]
    fn test_security_config_role_lookup() {
        let toml_str = r#"
[[fraiseql.security.role_definitions]]
name = "viewer"
scopes = ["read:User.*", "read:Post.*"]

[[fraiseql.security.role_definitions]]
name = "editor"
scopes = ["read:*", "write:*"]
"#;

        let config: FraiseQLConfig = toml::from_str(toml_str).expect("Failed to parse TOML");

        // Test find_role
        let viewer = config.fraiseql.security.find_role("viewer");
        assert!(viewer.is_some());
        assert_eq!(viewer.unwrap().name, "viewer");

        // Test get_role_scopes
        let scopes = config.fraiseql.security.get_role_scopes("viewer");
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&"read:User.*".to_string()));

        // Test non-existent role
        let scopes = config.fraiseql.security.get_role_scopes("non-existent");
        assert!(scopes.is_empty());
    }

    #[test]
    fn test_security_config_validation_empty_role_name() {
        let toml_str = r#"
[[fraiseql.security.role_definitions]]
name = ""
scopes = ["read:*"]
"#;

        let config: FraiseQLConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty role name");
    }

    #[test]
    fn test_security_config_validation_empty_scopes() {
        let toml_str = r#"
[[fraiseql.security.role_definitions]]
name = "viewer"
scopes = []
"#;

        let config: FraiseQLConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty scopes");
    }
}
