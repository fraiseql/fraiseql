#![allow(clippy::unwrap_used)] // Reason: benchmark setup code
#![allow(missing_docs)]

use std::collections::HashMap;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fraiseql_federation::{
    FederatedType, FederationMetadata, KeyDirective, construct_where_in_clause,
    parse_field_selection, parse_representations, validate_representations, validate_subgraph_url,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_metadata(type_count: usize) -> FederationMetadata {
    let types = (0..type_count)
        .map(|i| FederatedType {
            name: format!("Type{i}"),
            keys: vec![KeyDirective {
                fields: vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends: false,
            external_fields: vec![],
            shareable_fields: vec![],
            field_directives: HashMap::new(),
        })
        .collect();

    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types,
    }
}

fn make_representations_json(count: usize) -> serde_json::Value {
    let reps: Vec<_> = (0..count).map(|i| json!({"__typename": "Type0", "id": i})).collect();
    json!(reps)
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn representation_parsing(c: &mut Criterion) {
    let metadata = make_metadata(5);
    let mut group = c.benchmark_group("parse_representations");

    for count in [1, 10, 100, 500, 1000] {
        let input = make_representations_json(count);
        group.bench_with_input(BenchmarkId::from_parameter(count), &input, |b, input| {
            b.iter(|| parse_representations(black_box(input), black_box(&metadata)).unwrap());
        });
    }
    group.finish();
}

fn representation_validation(c: &mut Criterion) {
    let metadata = make_metadata(5);
    let input = make_representations_json(100);
    let reps = parse_representations(&input, &metadata).unwrap();

    c.bench_function("validate_representations_100", |b| {
        b.iter(|| {
            validate_representations(black_box(&reps), black_box(&metadata)).unwrap();
        });
    });
}

fn where_in_clause(c: &mut Criterion) {
    let metadata = make_metadata(5);
    let mut group = c.benchmark_group("construct_where_in_clause");

    for count in [1, 10, 100, 500] {
        let input = make_representations_json(count);
        let reps = parse_representations(&input, &metadata).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(count), &reps, |b, reps| {
            b.iter(|| {
                construct_where_in_clause(black_box("Type0"), black_box(reps), black_box(&metadata))
                    .unwrap()
            });
        });
    }
    group.finish();
}

fn url_validation(c: &mut Criterion) {
    let valid_https = "https://subgraph.example.com/graphql";
    let valid_localhost = "http://localhost:4001/graphql";
    let private_ip = "http://10.0.0.1/graphql";
    let ipv6_loopback = "http://[::1]:4001/graphql";
    let long_url = format!("https://{}.example.com/graphql", "a".repeat(200));

    let urls: &[(&str, &str)] = &[
        ("valid_https", valid_https),
        ("valid_http_localhost", valid_localhost),
        ("private_ip", private_ip),
        ("ipv6_loopback", ipv6_loopback),
        ("long_url", &long_url),
    ];

    let mut group = c.benchmark_group("validate_subgraph_url");
    for (name, url) in urls {
        group.bench_with_input(BenchmarkId::new("url", name), url, |b, url| {
            b.iter(|| {
                let _ = validate_subgraph_url(black_box(url));
            });
        });
    }
    group.finish();
}

fn field_selection_parsing(c: &mut Criterion) {
    let wide = (0..50).map(|i| format!("field{i}")).collect::<Vec<_>>().join(" ");
    let queries: &[(&str, &str)] = &[
        ("simple", "id name email"),
        ("nested", "id name address { street city country { code } }"),
        ("wide", &wide),
    ];

    let mut group = c.benchmark_group("parse_field_selection");
    for (name, query) in queries {
        group.bench_with_input(BenchmarkId::new("query", name), query, |b, q| {
            b.iter(|| parse_field_selection(black_box(q)).unwrap());
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    representation_parsing,
    representation_validation,
    where_in_clause,
    url_validation,
    field_selection_parsing,
);
criterion_main!(benches);
