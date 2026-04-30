//! Integration tests for Phase 19: Federation Maturity features.
//!
//! Tests cross-cutting scenarios spanning directives, caching,
//! observability, and health checking.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;
use std::time::Duration;

use fraiseql_federation::composition_validator::{CompositionError, CompositionValidator};
use fraiseql_federation::health::{SubgraphHealthAggregator, SubgraphHealthStatus};
use fraiseql_federation::observability::{EntityResolutionMetrics, SubgraphLatencyTracker};
use fraiseql_federation::query_plan_cache::{
    QueryPlan, QueryPlanCache, SubgraphFetch, normalize_query, schema_fingerprint,
};
use fraiseql_federation::service_sdl::generate_service_sdl;
use fraiseql_federation::types::{
    FederatedType, FederationMetadata, FieldFederationDirectives, KeyDirective,
};

// ---------------------------------------------------------------------------
// Directive completeness
// ---------------------------------------------------------------------------

#[test]
fn test_all_seven_directives_in_sdl() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![],
    };

    let sdl = generate_service_sdl("type Query { test: String }", &metadata);

    let required_directives = [
        "directive @key",
        "directive @external",
        "directive @requires",
        "directive @provides",
        "directive @shareable",
        "directive @inaccessible",
        "directive @override",
    ];

    for directive in &required_directives {
        assert!(sdl.contains(directive), "SDL missing {directive}:\n{sdl}");
    }
}

#[test]
fn test_link_directive_imports_all_directives() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![],
    };

    let sdl = generate_service_sdl("type Query { test: String }", &metadata);

    assert!(
        sdl.contains("@link(url: \"https://specs.apollo.dev/federation/v2.0\""),
        "SDL must contain @link: {sdl}"
    );

    for import in ["@key", "@external", "@requires", "@provides", "@shareable", "@inaccessible", "@override"] {
        assert!(
            sdl.contains(&format!("\"{import}\"")),
            "@link import missing {import}: {sdl}"
        );
    }
}

// ---------------------------------------------------------------------------
// Cross-directive composition validation
// ---------------------------------------------------------------------------

#[test]
fn test_composition_with_all_new_directives() {
    let mut users_type = FederatedType::new("User".to_string());
    users_type.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    users_type.set_field_directives(
        "email".to_string(),
        FieldFederationDirectives::new().shareable(),
    );
    users_type.set_field_directives(
        "ssn".to_string(),
        FieldFederationDirectives::new().inaccessible(),
    );

    let mut users_type_b = FederatedType::new("User".to_string());
    users_type_b.is_extends = true;
    users_type_b.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    users_type_b.set_field_directives(
        "email".to_string(),
        FieldFederationDirectives::new().shareable(),
    );
    users_type_b.set_field_directives(
        "ssn".to_string(),
        FieldFederationDirectives::new().inaccessible(),
    );

    let subgraphs = vec![
        (
            "users".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types:   vec![users_type],
            },
        ),
        (
            "auth".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types:   vec![users_type_b],
            },
        ),
    ];

    let validator = CompositionValidator::new();
    let result = validator.validate_composition(subgraphs);
    assert!(result.is_ok(), "Valid composition should pass: {result:?}");
}

#[test]
fn test_inaccessible_conflict_blocks_composition() {
    let mut type_a = FederatedType::new("Product".to_string());
    type_a.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    type_a.set_field_directives(
        "internal_code".to_string(),
        FieldFederationDirectives::new().inaccessible(),
    );

    let mut type_b = FederatedType::new("Product".to_string());
    type_b.is_extends = true;
    type_b.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    // NOT inaccessible — should conflict
    type_b.set_field_directives(
        "internal_code".to_string(),
        FieldFederationDirectives::new(),
    );

    let subgraphs = vec![
        (
            "catalog".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types:   vec![type_a],
            },
        ),
        (
            "pricing".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types:   vec![type_b],
            },
        ),
    ];

    let validator = CompositionValidator::new();
    let result = validator.validate_composition(subgraphs);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(e, CompositionError::InaccessibleFieldConflict { .. })));
}

// ---------------------------------------------------------------------------
// Query plan cache with schema evolution
// ---------------------------------------------------------------------------

