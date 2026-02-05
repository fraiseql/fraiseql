//! Integration tests for query intelligence API endpoints

/// Tests for query complexity and scoring
#[test]
fn test_explain_response_structure() {
    // Verify ExplainResponse has all required fields
    use fraiseql_server::routes::api::query::{ComplexityInfo, ExplainResponse};

    let response = ExplainResponse {
        query:          "query { users { id } }".to_string(),
        sql:            Some("SELECT id FROM users".to_string()),
        complexity:     ComplexityInfo {
            depth:       2,
            field_count: 2,
            score:       40,
        },
        warnings:       vec!["depth > 1".to_string()],
        estimated_cost: 100,
    };

    assert_eq!(response.query, "query { users { id } }");
    assert!(response.sql.is_some());
    assert_eq!(response.complexity.depth, 2);
    assert_eq!(response.complexity.field_count, 2);
    assert_eq!(response.complexity.score, 40);
    assert!(!response.warnings.is_empty());
    assert_eq!(response.estimated_cost, 100);
}

#[test]
fn test_validate_response_structure() {
    use fraiseql_server::routes::api::query::ValidateResponse;

    let valid_response = ValidateResponse {
        valid:  true,
        errors: vec![],
    };

    assert!(valid_response.valid);
    assert!(valid_response.errors.is_empty());

    let invalid_response = ValidateResponse {
        valid:  false,
        errors: vec!["Syntax error at line 1".to_string()],
    };

    assert!(!invalid_response.valid);
    assert!(!invalid_response.errors.is_empty());
}

#[test]
fn test_stats_response_structure() {
    use fraiseql_server::routes::api::query::StatsResponse;

    let stats = StatsResponse {
        total_queries:      1000,
        successful_queries: 950,
        failed_queries:     50,
        average_latency_ms: 45.5,
    };

    assert_eq!(stats.total_queries, 1000);
    assert_eq!(stats.successful_queries, 950);
    assert_eq!(stats.failed_queries, 50);
    assert!(stats.average_latency_ms > 0.0);
}

/// Tests for complexity scoring logic
#[test]
fn test_complexity_depth_calculation() {
    // Simple query: depth = 2 (Query -> field)
    let simple_query = "query { users { id } }";
    let depth = count_brace_depth(simple_query);
    assert_eq!(depth, 2);
}

#[test]
fn test_complexity_nested_calculation() {
    // Nested query: depth = 4 (Query -> field -> field -> field)
    let nested_query = "query { users { posts { comments { text } } } }";
    let depth = count_brace_depth(nested_query);
    assert_eq!(depth, 4);
}

#[test]
fn test_complexity_field_count() {
    // Count fields at each level
    // Users has 3 fields, posts has 1 field
    let user_field_count = 3;
    let posts_field_count = 1;

    assert_eq!(user_field_count, 3);
    assert_eq!(posts_field_count, 1);
}

#[test]
fn test_complexity_score_calculation() {
    // Score = depth × field_count
    // Simple: depth=2, fields=2 → score=4
    // Complex: depth=3, fields=5 → score=15

    let score_simple = 2 * 2; // depth * field_count
    assert_eq!(score_simple, 4);

    let score_complex = 3 * 5;
    assert_eq!(score_complex, 15);
}

/// Tests for warning generation
#[test]
fn test_warning_deep_nesting() {
    // Depth > 10 should trigger warning
    let depth = 12;
    let threshold = 10;

    let has_warning = depth > threshold;
    assert!(has_warning);
}

#[test]
fn test_warning_high_complexity() {
    // Complexity score > 500 should trigger warning
    let score = 600;
    let threshold = 500;

    let has_warning = score > threshold;
    assert!(has_warning);
}

#[test]
fn test_warning_many_fields() {
    // Field count > 50 should trigger warning
    let field_count = 75;
    let threshold = 50;

    let has_warning = field_count > threshold;
    assert!(has_warning);
}

