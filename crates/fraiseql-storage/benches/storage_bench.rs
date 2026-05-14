//! Storage performance benchmarks.
//!
//! Benchmarks local backend upload, download, and delete operations
//! at various payload sizes to establish throughput baselines.

#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! generate undocumented items

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fraiseql_storage::backend::local::LocalBackend;
use tokio::runtime::Runtime;

fn make_runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn bench_local_upload(c: &mut Criterion) {
    let rt = make_runtime();
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalBackend::new(dir.path().to_str().unwrap());

    let sizes: &[(&str, usize)] = &[
        ("1kb", 1_024),
        ("64kb", 65_536),
        ("1mb", 1_048_576),
    ];

    let mut group = c.benchmark_group("storage_local_upload");
    for (label, size) in sizes {
        let data: Vec<u8> = (0..(*size)).map(|i| (i % 256) as u8).collect();
        group.bench_with_input(BenchmarkId::from_parameter(label), label, |b, _| {
            b.iter(|| {
                rt.block_on(backend.upload(&format!("bench/{label}.bin"), &data, "application/octet-stream"))
                    .unwrap();
            });
        });
    }
    group.finish();
}

fn bench_local_download(c: &mut Criterion) {
    let rt = make_runtime();
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalBackend::new(dir.path().to_str().unwrap());

    let sizes: &[(&str, usize)] = &[
        ("1kb", 1_024),
        ("64kb", 65_536),
        ("1mb", 1_048_576),
    ];

    // Pre-seed the files
    for (label, size) in sizes {
        let data: Vec<u8> = (0..(*size)).map(|i| (i % 256) as u8).collect();
        rt.block_on(backend.upload(&format!("bench/{label}.bin"), &data, "application/octet-stream"))
            .unwrap();
    }

    let mut group = c.benchmark_group("storage_local_download");
    for (label, _) in sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), label, |b, _| {
            b.iter(|| {
                rt.block_on(backend.download(&format!("bench/{label}.bin")))
                    .unwrap();
            });
        });
    }
    group.finish();
}

fn bench_local_delete(c: &mut Criterion) {
    let rt = make_runtime();
    let dir = tempfile::tempdir().unwrap();
    let backend = LocalBackend::new(dir.path().to_str().unwrap());

    c.bench_function("storage_local_delete", |b| {
        b.iter(|| {
            let key = "bench/delete_me.bin";
            rt.block_on(backend.upload(key, b"hello", "application/octet-stream"))
                .unwrap();
            rt.block_on(backend.delete(key)).unwrap();
        });
    });
}

criterion_group!(benches, bench_local_upload, bench_local_download, bench_local_delete);
criterion_main!(benches);
