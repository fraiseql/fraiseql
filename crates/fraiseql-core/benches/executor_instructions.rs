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
//! `respond` measures a read whose rows need no shaping in Rust both ways, each to the
//! bytes a transport sends: `value_*` executes to a `Value` and serializes it, `raw_*`
//! splices the rows' text, which the adapter holds as the database would send it.
//! `numeric_*` does the same over rows of `Decimal`s and `Float`s, and `typename_*` over a
//! selection holding `__typename`, which the projection writes as a literal. The value path quotes
//! each row's `Decimal`s before it parses them, as the database adapter does; the raw path
//! receives them as the strings the projection reads, and writes each `Float`'s fraction
//! again as `serde_json` would.
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
use fixed_rows::{FixedRows, plain_user_row, reading_text, typename_user_row, user_row};

const ROWS: usize = 20;

struct Fixture {
    runtime:  tokio::runtime::Runtime,
    executor: Executor,
    query:    &'static str,
}

fn fixture(query: &'static str, fat: bool) -> Fixture {
    let schema = CompiledSchema::from_json(fixed_rows::SCHEMA, false).expect("bench schema loads");
    let rows = (0..ROWS).map(|i| user_row(i, fat)).collect();
    let executor = Executor::new(schema, Arc::new(FixedRows::new(rows)));
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

const PLAIN_LIST: &str = "{ users(limit: 20) { id name email bio } }";
const PLAIN_BY_ID: &str =
    r#"{ user(id: "00000000-0000-4000-8000-000000000000") { id name email bio } }"#;

/// A fixture over rows that also answer as text, the two paths checked to agree.
fn text_fixture(query: &'static str, fat: bool) -> Fixture {
    let rows = (0..ROWS).map(|i| plain_user_row(i, fat)).collect();
    text_fixture_over(fixed_rows::SCHEMA, FixedRows::with_text(rows), query)
}

/// [`text_fixture`] over `schema` and `adapter`.
fn text_fixture_over(schema: &str, adapter: FixedRows, query: &'static str) -> Fixture {
    let schema = CompiledSchema::from_json(schema, false).expect("bench schema loads");
    let executor = Executor::new(schema, Arc::new(adapter));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime builds");
    let value = runtime.block_on(executor.execute(query, None)).expect("warm-up call succeeds");
    assert!(value.get("errors").is_none(), "bench query must succeed: {value}");
    let body = runtime
        .block_on(executor.execute_operation_body(query, None, None))
        .expect("warm-up call succeeds");
    assert!(body.is_raw(), "bench query must be answered raw: {query}");
    assert_eq!(
        body.into_bytes().expect("raw body"),
        serde_json::to_vec(&value).expect("value serializes"),
        "the two paths must send the same bytes"
    );
    Fixture {
        runtime,
        executor,
        query,
    }
}

/// Drops the fixture and the body outside the measured call.
fn teardown_bytes(done: (Fixture, Vec<u8>)) {
    drop(done);
}

#[library_benchmark]
#[bench::value_list_lean(args = (PLAIN_LIST, false, false), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::value_list_fat(args = (PLAIN_LIST, true, false), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::value_by_id_lean(args = (PLAIN_BY_ID, false, false), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::value_by_id_fat(args = (PLAIN_BY_ID, true, false), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::raw_list_lean(args = (PLAIN_LIST, false, true), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::raw_list_fat(args = (PLAIN_LIST, true, true), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::raw_by_id_lean(args = (PLAIN_BY_ID, false, true), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::raw_by_id_fat(args = (PLAIN_BY_ID, true, true), setup = respond_fixture, teardown = teardown_bytes)]
#[bench::value_numeric_list(args = (false), setup = numeric_fixture, teardown = teardown_bytes)]
#[bench::raw_numeric_list(args = (true), setup = numeric_fixture, teardown = teardown_bytes)]
#[bench::value_typename_list(args = (false), setup = typename_fixture, teardown = teardown_bytes)]
#[bench::raw_typename_list(args = (true), setup = typename_fixture, teardown = teardown_bytes)]
fn respond((f, raw): (Fixture, bool)) -> (Fixture, Vec<u8>) {
    let query = black_box(f.query);
    let bytes = if raw {
        let body = f.runtime.block_on(f.executor.execute_operation_body(query, None, None));
        body.expect("bench query succeeds").into_bytes()
    } else {
        let value = f.runtime.block_on(f.executor.execute(query, None));
        serde_json::to_vec(&value.expect("bench query succeeds"))
    };
    let bytes = black_box(bytes.expect("the body serializes"));
    (f, bytes)
}

/// [`text_fixture`], and which path the measured call takes.
fn respond_fixture(query: &'static str, fat: bool, raw: bool) -> (Fixture, bool) {
    (text_fixture(query, fat), raw)
}

const TYPENAME_LIST: &str = "{ users(limit: 20) { id name __typename email bio } }";

/// A fixture over [`typename_user_row`] rows, and which path the measured call takes.
fn typename_fixture(raw: bool) -> (Fixture, bool) {
    let rows = (0..ROWS).map(typename_user_row).collect();
    (
        text_fixture_over(fixed_rows::SCHEMA, FixedRows::with_text(rows), TYPENAME_LIST),
        raw,
    )
}

const NUMERIC_LIST: &str =
    "{ readings(limit: 20) { id price cost tax total score ratio weight count } }";

/// A fixture over [`reading_text`] rows, and which path the measured call takes.
fn numeric_fixture(raw: bool) -> (Fixture, bool) {
    let rows = FixedRows::from_texts((0..ROWS).map(reading_text).collect());
    (
        text_fixture_over(include_str!("fixtures/numeric_schema.json"), rows, NUMERIC_LIST),
        raw,
    )
}

library_benchmark_group!(name = executor; benchmarks = execute, respond);
main!(
    config = LibraryBenchmarkConfig::default().env("GLIBC_TUNABLES", "glibc.malloc.mxfast=0");
    library_benchmark_groups = executor
);
