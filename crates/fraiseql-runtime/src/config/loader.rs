use std::path::Path;
use std::env;

use crate::config::RuntimeConfig;
use crate::config::validation::ConfigValidator;
use fraiseql_error::ConfigError;

impl RuntimeConfig {
    /// Load configuration from file with full validation
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();

        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError {
                path: path.to_path_buf(),
                source: e
            })?;

        let config: RuntimeConfig = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError { source: e })?;

        // Run comprehensive validation
        let validation = ConfigValidator::new(&config).validate();
        let warnings = validation.into_result()?;

        // Log warnings
        for warning in warnings {
            tracing::warn!("Configuration warning: {}", warning);
        }

        Ok(config)
    }

    /// Load configuration from default locations
    pub fn load() -> Result<Self, ConfigError> {
        // Check FRAISEQL_CONFIG environment variable
        if let Ok(path) = env::var("FRAISEQL_CONFIG") {
            return Self::from_file(&path);
        }

        // Check current directory
        let local_config = Path::new("./fraiseql.toml");
        if local_config.exists() {
            return Self::from_file(local_config);
        }

        // Check user config directory
        if let Some(config_dir) = dirs::config_dir() {
            let user_config = config_dir.join("fraiseql/config.toml");
            if user_config.exists() {
                return Self::from_file(&user_config);
            }
        }

        Err(ConfigError::NotFound)
    }

    /// Load configuration with optional file path (CLI argument)
    pub fn load_with_path(path: Option<&Path>) -> Result<Self, ConfigError> {
        match path {
            Some(p) => Self::from_file(p),
            None => Self::load(),
        }
    }

    /// Validate configuration without loading env vars (for dry-run/testing)
    pub fn validate_syntax(content: &str) -> Result<(), ConfigError> {
        let _config: RuntimeConfig = toml::from_str(content)
            .map_err(|e| ConfigError::ParseError { source: e })?;
        Ok(())
    }
}
