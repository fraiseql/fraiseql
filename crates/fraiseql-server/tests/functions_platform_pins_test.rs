//! Baseline pins for the functions-platform-maturity train (phase 00).
//!
//! Each test *characterizes today's behavior* for one reported gap so a later
//! phase can flip it — "fixed by accident, regressed silently" drift is caught
//! because the assertion is written against the wrong-but-current state and a
//! phase that fixes the gap must update the assertion here.
//!
//! Placement note: these are structural (source-inspection) and behavioral
//! characterizations that need no Cargo feature — they document dispatch-wiring
//! facts, not runtime execution. The #594 host-context pin (which needs
//! `LiveHostContext`) lives in `functions_query_bridge_pin_test.rs` behind the
//! `functions-runtime` feature; the #596 delivery pin lives in
//! `subscription_row_visibility_pin_test.rs`.
//!
//! Flipped by:
//! - #595 cron pin           → phase 03 (cron wiring)
//! - capture→functions pin   → phase 05 (after:capture dispatch)
//! - bridge-write pin        → phase 02 (settled: option (a) keeps it as an invariant)
//! - #598 metrics pin        → phase 07 (dispatch metrics)
//! - #597 predicate pin      → phase 04 (trigger predicates)

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)] // Reason: test code — fail-loud helpers

use std::path::{Path, PathBuf};

/// Workspace root, derived from this crate's manifest dir (`crates/fraiseql-server`).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is two levels below the workspace root")
        .to_path_buf()
}

/// Read a workspace-relative source file as a string.
fn read_ws(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Count non-doc, non-line-comment occurrences of `needle` in `src`.
///
/// A line whose first non-whitespace is `//` (`//`, `///`, `//!`) is treated as a
/// comment and skipped, so a doc-comment *mention* of a symbol does not count as a
/// *use* of it. This is what lets the #595 pin distinguish the doc reference at
/// `sources/poller.rs` from an actual scheduler construction.
fn code_occurrences(src: &str, needle: &str) -> usize {
    src.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
        })
        .map(|line| line.matches(needle).count())
        .sum()
}

// ── #595: `cron:` functions are wired into server startup (phase 03) ─────────
//
// Phase 03 wired a leased `CronPoller` per cron function into the server lifecycle
// (a cron function is "a scheduled source without a cursor"). The functional
// single-firing + `_fraiseql_cron_state` persistence are verified in-crate at
// `cron::tests`; this pin guards the *wiring* — that server startup builds the cron
// pollers — so a refactor that drops the lifecycle block is caught.

#[test]
fn cron_pollers_are_built_at_server_startup() {
    let lifecycle = read_ws("crates/fraiseql-server/src/server/lifecycle.rs");
    assert!(
        code_occurrences(&lifecycle, "build_cron_pollers") > 0,
        "expected the server lifecycle to construct cron pollers at startup (#595). If the \
         wiring moved, update this invariant to the new construction site."
    );
    // And the pollers must be spawned (fired), not merely built.
    assert!(
        code_occurrences(&lifecycle, "run_forever") > 0,
        "expected each cron poller to be spawned (run_forever) on the server JoinSet."
    );
}

// ── capture→functions: externally-captured writes never dispatch a function ──
//
// The change-log listener converts `tb_entity_change_log` rows to `EntityEvent`s
// for the observer/subscription fan-out, but has no path to function dispatch.
// A `generate-capture-triggers`-captured external INSERT therefore reaches
// observers + subscriptions and dispatches zero functions.

#[test]
fn pin_capture_change_log_listener_has_no_function_dispatch() {
    let src = read_ws("crates/fraiseql-observers/src/listener/change_log.rs");
    for marker in [
        "plan_after_mutation_dispatch",
        "spawn_after_mutation",
        "invoke_with_context",
    ] {
        assert_eq!(
            code_occurrences(&src, marker),
            0,
            "M-capture: change_log listener unexpectedly references `{marker}`. At baseline the \
             reader has no function-dispatch fan-out; phase 05 adds `after:capture` dispatch here."
        );
    }
}

#[test]
fn pin_capture_discriminator_marker_is_documented() {
    // Phase 05's reader-side filter keys on this marker to dispatch `after:capture`
    // only for captured rows (executor-written rows never carry it). Pin the
    // contract so phase 05 keys on the right string.
    let doc = read_ws("docs/architecture/external-write-capture.md");
    assert!(
        doc.contains("fallback_trigger"),
        "M-capture: the captured-row discriminator `cdc_source = \"fallback_trigger\"` must be \
         documented in external-write-capture.md — phase 05 keys after:capture dispatch on it."
    );
}

