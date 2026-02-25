//! Per-entity federation circuit breaker.
//!
//! Implements a count-based circuit breaker that protects federation entity
//! resolution from cascading failures. Supports three states:
//!
//! - **Closed**: Normal operation; all requests pass through.
//! - **Open**: Circuit tripped after consecutive failures; requests rejected with HTTP 503.
//! - **HalfOpen**: Recovery probe phase; a threshold of successes closes the circuit.
//!
//! The manager is initialized from the `federation.circuit_breaker` section of the
//! compiled schema JSON and holds one independent breaker per entity type name.

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use dashmap::DashMap;
use serde::Deserialize;
use tracing::info;

/// Prometheus gauge value: circuit is closed (normal operation).
pub const STATE_CLOSED: u64 = 0;
/// Prometheus gauge value: circuit is open (rejecting requests).
pub const STATE_OPEN: u64 = 1;
/// Prometheus gauge value: circuit is half-open (probing recovery).
pub const STATE_HALF_OPEN: u64 = 2;

/// Internal circuit state stored behind a `Mutex`.
#[derive(Debug)]
enum CircuitState {
    Closed,
    Open {
        opened_at:        Instant,
        recovery_timeout: Duration,
    },
    HalfOpen {
        successes: u32,
    },
}

/// Configuration for a single circuit breaker instance.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Consecutive failures required to trip the circuit open.
    pub failure_threshold:     u32,
    /// Seconds to hold the circuit open before transitioning to HalfOpen.
    pub recovery_timeout_secs: u64,
    /// Consecutive successes in HalfOpen required to close the circuit.
    pub success_threshold:     u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold:     5,
            recovery_timeout_secs: 30,
            success_threshold:     2,
        }
    }
}

/// Circuit breaker instance for a single federation entity type.
struct EntityCircuitBreaker {
    config:               CircuitBreakerConfig,
    state:                Mutex<CircuitState>,
    consecutive_failures: AtomicU32,
}

