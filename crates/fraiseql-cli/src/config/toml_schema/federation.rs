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
    /// Subgraph service name (surfaced in Apollo Studio and the subgraph listing)
    pub service_name:    Option<String>,
    /// Apollo Federation spec version string (e.g. `"v2"`).
    ///
    /// Preferred over [`apollo_version`](Self::apollo_version); when both are set,
    /// `version` wins. When neither is set, the runtime defaults to `"v2"`.
    pub version:         Option<String>,
    /// Subgraph SDL URL (exposed at `/__subgraph_schema`)
    pub schema_url:      Option<String>,
    /// Apollo federation major version (legacy integer form; `2` ⇒ `"v2"`).
    ///
    /// Retained for backward compatibility; prefer the [`version`](Self::version)
    /// string. Ignored when `version` is set.
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
            service_name:    None,
            version:         None,
            schema_url:      None,
            apollo_version:  Some(2),
            entities:        vec![],
            circuit_breaker: None,
        }
    }
}

impl FederationConfig {
    /// The effective Apollo Federation spec version string.
    ///
    /// Prefers the explicit [`version`](Self::version) string, falling back to the
    /// legacy integer [`apollo_version`](Self::apollo_version) (`2` ⇒ `"v2"`).
    /// `None` when neither is set, in which case the runtime defaults to `"v2"`.
    #[must_use]
    pub fn effective_version(&self) -> Option<String> {
        self.version.clone().or_else(|| self.apollo_version.map(|v| format!("v{v}")))
    }

    /// Lower this TOML-authored config into the compiled
    /// [`fraiseql_core::schema::FederationConfig`] the runtime consumes.
    ///
    /// The mapping is explicit (rather than a raw serde passthrough) so the TOML and
    /// compiled field names cannot silently diverge: in particular `apollo_version`
    /// becomes `version`, and per-entity circuit-breaker overrides authored as
    /// `per_database` become the runtime's `per_entity` list.
    #[must_use]
    pub fn to_compiled(&self) -> fraiseql_core::schema::FederationConfig {
        use fraiseql_core::schema as core;

        core::FederationConfig {
            enabled:         self.enabled,
            version:         self.effective_version(),
            service_name:    self.service_name.clone(),
            schema_url:      self.schema_url.clone(),
            shareable_types: Vec::new(),
            entities:        self
                .entities
                .iter()
                .map(|e| core::FederationEntity {
                    name: e.name.clone(),
                    key_fields: e.key_fields.clone(),
                    ..Default::default()
                })
                .collect(),
            circuit_breaker: self.circuit_breaker.as_ref().map(|cb| core::CircuitBreakerConfig {
                enabled:               cb.enabled,
                failure_threshold:     cb.failure_threshold,
                recovery_timeout_secs: cb.recovery_timeout_secs,
                success_threshold:     cb.success_threshold,
                per_entity:            cb
                    .per_database
                    .iter()
                    .map(|o| core::EntityCircuitBreakerOverride {
                        entity:            o.database.clone(),
                        failure_threshold: o.failure_threshold,
                        recovery_timeout:  o.recovery_timeout_secs,
                        success_threshold: o.success_threshold,
                    })
                    .collect(),
            }),
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
