//! Configuration loading and management
//!
//! This module handles loading configuration from fraiseql.toml files,
//! including security settings, project metadata, and compilation options.

pub mod runtime;
pub mod security;
pub mod toml_schema;

use std::path::Path;

use anyhow::{Context, Result};
pub use runtime::{DatabaseRuntimeConfig, ServerRuntimeConfig};
pub use security::SecurityConfig;
use serde::{Deserialize, Serialize};
pub use toml_schema::TomlSchema;
use tracing::info;

/// Project configuration from fraiseql.toml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TomlProjectConfig {
    /// Project metadata (name, version, description)
    #[serde(rename = "project")]
    pub project: ProjectConfig,

    /// FraiseQL-specific settings
    #[serde(rename = "fraiseql")]
    pub fraiseql: FraiseQLSettings,

    /// HTTP server runtime configuration (optional — all fields have defaults).
    #[serde(default)]
    pub server: ServerRuntimeConfig,

    /// Database connection pool configuration (optional — all fields have defaults).
    #[serde(default)]
    pub database: DatabaseRuntimeConfig,
}

/// Project metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    /// Project name
    pub name:            String,
    /// Project version
    pub version:         String,
    /// Optional project description
    pub description:     Option<String>,
    /// Target database backend (e.g. "postgresql", "mysql", "sqlite", "sqlserver")
    pub database_target: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name:            "my-fraiseql-app".to_string(),
            version:         "1.0.0".to_string(),
            description:     None,
            database_target: None,
        }
    }
}

/// FraiseQL-specific settings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
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

impl TomlProjectConfig {
    /// Load configuration from fraiseql.toml file.
    ///
    /// Supports `${VAR}` environment variable interpolation throughout the file.
    pub fn from_file(path: &str) -> Result<Self> {
        info!("Loading configuration from {path}");

        let path = Path::new(path);
        if !path.exists() {
            anyhow::bail!("Configuration file not found: {}", path.display());
        }

        let raw = std::fs::read_to_string(path).context("Failed to read fraiseql.toml")?;
        let toml_content = expand_env_vars(&raw);

        let config: TomlProjectConfig = toml::from_str(&toml_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse fraiseql.toml: {e}"))?;

        Ok(config)
    }

    /// Validate configuration.
    pub fn validate(&self) -> Result<()> {
        info!("Validating configuration");
        self.fraiseql.security.validate()?;
        self.server.validate()?;
        self.database.validate()?;
        Ok(())
    }
}

/// Expand `${VAR}` environment variable placeholders in a string.
///
/// Unknown variables are left as-is (no panic, silent passthrough).
#[allow(clippy::expect_used)] // Reason: regex pattern is a compile-time constant guaranteed to be valid
pub(crate) fn expand_env_vars(content: &str) -> String {
    use std::sync::LazyLock;

    static ENV_VAR_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("env var regex is valid")
    });

    ENV_VAR_REGEX
        .replace_all(content, |caps: &regex::Captures| {
            std::env::var(&caps[1]).unwrap_or_else(|_| format!("${{{}}}", &caps[1]))
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TomlProjectConfig::default();
        assert_eq!(config.project.name, "my-fraiseql-app");
        assert_eq!(config.fraiseql.schema_file, "schema.json");
    }

    #[test]
    fn test_default_security_config() {
        let config = TomlProjectConfig::default();
        assert!(config.fraiseql.security.audit_logging.enabled);
        assert!(config.fraiseql.security.rate_limiting.enabled);
    }

    #[test]
    fn test_validation() {
        let config = TomlProjectConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_role_definitions_default() {
        let config = TomlProjectConfig::default();
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

        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");

        assert_eq!(config.fraiseql.security.role_definitions.len(), 2);
        assert_eq!(config.fraiseql.security.role_definitions[0].name, "viewer");
        assert_eq!(config.fraiseql.security.role_definitions[0].scopes[0], "read:*");
        assert_eq!(config.fraiseql.security.role_definitions[1].name, "admin");
        assert_eq!(config.fraiseql.security.default_role, Some("viewer".to_string()));
    }

    #[test]
    fn test_security_config_validation_empty_role_name() {
        let toml_str = r#"
[[fraiseql.security.role_definitions]]
name = ""
scopes = ["read:*"]
"#;

        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty role name");
    }

    #[test]
    fn test_security_config_validation_empty_scopes() {
        let toml_str = r#"
[[fraiseql.security.role_definitions]]
name = "viewer"
scopes = []
"#;

        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert!(config.validate().is_err(), "Should fail with empty scopes");
    }

    #[test]
    fn test_fraiseql_config_parses_server_section() {
        let toml_str = r#"
[server]
host = "127.0.0.1"
port = 9000

[server.cors]
origins = ["https://example.com"]
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.cors.origins, ["https://example.com"]);
    }

    #[test]
    fn test_fraiseql_config_parses_database_section() {
        let toml_str = r#"
[database]
url      = "postgresql://localhost/testdb"
pool_min = 3
pool_max = 15
ssl_mode = "require"
"#;
        let config: TomlProjectConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.database.url, Some("postgresql://localhost/testdb".to_string()));
        assert_eq!(config.database.pool_min, 3);
        assert_eq!(config.database.pool_max, 15);
        assert_eq!(config.database.ssl_mode, "require");
    }

    #[test]
    fn test_env_var_expansion_in_fraiseql_config() {
        temp_env::with_var("TEST_DB_URL", Some("postgres://test/db"), || {
            let toml_str = r#"
[database]
url = "${TEST_DB_URL}"
"#;
            let expanded = expand_env_vars(toml_str);
            let config: TomlProjectConfig =
                toml::from_str(&expanded).expect("Failed to parse TOML");
            assert_eq!(config.database.url, Some("postgres://test/db".to_string()));
        });
    }

    #[test]
    fn test_env_var_expansion_unknown_var_passthrough() {
        // Unknown variables should be left as-is, not panic
        let toml_str = r#"url = "${NONEXISTENT_VAR_XYZ123}""#;
        let expanded = expand_env_vars(toml_str);
        assert_eq!(expanded, toml_str, "Unknown vars must be left unchanged");
    }

    #[test]
    fn test_env_var_expansion_multiple_occurrences() {
        temp_env::with_var("FRAISEQL_TEST_HOST", Some("db.example.com"), || {
            let toml_str = r#"primary = "${FRAISEQL_TEST_HOST}" replica = "${FRAISEQL_TEST_HOST}""#;
            let expanded = expand_env_vars(toml_str);
            assert_eq!(expanded, r#"primary = "db.example.com" replica = "db.example.com""#);
        });
    }
}
