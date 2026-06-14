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
    /// Tenancy isolation configuration
    #[serde(default)]
    pub tenancy:     security::TenancyTomlConfig,
    /// Mutation compilation options (`[fraiseql.mutations]`)
    #[serde(default)]
    pub mutations:   MutationsConfig,
}

impl Default for FraiseQLSettings {
    fn default() -> Self {
        Self {
            schema_file: "schema.json".to_string(),
            output_file: "schema.compiled.json".to_string(),
            security:    SecurityConfig::default(),
            tenancy:     security::TenancyTomlConfig::default(),
            mutations:   MutationsConfig::default(),
        }
    }
}

/// Mutation compilation options from `[fraiseql.mutations]`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MutationsConfig {
    /// Opt-in: auto-synthesize a shared `MutationError` type and a per-mutation
    /// result union (`<Mutation>Result = Entity | MutationError`) for every
    /// object-returning mutation, rewriting its return type to that union so the
    /// server's success/error discrimination has a union to resolve against.
    /// Off by default; mutations that already return a union are left untouched.
    pub auto_error_union: bool,
}

impl TomlProjectConfig {
    /// Load configuration from fraiseql.toml file.
    ///
    /// Supports `${VAR}` environment variable interpolation throughout the file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file does not exist, cannot be read, or cannot be
    /// parsed as valid TOML matching the `TomlProjectConfig` structure.
    pub fn from_file(path: &str) -> Result<Self> {
        info!("Loading configuration from {path}");

        let path = Path::new(path);
        if !path.exists() {
            anyhow::bail!("Configuration file not found: {}", path.display());
        }

        let raw = std::fs::read_to_string(path).context("Failed to read fraiseql.toml")?;
        let toml_content = expand_env_vars(&raw)?;

        let config: TomlProjectConfig = toml::from_str(&toml_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse fraiseql.toml: {e}"))?;

        Ok(config)
    }

    /// Validate configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any security, server, or database configuration value
    /// is invalid (e.g. unsupported algorithm, zero window, or bad port range).
    pub fn validate(&self) -> Result<()> {
        info!("Validating configuration");
        self.fraiseql.security.validate()?;
        self.fraiseql.tenancy.validate()?;
        self.server.validate()?;
        self.database.validate()?;
        Ok(())
    }
}

/// Expand `${VAR}` environment variable placeholders in a string.
///
/// Unknown variables are left as-is (no panic, silent passthrough).
#[allow(clippy::expect_used)] // Reason: regex pattern is a compile-time constant guaranteed to be valid
pub(crate) fn expand_env_vars(content: &str) -> Result<String> {
    use std::sync::LazyLock;

    static ENV_VAR_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("env var regex is valid")
    });

    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for cap in ENV_VAR_REGEX.captures_iter(content) {
        // INVARIANT: Regex captures iterator yields Captures where group 0 (the full match) is
        // always present
        let m = cap.get(0).expect("INVARIANT: Regex captures group 0 is always present");
        result.push_str(&content[last_end..m.start()]);
        let var_name = &cap[1];
        match std::env::var(var_name) {
            Ok(val) => {
                validate_env_var_value(var_name, &val)?;
                result.push_str(&val);
            },
            Err(_) => {
                result.push_str(&format!("${{{}}}", var_name));
            },
        }
        last_end = m.end();
    }
    result.push_str(&content[last_end..]);
    Ok(result)
}

fn validate_env_var_value(var_name: &str, value: &str) -> Result<()> {
    if value.contains('\n') {
        anyhow::bail!("Environment variable {} contains newline character", var_name);
    }
    if value.contains('\r') {
        anyhow::bail!("Environment variable {} contains carriage return character", var_name);
    }
    if value.contains('\0') {
        anyhow::bail!("Environment variable {} contains null character", var_name);
    }
    // Check for unescaped TOML metacharacters: ", ', \, ], [, {, }
    if value.contains('"')
        || value.contains('\'')
        || value.contains('\\')
        || value.contains(']')
        || value.contains('[')
        || value.contains('{')
        || value.contains('}')
    {
        anyhow::bail!(
            "Environment variable {} contains TOML metacharacter that could break TOML parsing",
            var_name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
