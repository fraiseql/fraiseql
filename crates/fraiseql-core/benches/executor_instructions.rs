//! Instruction counts of `Executor::execute` over an in-memory adapter.
//!
//! Wall-clock benches on a shared runner move by more than the regressions they are meant
//! to catch. Callgrind counts the instructions one call executes, which is deterministic
//! for a given binary, so a change of a few percent is a change in the code. The leg
//! that runs this (`tools/check-perf-instructions.py`) compares each count with
//! `benchmarks/instructions-baseline.json` and fails above 3%.
//!
//! The adapter returns fixed rows and does no I/O, so what is measured is the engine:
//! parsing, validation, planning, projection and response shaping. Each shape runs once
//! in setup first, so the measured call is a warm one (parse cache, lazily built state).
//!
//! Shapes: a root list, a by-id read, and a list with a nested list, each over lean rows
//! (about 200 bytes) and fat rows (about 4.5 KB).
//!
//! glibc's fastbins are off (`glibc.malloc.mxfast=0`). With them on, `malloc_consolidate`
//! runs inside the measured call or not depending on the heap state setup leaves, which
//! moves a count by about 4,400 instructions (3.5% of `by_id_lean`) with no code change.
//! The tunable is set in the bench config because the runner clears the environment.
#![allow(missing_docs)] // Reason: library_benchmark!/main! macros generate undocumented items
#![allow(clippy::expect_used)] // Reason: benchmark setup code, a panic is the right failure

use std::{hint::black_box, sync::Arc};

use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use iai_callgrind::{LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main};
use serde_json::Value;

#[path = "support/fixed_rows.rs"]
mod fixed_rows;
use fixed_rows::{FixedRows, user_row};

const ROWS: usize = 20;

struct Fixture {
    runtime:  tokio::runtime::Runtime,
    executor: Executor,
    query:    &'static str,
}

fn fixture(query: &'static str, fat: bool) -> Fixture {
    let schema = CompiledSchema::from_json(fixed_rows::SCHEMA, false).expect("bench schema loads");
    let rows = (0..ROWS).map(|i| user_row(i, fat)).collect();
    let executor = Executor::new(schema, Arc::new(FixedRows(rows)));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime builds");
    let warm = runtime.block_on(executor.execute(query, None)).expect("warm-up call succeeds");
    assert!(warm.get("errors").is_none(), "bench query must succeed: {warm}");
    Fixture {
        runtime,
        executor,
        query,
    }
}

const ROOT_LIST: &str = "{ users(limit: 20) { id name email bio createdAt } }";
const BY_ID: &str =
    r#"{ user(id: "00000000-0000-4000-8000-000000000000") { id name email bio createdAt } }"#;
const NESTED: &str = "{ users(limit: 20) { id name posts { id title published } } }";

/// Drops the fixture and the response outside the measured call.
fn teardown(done: (Fixture, Value)) {
    drop(done);
}

#[library_benchmark]
#[bench::root_list_lean(args = (ROOT_LIST, false), setup = fixture, teardown = teardown)]
#[bench::root_list_fat(args = (ROOT_LIST, true), setup = fixture, teardown = teardown)]
#[bench::by_id_lean(args = (BY_ID, false), setup = fixture, teardown = teardown)]
#[bench::by_id_fat(args = (BY_ID, true), setup = fixture, teardown = teardown)]
#[bench::nested_lean(args = (NESTED, false), setup = fixture, teardown = teardown)]
#[bench::nested_fat(args = (NESTED, true), setup = fixture, teardown = teardown)]
fn execute(f: Fixture) -> (Fixture, Value) {
    let out = f.runtime.block_on(f.executor.execute(black_box(f.query), None));
    let out = black_box(out.expect("bench query succeeds"));
    (f, out)
}

library_benchmark_group!(name = executor; benchmarks = execute);
main!(
    config = LibraryBenchmarkConfig::default().env("GLIBC_TUNABLES", "glibc.malloc.mxfast=0");
    library_benchmark_groups = executor
);