impl EntityCircuitBreaker {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Mutex::new(CircuitState::Closed),
            consecutive_failures: AtomicU32::new(0),
        }
    }

    /// Returns `Some(retry_after_secs)` if the request should be rejected, `None` to allow it.
    ///
    /// A transition from Open → HalfOpen occurs automatically when the recovery
    /// timeout has elapsed.
    fn check(&self) -> Option<u64> {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| {
            tracing::error!("Circuit breaker state mutex poisoned; recovering with current state");
            poisoned.into_inner()
        });
        match *state {
            CircuitState::Closed => None,
            CircuitState::Open {
                opened_at,
                recovery_timeout,
            } => {
                if opened_at.elapsed() >= recovery_timeout {
                    *state = CircuitState::HalfOpen { successes: 0 };
                    None
                } else {
                    Some(self.config.recovery_timeout_secs)
                }
            },
            CircuitState::HalfOpen { .. } => None,
        }
    }

    /// Record a successful entity resolution.
    fn record_success(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| {
            tracing::error!("Circuit breaker state mutex poisoned; recovering with current state");
            poisoned.into_inner()
        });
        match *state {
            CircuitState::HalfOpen { successes } => {
                let new_successes = successes + 1;
                if new_successes >= self.config.success_threshold {
                    *state = CircuitState::Closed;
                    self.consecutive_failures.store(0, Ordering::Relaxed);
                    info!("Federation circuit breaker closed after successful recovery");
                } else {
                    *state = CircuitState::HalfOpen {
                        successes: new_successes,
                    };
                }
            },
            CircuitState::Closed => {
                self.consecutive_failures.store(0, Ordering::Relaxed);
            },
            CircuitState::Open { .. } => {},
        }
    }

    /// Record a failed entity resolution.
    ///
    /// Opens the circuit when `failure_threshold` consecutive failures have occurred.
    fn record_failure(&self) {
        let consecutive = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if consecutive >= self.config.failure_threshold {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| {
                tracing::error!(
                    "Circuit breaker state mutex poisoned; recovering with current state"
                );
                poisoned.into_inner()
            });
            if matches!(*state, CircuitState::Closed | CircuitState::HalfOpen { .. }) {
                *state = CircuitState::Open {
                    opened_at:        Instant::now(),
                    recovery_timeout: Duration::from_secs(self.config.recovery_timeout_secs),
                };
                info!(
                    consecutive_failures = consecutive,
                    recovery_timeout_secs = self.config.recovery_timeout_secs,
                    "Federation circuit breaker opened"
                );
            }
        }
    }

    /// Returns the numeric state code for Prometheus export.
    ///
    /// `0` = Closed, `1` = Open, `2` = HalfOpen.
    fn state_code(&self) -> u64 {
        let state = self.state.lock().unwrap_or_else(|poisoned| {
            tracing::error!("Circuit breaker state mutex poisoned; recovering with current state");
            poisoned.into_inner()
        });
        match *state {
            CircuitState::Closed => STATE_CLOSED,
            CircuitState::Open { .. } => STATE_OPEN,
            CircuitState::HalfOpen { .. } => STATE_HALF_OPEN,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// JSON deserialization helpers (reads from compiled schema's `federation` blob)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct CircuitBreakerJson {
    #[serde(default)]
    enabled:               bool,
    failure_threshold:     Option<u32>,
    recovery_timeout_secs: Option<u64>,
    success_threshold:     Option<u32>,
    #[serde(default)]
    per_database:          Vec<PerEntityJson>,
}

#[derive(Deserialize, Debug)]
struct PerEntityJson {
    database:              String,
    failure_threshold:     Option<u32>,
    recovery_timeout_secs: Option<u64>,
    success_threshold:     Option<u32>,
}

// ────────────────────────────────────────────────────────────────────────────
// Public manager
// ────────────────────────────────────────────────────────────────────────────

/// Manages one [`EntityCircuitBreaker`] per federation entity type.
///
/// Instantiated from the compiled schema JSON and shared via `Arc` across
/// request handlers and the metrics endpoint.
pub struct FederationCircuitBreakerManager {
    breakers:          DashMap<String, Arc<EntityCircuitBreaker>>,
    default_config:    CircuitBreakerConfig,
    per_entity_config: DashMap<String, CircuitBreakerConfig>,
}

impl FederationCircuitBreakerManager {
    fn new(default_config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: DashMap::new(),
            default_config,
            per_entity_config: DashMap::new(),
        }
    }

    /// Construct a manager from the `federation` JSON blob embedded in the compiled schema.
    ///
    /// Returns `None` when the circuit breaker section is absent or `enabled` is `false`.
    #[must_use]
    pub fn from_schema_json(federation_json: &serde_json::Value) -> Option<Arc<Self>> {
        let cb_json: CircuitBreakerJson = federation_json
            .get("circuit_breaker")
            .and_then(|v| serde_json::from_value(v.clone()).ok())?;

        if !cb_json.enabled {
            return None;
        }

        let default_config = CircuitBreakerConfig {
            failure_threshold:     cb_json.failure_threshold.unwrap_or(5),
            recovery_timeout_secs: cb_json.recovery_timeout_secs.unwrap_or(30),
            success_threshold:     cb_json.success_threshold.unwrap_or(2),
        };

        let manager = Arc::new(Self::new(default_config));

        for override_entry in cb_json.per_database {
            let entity_config = CircuitBreakerConfig {
                failure_threshold:     override_entry
                    .failure_threshold
                    .unwrap_or(manager.default_config.failure_threshold),
                recovery_timeout_secs: override_entry
                    .recovery_timeout_secs
                    .unwrap_or(manager.default_config.recovery_timeout_secs),
                success_threshold:     override_entry
                    .success_threshold
                    .unwrap_or(manager.default_config.success_threshold),
            };
            manager.per_entity_config.insert(override_entry.database, entity_config);
        }

        info!(
            failure_threshold = manager.default_config.failure_threshold,
            recovery_timeout_secs = manager.default_config.recovery_timeout_secs,
            success_threshold = manager.default_config.success_threshold,
            per_entity_overrides = manager.per_entity_config.len(),
            "Federation circuit breaker initialized"
        );

        Some(manager)
    }

    fn get_or_create(&self, entity: &str) -> Arc<EntityCircuitBreaker> {
        self.breakers
            .entry(entity.to_string())
            .or_insert_with(|| {
                let config = self
                    .per_entity_config
                    .get(entity)
                    .map(|r| r.value().clone())
                    .unwrap_or_else(|| self.default_config.clone());
                Arc::new(EntityCircuitBreaker::new(config))
            })
            .clone()
    }

    /// Check whether the circuit is open for the given entity type.
    ///
    /// Returns `Some(retry_after_secs)` to reject the request, or `None` to allow it.
    pub fn check(&self, entity: &str) -> Option<u64> {
        self.get_or_create(entity).check()
    }

    /// Record a successful entity resolution for the given entity type.
    pub fn record_success(&self, entity: &str) {
        self.get_or_create(entity).record_success();
    }

    /// Record a failed entity resolution for the given entity type.
    pub fn record_failure(&self, entity: &str) {
        self.get_or_create(entity).record_failure();
    }

    /// Collect `(entity_name, state_code)` pairs for Prometheus export.
    ///
    /// State codes: `0` = Closed, `1` = Open, `2` = HalfOpen.
    #[must_use]
    pub fn collect_states(&self) -> Vec<(String, u64)> {
        self.breakers
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().state_code()))
            .collect()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helper: entity-type extraction from GraphQL variables
// ────────────────────────────────────────────────────────────────────────────

