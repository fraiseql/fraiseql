//! Realtime `WebSocket` performance benchmarks.
//!
//! Benchmarks connection registration, subscription fan-out lookup,
//! and security context hashing.

#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! generate undocumented items

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_server::realtime::{
    connections::{ConnectionManager, ConnectionState},
    context_hash::{SecurityContextHashInput, security_context_hash},
    subscriptions::{SubscriptionDetails, SubscriptionManager},
};

fn bench_connection_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("realtime_connection_insert");

    for n in [10_usize, 100, 1_000] {
        group.bench_with_input(BenchmarkId::new("n_connections", n), &n, |b, &n| {
            b.iter(|| {
                let mgr = ConnectionManager::new(3, 64);
                for i in 0..n {
                    let state = ConnectionState::new(
                        format!("conn-{i}"),
                        format!("user-{i}"),
                        u64::try_from(i % 10).unwrap(),
                        i64::MAX,
                    );
                    let _ = mgr.insert(state);
                }
                mgr.count()
            });
        });
    }
    group.finish();
}

fn bench_subscription_fanout_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("realtime_subscription_fanout");

    for n in [100_usize, 1_000, 10_000] {
        // Pre-register n connections subscribed to "Order" entity
        let mgr = SubscriptionManager::new(20_000);
        for i in 0..n {
            let details = SubscriptionDetails {
                event_filter: None,
                field_filters: vec![],
                security_context_hash: u64::try_from(i % 10).unwrap(),
            };
            let conn_id = format!("conn-{i}");
            mgr.subscribe(&conn_id, "Order", details).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("n_subscribers", n),
            &n,
            |b, _| {
                b.iter(|| {
                    mgr.get_subscribers(black_box("Order"))
                });
            },
        );
    }
    group.finish();
}

fn bench_context_hash(c: &mut Criterion) {
    let roles = vec!["admin", "user"];
    let scopes = vec!["read:profile", "write:orders"];

    c.bench_function("realtime_context_hash", |b| {
        b.iter(|| {
            security_context_hash(black_box(&SecurityContextHashInput {
                user_id: "user-12345",
                roles: &roles,
                tenant_id: Some("tenant-abc"),
                scopes: &scopes,
            }))
        });
    });
}

criterion_group!(
    benches,
    bench_connection_insert,
    bench_subscription_fanout_lookup,
    bench_context_hash
);
criterion_main!(benches);
