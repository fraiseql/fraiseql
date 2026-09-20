//! Benchmarks for the outbound mutation client a remote saga step dispatches with.
//!
//! Moved from `fraiseql-core`'s federation bench when the saga moved above the
//! engine (#1354); the entity-representation benches stayed there.
#![allow(missing_docs)] // Reason: criterion_group!/criterion_main! macros generate undocumented items
#![allow(clippy::unwrap_used)] // Reason: bench code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: criterion! macros generate undocumented items
#![allow(clippy::missing_errors_doc)] // Reason: criterion_group! macro generates undocumented items
use fraiseql_federation::types::{FederatedType, FederationMetadata, KeyDirective};
use fraiseql_saga::{HttpMutationClient, HttpMutationConfig};
use serde_json::json;

fn create_test_metadata() -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![
            FederatedType {
                name:                "User".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:          false,
                external_fields:     vec![],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
            FederatedType {
                name:                "Order".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:          true,
                external_fields:     vec!["customerId".to_string()],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
        ],
        remote_subscription_fields: std::collections::HashMap::new(),
    }
}

fn criterion_benchmark(c: &mut criterion::Criterion) {
    c.bench_function("build_variable_definitions", |b| {
        let config = HttpMutationConfig::default();
        let client = HttpMutationClient::new(config).unwrap();

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
        let client = HttpMutationClient::new(config).unwrap();

        let response = fraiseql_saga::mutation_http_client::GraphQLResponse {
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
        let client = HttpMutationClient::new(config).unwrap();

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
}

criterion::criterion_group!(benches, criterion_benchmark);
criterion::criterion_main!(benches);