/// Extract unique `__typename` values from the `representations` variable.
///
/// Used to identify which entity types are being resolved in an `_entities` query
/// so the circuit breaker can be checked and recorded per entity.
#[must_use]
pub fn extract_entity_types(variables: Option<&serde_json::Value>) -> Vec<String> {
    let Some(vars) = variables else {
        return vec![];
    };
    let Some(representations) = vars.get("representations").and_then(|r| r.as_array()) else {
        return vec![];
    };

    let mut seen = HashSet::new();
    for rep in representations {
        if let Some(typename) = rep.get("__typename").and_then(|t| t.as_str()) {
            seen.insert(typename.to_string());
        }
    }
    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_starts_closed() {
        let breaker = EntityCircuitBreaker::new(CircuitBreakerConfig::default());
        assert!(breaker.check().is_none());
        assert_eq!(breaker.state_code(), STATE_CLOSED);
    }

    #[test]
    fn test_circuit_opens_after_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold:     3,
            recovery_timeout_secs: 60,
            success_threshold:     2,
        };
        let breaker = EntityCircuitBreaker::new(config);

        breaker.record_failure();
        assert!(breaker.check().is_none()); // still closed

        breaker.record_failure();
        assert!(breaker.check().is_none()); // still closed

        breaker.record_failure();
        // Circuit is now open
        assert_eq!(breaker.check(), Some(60));
        assert_eq!(breaker.state_code(), STATE_OPEN);
    }

    #[test]
    fn test_circuit_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold:     1,
            recovery_timeout_secs: 0, // instant recovery for testing
            success_threshold:     2,
        };
        let breaker = EntityCircuitBreaker::new(config);

        breaker.record_failure();
        // With recovery_timeout = 0, check() transitions from Open → HalfOpen
        assert!(breaker.check().is_none());
        assert_eq!(breaker.state_code(), STATE_HALF_OPEN);
    }

    #[test]
    fn test_circuit_closes_after_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold:     1,
            recovery_timeout_secs: 0,
            success_threshold:     2,
        };
        let breaker = EntityCircuitBreaker::new(config);

        breaker.record_failure();
        breaker.check(); // → HalfOpen
        assert_eq!(breaker.state_code(), STATE_HALF_OPEN);

        breaker.record_success();
        assert_eq!(breaker.state_code(), STATE_HALF_OPEN); // still needs one more

        breaker.record_success();
        assert_eq!(breaker.state_code(), STATE_CLOSED); // fully recovered
    }

    #[test]
    fn test_extract_entity_types_from_representations() {
        let vars = serde_json::json!({
            "representations": [
                {"__typename": "Product", "id": "1"},
                {"__typename": "User", "id": "2"},
                {"__typename": "Product", "id": "3"},
            ]
        });
        let mut types = extract_entity_types(Some(&vars));
        types.sort();
        assert_eq!(types, vec!["Product", "User"]);
    }

    #[test]
    fn test_extract_entity_types_missing_representations() {
        let vars = serde_json::json!({ "other": "data" });
        assert!(extract_entity_types(Some(&vars)).is_empty());
    }

    #[test]
    fn test_extract_entity_types_no_variables() {
        assert!(extract_entity_types(None).is_empty());
    }

    #[test]
    fn test_manager_from_schema_json_disabled() {
        let json = serde_json::json!({ "circuit_breaker": { "enabled": false } });
        assert!(FederationCircuitBreakerManager::from_schema_json(&json).is_none());
    }

    #[test]
    fn test_manager_from_schema_json_missing_section() {
        let json = serde_json::json!({ "enabled": true, "entities": [] });
        assert!(FederationCircuitBreakerManager::from_schema_json(&json).is_none());
    }

    #[test]
    fn test_manager_from_schema_json_enabled() {
        let json = serde_json::json!({
            "circuit_breaker": {
                "enabled": true,
                "failure_threshold": 3,
                "recovery_timeout_secs": 30,
                "success_threshold": 2,
                "per_database": []
            }
        });
        let manager = FederationCircuitBreakerManager::from_schema_json(&json).unwrap();
        assert_eq!(manager.default_config.failure_threshold, 3);
    }

    #[test]
    fn test_manager_from_schema_json_per_entity_override() {
        let json = serde_json::json!({
            "circuit_breaker": {
                "enabled": true,
                "failure_threshold": 5,
                "recovery_timeout_secs": 30,
                "success_threshold": 2,
                "per_database": [
                    {
                        "database": "Product",
                        "failure_threshold": 2
                    }
                ]
            }
        });
        let manager = FederationCircuitBreakerManager::from_schema_json(&json).unwrap();
        // Product has an override; check that its breaker uses failure_threshold = 2
        manager.record_failure("Product");
        manager.record_failure("Product");
        // 2 failures should open Product's circuit
        assert!(manager.check("Product").is_some());
        // User entity uses default (5 failures needed)
        manager.record_failure("User");
        assert!(manager.check("User").is_none());
    }

    #[test]
    fn test_manager_collect_states() {
        let json = serde_json::json!({
            "circuit_breaker": {
                "enabled": true,
                "failure_threshold": 1,
                "recovery_timeout_secs": 60,
                "success_threshold": 1,
                "per_database": []
            }
        });
        let manager = FederationCircuitBreakerManager::from_schema_json(&json).unwrap();
        manager.record_failure("Product");
        // Product circuit is now open
        let states = manager.collect_states();
        let product_state = states.iter().find(|(e, _)| e == "Product").map(|(_, s)| *s);
        assert_eq!(product_state, Some(STATE_OPEN));
    }
}