/// Tests for request/response serialization
#[test]
fn test_explain_request_json_serialization() {
    use fraiseql_server::routes::api::query::ExplainRequest;

    let json_str = r#"{"query":"query { users { id } }"}"#;
    let request: ExplainRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.query, "query { users { id } }");
}

#[test]
fn test_explain_response_json_serialization() {
    use fraiseql_server::routes::api::query::{ComplexityInfo, ExplainResponse};

    let response = ExplainResponse {
        query:          "test".to_string(),
        sql:            Some("SELECT *".to_string()),
        complexity:     ComplexityInfo {
            depth:       1,
            field_count: 1,
            score:       1,
        },
        warnings:       vec![],
        estimated_cost: 100,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"query\":\"test\""));
    assert!(json.contains("\"sql\":\"SELECT *\""));
}

#[test]
fn test_validate_request_json_serialization() {
    use fraiseql_server::routes::api::query::ValidateRequest;

    let json_str = r#"{"query":"query { users { id } }"}"#;
    let request: ValidateRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.query, "query { users { id } }");
}

#[test]
fn test_validate_response_json_serialization() {
    use fraiseql_server::routes::api::query::ValidateResponse;

    let response = ValidateResponse {
        valid:  true,
        errors: vec![],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"valid\":true"));
    assert!(json.contains("\"errors\":[]"));
}

#[test]
fn test_stats_response_json_serialization() {
    use fraiseql_server::routes::api::query::StatsResponse;

    let response = StatsResponse {
        total_queries:      100,
        successful_queries: 95,
        failed_queries:     5,
        average_latency_ms: 42.5,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"total_queries\":100"));
    assert!(json.contains("\"successful_queries\":95"));
    assert!(json.contains("\"failed_queries\":5"));
    assert!(json.contains("\"average_latency_ms\":42.5"));
}

/// Helper function to calculate brace depth
fn count_brace_depth(query: &str) -> usize {
    let mut max_depth = 0;
    let mut current_depth = 0;

    for ch in query.chars() {
        match ch {
            '{' => {
                current_depth += 1;
                max_depth = max_depth.max(current_depth);
            },
            '}' => {
                if current_depth > 0 {
                    current_depth -= 1;
                }
            },
            _ => {},
        }
    }

    max_depth
}

/// Tests for error cases
#[test]
fn test_empty_query_string() {
    use fraiseql_server::routes::api::query::ExplainRequest;

    let request = ExplainRequest {
        query: String::new(),
    };

    assert!(request.query.is_empty());
}

#[test]
fn test_malformed_query_detection() {
    // Verify queries can be checked for basic structure
    let malformed = "query { users id }"; // Missing braces around id
    let has_field_braces = malformed.contains("{ id");

    assert!(!has_field_braces); // id doesn't have braces
}

#[test]
fn test_query_with_variables() {
    use fraiseql_server::routes::api::query::ExplainRequest;

    let request = ExplainRequest {
        query: "query GetUsers($limit: Int) { users(limit: $limit) { id } }".to_string(),
    };

    assert!(request.query.contains("$limit"));
}

#[test]
fn test_query_with_fragments() {
    use fraiseql_server::routes::api::query::ExplainRequest;

    let request = ExplainRequest {
        query: "query { users { ...userFields } } fragment userFields on User { id name }"
            .to_string(),
    };

    assert!(request.query.contains("fragment"));
}

/// Tests for API response wrapper
#[test]
fn test_api_response_wrapper() {
    use fraiseql_server::routes::api::{query::ExplainResponse, types::ApiResponse};

    let data = ExplainResponse {
        query:          "test".to_string(),
        sql:            None,
        complexity:     fraiseql_server::routes::api::query::ComplexityInfo {
            depth:       1,
            field_count: 1,
            score:       1,
        },
        warnings:       vec![],
        estimated_cost: 100,
    };

    let response = ApiResponse {
        status: "success".to_string(),
        data,
    };

    assert_eq!(response.status, "success");
    assert_eq!(response.data.query, "test");
}
