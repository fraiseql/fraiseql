#![allow(clippy::unwrap_used)] // Reason: benchmark setup code, panics acceptable
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! macros generate undocumented items

//! GraphQL parse benchmarks.
//!
//! Measures `parse_query` throughput for simple and complex queries
//! to catch regressions in the parser hot path.
//!
//! # Running
//!
//! ```bash
//! cargo bench -p fraiseql-core --bench graphql_parse
//!
//! # Save a baseline for regression comparison:
//! cargo bench -p fraiseql-core --bench graphql_parse -- --save-baseline graphql-v1
//!
//! # Compare against baseline:
//! critcmp graphql-v1 graphql-v2
//! ```

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fraiseql_core::graphql::parse_query;

const SIMPLE_QUERY: &str = "{ users { id name } }";

const COMPLEX_QUERY: &str = r#"
    query GetUserWithPosts($userId: Int!, $limit: Int) {
        user(id: $userId) {
            id
            name
            email
            posts(limit: $limit) {
                id
                title
                published
                author { id name }
            }
        }
    }
"#;

const FRAGMENT_QUERY: &str = r#"
    fragment UserFields on User {
        id
        name
        email
    }

    fragment PostFields on Post {
        id
        title
        published
        author { ...UserFields }
    }

    query GetFeed($limit: Int, $offset: Int) {
        feed(limit: $limit, offset: $offset) {
            ...PostFields
        }
    }
"#;

fn bench_graphql_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("graphql_parse");

    group.bench_function(BenchmarkId::new("query", "simple"), |b| {
        b.iter(|| {
            let _ = parse_query(criterion::black_box(SIMPLE_QUERY));
        });
    });

    group.bench_function(BenchmarkId::new("query", "complex"), |b| {
        b.iter(|| {
            let _ = parse_query(criterion::black_box(COMPLEX_QUERY));
        });
    });

    group.bench_function(BenchmarkId::new("query", "fragments"), |b| {
        b.iter(|| {
            let _ = parse_query(criterion::black_box(FRAGMENT_QUERY));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_graphql_parse);
criterion_main!(benches);