#[test]
fn test_query_plan_cache_invalidated_on_schema_change() {
    let cache = QueryPlanCache::new(100);
    let fp_v1 = schema_fingerprint(&[("User", &["id"]), ("Order", &["id"])]);
    let fp_v2 = schema_fingerprint(&[("User", &["id", "email"]), ("Order", &["id"])]);

    let plan = QueryPlan {
        fetches:            vec![SubgraphFetch {
            subgraph:     "users".to_string(),
            query:        "{ user { name } }".to_string(),
            entity_types: vec!["User".to_string()],
            depends_on:   None,
        }],
        schema_fingerprint: fp_v1.clone(),
    };

    let query = normalize_query("query GetUser { user { name } }");
    cache.put(&query, plan);

    // Same schema version — cache hit
    assert!(cache.get(&query, &fp_v1).is_some());

    // Schema evolved — cache miss
    assert!(
        cache.get(&query, &fp_v2).is_none(),
        "stale plan should not be returned after schema change"
    );
}

#[test]
fn test_query_plan_cache_normalized_hit() {
    let cache = QueryPlanCache::new(100);
    let fp = schema_fingerprint(&[("User", &["id"])]);

    let plan = QueryPlan {
        fetches:            vec![],
        schema_fingerprint: fp.clone(),
    };

    let q1 = normalize_query("query  GetUser  {\n  user  {\n    name\n  }\n}");
    let q2 = normalize_query("query GetUser { user { name } }");

    cache.put(&q1, plan);
    assert!(cache.get(&q2, &fp).is_some(), "normalized queries should share cache entry");
}

// ---------------------------------------------------------------------------
// Observability end-to-end
// ---------------------------------------------------------------------------

#[test]
fn test_observability_full_flow() {
    let tracker = SubgraphLatencyTracker::new();
    let metrics = EntityResolutionMetrics::new();

    // Simulate a federation query with 2 subgraph fetches
    tracker.record("users", Duration::from_millis(12), 10, true);
    metrics.record_success(10);

    tracker.record("orders", Duration::from_millis(25), 5, true);
    metrics.record_success(5);

    // Verify tracker
    let attrs = tracker.to_span_attributes();
    assert_eq!(attrs.len(), 6); // 3 attrs × 2 subgraphs
    assert!(attrs.contains_key("federation.subgraph.users.latency_ms"));
    assert!(attrs.contains_key("federation.subgraph.orders.latency_ms"));

    // Verify metrics
    assert_eq!(metrics.successes(), 2);
    assert_eq!(metrics.entities_resolved(), 15);
    assert_eq!(metrics.failures(), 0);

    // Total latency
    assert_eq!(tracker.total_latency(), Duration::from_millis(37));
}

// ---------------------------------------------------------------------------
// Health check integration
// ---------------------------------------------------------------------------

#[test]
fn test_health_check_full_lifecycle() {
    let health = SubgraphHealthAggregator::new();

    // Register subgraphs
    health.register("users", "http://users:4000/graphql");
    health.register("orders", "http://orders:4001/graphql");
    health.register("products", "http://products:4002/graphql");

    // Initially all unknown
    let report = health.aggregate();
    assert_eq!(report.overall_status, SubgraphHealthStatus::Unknown);
    assert_eq!(report.subgraphs.len(), 3);

    // Health checks come in
    health.report_healthy("users", Duration::from_millis(5));
    health.report_healthy("orders", Duration::from_millis(10));
    health.report_unhealthy("products");

    let report = health.aggregate();
    assert_eq!(
        report.overall_status,
        SubgraphHealthStatus::Unhealthy,
        "one unhealthy should make overall unhealthy"
    );
    assert_eq!(report.healthy_count, 2);
    assert_eq!(report.unhealthy_count, 1);

    // Products recovers
    health.report_healthy("products", Duration::from_millis(20));

    let report = health.aggregate();
    assert_eq!(report.overall_status, SubgraphHealthStatus::Healthy);
    assert_eq!(report.healthy_count, 3);
    assert_eq!(report.unhealthy_count, 0);
}

// ---------------------------------------------------------------------------
// Multi-key entity resolution
// ---------------------------------------------------------------------------

#[test]
fn test_multi_key_where_clause_for_compound_keys() {
    use fraiseql_federation::entity_resolver::construct_batch_where_clause;
    use fraiseql_federation::types::EntityRepresentation;
    use serde_json::json;

    let mut rep = EntityRepresentation {
        typename:   "OrderItem".to_string(),
        key_fields: HashMap::new(),
        all_fields: HashMap::new(),
    };
    rep.key_fields.insert("order_id".to_string(), json!("O1"));
    rep.key_fields.insert("product_id".to_string(), json!("P1"));

    let clause = construct_batch_where_clause(
        &[rep],
        &["order_id".to_string(), "product_id".to_string()],
    )
    .unwrap();

    assert!(clause.contains("\"order_id\" IN"), "clause: {clause}");
    assert!(clause.contains("\"product_id\" IN"), "clause: {clause}");
    assert!(clause.contains("AND"), "compound key needs AND: {clause}");
}
