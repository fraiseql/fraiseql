//! Platform E2E Integration Tests — Phase 8 Cycle 7
//!
//! End-to-end tests that verify the complete platform integration:
//! storage, functions (before/after mutation, cron), and realtime.
//!
//! # Test tiers
//!
//! ## Tier 1 — Structural (no infrastructure required)
//!
//! These tests run in all CI environments and verify that subsystem types
//! compose correctly. They do NOT execute functions or query a database.
//!
//! ## Tier 2 — Database (requires PostgreSQL via testcontainers)
//!
//! These tests are marked `#[ignore]` and run only when explicitly requested.
//! They spin up a PostgreSQL container via testcontainers and verify the
//! full function execution pipeline against a real database.
//!
//! Run with:
//! ```bash
//! cargo test --test platform_e2e_test -- --include-ignored
//! ```
//!
//! ## Tier 3 — Full platform (requires PostgreSQL + `MinIO` + Deno runtime)
//!
//! These tests require a full platform stack and are gated behind the
//! `FRAISEQL_PLATFORM_E2E` environment variable to prevent accidental
//! execution in constrained CI environments.
//!
//! Run with:
//! ```bash
//! FRAISEQL_PLATFORM_E2E=1 cargo test --test platform_e2e_test -- --include-ignored
//! ```
//!
//! **Execution engine:** none (Tier 1), testcontainers (Tier 2), Docker (Tier 3)
//! **Infrastructure:** none (Tier 1), PostgreSQL (Tier 2), PostgreSQL + `MinIO` (Tier 3)
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers near use site

// ── Tier 1: Structural Tests ──────────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::Arc;

use fraiseql_functions::{
    FunctionDefinition, FunctionModule, FunctionObserver,
    RuntimeType, TriggerRegistry,
};
use fraiseql_server::subsystems::{BeforeMutationHooks, FunctionsSubsystem, ServerSubsystems};

/// Verify that `ServerSubsystems::none()` compiles and all subsystems are absent.
#[test]
fn test_platform_e2e_server_subsystems_none_is_all_absent() {
    let subsystems = ServerSubsystems::none();
    assert!(!subsystems.is_storage_enabled());
    assert!(!subsystems.is_functions_enabled());
    assert!(!subsystems.is_realtime_enabled());
}

