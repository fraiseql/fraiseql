//! Instruction counts of a REST `GET` through the server's router, over an in-memory adapter.
//!
//! The REST half of the performance gate (#1636); the GraphQL half is
//! `fraiseql-core/benches/executor_instructions.rs`, which this shares its adapter, rows
//! and schema with. A request goes through `rest_router` with `tower::ServiceExt::oneshot`,
//! so routing, extraction, the engine and the response body are measured, and the socket
//! is not. Each shape runs once in setup first, so the measured request is a warm one.
//! glibc's fastbins are off for the reason the GraphQL half gives.
#![allow(missing_docs)] // Reason: library_benchmark!/main! macros generate undocumented items
#![allow(clippy::expect_used)] // Reason: benchmark setup code, a panic is the right failure

use std::{hint::black_box, sync::Arc};

use axum::{Router, body::Body};
use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::{RestMountConfig, rest_router},
};
use http::{Request, StatusCode};
use iai_callgrind::{LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main};
use tower::ServiceExt;

#[path = "../../fraiseql-core/benches/support/fixed_rows.rs"]
mod fixed_rows;
use fixed_rows::{FixedRows, user_row};

const ROWS: usize = 20;

struct Fixture {
    runtime: tokio::runtime::Runtime,
    router:  Router,
    uri:     &'static str,
}

/// Sends one `GET`, returning the status and the whole body.
async fn get(router: &Router, uri: &str) -> (StatusCode, axum::body::Bytes) {
    let request = Request::builder().uri(uri).body(Body::empty()).expect("request builds");
    let response = router.clone().oneshot(request).await.expect("router answers");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body reads");
    (status, body)
}

fn fixture(uri: &'static str, fat: bool) -> Fixture {
    let schema = CompiledSchema::from_json(fixed_rows::SCHEMA, false).expect("bench schema loads");
    let rows = (0..ROWS).map(|i| user_row(i, fat)).collect();
    let executor = Arc::new(Executor::new(schema, Arc::new(FixedRows(rows))));
    let router = rest_router(&AppState::new(executor), &RestMountConfig::default())
        .expect("REST router builds");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime builds");
    let (status, body) = runtime.block_on(get(&router, uri));
    assert_eq!(status, StatusCode::OK, "bench request must succeed: {body:?}");
    Fixture {
        runtime,
        router,
        uri,
    }
}

const LIST: &str = "/rest/v1/users?limit=20";
const BY_ID: &str = "/rest/v1/users/00000000-0000-4000-8000-000000000000";

/// Drops the fixture and the response outside the measured request.
fn teardown(done: (Fixture, axum::body::Bytes)) {
    drop(done);
}

#[library_benchmark]
#[bench::list_lean(args = (LIST, false), setup = fixture, teardown = teardown)]
#[bench::list_fat(args = (LIST, true), setup = fixture, teardown = teardown)]
#[bench::by_id_lean(args = (BY_ID, false), setup = fixture, teardown = teardown)]
#[bench::by_id_fat(args = (BY_ID, true), setup = fixture, teardown = teardown)]
fn rest_get(f: Fixture) -> (Fixture, axum::body::Bytes) {
    let (status, body) = f.runtime.block_on(get(&f.router, black_box(f.uri)));
    assert_eq!(status, StatusCode::OK, "the measured request must succeed");
    (f, black_box(body))
}

library_benchmark_group!(name = rest; benchmarks = rest_get);
main!(
    config = LibraryBenchmarkConfig::default().env("GLIBC_TUNABLES", "glibc.malloc.mxfast=0");
    library_benchmark_groups = rest
);
