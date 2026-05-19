//! Per-entity federation circuit breaker.
//!
//! Implements a count-based circuit breaker that protects federation entity
//! resolution from cascading failures. Supports three states:
//!
//! - **Closed**: Normal operation; all requests pass through.
//! - **Open**: Circuit tripped after consecutive failures; requests rejected with HTTP 503.
//! - **`HalfOpen`**: Recovery probe phase; a threshold of successes closes the circuit.
//!
//! The manager is initialized from the `federation.circuit_breaker` section of the
//! compiled schema JSON and holds one independent breaker per entity type name.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use parking_lot::Mutex;
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
#[non_exhaustive]
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
        opened_at: Instant,
        recovery_timeout: Duration,
    },
    /// Recovery probe phase.
    ///
    /// `probe_in_flight` ensures exactly one probe request passes through at a time,
    /// preventing a thundering herd when the circuit first becomes eligible for recovery.
    HalfOpen {
        consecutive_failures: u32,
        probe_in_flight: bool,
        successes: u32,
    },
}

/// Configuration for a single circuit breaker instance.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Consecutive failures required to trip the circuit open.
    pub failure_threshold: u32,
    /// Seconds to hold the circuit open before transitioning to `HalfOpen`.
    pub recovery_timeout_secs: u64,
    /// Consecutive successes in `HalfOpen` required to close the circuit.
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout_secs: 30,
            success_threshold: 2,
        }
    }
}

/// Circuit breaker instance for a single federation entity type.
pub(crate) struct EntityCircuitBreaker {
    pub(crate) config: CircuitBreakerConfig,
    /// All mutable state — including the consecutive-failure counter — lives inside
    /// this single mutex so that counter increments and state transitions are atomic.
    state: Mutex<CircuitState>,
}

impl EntityCircuitBreaker {
    pub(crate) const fn new(config: CircuitBreakerConfig) -> Self {
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
    /// - `Open` → `HalfOpen` when `recovery_timeout` has elapsed; this first call becomes the
    ///   recovery probe (sets `probe_in_flight = true`).
    /// - `HalfOpen`: allows exactly one in-flight probe; subsequent calls are rejected until the
    ///   probe outcome is recorded via [`record_success`] or [`record_failure`].
    pub(crate) fn check(&self) -> Option<u64> {
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
                    probe_in_flight: true,
                    successes: 0,
                };
            },
            CircuitState::HalfOpen {
                probe_in_flight, ..
            } => {
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
    pub(crate) fn record_success(&self) {
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
    pub(crate) fn record_failure(&self) {
        let mut state = self.state.lock();

        // Read new failure count; short-circuit if already Open.
        let new_count = match &*state {
            CircuitState::Open { .. } => return,
            CircuitState::Closed {
                consecutive_failures,
            }
            | CircuitState::HalfOpen {
                consecutive_failures,
                ..
            } => *consecutive_failures + 1,
        };

        if new_count >= self.config.failure_threshold {
            let from_half_open = matches!(*state, CircuitState::HalfOpen { .. });
            *state = CircuitState::Open {
                opened_at: Instant::now(),
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
                CircuitState::Closed {
                    consecutive_failures,
                } => {
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
    /// `0` = Closed, `1` = Open, `2` = `HalfOpen`.
    pub(crate) fn state_code(&self) -> u64 {
        let state = self.state.lock();
        match &*state {
            CircuitState::Closed { .. } => STATE_CLOSED,
            CircuitState::Open { .. } => STATE_OPEN,
            CircuitState::HalfOpen { .. } => STATE_HALF_OPEN,
        }
    }

    /// Returns the typed health state for the `/health` endpoint.
    pub(crate) fn state_for_health(&self) -> CircuitHealthState {
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
    enabled: bool,
    failure_threshold: Option<u32>,
    recovery_timeout_secs: Option<u64>,
    success_threshold: Option<u32>,
    /// Per-entity overrides. Also accepts the legacy key `per_database`.
    #[serde(default, alias = "per_database")]
    per_entity: Vec<PerEntityJson>,
}

#[derive(Deserialize, Debug)]
struct PerEntityJson {
    /// Entity type name (GraphQL `__typename`). Also accepts the legacy key `database`.
    #[serde(alias = "database")]
    entity_type: String,
    failure_threshold: Option<u32>,
    recovery_timeout_secs: Option<u64>,
    success_threshold: Option<u32>,
}

// ────────────────────────────────────────────────────────────────────────────
// Public manager
// ────────────────────────────────────────────────────────────────────────────

/// Manages one circuit breaker per federation entity type.
///
/// Instantiated from the compiled schema JSON and shared via `Arc` across
/// request handlers and the metrics endpoint.
pub struct FederationCircuitBreakerManager {
    breakers: DashMap<String, Arc<EntityCircuitBreaker>>,
    pub(crate) default_config: CircuitBreakerConfig,
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
            failure_threshold: cb.failure_threshold,
            recovery_timeout_secs: cb.recovery_timeout_secs,
            success_threshold: cb.success_threshold,
        };
        let manager = Arc::new(Self::new(default_config));
        for override_entry in &cb.per_entity {
            let entity_config = CircuitBreakerConfig {
                failure_threshold: override_entry
                    .failure_threshold
                    .unwrap_or(manager.default_config.failure_threshold),
                recovery_timeout_secs: override_entry
                    .recovery_timeout
                    .unwrap_or(manager.default_config.recovery_timeout_secs),
                success_threshold: override_entry
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
            failure_threshold: cb_json.failure_threshold.unwrap_or(5),
            recovery_timeout_secs: cb_json.recovery_timeout_secs.unwrap_or(30),
            success_threshold: cb_json.success_threshold.unwrap_or(2),
        };

        let manager = Arc::new(Self::new(default_config));

        for override_entry in cb_json.per_entity {
            let entity_config = CircuitBreakerConfig {
                failure_threshold: override_entry
                    .failure_threshold
                    .unwrap_or(manager.default_config.failure_threshold),
                recovery_timeout_secs: override_entry
                    .recovery_timeout_secs
                    .unwrap_or(manager.default_config.recovery_timeout_secs),
                success_threshold: override_entry
                    .success_threshold
                    .unwrap_or(manager.default_config.success_threshold),
            };
            manager.per_entity_config.insert(override_entry.entity_type, entity_config);
        }

        // Pre-seed breakers for entities with per-entity overrides so they appear in
        // Prometheus metrics from startup rather than only after first traffic.
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

    fn get_or_create(&self, entity: &str) -> Arc<EntityCircuitBreaker> {
        self.breakers
            .entry(entity.to_string())
            .or_insert_with(|| {
                let config = self
                    .per_entity_config
                    .get(entity)
                    .map_or_else(|| self.default_config.clone(), |r| r.value().clone());
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
    /// State codes: `0` = Closed, `1` = Open, `2` = `HalfOpen`.
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
                state: entry.value().state_for_health(),
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
