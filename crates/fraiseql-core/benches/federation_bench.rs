//! Performance benchmarks for federation operations
//!
//! Measures latency and throughput for:
//! - Entity representation parsing
//! - Strategy selection
//! - Batching and deduplication
//! - GraphQL query building

use fraiseql_core::federation::{
    mutation_http_client::{HttpMutationClient, HttpMutationConfig},
    types::{FederatedType, FederationMetadata, KeyDirective},
};
use serde_json::json;

fn create_test_metadata() -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![
            FederatedType {
                name:             "User".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
            FederatedType {
                name:             "Order".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       true,
                external_fields:  vec!["customerId".to_string()],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
        ],
    }
}

fn criterion_benchmark(c: &mut criterion::Criterion) {
    // ========================================================================
    // Entity Representation Parsing Benchmarks
    // ========================================================================

    c.bench_function("parse_single_entity_representation", |b| {
        b.iter(|| {
            let _rep = json!({
                "__typename": "User",
                "id": "user123",
                "name": "Alice"
            });
        });
    });

    c.bench_function("parse_batch_100_representations", |b| {
        b.iter(|| {
            let _representations: Vec<_> = (0..100)
                .map(|i| {
                    json!({
                        "__typename": "User",
                        "id": format!("user{}", i),
                        "name": format!("User{}", i)
                    })
                })
                .collect();
        });
    });

    c.bench_function("parse_composite_key_representation", |b| {
        b.iter(|| {
            let _rep = json!({
                "__typename": "TenantUser",
                "tenantId": "org123",
                "userId": "user456",
                "email": "user@example.com"
            });
        });
    });

    // ========================================================================
    // HTTP Mutation Client Benchmarks
    // ========================================================================

    c.bench_function("build_variable_definitions", |b| {
        let config = HttpMutationConfig::default();
        let client = HttpMutationClient::new(config);

        let variables = json!({
            "id": "user123",
            "name": "Alice",
            "email": "alice@example.com",
            "active": true
        });

        b.iter(|| {
            let _ = client.build_variable_definitions(&variables);
        });
    });

    c.bench_function("parse_graphql_response", |b| {
        let config = HttpMutationConfig::default();
        let client = HttpMutationClient::new(config);

        let response = fraiseql_core::federation::mutation_http_client::GraphQLResponse {
            data:   Some(json!({
                "updateUser": {
                    "__typename": "User",
                    "id": "user123",
                    "name": "Alice",
                    "email": "alice@example.com"
                }
            })),
            errors: None,
        };

        b.iter(|| {
            let _ = client.parse_response(response.clone(), "updateUser");
        });
    });

    c.bench_function("build_mutation_query", |b| {
        let config = HttpMutationConfig::default();
        let client = HttpMutationClient::new(config);

        let metadata = create_test_metadata();
        let fed_type = &metadata.types[1]; // Order (extended)

        let variables = json!({
            "id": "order123",
            "status": "shipped"
        });

        b.iter(|| {
            let _ = client.build_mutation_query("Order", "shipOrder", &variables, fed_type);
        });
    });

    // ========================================================================
    // Federation Metadata Operations Benchmarks
    // ========================================================================

    c.bench_function("lookup_entity_type_in_metadata", |b| {
        let metadata = create_test_metadata();

        b.iter(|| {
            let _ = metadata.types.iter().find(|t| t.name == "User");
        });
    });

    c.bench_function("check_is_local_vs_extended", |b| {
        let metadata = create_test_metadata();

        b.iter(|| {
            for fed_type in &metadata.types {
                let _ = !fed_type.is_extends;
            }
        });
    });

    c.bench_function("filter_external_fields", |b| {
        let metadata = create_test_metadata();
        let variables = json!({
            "id": "order123",
            "status": "shipped",
            "customerId": "cust456"
        });

        b.iter(|| {
            if let Some(order) = metadata.types.iter().find(|t| t.name == "Order") {
                let _filtered: Vec<_> = variables
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|k| !order.external_fields.contains(k))
                    .collect();
            }
        });
    });

    // ========================================================================
    // Batching and Deduplication Benchmarks
    // ========================================================================

    c.bench_function("deduplicate_100_representations", |b| {
        b.iter(|| {
            let mut ids = Vec::new();
            for i in 0..100 {
                ids.push(format!("user{}", i % 20)); // 20 unique IDs repeated
            }

            let mut unique = std::collections::HashSet::new();
            for id in &ids {
                unique.insert(id.clone());
            }

            assert_eq!(unique.len(), 20); // Should have 20 unique IDs
        });
    });

    c.bench_function("sort_representations_by_type", |b| {
        b.iter(|| {
            let mut representations = [
                ("User", "user1"),
                ("Order", "order1"),
                ("User", "user2"),
                ("Product", "product1"),
                ("Order", "order2"),
                ("User", "user3"),
            ];

            representations.sort_by_key(|r| r.0);
        });
    });

    // ========================================================================
    // Key Field Operations Benchmarks
    // ========================================================================

    c.bench_function("extract_key_fields_single", |b| {
        let metadata = create_test_metadata();

        b.iter(|| {
            if let Some(user) = metadata.types.iter().find(|t| t.name == "User") {
                let _ = &user.keys[0].fields;
            }
        });
    });

    c.bench_function("validate_composite_key_uniqueness", |b| {
        let key = KeyDirective {
            fields:     vec!["organizationId".to_string(), "userId".to_string()],
            resolvable: true,
        };

        b.iter(|| {
            let unique_fields: std::collections::HashSet<_> = key.fields.iter().collect();
            assert_eq!(unique_fields.len(), key.fields.len());
        });
    });
}

criterion::criterion_group!(benches, criterion_benchmark);
criterion::criterion_main!(benches);