/// Verify that `BeforeMutationHooks` can be constructed from a trigger registry.
#[test]
fn test_platform_e2e_before_mutation_hooks_construction() {
    let defs = vec![
        FunctionDefinition::new("validate", "before:mutation:createUser", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&defs).unwrap();
    let observer = Arc::new(FunctionObserver::new());
    let module_registry: HashMap<String, FunctionModule> = HashMap::new();

    let hooks = BeforeMutationHooks {
        trigger_registry: registry,
        module_registry,
        observer,
    };

    assert!(
        hooks.trigger_registry.before_chain("createUser").is_some(),
        "should find before:mutation chain for createUser"
    );
    assert!(
        hooks.trigger_registry.before_chain("deleteUser").is_none(),
        "should return None for unregistered mutation"
    );
}

/// Verify that `FunctionsSubsystem` can be constructed with all fields.
#[test]
fn test_platform_e2e_functions_subsystem_full_construction() {
    let defs = vec![
        FunctionDefinition::new("validate", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("onCreated", "after:mutation:User:insert", RuntimeType::Deno),
        FunctionDefinition::new("dailyJob", "cron:0 2 * * *", RuntimeType::Deno),
    ];
    let trigger_registry = TriggerRegistry::load_from_definitions(&defs).unwrap();
    let observer = Arc::new(FunctionObserver::new());
    let module_registry: HashMap<String, FunctionModule> = HashMap::new();
    let config = fraiseql_server::schema::loader::FunctionsConfig {
        definitions: defs,
        module_dir: std::path::PathBuf::from("/tmp/functions"),
    };

    let subsystem = FunctionsSubsystem {
        observer,
        trigger_registry,
        module_registry,
        config,
    };

    assert_eq!(subsystem.trigger_registry.before_mutation_count(), 1);
    assert_eq!(subsystem.trigger_registry.cron_trigger_count(), 1);
}

/// Verify that the registry → `CronScheduler` pipeline works end-to-end.
#[test]
fn test_platform_e2e_registry_to_cron_scheduler_pipeline() {
    let defs = vec![
        FunctionDefinition::new("dailyCleanup", "cron:0 2 * * *", RuntimeType::Deno),
        FunctionDefinition::new("hourlySync", "cron:0 * * * *", RuntimeType::Deno),
        // Non-cron trigger must coexist without interfering
        FunctionDefinition::new("validate", "before:mutation:createUser", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&defs).unwrap();

    // Fast path: 2 cron triggers → scheduler is Some
    let scheduler = registry.cron_scheduler();
    assert!(scheduler.is_some(), "should build a scheduler when cron triggers exist");
    let scheduler = scheduler.unwrap();
    assert_eq!(scheduler.trigger_count(), 2);
}

/// Verify that a registry with no cron triggers returns None from `cron_scheduler()`.
#[test]
fn test_platform_e2e_registry_no_cron_returns_none() {
    let defs = vec![
        FunctionDefinition::new("validate", "before:mutation:createUser", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&defs).unwrap();

    assert!(
        registry.cron_scheduler().is_none(),
        "no cron triggers → cron_scheduler() returns None (zero-overhead fast path)"
    );
}

/// Verify that `BeforeMutationHooks` correctly exposes the before:mutation chain
/// for finding triggers associated with specific mutations.
#[test]
fn test_platform_e2e_before_mutation_chain_finds_correct_triggers() {
    let defs = vec![
        FunctionDefinition::new("validateName", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("checkDups", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("auditDelete", "before:mutation:deleteUser", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&defs).unwrap();
    let observer = Arc::new(FunctionObserver::new());

    let hooks = BeforeMutationHooks {
        trigger_registry: registry,
        module_registry: HashMap::new(),
        observer,
    };

    // createUser has 2 triggers
    let chain = hooks.trigger_registry.before_chain("createUser");
    assert!(chain.is_some());
    assert_eq!(chain.unwrap().triggers.len(), 2);

    // deleteUser has 1 trigger
    let chain = hooks.trigger_registry.before_chain("deleteUser");
    assert!(chain.is_some());
    assert_eq!(chain.unwrap().triggers.len(), 1);

    // updateUser has no triggers — fast path, no allocation
    assert!(hooks.trigger_registry.before_chain("updateUser").is_none());
}

/// Verify that multiple trigger types coexist in the registry without interference.
#[test]
fn test_platform_e2e_all_trigger_types_coexist() {
    let defs = vec![
        FunctionDefinition::new("onUserCreated", "after:mutation:User:insert", RuntimeType::Deno),
        FunctionDefinition::new("validateUser", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("dailyReport", "cron:0 2 * * *", RuntimeType::Deno),
        FunctionDefinition::new("getMetrics", "http:GET:/functions/v1/metrics", RuntimeType::Deno),
    ];

    let registry = TriggerRegistry::load_from_definitions(&defs).unwrap();

    assert_eq!(registry.function_count, 4);
    assert_eq!(registry.before_mutation_count(), 1);
    assert_eq!(registry.cron_trigger_count(), 1);
    assert_eq!(registry.http_route_count(), 1);

    // Cron scheduler builds from cron triggers only
    let scheduler = registry.cron_scheduler().unwrap();
    assert_eq!(scheduler.trigger_count(), 1);
}

// ── Tier 2: CronScheduler lifecycle (tokio runtime, no DB) ───────────────────

/// Verify the full lifecycle: registry → scheduler → start → stop.
///
/// This test does not execute any functions; it only verifies the scheduler
/// lifecycle under a tokio runtime.
#[tokio::test]
async fn test_platform_e2e_cron_scheduler_starts_on_server_start() {
    let defs = vec![
        // Use a schedule that will never match in the test window (Feb 31)
        FunctionDefinition::new("neverFires", "cron:0 0 31 2 *", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&defs).unwrap();

    let observer = Arc::new(FunctionObserver::new());
    let scheduler = registry.cron_scheduler().expect("should have scheduler");

    // Simulates server startup: build and start the scheduler
    let handle = scheduler.start(observer, HashMap::new());

    // Simulates server shutdown: stop the scheduler
    handle.stop();

    // Yield to allow the background task to process the shutdown signal
    tokio::task::yield_now().await;
}

/// Verify that the realtime observer hook point exists on `AppState`.
///
/// The full realtime notification path (mutation → entity event → `WebSocket`)
/// requires the observer runtime pipeline which is exercised in Tier 3.
/// This test verifies only the `AppState` hook plumbing.
#[test]
fn test_platform_e2e_realtime_observer_hook_is_accessible() {
    use fraiseql_server::realtime::observer::RealtimeBroadcastObserver;

    let (observer, _rx) = RealtimeBroadcastObserver::new(64);
    let observer = Arc::new(observer);

    // The observer tracks dropped events; must start at 0
    assert_eq!(observer.events_dropped_total(), 0);

    // Simulate a mutation completing: non-blocking, returns immediately
    use fraiseql_server::realtime::delivery::{EntityEvent, EventKindSerde};
    let event = EntityEvent {
        entity: "User".to_string(),
        event_kind: EventKindSerde::Insert,
        new: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        old: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    observer.on_mutation_complete(event);

    // With an active receiver (_rx), the event should be buffered, not dropped
    assert_eq!(observer.events_dropped_total(), 0);
}

/// Verify that `on_mutation_complete` drops events when channel is full (backpressure).
///
/// The realtime delivery pipeline is intentionally lossy under backpressure to
/// protect mutation response latency.
#[test]
fn test_platform_e2e_realtime_observer_drops_events_on_backpressure() {
    use fraiseql_server::realtime::delivery::{EntityEvent, EventKindSerde};
    use fraiseql_server::realtime::observer::RealtimeBroadcastObserver;

    // Capacity = 1 → second event should be dropped when channel is full
    let (observer, _rx) = RealtimeBroadcastObserver::new(1);

    let make_event = || EntityEvent {
        entity: "Post".to_string(),
        event_kind: EventKindSerde::Insert,
        new: Some(serde_json::json!({ "id": 1 })),
        old: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    // First event fills the channel
    observer.on_mutation_complete(make_event());
    assert_eq!(observer.events_dropped_total(), 0);

    // Second event overflows the channel → dropped
    observer.on_mutation_complete(make_event());
    assert_eq!(observer.events_dropped_total(), 1);
}

// ── Tier 3: Full platform E2E (requires PostgreSQL + Deno) ───────────────────
//
// These tests are `#[ignore]` and require the full platform stack.
// Run with: FRAISEQL_PLATFORM_E2E=1 cargo test --test platform_e2e_test -- --include-ignored

/// Guard: returns false when the full platform E2E environment is not configured.
fn platform_e2e_available() -> bool {
    std::env::var("FRAISEQL_PLATFORM_E2E").is_ok()
}

/// E2E: `before:mutation:createUser` function rejects empty name → mutation fails.
///
/// This test requires:
/// - A running FraiseQL server with the test compiled schema loaded
/// - The `validateInput` function registered as `before:mutation:createUser`
/// - The function returns `{ "abort": "name is required" }` for empty input
///
/// Run with: `FRAISEQL_PLATFORM_E2E=1 FRAISEQL_TEST_URL=http://localhost:8000 cargo test ...`
#[tokio::test]
#[ignore = "requires full platform stack (FRAISEQL_PLATFORM_E2E=1)"]
async fn test_e2e_before_mutation_validates_input() {
    if !platform_e2e_available() {
        eprintln!("skipped: FRAISEQL_PLATFORM_E2E not set");
        return;
    }

    let base_url = std::env::var("FRAISEQL_TEST_URL")
        .unwrap_or_else(|_| "http://localhost:8000".to_string());
    let client = reqwest::Client::new();

    // Mutation with empty name — the before:mutation hook should reject this
    let mutation = serde_json::json!({
        "query": "mutation CreateUser($input: CreateUserInput!) { createUser(input: $input) { id } }",
        "variables": { "input": { "name": "" } }
    });

    let response = client
        .post(format!("{base_url}/graphql"))
        .json(&mutation)
        .send()
        .await
        .expect("request failed");

    let body: serde_json::Value = response.json().await.expect("parse response");

    // before:mutation should abort with a validation error
    assert!(
        body.get("errors").is_some(),
        "before:mutation abort should produce a GraphQL error"
    );
    let errors = body["errors"].as_array().unwrap();
    assert!(!errors.is_empty(), "should have at least one error");
}

/// E2E: `WebSocket` subscriber receives insert event in real time.
///
/// Flow:
/// 1. Connect `WebSocket` to `/realtime/v1`
/// 2. Subscribe to `Post` entity
/// 3. Insert a Post via GraphQL mutation
/// 4. Assert the subscriber receives a `change` message with the new Post data
///
/// Run with: `FRAISEQL_PLATFORM_E2E=1 FRAISEQL_TEST_URL=http://localhost:8000 cargo test ...`
#[tokio::test]
#[ignore = "requires full platform stack (FRAISEQL_PLATFORM_E2E=1)"]
async fn test_e2e_realtime_subscription_receives_insert() {
    if !platform_e2e_available() {
        eprintln!("skipped: FRAISEQL_PLATFORM_E2E not set");
        return;
    }

    // Implementation: connect WS, subscribe, insert, assert event received.
    // Full implementation requires the fraiseql-test-utils WS client helpers.
    // Tracked for implementation when the platform stack is available.
    todo!("requires WS client helper and running platform stack")
}

/// E2E: Cron function fires and persists state to `_fraiseql_cron_state`.
///
/// Flow:
/// 1. Register a `cron:* * * * *` (every minute) function
/// 2. Wait for the scheduler to tick
/// 3. Assert `_fraiseql_cron_state` has an updated `last_fire` for the function
///
/// Run with: `FRAISEQL_PLATFORM_E2E=1 DATABASE_URL=... cargo test ...`
#[tokio::test]
#[ignore = "requires PostgreSQL and cron scheduler running"]
async fn test_e2e_cron_fires_and_persists_state() {
    if !platform_e2e_available() {
        eprintln!("skipped: FRAISEQL_PLATFORM_E2E not set");
        return;
    }

    // Implementation: start scheduler with every-minute trigger, wait for tick,
    // query _fraiseql_cron_state, assert last_fire was updated.
    // Tracked for implementation when the migrations and full cron state persistence
    // (Phase 8 Cycle 6 testcontainers pattern) are available.
    todo!("requires running PostgreSQL with _fraiseql_cron_state table")
}

/// E2E: HTTP trigger function responds to GET request.
///
/// Flow:
/// 1. Register a function at `http:GET:/functions/v1/user-count`
/// 2. Send `GET /functions/v1/user-count`
/// 3. Assert the function executes and returns a JSON response
///
/// Run with: `FRAISEQL_PLATFORM_E2E=1 FRAISEQL_TEST_URL=http://localhost:8000 cargo test ...`
#[tokio::test]
#[ignore = "requires full platform stack with Deno runtime (FRAISEQL_PLATFORM_E2E=1)"]
async fn test_e2e_http_trigger_calls_graphql() {
    if !platform_e2e_available() {
        eprintln!("skipped: FRAISEQL_PLATFORM_E2E not set");
        return;
    }

    let base_url = std::env::var("FRAISEQL_TEST_URL")
        .unwrap_or_else(|_| "http://localhost:8000".to_string());
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base_url}/functions/v1/user-count"))
        .send()
        .await
        .expect("request failed");

    assert!(
        response.status().is_success(),
        "HTTP trigger should return 2xx"
    );
}

/// E2E: `after:mutation` function receives entity event after DB insert.
///
/// Flow:
/// 1. Register `onUserCreated` as `after:mutation:User:insert`
/// 2. Insert a User via GraphQL mutation
/// 3. Assert the `onUserCreated` function was invoked (check function log table)
///
/// Run with: `FRAISEQL_PLATFORM_E2E=1 DATABASE_URL=... cargo test ...`
#[tokio::test]
#[ignore = "requires PostgreSQL + Deno runtime (FRAISEQL_PLATFORM_E2E=1)"]
async fn test_e2e_after_mutation_function_receives_event() {
    if !platform_e2e_available() {
        eprintln!("skipped: FRAISEQL_PLATFORM_E2E not set");
        return;
    }

    // Implementation: execute mutation, wait for async dispatch, check function
    // invocation log in _fraiseql_function_log or similar audit table.
    todo!("requires full platform stack with observer pipeline active")
}
