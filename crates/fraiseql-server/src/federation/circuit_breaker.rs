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
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Prometheus gauge value: circuit is closed (normal operation).
pub const STATE_CLOSED: u64 = 0;
/// Prometheus gauge value: circuit is open (rejecting requests).
pub const STATE_OPEN: u64 = 1;
/// Prometheus gauge value: circuit is half-open (probing recovery).
pub const STATE_HALF_OPEN: u64 = 2;

/// Summary of circuit state for health reporting.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitHealthState {
    /// Accepting requests normally.
    Closed,
    /// Rejecting requests; recovery probe pending.
    Open,
    /// Probe request in flight; evaluating recovery.
    HalfOpen,
}

/// Circuit health snapshot for a single federation entity type.
#[derive(Debug, Clone, Serialize)]
pub struct SubgraphCircuitHealth {
    /// Entity type name as defined in the compiled schema.
    pub subgraph: String,
    /// Current circuit state.
    pub state: CircuitHealthState,
}

/// Internal circuit state stored behind a `Mutex`.
///
/// Consecutive failure counts are co-located with the state they guard to eliminate
/// the TOCTOU race that arises from a separate `AtomicU32` counter: the counter
/// increment and the state transition now happen atomically under one lock.
#[derive(Debug)]
enum CircuitState {
    /// Normal operation. Tracks consecutive failures toward the trip threshold.
    Closed { consecutive_failures: u32 },
    /// Circuit tripped; requests are rejected until `recovery_timeout` elapses.
    Open {
        opened_at:        Instant,
        recovery_timeout: Duration,
    },
    /// Recovery probe phase.
    ///
    /// `probe_in_flight` ensures exactly one probe request passes through at a time,
    /// preventing a thundering herd when the circuit first becomes eligible for recovery.
    HalfOpen {
        consecutive_failures: u32,
        probe_in_flight:      bool,
        successes:            u32,
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
    config: CircuitBreakerConfig,
    /// All mutable state — including the consecutive-failure counter — lives inside
    /// this single mutex so that counter increments and state transitions are atomic.
    state:  Mutex<CircuitState>,
}

impl EntityCircuitBreaker {
    const fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Mutex::new(CircuitState::Closed {
                consecutive_failures: 0,
            }),
        }
    }

    /// Returns `Some(retry_after_secs)` if the request should be rejected, `None` to allow it.
    ///
    /// Transitions:
    /// - `Open` → `HalfOpen` when `recovery_timeout` has elapsed; this first call
    ///   becomes the recovery probe (sets `probe_in_flight = true`).
    /// - `HalfOpen`: allows exactly one in-flight probe; subsequent calls are rejected
    ///   until the probe outcome is recorded via [`record_success`] or [`record_failure`].
    fn check(&self) -> Option<u64> {
        let mut state = self.state.lock();

        // Read-only phase: decide what action to take (or return early).
        // The immutable borrow of `*state` ends when this match completes.
        match &*state {
            CircuitState::Closed { .. } => return None,
            CircuitState::Open {
                opened_at,
                recovery_timeout,
            } => {
                if opened_at.elapsed() < *recovery_timeout {
                    return Some(self.config.recovery_timeout_secs);
                }
                // Timeout elapsed: fall through to transition Open → HalfOpen.
            },
            CircuitState::HalfOpen {
                probe_in_flight: true,
                ..
            } => {
                return Some(self.config.recovery_timeout_secs);
            },
            CircuitState::HalfOpen {
                probe_in_flight: false,
                ..
            } => {
                // No probe in flight: fall through to mark this call as the probe.
            },
        }

        // Write phase: apply the state mutation.
        // The prior immutable borrow has ended under NLL, so mutable access is valid.
        match &mut *state {
            CircuitState::Open { .. } => {
                // Recovery timeout elapsed: transition to HalfOpen.
                // This call becomes the first (and only) probe.
                *state = CircuitState::HalfOpen {
                    consecutive_failures: 0,
                    probe_in_flight:      true,
                    successes:            0,
                };
            },
            CircuitState::HalfOpen { probe_in_flight, .. } => {
                // Allow this call through as the sole probe.
                *probe_in_flight = true;
            },
            CircuitState::Closed { .. } => {
                // Unreachable: Closed returns early in the read phase.
            },
        }

        None
    }

    /// Record a successful entity resolution.
    ///
    /// In `HalfOpen`, increments the success counter, clears `probe_in_flight` so the
    /// next probe can be issued, and closes the circuit when the threshold is reached.
    fn record_success(&self) {
        let mut state = self.state.lock();

        // Read current successes (HalfOpen only); return early otherwise.
        let new_successes = match &*state {
            CircuitState::HalfOpen { successes, .. } => *successes + 1,
            CircuitState::Closed { .. } | CircuitState::Open { .. } => return,
        };

        if new_successes >= self.config.success_threshold {
            *state = CircuitState::Closed {
                consecutive_failures: 0,
            };
            info!("Federation circuit breaker closed after successful recovery");
        } else if let CircuitState::HalfOpen {
            successes,
            probe_in_flight,
            ..
        } = &mut *state
        {
            *successes = new_successes;
            // Clear probe_in_flight so a new probe can be issued.
            *probe_in_flight = false;
        }
    }

    /// Record a failed entity resolution.
    ///
    /// Opens the circuit when `failure_threshold` consecutive failures have occurred.
    /// Works from both `Closed` and `HalfOpen` states; the counter resets to zero on
    /// the next successful recovery so `HalfOpen` re-trips cleanly.
    fn record_failure(&self) {
        let mut state = self.state.lock();

        // Read new failure count; short-circuit if already Open.
        let new_count = match &*state {
            CircuitState::Open { .. } => return,
            CircuitState::Closed { consecutive_failures }
            | CircuitState::HalfOpen {
                consecutive_failures,
                ..
            } => *consecutive_failures + 1,
        };

        if new_count >= self.config.failure_threshold {
            let from_half_open = matches!(*state, CircuitState::HalfOpen { .. });
            *state = CircuitState::Open {
                opened_at:        Instant::now(),
                recovery_timeout: Duration::from_secs(self.config.recovery_timeout_secs),
            };
            if from_half_open {
                info!(
                    consecutive_failures = new_count,
                    recovery_timeout_secs = self.config.recovery_timeout_secs,
                    "Federation circuit breaker re-opened from HalfOpen"
                );
            } else {
                info!(
                    consecutive_failures = new_count,
                    recovery_timeout_secs = self.config.recovery_timeout_secs,
                    "Federation circuit breaker opened"
                );
            }
        } else {
            match &mut *state {
                CircuitState::Closed { consecutive_failures } => {
                    *consecutive_failures = new_count;
                },
                CircuitState::HalfOpen {
                    consecutive_failures,
                    probe_in_flight,
                    ..
                } => {
                    *consecutive_failures = new_count;
                    // Clear probe flag so a new probe can be issued after recording this failure.
                    *probe_in_flight = false;
                },
                CircuitState::Open { .. } => {
                    // Unreachable: we returned early above.
                },
            }
        }
    }

    /// Returns the numeric state code for Prometheus export.
    ///
    /// `0` = Closed, `1` = Open, `2` = HalfOpen.
    fn state_code(&self) -> u64 {
        let state = self.state.lock();
        match &*state {
            CircuitState::Closed { .. } => STATE_CLOSED,
            CircuitState::Open { .. } => STATE_OPEN,
            CircuitState::HalfOpen { .. } => STATE_HALF_OPEN,
        }
    }

    /// Returns the typed health state for the `/health` endpoint.
    fn state_for_health(&self) -> CircuitHealthState {
        let state = self.state.lock();
        match &*state {
            CircuitState::Closed { .. } => CircuitHealthState::Closed,
            CircuitState::Open { .. } => CircuitHealthState::Open,
            CircuitState::HalfOpen { .. } => CircuitHealthState::HalfOpen,
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
    /// Per-entity overrides. Also accepts the legacy key `per_database`.
    #[serde(default, alias = "per_database")]
    per_entity:            Vec<PerEntityJson>,
}

#[derive(Deserialize, Debug)]
struct PerEntityJson {
    /// Entity type name (GraphQL `__typename`). Also accepts the legacy key `database`.
    #[serde(alias = "database")]
    entity_type:           String,
    failure_threshold:     Option<u32>,
    recovery_timeout_secs: Option<u64>,
    success_threshold:     Option<u32>,
}

// ────────────────────────────────────────────────────────────────────────────
// Public manager
// ────────────────────────────────────────────────────────────────────────────

/// Manages one circuit breaker per federation entity type.
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

    /// Construct a manager from a typed [`fraiseql_core::schema::FederationConfig`].
    ///
    /// Returns `None` when `circuit_breaker` is absent or `enabled` is `false`.
    #[must_use]
    pub fn from_config(fed: &fraiseql_core::schema::FederationConfig) -> Option<Arc<Self>> {
        let cb = fed.circuit_breaker.as_ref()?;
        if !cb.enabled {
            return None;
        }
        let default_config = CircuitBreakerConfig {
            failure_threshold:     cb.failure_threshold,
            recovery_timeout_secs: cb.recovery_timeout_secs,
            success_threshold:     cb.success_threshold,
        };
        let manager = Arc::new(Self::new(default_config));
        for override_entry in &cb.per_entity {
            let entity_config = CircuitBreakerConfig {
                failure_threshold:     override_entry
                    .failure_threshold
                    .unwrap_or(manager.default_config.failure_threshold),
                recovery_timeout_secs: override_entry
                    .recovery_timeout
                    .unwrap_or(manager.default_config.recovery_timeout_secs),
                success_threshold:     override_entry
                    .success_threshold
                    .unwrap_or(manager.default_config.success_threshold),
            };
            manager.per_entity_config.insert(override_entry.entity.clone(), entity_config);
        }
        let override_keys: Vec<String> =
            manager.per_entity_config.iter().map(|r| r.key().clone()).collect();
        for entity_type in override_keys {
            manager.get_or_create(&entity_type);
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

    /// Construct a manager from the `federation` JSON blob embedded in the compiled schema.
    ///
    /// Returns `None` when the circuit breaker section is absent or `enabled` is `false`.
    ///
    /// When the section is present but malformed (e.g. wrong field types), a `warn`-level
    /// diagnostic is emitted and `None` is returned instead of silently disabling the
    /// feature without explanation.
    #[must_use]
    pub fn from_schema_json(federation_json: &serde_json::Value) -> Option<Arc<Self>> {
        let cb_json: CircuitBreakerJson = match federation_json.get("circuit_breaker") {
            None => return None,
            Some(v) => match serde_json::from_value(v.clone()) {
                Ok(j) => j,
                Err(e) => {
                    warn!(
                        error = %e,
                        "circuit_breaker config present but malformed — circuit breaker disabled"
                    );
                    return None;
                },
            },
        };

        if !cb_json.enabled {
            return None;
        }

        let default_config = CircuitBreakerConfig {
            failure_threshold:     cb_json.failure_threshold.unwrap_or(5),
            recovery_timeout_secs: cb_json.recovery_timeout_secs.unwrap_or(30),
            success_threshold:     cb_json.success_threshold.unwrap_or(2),
        };

        let manager = Arc::new(Self::new(default_config));

        for override_entry in cb_json.per_entity {
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
            manager
                .per_entity_config
                .insert(override_entry.entity_type, entity_config);
        }

        // Pre-seed breakers for entities with per-entity overrides so they appear in
        // Prometheus metrics from startup rather than only after first traffic.
        let override_keys: Vec<String> = manager
            .per_entity_config
            .iter()
            .map(|r| r.key().clone())
            .collect();
        for entity_type in override_keys {
            manager.get_or_create(&entity_type);
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

    /// Returns a health snapshot: one entry per configured entity type.
    ///
    /// Used to populate the `federation.subgraphs` field in the `/health` response.
    #[must_use]
    pub fn health_snapshot(&self) -> Vec<SubgraphCircuitHealth> {
        self.breakers
            .iter()
            .map(|entry| SubgraphCircuitHealth {
                subgraph: entry.key().clone(),
                state:    entry.value().state_for_health(),
            })
            .collect()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helper: entity-type extraction from GraphQL variables
// ────────────────────────────────────────────────────────────────────────────

/// Extract unique `__typename` values from the `representations` variable.
///
/// The returned list is sorted for deterministic ordering. Representations that
/// are missing a `__typename` field are skipped with a `warn`-level diagnostic.
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

    let mut types = Vec::new();
    for rep in representations {
        if let Some(typename) = rep.get("__typename").and_then(|t| t.as_str()) {
            types.push(typename.to_string());
        } else {
            warn!(
                "Federation representation missing __typename field; entity skipped for circuit \
                 breaker"
            );
        }
    }
    types.sort_unstable();
    types.dedup();
    types
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    #[test]
    fn test_state_for_health_returns_closed_initially() {
        let breaker = EntityCircuitBreaker::new(CircuitBreakerConfig::default());
        assert!(matches!(breaker.state_for_health(), CircuitHealthState::Closed));
    }

    #[test]
    fn test_state_for_health_returns_open_after_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold:     1,
            recovery_timeout_secs: 3600,
            success_threshold:     2,
        };
        let breaker = EntityCircuitBreaker::new(config);
        breaker.record_failure();
        assert!(matches!(breaker.state_for_health(), CircuitHealthState::Open));
    }

    #[test]
    fn test_state_for_health_returns_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold:     1,
            recovery_timeout_secs: 0, // instant recovery for testing
            success_threshold:     5,
        };
        let breaker = EntityCircuitBreaker::new(config);
        breaker.record_failure();
        breaker.check(); // transitions Open → HalfOpen
        assert!(matches!(breaker.state_for_health(), CircuitHealthState::HalfOpen));
    }

    #[test]
    fn test_health_snapshot_returns_entries_for_all_breakers() {
        let json = serde_json::json!({
            "circuit_breaker": {
                "enabled": true,
                "failure_threshold": 1,
                "recovery_timeout_secs": 3600,
                "success_threshold": 2,
                "per_entity": [
                    { "entity_type": "Product", "failure_threshold": 1 },
                    { "entity_type": "User", "failure_threshold": 1 }
                ]
            }
        });
        let manager = FederationCircuitBreakerManager::from_schema_json(&json).unwrap();
        // Trip Product's circuit
        manager.record_failure("Product");

        let snapshot = manager.health_snapshot();
        assert_eq!(snapshot.len(), 2, "should have one entry per configured entity");

        let product = snapshot.iter().find(|s| s.subgraph == "Product").unwrap();
        assert!(matches!(product.state, CircuitHealthState::Open));

        let user = snapshot.iter().find(|s| s.subgraph == "User").unwrap();
        assert!(matches!(user.state, CircuitHealthState::Closed));
    }

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
    fn test_circuit_stays_open_before_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold:     1,
            recovery_timeout_secs: 3600, // very long timeout — should not auto-recover
            success_threshold:     2,
        };
        let breaker = EntityCircuitBreaker::new(config);

        breaker.record_failure();
        assert_eq!(breaker.check(), Some(3600));
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
    fn test_circuit_half_open_blocks_concurrent_probes() {
        let config = CircuitBreakerConfig {
            failure_threshold:     1,
            recovery_timeout_secs: 0,
            success_threshold:     5, // high threshold to stay in HalfOpen
        };
        let breaker = EntityCircuitBreaker::new(config);

        breaker.record_failure();
        // First check: Open → HalfOpen, allows the probe (probe_in_flight = true)
        assert!(breaker.check().is_none(), "first probe should be allowed");
        // Second check: probe_in_flight = true, must be rejected
        assert!(
            breaker.check().is_some(),
            "second concurrent probe should be rejected"
        );
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
    fn test_circuit_half_open_probe_cleared_after_success() {
        // After a successful probe, probe_in_flight is cleared so the next probe can proceed.
        let config = CircuitBreakerConfig {
            failure_threshold:     1,
            recovery_timeout_secs: 0,
            success_threshold:     3,
        };
        let breaker = EntityCircuitBreaker::new(config);

        breaker.record_failure();
        breaker.check(); // → HalfOpen, probe_in_flight = true
        assert!(breaker.check().is_some(), "second check should return backoff while probe is in flight"); // blocked: probe in flight

        breaker.record_success(); // successes=1, probe_in_flight = false
        assert!(breaker.check().is_none()); // second probe now allowed
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
        let types = extract_entity_types(Some(&vars));
        // Must be sorted and deduplicated
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
    fn test_extract_entity_types_missing_typename_skipped() {
        // Representations without __typename are skipped (a warning is emitted).
        let vars = serde_json::json!({
            "representations": [
                {"id": "1"},               // missing __typename
                {"__typename": "User", "id": "2"},
            ]
        });
        let types = extract_entity_types(Some(&vars));
        assert_eq!(types, vec!["User"]);
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
    fn test_manager_from_schema_json_malformed_config() {
        // failure_threshold must be a u32, not a string.
        // Should return None and emit a warning rather than panicking.
        let json = serde_json::json!({
            "circuit_breaker": {
                "enabled": true,
                "failure_threshold": "five"
            }
        });
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
    fn test_manager_from_schema_json_per_entity_new_key() {
        // The new canonical `per_entity` / `entity_type` keys should work.
        let json = serde_json::json!({
            "circuit_breaker": {
                "enabled": true,
                "per_entity": [
                    { "entity_type": "Product", "failure_threshold": 2 }
                ]
            }
        });
        let manager = FederationCircuitBreakerManager::from_schema_json(&json).unwrap();
        manager.record_failure("Product");
        manager.record_failure("Product");
        assert!(manager.check("Product").is_some());
    }

    #[test]
    fn test_manager_from_schema_json_per_entity_override() {
        // Legacy `per_database` / `database` keys must still work via serde alias.
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
    fn test_manager_pre_seeds_overridden_entities() {
        // Entities with per-entity overrides should appear in Prometheus metrics from
        // startup, before any traffic has been seen.
        let json = serde_json::json!({
            "circuit_breaker": {
                "enabled": true,
                "per_entity": [
                    { "entity_type": "Product", "failure_threshold": 2 }
                ]
            }
        });
        let manager = FederationCircuitBreakerManager::from_schema_json(&json).unwrap();
        let states = manager.collect_states();
        assert!(
            states.iter().any(|(e, _)| e == "Product"),
            "Product should be pre-seeded in the breakers map"
        );
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

    #[test]
    fn test_concurrent_failures_no_spurious_open() {
        use std::{sync::Arc as StdArc, thread};

        // With threshold=10, 8 concurrent failures must NOT trip the circuit.
        // The merged counter+state mutex ensures no TOCTOU race between the old
        // AtomicU32 counter and the separate state mutex.
        let config = CircuitBreakerConfig {
            failure_threshold:     10,
            recovery_timeout_secs: 60,
            success_threshold:     2,
        };
        let breaker = StdArc::new(EntityCircuitBreaker::new(config));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let b = StdArc::clone(&breaker);
                thread::spawn(move || b.record_failure())
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread panicked");
        }

        // 8 failures < threshold of 10: circuit must still be closed.
        assert!(
            breaker.check().is_none(),
            "circuit should remain closed after 8 < 10 failures"
        );
        assert_eq!(breaker.state_code(), STATE_CLOSED);
    }
}
