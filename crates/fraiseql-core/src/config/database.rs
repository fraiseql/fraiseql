//! Database connection configuration.

use serde::{Deserialize, Serialize};

/// Database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// `PostgreSQL` connection URL.
    pub url: String,

    /// Maximum connections in pool.
    pub max_connections: u32,

    /// Minimum connections to maintain.
    pub min_connections: u32,

    /// Connection timeout in seconds.
    pub connect_timeout_secs: u64,

    /// Query timeout in seconds.
    pub query_timeout_secs: u64,

    /// Idle timeout in seconds (0 = no timeout).
    pub idle_timeout_secs: u64,

    /// Enable SSL for database connections.
    pub ssl_mode: SslMode,

    /// Mutation timing configuration.
    pub mutation_timing: MutationTimingConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url:                  String::new(),
            max_connections:      10,
            min_connections:      1,
            connect_timeout_secs: 10,
            query_timeout_secs:   30,
            idle_timeout_secs:    600,
            ssl_mode:             SslMode::Prefer,
            mutation_timing:      MutationTimingConfig::default(),
        }
    }
}

/// Mutation timing configuration.
///
/// When enabled, the adapter injects `SET LOCAL <variable_name> = clock_timestamp()::text`
/// before each mutation function call, allowing SQL functions to compute their own
/// execution duration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationTimingConfig {
    /// Whether mutation timing is enabled.
    pub enabled: bool,

    /// The PostgreSQL session variable name to set.
    #[serde(default = "default_timing_variable")]
    pub variable_name: String,
}

fn default_timing_variable() -> String {
    "fraiseql.started_at".to_string()
}

impl Default for MutationTimingConfig {
    fn default() -> Self {
        Self {
            enabled:       false,
            variable_name: default_timing_variable(),
        }
    }
}

/// SSL mode for database connections.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SslMode {
    /// Disable SSL.
    Disable,
    /// Prefer SSL but allow non-SSL.
    #[default]
    Prefer,
    /// Require SSL.
    Require,
    /// Require SSL and verify CA.
    VerifyCa,
    /// Require SSL and verify full certificate.
    VerifyFull,
}
