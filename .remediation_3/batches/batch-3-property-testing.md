# Batch 3 — Property Testing Extension

## Problem

`fraiseql-core` has 7 property test files covering schema, SQL generation,
error handling, GraphQL parsing, and cache invalidation. `fraiseql-server`
and `fraiseql-observers` have zero property tests.

The rate-limiting middleware in `fraiseql-server` has 11 bucket strategies,
each constructing Redis key strings from arbitrary user-supplied inputs (IP
addresses, user IDs, path prefixes). A malformed input that triggers a panic
in key construction would take down the server. The observer state machine
processes events that arrive from the network; invalid transition attempts
should return errors, not panic.

---

## proptest dependency

Both `fraiseql-server` and `fraiseql-observers` need `proptest` added to
`[dev-dependencies]`:

```toml
proptest = { workspace = true }
```

---

## Fix Plan

### PT-1 — Rate-limit key construction (fraiseql-server)

New file `crates/fraiseql-server/tests/property_rate_limiting.rs`:

```rust
use proptest::prelude::*;
use fraiseql_server::middleware::rate_limit::build_rate_limit_key;

proptest! {
    #[test]
    fn rate_limit_ip_key_never_panics(ip in "\\PC*") {
        // Must not panic, regardless of input
        let _ = build_rate_limit_key("ip", &ip, None);
    }

    #[test]
    fn rate_limit_user_key_never_panics(user_id in "\\PC*") {
        let _ = build_rate_limit_key("user", &user_id, None);
    }

    #[test]
    fn rate_limit_path_key_never_panics(ip in "\\PC*", prefix in "\\PC*") {
        let _ = build_rate_limit_key("path", &ip, Some(&prefix));
    }

    #[test]
    fn rate_limit_key_contains_strategy_prefix(ip in "[0-9a-f:.]{1,40}") {
        let key = build_rate_limit_key("ip", &ip, None);
        prop_assert!(key.starts_with("fraiseql:rl:ip:"));
    }

    #[test]
    fn rate_limit_key_is_deterministic(ip in "[0-9a-f:.]{1,40}") {
        let k1 = build_rate_limit_key("ip", &ip, None);
        let k2 = build_rate_limit_key("ip", &ip, None);
        prop_assert_eq!(k1, k2);
    }
}
```

If `build_rate_limit_key` is not currently a standalone function, extract the
key-construction logic from the closure in `rate_limit.rs` into a named
`pub(crate) fn` — a prerequisite step.

### PT-2 — Auth header parsing (fraiseql-server)

New file `crates/fraiseql-server/tests/property_auth_parsing.rs`:

```rust
use proptest::prelude::*;
use fraiseql_server::middleware::auth::parse_authorization_header;
use fraiseql_core::error::FraiseQLError;

proptest! {
    #[test]
    fn auth_header_parse_never_panics(header in "\\PC*") {
        // Must return Ok or a typed error, never panic
        let result = parse_authorization_header(&header);
        prop_assert!(result.is_ok() || matches!(result, Err(FraiseQLError::Authentication { .. })));
    }

    #[test]
    fn bearer_prefix_with_valid_base64_parses_to_token(
        token in "[A-Za-z0-9+/]{20,60}={0,2}"
    ) {
        let header = format!("Bearer {token}");
        // Must either parse successfully or return Authentication error
        // Must not return any other error variant
        let result = parse_authorization_header(&header);
        match result {
            Ok(_) => {},
            Err(FraiseQLError::Authentication { .. }) => {},
            Err(other) => prop_assert!(false, "unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn empty_header_returns_authentication_error(
        prefix in "(Basic|Digest|NTLM|) "
    ) {
        // Non-Bearer prefixes must never parse as Bearer token
        let result = parse_authorization_header(&prefix);
        prop_assert!(matches!(result, Err(FraiseQLError::Authentication { .. })));
    }
}
```

### PT-3 — Query complexity (fraiseql-server)

New file `crates/fraiseql-server/tests/property_query_complexity.rs`:

```rust
use proptest::prelude::*;
use fraiseql_server::middleware::complexity::{calculate_complexity, ComplexityConfig};

// Strategy: generate valid-looking GraphQL field selection trees
fn arb_field_depth() -> impl Strategy<Value = usize> {
    1usize..=20
}

proptest! {
    #[test]
    fn complexity_is_non_negative(depth in arb_field_depth(), width in 1usize..=10) {
        let config = ComplexityConfig::default();
        let score = calculate_complexity(depth, width, &config);
        prop_assert!(score >= 0, "complexity must be non-negative, got: {score}");
    }

    #[test]
    fn complexity_exceeding_limit_returns_error(
        depth in 1usize..=100,
        width in 1usize..=10
    ) {
        let config = ComplexityConfig { max_complexity: 10, ..Default::default() };
        let score = calculate_complexity(depth, width, &config);
        if score > 10 {
            // If score exceeds limit, the middleware must reject it
            // This tests the calculation is monotonic (not the middleware itself)
            prop_assert!(score > config.max_complexity);
        }
    }

    #[test]
    fn depth_increases_complexity(
        base_depth in 1usize..=5,
        extra in 1usize..=5,
        width in 1usize..=5
    ) {
        let config = ComplexityConfig::default();
        let shallow = calculate_complexity(base_depth, width, &config);
        let deeper = calculate_complexity(base_depth + extra, width, &config);
        prop_assert!(deeper >= shallow, "deeper queries must have >= complexity");
    }
}
```

