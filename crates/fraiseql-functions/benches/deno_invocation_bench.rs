//! Per-invocation cost of a Deno function, phase by phase (#1343).
//!
//! A single total hides which term regressed, so each phase of
//! [`run_in_dedicated_thread`] is its own benchmark, measured by the executor itself
//! ([`PhaseTimings`]) and summed over criterion's iterations with `iter_custom`:
//!
//! | benchmark | what it times |
//! |---|---|
//! | `deno_invocation/total` | one invocation, wall clock, thread spawn and join included |
//! | `deno_invocation/tokio_runtime` | the invocation's current-thread tokio runtime |
//! | `deno_invocation/isolate` | `JsRuntime::new` with the fraiseql extension |
//! | `deno_invocation/script` | compiling and running the wrapped guest |
//! | `deno_invocation/event_loop` | driving the guest's promise to completion |
//! | `deno_invocation/result` | reading the result back out of the isolate |
//! | `deno_invocation/teardown` | dropping the isolate and the tokio runtime |
//! | `deno_floor/bare_isolate` | `JsRuntime::new(RuntimeOptions::default())`: the floor |
//!
//! The guest is trivial on purpose: every number here is overhead a real function pays
//! before and after its own work.
//!
//! # Measured floor
//!
//! Recorded so that a regression shows up as a number moving. Release build, archbox
//! (AMD, Linux 7.0), criterion medians. Absolute numbers move with the box's load (it
//! also runs CI), so compare against `deno_floor/bare_isolate` from the same run: it is
//! an isolate built without the startup snapshot.
//!
//! | benchmark | before the snapshot | with the snapshot |
//! |---|---|---|
//! | `deno_invocation/total` | 4.58 ms | 1.90 ms |
//! | `deno_invocation/isolate` | 4.05 ms | 1.48 ms |
//! | `deno_invocation/script` | 0.06 ms | 0.09 ms |
//! | `deno_invocation/event_loop` | 0.39 ms | 0.07 ms |
//! | `deno_invocation/teardown` | (not split out) | 0.12 ms |
//! | `deno_floor/bare_isolate` | 3.86 ms | 4.37 ms |
//!
//! Before the snapshot, the fraiseql isolate cost what a bare one did plus ~0.2 ms of op
//! registration: almost all of an invocation was `deno_core` rebuilding its own global
//! environment. With it, isolate construction is about a third of the bare floor, and it
//! is still the largest term.
//!
//! Run with `cargo bench -p fraiseql-functions --features runtime-deno --bench
//! deno_invocation_bench`.

#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! generate undocumented items

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use deno_core::{JsRuntime, RuntimeOptions};
use fraiseql_functions::{
    ResourceLimits,
    runtime::deno::executor::{PhaseTimings, run_in_dedicated_thread},
};
use serde_json::json;

const GUEST: &str = "export default async (event) => ({ doubled: event.n * 2 });";

/// One invocation on its own thread, as production runs it.
fn invoke() -> (Duration, PhaseTimings) {
    let started = Instant::now();
    let out = std::thread::spawn(|| {
        run_in_dedicated_thread(GUEST, &json!({ "n": 21 }), &ResourceLimits::default(), None)
    })
    .join()
    .unwrap()
    .unwrap();
    (started.elapsed(), out.phases)
}

fn bench_phases(c: &mut Criterion) {
    // The first invocation initialises the process-wide V8 platform; keep it out of
    // every sample.
    invoke();

    let mut group = c.benchmark_group("deno_invocation");
    group.sample_size(30);
    let phases: [(&str, fn(Duration, &PhaseTimings) -> Duration); 7] = [
        ("total", |wall, _| wall),
        ("tokio_runtime", |_, p| p.tokio_runtime),
        ("isolate", |_, p| p.isolate),
        ("script", |_, p| p.script),
        ("event_loop", |_, p| p.event_loop),
        ("result", |_, p| p.result),
        ("teardown", |_, p| p.teardown),
    ];
    for (name, pick) in phases {
        group.bench_function(name, |b| {
            b.iter_custom(|iters| {
                (0..iters)
                    .map(|_| {
                        let (wall, p) = invoke();
                        pick(wall, &p)
                    })
                    .sum()
            });
        });
    }
    group.finish();

    let mut floor = c.benchmark_group("deno_floor");
    floor.sample_size(30);
    floor.bench_function("bare_isolate", |b| {
        b.iter_custom(|iters| {
            (0..iters)
                .map(|_| {
                    std::thread::spawn(|| {
                        let started = Instant::now();
                        let runtime = JsRuntime::new(RuntimeOptions::default());
                        let elapsed = started.elapsed();
                        drop(runtime);
                        elapsed
                    })
                    .join()
                    .unwrap()
                })
                .sum()
        });
    });
    floor.finish();
}

criterion_group!(benches, bench_phases);
criterion_main!(benches);
