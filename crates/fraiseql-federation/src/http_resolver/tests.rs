#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use std::collections::HashMap;

use super::*;

fn mock_representation(typename: &str, id: &str) -> EntityRepresentation {
    let mut key_fields = HashMap::new();
    key_fields.insert("id".to_string(), Value::String(id.to_string()));

    let mut all_fields = key_fields.clone();
    all_fields.insert("__typename".to_string(), Value::String(typename.to_string()));

    EntityRepresentation {
        typename: typename.to_string(),
        key_fields,
        all_fields,
    }
}

// ── SSRF / URL validation ─────────────────────────────────────────────────

#[test]
fn test_subgraph_url_allows_public_https() {
    validate_subgraph_url("https://api.example.com/graphql")
        .unwrap_or_else(|e| panic!("public HTTPS URL should be allowed: {e}"));
    validate_subgraph_url("https://subgraph.mycompany.io/")
        .unwrap_or_else(|e| panic!("public HTTPS URL should be allowed: {e}"));
}

#[test]
fn test_subgraph_url_rejects_http_scheme_unconditionally() {
    // http:// is rejected unconditionally — there is no bypass env var.
    let result = validate_subgraph_url("http://api.example.com/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for http:// scheme, got: {result:?}"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("https://"), "error should mention the https requirement: {msg}");
}

#[test]
fn test_subgraph_url_rejects_non_http_scheme() {
    let result = validate_subgraph_url("ftp://example.com/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for ftp:// scheme, got: {result:?}"
    );
    let result = validate_subgraph_url("file:///etc/passwd");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for file:// scheme, got: {result:?}"
    );
    let result = validate_subgraph_url("no-scheme-at-all");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for missing scheme, got: {result:?}"
    );
}

#[test]
fn test_subgraph_url_rejects_loopback() {
    let result = validate_subgraph_url("https://localhost/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for localhost, got: {result:?}"
    );
    let result = validate_subgraph_url("https://localhost:8080/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for localhost:8080, got: {result:?}"
    );
    let result = validate_subgraph_url("https://sub.localhost/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for sub.localhost, got: {result:?}"
    );
}

#[test]
fn test_subgraph_url_rejects_loopback_ip() {
    let result = validate_subgraph_url("https://127.0.0.1/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for 127.0.0.1, got: {result:?}"
    );
    let result = validate_subgraph_url("https://127.255.255.255/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for 127.255.255.255, got: {result:?}"
    );
}

#[test]
fn test_subgraph_url_rejects_private_ranges() {
    for url in [
        "https://10.0.0.1/graphql",
        "https://172.16.0.1/graphql",
        "https://172.31.255.255/graphql",
        "https://192.168.1.1/graphql",
    ] {
        let result = validate_subgraph_url(url);
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for private IP in {url}, got: {result:?}"
        );
    }
}

#[test]
fn test_subgraph_url_rejects_link_local() {
    let result = validate_subgraph_url("https://169.254.0.1/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for link-local 169.254.0.1, got: {result:?}"
    );
    let result = validate_subgraph_url("https://169.254.169.254/graphql"); // AWS metadata
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for link-local 169.254.169.254, got: {result:?}"
    );
}

#[test]
fn test_subgraph_url_rejects_cgnat() {
    let result = validate_subgraph_url("https://100.64.0.1/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for CGNAT 100.64.0.1, got: {result:?}"
    );
    let result = validate_subgraph_url("https://100.127.255.255/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for CGNAT 100.127.255.255, got: {result:?}"
    );
}

#[test]
fn test_subgraph_url_rejects_ipv6_loopback() {
    let result = validate_subgraph_url("https://[::1]/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for IPv6 loopback, got: {result:?}"
    );
}

#[test]
fn test_subgraph_url_rejects_ipv6_ula() {
    let result = validate_subgraph_url("https://[fc00::1]/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for IPv6 ULA fc00::1, got: {result:?}"
    );
    let result = validate_subgraph_url("https://[fd00::1]/graphql");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for IPv6 ULA fd00::1, got: {result:?}"
    );
}

// ── Existing tests (updated for new() returning Result) ───────────────────

#[test]
fn test_http_resolver_creation() {
    let config = HttpClientConfig::default();
    let _resolver = HttpEntityResolver::new(config, None).unwrap();
}

#[test]
fn test_empty_representations() {
    // Empty representations return early (no URL contact) — https:// check not triggered.
    let resolver = HttpEntityResolver::new(HttpClientConfig::default(), None).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let result = resolver
            .resolve_entities("https://example.com/graphql", &[], &FieldSelection::default())
            .await;

        let entities =
            result.unwrap_or_else(|e| panic!("empty representations should succeed: {e}"));
        assert_eq!(entities.len(), 0);
    });
}

#[test]
fn test_graphql_query_building() {
    let resolver = HttpEntityResolver::new(HttpClientConfig::default(), None).unwrap();
    let reps = vec![mock_representation("User", "123")];
    let selection = FieldSelection {
        fields: vec!["id".to_string(), "email".to_string()],
    };

    let request = resolver.build_entities_query(&reps, &selection).unwrap();

    assert!(request.query.contains("_entities"));
    assert!(request.query.contains("_Any!"));
    assert!(request.query.contains("User"));
    assert!(request.query.contains("id"));
    assert!(request.query.contains("email"));
}

#[test]
fn test_multiple_types_in_query() {
    let resolver = HttpEntityResolver::new(HttpClientConfig::default(), None).unwrap();
    let reps = vec![
        mock_representation("User", "123"),
        mock_representation("Order", "456"),
    ];
    let selection = FieldSelection {
        fields: vec!["id".to_string()],
    };

    let request = resolver.build_entities_query(&reps, &selection).unwrap();

    assert!(request.query.contains("User"));
    assert!(request.query.contains("Order"));
}

