#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::run_in_dedicated_thread;
use crate::types::ResourceLimits;

/// Each phase of an invocation is measured, and the phases account for the whole of it:
/// a benchmark that reports them separately (#1343) is only as honest as this split.
#[test]
fn every_phase_of_an_invocation_is_timed() {
    let source = "export default async (event) => ({ doubled: event.n * 2 });";
    let started = std::time::Instant::now();
    let out = std::thread::spawn(move || {
        run_in_dedicated_thread(source, &json!({ "n": 21 }), &ResourceLimits::default(), None)
    })
    .join()
    .unwrap()
    .unwrap();
    let wall = started.elapsed();

    assert_eq!(out.value, json!({ "doubled": 42 }));
    let p = out.phases;
    for (name, d) in [
        ("tokio_runtime", p.tokio_runtime),
        ("isolate", p.isolate),
        ("script", p.script),
        ("event_loop", p.event_loop),
        ("result", p.result),
        ("teardown", p.teardown),
    ] {
        assert!(!d.is_zero(), "{name} was not timed: {p:?}");
    }
    assert!(p.total() <= wall, "the phases cannot exceed the invocation: {p:?} vs {wall:?}");
}
