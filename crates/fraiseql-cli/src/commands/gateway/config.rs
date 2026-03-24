//! Gateway configuration types
//!
//! Defines the TOML configuration format for `fraiseql federation gateway`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Maximum number of subgraphs a single gateway can route to.
const MAX_SUBGRAPHS: usize = 64;

/// Maximum total request timeout (5 minutes).
const MAX_TOTAL_REQUEST_MS: u64 = 300_000;

/// Gateway configuration loaded from TOML.
///
/// # Example
///
/// ```toml
/// [gateway]
/// listen = "0.0.0.0:4000"
/// playground = true
///
/// [gateway.subgraphs.users]
/// url = "http://localhost:4001/graphql"
/// schema = "./schemas/users.graphql"
///
/// [gateway.timeouts]
/// subgraph_request_ms = 5000
/// total_request_ms = 30000
///
/// [gateway.circuit_breaker]
/// failure_threshold = 5
/// recovery_timeout_ms = 30000
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfigFile {
    /// Top-level gateway section
    pub gateway: GatewayConfig,
}

/// Core gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Address to bind the gateway server (e.g., `"0.0.0.0:4000"`).
    #[serde(default = "default_listen")]
    pub listen: String,

    /// Enable the GraphQL playground UI.
    #[serde(default)]
    pub playground: bool,

    /// Named subgraph definitions.
    #[serde(default)]
    pub subgraphs: HashMap<String, SubgraphConfig>,

    /// Request timeout settings.
    #[serde(default)]
    pub timeouts: TimeoutConfig,

    /// Circuit breaker settings.
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
}

/// Configuration for a single subgraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphConfig {
    /// HTTP URL of the subgraph's GraphQL endpoint.
    pub url: String,

    /// Optional path to a local SDL file. If absent, the gateway fetches SDL
    /// from the subgraph via the `_service` introspection query at startup.
    pub schema: Option<PathBuf>,
}

/// Timeout configuration for gateway requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Timeout per individual subgraph request (milliseconds).
    #[serde(default = "default_subgraph_timeout")]
    pub subgraph_request_ms: u64,

    /// Total timeout for the entire gateway request (milliseconds).
    #[serde(default = "default_total_timeout")]
    pub total_request_ms: u64,
}

/// Circuit breaker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before the circuit opens.
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Time to wait before attempting recovery (milliseconds).
    #[serde(default = "default_recovery_timeout")]
    pub recovery_timeout_ms: u64,
}

fn default_listen() -> String {
    "127.0.0.1:4000".to_string()
}

const fn default_subgraph_timeout() -> u64 {
    5_000
}

const fn default_total_timeout() -> u64 {
    30_000
}

const fn default_failure_threshold() -> u32 {
    5
}

const fn default_recovery_timeout() -> u64 {
    30_000
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            subgraph_request_ms: default_subgraph_timeout(),
            total_request_ms:    default_total_timeout(),
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold:   default_failure_threshold(),
            recovery_timeout_ms: default_recovery_timeout(),
        }
    }
}

/// Validation errors for gateway configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// No subgraphs defined.
    NoSubgraphs,
    /// Too many subgraphs.
    TooManySubgraphs {
        /// Number of subgraphs found.
        count: usize,
        /// Maximum allowed.
        max:   usize,
    },
    /// Invalid subgraph URL.
    InvalidUrl {
        /// Subgraph name.
        name:   String,
        /// The invalid URL.
        url:    String,
        /// Parse error reason.
        reason: String,
    },
    /// Schema file does not exist.
    SchemaFileNotFound {
        /// Subgraph name.
        name: String,
        /// Path that was not found.
        path: PathBuf,
    },
    /// Total timeout is less than subgraph timeout.
    TotalTimeoutTooSmall,
    /// Total timeout exceeds maximum.
    TotalTimeoutTooLarge {
        /// Configured value.
        ms:  u64,
        /// Maximum allowed.
        max: u64,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSubgraphs => write!(f, "No subgraphs defined in [gateway.subgraphs]"),
            Self::TooManySubgraphs { count, max } => {
                write!(f, "Too many subgraphs: {count} (max {max})")
            },
            Self::InvalidUrl { name, url, reason } => {
                write!(f, "Subgraph '{name}' has invalid URL '{url}': {reason}")
            },
            Self::SchemaFileNotFound { name, path } => {
                write!(f, "Subgraph '{name}' schema file not found: {}", path.display())
            },
            Self::TotalTimeoutTooSmall => {
                write!(f, "total_request_ms must be >= subgraph_request_ms")
            },
            Self::TotalTimeoutTooLarge { ms, max } => {
                write!(f, "total_request_ms ({ms}ms) exceeds maximum ({max}ms)")
            },
        }
    }
}

impl std::error::Error for ConfigError {}

/// Load and parse the gateway configuration from a TOML file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the TOML is malformed.
pub fn load_config(path: &Path) -> anyhow::Result<GatewayConfig> {
    let content = std::fs::read_to_string(path)?;
    let file: GatewayConfigFile = toml::from_str(&content)?;
    Ok(file.gateway)
}

