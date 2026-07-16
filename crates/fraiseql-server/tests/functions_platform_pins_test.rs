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

// ── #595: a `cron:` function never fires from a stock server boot ────────────
//
// The functions `CronScheduler` is fully implemented and unit-tested in
// `fraiseql-functions` (see `platform_e2e_test.rs`), but nothing in the server's
// startup wiring ever constructs one from the compiled schema — the only mention
// in server `src/` is a doc-comment cross-reference in `sources/poller.rs`.

#[test]
fn pin_595_cron_scheduler_is_never_constructed_in_server_startup() {
    // `registry.cron_scheduler()` is the single call site that would wire the
    // scheduler at boot. It appears only in tests today.
    let dirs = ["crates/fraiseql-server/src"];
    let mut hits: Vec<String> = Vec::new();
    for dir in dirs {
        for entry in walk_rs_files(&workspace_root().join(dir)) {
            // Skip test modules — the scheduler is exercised there directly.
            let name = entry.to_string_lossy().to_string();
            if name.contains("tests.rs") || name.contains("/tests/") {
                continue;
            }
            let src = std::fs::read_to_string(&entry).unwrap();
            if code_occurrences(&src, ".cron_scheduler(") > 0 {
                hits.push(name);
            }
        }
    }
    assert!(
        hits.is_empty(),
        "M-595: expected NO server-startup construction of the cron scheduler at baseline, \
         but `.cron_scheduler()` is called in: {hits:?}. Phase 03 wires it — when it does, \
         flip this pin to assert the startup path DOES build the scheduler."
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

// ── #597: an after:mutation:X:update trigger fires on EVERY update ───────────
//
// `AfterMutationTrigger::matches` keys on entity + event kind only. There is no
// way to say "only when new.status == 'approved'"; the condition lives as
// invisible guard code inside the guest. Phase 04 adds declarative `when`.

#[test]
fn pin_597_after_mutation_matches_ignore_field_values() {
    use fraiseql_functions::triggers::mutation::{AfterMutationTrigger, EventKind};

    let trigger = AfterMutationTrigger {
        function_name: "notify_approved".to_string(),
        entity_type:   "Order".to_string(),
        event_filter:  Some(EventKind::Update),
    };

    // Two updates to the same entity with DIFFERENT field values both match —
    // there is no field/transition predicate at baseline.
    assert!(
        trigger.matches("Order", EventKind::Update),
        "M-597: baseline matches entity+operation only"
    );
    // The trigger cannot distinguish an approving update from any other update:
    // `matches` takes no payload, so field values cannot influence the decision.
    // Phase 04 introduces a `when` predicate evaluated on `old`/`new` before
    // dispatch; when it lands, this pin is replaced by predicate-match tests.
    assert!(
        trigger.matches("Order", EventKind::Update),
        "M-597: a second, unrelated update also matches — the fire-on-every-update gap"
    );
    // A different entity never matches (sanity: the matcher is not degenerate).
    assert!(!trigger.matches("Invoice", EventKind::Update));
}

/// Recursively collect `.rs` files under `dir`.
fn walk_rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rs_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
