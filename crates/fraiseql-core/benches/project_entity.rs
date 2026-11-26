#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! macros generate undocumented items

//! The time the selection-driven projector takes per row.
//!
//! One stored `Order` row, keyed `snake_case` as a view stores it, with `width` integer
//! fields and a `shippingAddress` holding `width` more; every field selected. The allocation
//! count of the same projection is pinned in `memory_profile`.
//!
//! Run with: `cargo bench -p fraiseql-core --bench project_entity`.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fraiseql_core::runtime::project_entity;

#[path = "support/wide_order.rs"]
mod wide_order;

use wide_order::{row, schema, selections};

fn bench_project_entity(c: &mut Criterion) {
    let mut group = c.benchmark_group("project_entity");
    for width in [4, 16, 64] {
        let schema = schema(width);
        let selections = selections(width);
        let row = row(width);
        // The root's fields and the nested level's, plus the nested field itself.
        group.throughput(Throughput::Elements(2 * width as u64 + 1));
        group.bench_function(BenchmarkId::new("snake_case_row", width), |b| {
            b.iter(|| project_entity(black_box(&row), "Order", black_box(&selections), &schema));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_project_entity);
criterion_main!(benches);