/// Validate the gateway configuration.
///
/// # Errors
///
/// Returns a vector of all validation errors found.
pub fn validate_config(config: &GatewayConfig, base_dir: &Path) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    // Must have at least one subgraph
    if config.subgraphs.is_empty() {
        errors.push(ConfigError::NoSubgraphs);
    }

    // Cap subgraph count
    if config.subgraphs.len() > MAX_SUBGRAPHS {
        errors.push(ConfigError::TooManySubgraphs {
            count: config.subgraphs.len(),
            max:   MAX_SUBGRAPHS,
        });
    }

    // Validate each subgraph
    for (name, sg) in &config.subgraphs {
        // URL must be parseable
        if let Err(e) = url::Url::parse(&sg.url) {
            errors.push(ConfigError::InvalidUrl {
                name:   name.clone(),
                url:    sg.url.clone(),
                reason: e.to_string(),
            });
        }

        // Schema file, if specified, must exist
        if let Some(schema_path) = &sg.schema {
            let resolved = if schema_path.is_relative() {
                base_dir.join(schema_path)
            } else {
                schema_path.clone()
            };
            if !resolved.exists() {
                errors.push(ConfigError::SchemaFileNotFound {
                    name: name.clone(),
                    path: resolved,
                });
            }
        }
    }

    // Timeout sanity
    if config.timeouts.total_request_ms < config.timeouts.subgraph_request_ms {
        errors.push(ConfigError::TotalTimeoutTooSmall);
    }
    if config.timeouts.total_request_ms > MAX_TOTAL_REQUEST_MS {
        errors.push(ConfigError::TotalTimeoutTooLarge {
            ms:  config.timeouts.total_request_ms,
            max: MAX_TOTAL_REQUEST_MS,
        });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_minimal_config() {
        let toml_str = r#"
[gateway]
listen = "0.0.0.0:4000"

[gateway.subgraphs.users]
url = "http://localhost:4001/graphql"
"#;
        let file: GatewayConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(file.gateway.listen, "0.0.0.0:4000");
        assert_eq!(file.gateway.subgraphs.len(), 1);
        assert!(file.gateway.subgraphs.contains_key("users"));
    }

    #[test]
    fn test_deserialize_full_config() {
        let toml_str = r#"
[gateway]
listen = "0.0.0.0:4000"
playground = true

[gateway.subgraphs.users]
url = "http://localhost:4001/graphql"
schema = "./schemas/users.graphql"

[gateway.subgraphs.products]
url = "http://localhost:4002/graphql"

[gateway.timeouts]
subgraph_request_ms = 3000
total_request_ms = 15000

[gateway.circuit_breaker]
failure_threshold = 10
recovery_timeout_ms = 60000
"#;
        let file: GatewayConfigFile = toml::from_str(toml_str).unwrap();
        let gw = &file.gateway;

        assert!(gw.playground);
        assert_eq!(gw.subgraphs.len(), 2);
        assert_eq!(gw.timeouts.subgraph_request_ms, 3000);
        assert_eq!(gw.timeouts.total_request_ms, 15000);
        assert_eq!(gw.circuit_breaker.failure_threshold, 10);
        assert_eq!(gw.circuit_breaker.recovery_timeout_ms, 60000);

        let users = &gw.subgraphs["users"];
        assert_eq!(users.url, "http://localhost:4001/graphql");
        assert_eq!(users.schema.as_deref(), Some(Path::new("./schemas/users.graphql")));
    }

    #[test]
    fn test_defaults() {
        let toml_str = r#"
[gateway]

[gateway.subgraphs.svc]
url = "http://localhost:4001/graphql"
"#;
        let file: GatewayConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(file.gateway.listen, "127.0.0.1:4000");
        assert!(!file.gateway.playground);
        assert_eq!(file.gateway.timeouts.subgraph_request_ms, 5000);
        assert_eq!(file.gateway.timeouts.total_request_ms, 30000);
        assert_eq!(file.gateway.circuit_breaker.failure_threshold, 5);
    }

    #[test]
    fn test_validate_no_subgraphs() {
        let config = GatewayConfig {
            listen:          "127.0.0.1:4000".to_string(),
            playground:      false,
            subgraphs:       HashMap::new(),
            timeouts:        TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::NoSubgraphs)));
    }

    #[test]
    fn test_validate_invalid_url() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "bad".to_string(),
            SubgraphConfig {
                url:    "not a url".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::InvalidUrl { .. })));
    }

    #[test]
    fn test_validate_timeout_sanity() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "svc".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig {
                subgraph_request_ms: 10_000,
                total_request_ms:    5_000,
            },
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::TotalTimeoutTooSmall)));
    }

    #[test]
    fn test_validate_total_timeout_too_large() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "svc".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig {
                subgraph_request_ms: 5_000,
                total_request_ms:    999_999,
            },
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::TotalTimeoutTooLarge { .. })));
    }

    #[test]
    fn test_validate_valid_config() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "users".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: None,
            },
        );
        subgraphs.insert(
            "products".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4002/graphql".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "0.0.0.0:4000".to_string(),
            playground: true,
            subgraphs,
            timeouts: TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        assert!(validate_config(&config, Path::new(".")).is_ok());
    }

    #[test]
    fn test_validate_schema_file_not_found() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "svc".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: Some(PathBuf::from("./nonexistent.graphql")),
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::SchemaFileNotFound { .. })));
    }
}
