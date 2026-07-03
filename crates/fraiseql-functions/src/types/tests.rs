//! Tests for function type serde, including durable-dispatch config round-trip.

use super::*;

#[test]
fn function_definition_defaults_to_durable_with_no_retry_override() {
    // A compiled-schema entry that omits the durability fields deserializes to
    // the durable default: not re-runnable, no per-function retry override.
    let json = serde_json::json!({
        "name": "onOrderPaid",
        "trigger": "after:mutation:Order:update",
        "runtime": "Wasm"
    });
    let definition: FunctionDefinition = serde_json::from_value(json).expect("valid definition");
    assert!(!definition.re_runnable);
    assert!(definition.retry.is_none());
}

#[test]
fn function_definition_round_trips_re_runnable_and_retry_policy() {
    // A per-trigger retry policy + re_runnable flag round-trip from the
    // compiled schema.
    let json = serde_json::json!({
        "name": "scoreDeal",
        "trigger": "after:mutation:Deal:insert",
        "runtime": "Deno",
        "re_runnable": true,
        "retry": {
            "max_attempts": 7,
            "initial_delay_ms": 250,
            "max_delay_ms": 60_000,
            "backoff_strategy": "exponential"
        }
    });
    let definition: FunctionDefinition = serde_json::from_value(json).expect("valid definition");
    assert!(definition.re_runnable);
    let retry = definition.retry.expect("retry policy present");
    assert_eq!(retry.max_attempts, 7);
    assert_eq!(retry.initial_delay_ms, 250);
    assert_eq!(retry.max_delay_ms, 60_000);
}