#[test]
fn test_response_parsing_success() {
    let resolver = HttpEntityResolver::new(HttpClientConfig::default(), None).unwrap();
    let representations = vec![mock_representation("User", "123")];

    let response = GraphQLResponse {
        data:   Some(json!({
            "_entities": [
                { "id": "123", "email": "user@example.com" }
            ]
        })),
        errors: None,
    };

    let entities = resolver
        .parse_response(&response, &representations)
        .unwrap_or_else(|e| panic!("parse_response should succeed for valid response: {e}"));
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());
}

#[test]
fn test_response_parsing_with_errors() {
    let resolver = HttpEntityResolver::new(HttpClientConfig::default(), None).unwrap();
    let representations = vec![mock_representation("User", "123")];

    let response = GraphQLResponse {
        data:   None,
        errors: Some(vec![GraphQLError::new("Entity not found")]),
    };

    let result = resolver.parse_response(&response, &representations);
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for GraphQL errors in response, got: {result:?}"
    );
}

#[test]
fn test_response_parsing_entity_count_mismatch() {
    let resolver = HttpEntityResolver::new(HttpClientConfig::default(), None).unwrap();
    let representations = vec![
        mock_representation("User", "123"),
        mock_representation("User", "456"),
    ];

    let response = GraphQLResponse {
        data:   Some(json!({
            "_entities": [
                { "id": "123" }
            ]
        })),
        errors: None,
    };

    let result = resolver.parse_response(&response, &representations);
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
        "expected Internal error for entity count mismatch, got: {result:?}"
    );
}

#[test]
fn test_config_defaults() {
    let config = HttpClientConfig::default();
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_delay_ms, 100);
}

#[test]
fn test_config_custom() {
    let config = HttpClientConfig {
        timeout_ms:     10000,
        max_retries:    5,
        retry_delay_ms: 200,
    };
    assert_eq!(config.timeout_ms, 10000);
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.retry_delay_ms, 200);
}

// ── URL-parser-based SSRF host extraction ─────────────────────────────────

#[test]
fn test_subgraph_url_rejects_ipv6_loopback_via_brackets() {
    // An attacker crafted URL with IPv6 loopback — the old split-based parser
    // was fragile against bracket notation; the url-crate parser is not.
    let result = validate_subgraph_url("https://[::1]/endpoint");
    assert!(result.is_err(), "IPv6 loopback must be rejected: {result:?}");
}

#[test]
fn test_subgraph_url_rejects_ipv6_private() {
    // fc00::/7 ULA — private range.
    let result = validate_subgraph_url("https://[fc00::1]/endpoint");
    assert!(result.is_err(), "IPv6 ULA must be rejected: {result:?}");
}

#[test]
fn test_subgraph_url_malformed_is_rejected() {
    let result = validate_subgraph_url("https://");
    assert!(result.is_err(), "URL with empty host must be rejected");
}

#[test]
fn test_subgraph_url_accepts_public_ipv6() {
    // 2001:db8::/32 is documentation range; real public addresses should pass.
    // Using a known-public, non-reserved address for test purposes.
    // 2606:4700:4700::1111 is Cloudflare DNS — public, non-reserved.
    let result = validate_subgraph_url("https://[2606:4700:4700::1111]/graphql");
    assert!(result.is_ok(), "public IPv6 address must be accepted: {result:?}");
}

// ── S23-H1: Entity resolver response body cap ─────────────────────────────

#[test]
fn entity_response_cap_constant_is_reasonable() {
    const { assert!(MAX_ENTITY_RESPONSE_BYTES >= 1024 * 1024) }
    const { assert!(MAX_ENTITY_RESPONSE_BYTES <= 500 * 1024 * 1024) }
}

#[tokio::test]
async fn entity_resolver_oversized_response_is_rejected() {
    use std::collections::HashMap;

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use crate::{selection_parser::FieldSelection, types::EntityRepresentation};

    let mock = MockServer::start().await;
    let oversized = vec![b'x'; MAX_ENTITY_RESPONSE_BYTES + 1];
    Mock::given(method("POST"))
        .and(path("/_entities"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
        .mount(&mock)
        .await;

    let config = HttpClientConfig {
        timeout_ms:     5000,
        max_retries:    1,
        retry_delay_ms: 0,
    };
    // new_for_test bypasses SSRF guard so we can reach the loopback mock server.
    let resolver = HttpEntityResolver::new_for_test(config).unwrap();
    let url = format!("{}/_entities", mock.uri());
    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
        all_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
    };
    let selection = FieldSelection::new(vec!["id".to_string()]);

    let result = resolver.resolve_entities(&url, &[repr], &selection).await;

    assert!(result.is_err(), "oversized entity response must be rejected");
    let msg = result.err().unwrap().to_string();
    assert!(msg.contains("too large"), "error must mention size limit: {msg}");
}

#[tokio::test]
async fn entity_resolver_valid_response_is_parsed() {
    use std::collections::HashMap;

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use crate::{selection_parser::FieldSelection, types::EntityRepresentation};

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/_entities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "_entities": [{ "id": "1", "__typename": "Order" }] }
        })))
        .mount(&mock)
        .await;

    let config = HttpClientConfig {
        timeout_ms:     5000,
        max_retries:    1,
        retry_delay_ms: 0,
    };
    let resolver = HttpEntityResolver::new_for_test(config).unwrap();
    let url = format!("{}/_entities", mock.uri());
    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
        all_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
    };
    let selection = FieldSelection::new(vec!["id".to_string()]);

    let result = resolver.resolve_entities(&url, &[repr], &selection).await;
    assert!(result.is_ok(), "valid entity response must be accepted");
}