// ── bridge-write dispatch: after:mutation dispatch is route-layer only ───────
//
// The sources query bridge (`SourceQueryExecutor` → core `Executor`) writes by
// calling `Executor::execute_with_security` directly, bypassing the route
// handlers where after:mutation dispatch is invoked. So a bridge write fires no
// after:mutation function today — settling phase 02's recursion question (no
// bridge→after:mutation loop can exist) and phase 05's loop analysis.

#[test]
fn pin_bridge_write_executor_does_not_dispatch_after_mutation() {
    let bridge = read_ws("crates/fraiseql-server/src/sources/executor.rs");
    for marker in ["plan_after_mutation_dispatch", "spawn_after_mutation"] {
        assert_eq!(
            code_occurrences(&bridge, marker),
            0,
            "M-bridge: the sources query bridge must not invoke after:mutation dispatch \
             (`{marker}`) — dispatch is route-layer only, which is why a bridge write cannot \
             loop back into after:mutation. Phase 02 (option a) keeps this as an invariant."
        );
    }
}

#[test]
fn pin_after_mutation_dispatch_lives_only_in_route_handlers() {
    // The two dispatch sites: the GraphQL handler and the REST mutation handler.
    let graphql = read_ws("crates/fraiseql-server/src/routes/graphql/handler.rs");
    let rest = read_ws("crates/fraiseql-server/src/routes/rest/handler/mutation.rs");
    assert!(
        code_occurrences(&graphql, "spawn_after_mutation") > 0,
        "M-bridge: expected the GraphQL route handler to be an after:mutation dispatch site."
    );
    assert!(
        code_occurrences(&rest, "spawn_after_mutation") > 0,
        "M-bridge: expected the REST mutation handler to be an after:mutation dispatch site."
    );
}

// ── #598: function dispatch is unobservable — no metrics on the dispatch path ─
//
// Sources got a full metric set (`sources/metrics.rs`); function dispatch has
// none. Nothing about a fired / failed / dead-lettered function reaches
// `/metrics`.

#[test]
fn pin_598_no_metrics_in_function_dispatch_path() {
    let dispatch = read_ws("crates/fraiseql-server/src/routes/after_mutation/mod.rs");
    for macro_name in ["counter!", "histogram!", "gauge!"] {
        assert_eq!(
            code_occurrences(&dispatch, macro_name),
            0,
            "M-598: the after:mutation dispatch path has no `{macro_name}` at baseline — \
             function dispatch is unobservable. Phase 07 instruments it; flip this pin then."
        );
    }
    // Contrast: sources ARE instrumented (proof the pattern exists and is unused here).
    let sources_metrics = read_ws("crates/fraiseql-server/src/sources/metrics.rs");
    assert!(
        code_occurrences(&sources_metrics, "counter!") > 0 || sources_metrics.contains("counter"),
        "sources metrics module should demonstrate the metric pattern functions lack"
    );
}

// ── #597: declarative `when` predicates gate after:mutation firing (phase 04) ─
//
// A trigger with a `when` conjunction no longer fires on *every* update: the
// dispatcher evaluates the predicates on the row images before spawning any
// runtime. `matches` still keys on entity + event kind; `predicates_hold` is the
// new gate. The pure-predicate table is exercised in
// `fraiseql-functions::triggers::mutation::tests::trigger_predicates`; this pin
// guards that a schema-declared `when` is honored end to end through the trigger.

#[test]
fn after_mutation_when_predicates_gate_firing() {
    use fraiseql_functions::triggers::mutation::{
        AfterMutationTrigger, EntityEvent, EventKind, TriggerPredicate,
    };

    let trigger = AfterMutationTrigger {
        function_name: "notify_approved".to_string(),
        entity_type:   "Order".to_string(),
        event_filter:  Some(EventKind::Update),
        predicates:    vec![TriggerPredicate {
            field:      "status".to_string(),
            eq:         None,
            changed_to: Some(serde_json::json!("approved")),
        }],
    };

    let now = chrono::Utc::now();
    let event = |old: serde_json::Value, new: serde_json::Value| EntityEvent {
        entity:     "Order".to_string(),
        event_kind: EventKind::Update,
        old:        Some(old),
        new:        Some(new),
        timestamp:  now,
    };

    // The entity+operation still matches every Order update...
    assert!(trigger.matches("Order", EventKind::Update));

    // ...but the `when` predicate fires ONLY on the pending→approved transition.
    assert!(
        trigger.predicates_hold(&event(
            serde_json::json!({ "status": "pending" }),
            serde_json::json!({ "status": "approved" }),
        )),
        "fires on the approving transition"
    );
    assert!(
        !trigger.predicates_hold(&event(
            serde_json::json!({ "status": "approved" }),
            serde_json::json!({ "status": "approved" }),
        )),
        "does NOT fire on an unrelated re-save (approved→approved) — the #597 fix"
    );
}
