//! Benchmarks for database pool performance.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use fraiseql_rs::db::{DatabaseConfig, ProductionPool, SslMode};
use tokio::runtime::Runtime;

fn create_runtime() -> Runtime {
    Runtime::new().unwrap()
}

fn benchmark_pool_creation(c: &mut Criterion) {
    c.bench_function("pool_creation_notls", |b| {
        b.iter(|| {
            let config = DatabaseConfig::new("benchmark").with_ssl_mode(SslMode::Disable);
            black_box(ProductionPool::new(config))
        });
    });
}

fn benchmark_connection_acquisition(c: &mut Criterion) {
    let rt = create_runtime();

    // Create pool once
    let config = DatabaseConfig::new("postgres")
        .with_ssl_mode(SslMode::Disable)
        .with_max_size(10);

    let pool = match ProductionPool::new(config) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Skipping connection benchmark: PostgreSQL not available");
            return;
        }
    };

    c.bench_function("connection_acquisition", |b| {
        b.to_async(&rt)
            .iter(|| async { black_box(pool.get_connection().await) });
    });
}

fn benchmark_concurrent_queries(c: &mut Criterion) {
    let rt = create_runtime();

    let config = DatabaseConfig::new("postgres")
        .with_ssl_mode(SslMode::Disable)
        .with_max_size(20);

    let pool = match ProductionPool::new(config) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Skipping concurrent benchmark: PostgreSQL not available");
            return;
        }
    };

    for num_concurrent in [1, 5, 10, 20] {
        c.bench_with_input(
            BenchmarkId::new("concurrent_queries", num_concurrent),
            &num_concurrent,
            |b, &num| {
                b.to_async(&rt).iter(|| async {
                    let mut handles = vec![];

                    for _ in 0..num {
                        let pool_clone = pool.clone();
                        let handle =
                            tokio::spawn(async move { pool_clone.execute_query("SELECT 1").await });
                        handles.push(handle);
                    }

                    for handle in handles {
                        black_box(handle.await);
                    }
                });
            },
        );
    }
}

fn benchmark_health_check(c: &mut Criterion) {
    let rt = create_runtime();

    let config = DatabaseConfig::new("postgres").with_ssl_mode(SslMode::Disable);

    let pool = match ProductionPool::new(config) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Skipping health check benchmark: PostgreSQL not available");
            return;
        }
    };

    c.bench_function("health_check", |b| {
        b.to_async(&rt)
            .iter(|| async { black_box(pool.health_check().await) });
    });
}

criterion_group!(
    benches,
    benchmark_pool_creation,
    benchmark_connection_acquisition,
    benchmark_concurrent_queries,
    benchmark_health_check
);
criterion_main!(benches);