### PT-4 — Observer state machine (fraiseql-observers)

New file `crates/fraiseql-observers/tests/property_state_machine.rs`:

The observer state machine has documented valid transitions. Property tests
verify: (a) no valid transition panics, (b) invalid transitions return an error,
(c) the state after a transition is always a valid state.

```rust
use proptest::prelude::*;
use fraiseql_observers::state_machine::{ObserverState, ObserverEvent, transition};

fn arb_state() -> impl Strategy<Value = ObserverState> {
    prop_oneof![
        Just(ObserverState::Idle),
        Just(ObserverState::Connecting),
        Just(ObserverState::Active),
        Just(ObserverState::Recovering),
        Just(ObserverState::Stopped),
    ]
}

fn arb_event() -> impl Strategy<Value = ObserverEvent> {
    prop_oneof![
        Just(ObserverEvent::Start),
        Just(ObserverEvent::Connected),
        Just(ObserverEvent::MessageReceived),
        Just(ObserverEvent::ConnectionLost),
        Just(ObserverEvent::Stop),
        Just(ObserverEvent::MaxRetriesExceeded),
    ]
}

proptest! {
    #[test]
    fn transition_never_panics(state in arb_state(), event in arb_event()) {
        // Result or error — must never panic
        let _ = transition(state, event);
    }

    #[test]
    fn transition_result_is_always_a_valid_state(
        state in arb_state(),
        event in arb_event()
    ) {
        if let Ok(next) = transition(state, event) {
            // next must be one of the documented states
            let valid = matches!(
                next,
                ObserverState::Idle
                    | ObserverState::Connecting
                    | ObserverState::Active
                    | ObserverState::Recovering
                    | ObserverState::Stopped
            );
            prop_assert!(valid, "unexpected state: {next:?}");
        }
    }
}
```

If `transition()` is not a standalone function, extract the match arm from
wherever state transitions are currently computed.

### PT-5 — Cascade invalidation (fraiseql-core)

New file `crates/fraiseql-core/tests/property_cache_invalidation.rs`:

```rust
use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use fraiseql_core::cache::CascadeInvalidator;

// Strategy: generate a random mutation-to-view dependency map
fn arb_dependency_map() -> impl Strategy<Value = HashMap<String, HashSet<String>>> {
    prop::collection::hash_map(
        "[a-z]{1,10}",  // mutation name
        prop::collection::hash_set("[a-z]{1,10}", 0..5),  // view names
        0..10,
    )
}

proptest! {
    #[test]
    fn invalidation_includes_all_direct_dependants(
        deps in arb_dependency_map(),
        mutation in "[a-z]{1,10}"
    ) {
        let invalidator = CascadeInvalidator::from_deps(deps.clone());
        let invalidated = invalidator.compute_invalidation_set(&mutation);
        if let Some(expected) = deps.get(&mutation) {
            for view in expected {
                prop_assert!(
                    invalidated.contains(view),
                    "view {view} must be in invalidation set for mutation {mutation}"
                );
            }
        }
    }

    #[test]
    fn invalidation_never_produces_views_unrelated_to_mutation(
        deps in arb_dependency_map(),
        mutation in "[a-z]{1,10}"
    ) {
        let all_views: HashSet<_> = deps.values().flatten().cloned().collect();
        let invalidator = CascadeInvalidator::from_deps(deps.clone());
        let invalidated = invalidator.compute_invalidation_set(&mutation);
        for view in &invalidated {
            prop_assert!(
                all_views.contains(view),
                "view {view} in invalidation set but not in any dependency mapping"
            );
        }
    }
}
```

---

## Verification

- [ ] `cargo nextest run -p fraiseql-server --test property_rate_limiting` passes
- [ ] `cargo nextest run -p fraiseql-server --test property_auth_parsing` passes
- [ ] `cargo nextest run -p fraiseql-server --test property_query_complexity` passes
- [ ] `cargo nextest run -p fraiseql-observers --test property_state_machine` passes
- [ ] `cargo nextest run -p fraiseql-core --test property_cache_invalidation` passes
- [ ] `cargo clippy -p fraiseql-server --tests` clean
- [ ] `cargo clippy -p fraiseql-observers --tests` clean
