mod circuit_breaker_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::super::circuit_breaker::{
        CircuitBreakerConfig, CircuitHealthState, EntityCircuitBreaker,
        FederationCircuitBreakerManager, STATE_CLOSED, STATE_HALF_OPEN, STATE_OPEN,
        extract_entity_types,
    };

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
        assert!(breaker.check().is_some(), "second concurrent probe should be rejected");
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
        assert!(
            breaker.check().is_some(),
            "second check should return backoff while probe is in flight"
        ); // blocked: probe in flight

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
        assert!(breaker.check().is_none(), "circuit should remain closed after 8 < 10 failures");
        assert_eq!(breaker.state_code(), STATE_CLOSED);
    }
}

mod health_checker_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use chrono::Utc;

    use super::super::health_checker::{
        HEALTH_CHECK_TIMEOUT, RollingErrorWindow, SubgraphConfig, SubgraphHealthChecker,
        SubgraphHealthStatus,
    };

    #[test]
    fn test_rolling_error_window_creation() {
        let window = RollingErrorWindow::new();
        assert_eq!(window.error_count(), 0);
        assert!((window.error_rate_percent() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rolling_error_window_success() {
        let window = RollingErrorWindow::new();
        window.record_success();
        window.record_success();

        assert_eq!(window.error_count(), 0);
        assert!((window.error_rate_percent() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rolling_error_window_mixed() {
        let window = RollingErrorWindow::new();
        window.record_success();
        window.record_success();
        window.record_error();

        assert_eq!(window.error_count(), 1);
        assert!((window.error_rate_percent() - 33.33).abs() < 0.1);
    }

    #[test]
    fn test_health_status_serialization() {
        let status = SubgraphHealthStatus {
            name:                 "test-subgraph".to_string(),
            available:            true,
            latency_ms:           25.5,
            last_check:           Utc::now().to_rfc3339(),
            error_count_last_60s: 0,
            error_rate_percent:   0.0,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("test-subgraph"));
        assert!(json.contains("true"));
    }

    // ── S25-H3: SubgraphHealthChecker client timeout ──────────────────────────

    #[test]
    fn health_check_timeout_is_set() {
        let secs = HEALTH_CHECK_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 60, "Health-check timeout should be 1–60 s, got {secs}");
    }

    #[test]
    fn health_checker_new_creates_instance() {
        let checker = SubgraphHealthChecker::new(vec![SubgraphConfig {
            name:     "test".to_string(),
            endpoint: "https://test.example.com/graphql".to_string(),
        }]);
        assert_eq!(checker.subgraphs.len(), 1);
    }
}
