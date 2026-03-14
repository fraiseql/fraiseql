//! Apollo Federation configuration types.

use serde::{Deserialize, Serialize};

/// Circuit breaker configuration for a specific federated database/service
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerDatabaseCircuitBreakerOverride {
    /// Database or service name matching a federation entity
    pub database:              String,
    /// Override: number of consecutive failures before opening (must be > 0)
    pub failure_threshold:     Option<u32>,
    /// Override: seconds to wait before attempting recovery (must be > 0)
    pub recovery_timeout_secs: Option<u64>,
    /// Override: successes required in half-open state to close the breaker (must be > 0)
    pub success_threshold:     Option<u32>,
}

/// Circuit breaker configuration for Apollo Federation fan-out requests
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FederationCircuitBreakerConfig {
    /// Enable circuit breaker protection on federation fan-out
    pub enabled:               bool,
    /// Consecutive failures before the breaker opens (default: 5, must be > 0)
    pub failure_threshold:     u32,
    /// Seconds to wait before attempting a probe request (default: 30, must be > 0)
    pub recovery_timeout_secs: u64,
    /// Probe successes needed to transition from half-open to closed (default: 2, must be > 0)
    pub success_threshold:     u32,
    /// Per-database overrides (database name must match a defined federation entity)
    pub per_database:          Vec<PerDatabaseCircuitBreakerOverride>,
}

impl Default for FederationCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled:               true,
            failure_threshold:     5,
            recovery_timeout_secs: 30,
            success_threshold:     2,
            per_database:          vec![],
        }
    }
}

/// Federation configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FederationConfig {
    /// Enable Apollo federation
    #[serde(default)]
    pub enabled:         bool,
    /// Apollo federation version
    pub apollo_version:  Option<u32>,
    /// Federated entities
    pub entities:        Vec<FederationEntity>,
    /// Circuit breaker configuration for federation fan-out requests
    pub circuit_breaker: Option<FederationCircuitBreakerConfig>,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled:         false,
            apollo_version:  Some(2),
            entities:        vec![],
            circuit_breaker: None,
        }
    }
}

/// Federation entity
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FederationEntity {
    /// Entity name
    pub name:       String,
    /// Key fields for entity resolution
    pub key_fields: Vec<String>,
}
