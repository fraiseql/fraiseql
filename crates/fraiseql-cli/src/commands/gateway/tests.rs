#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod gateway_tests {
    use super::super::*;

    #[test]
    fn test_validate_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("gateway.toml");
        std::fs::write(
            &config_path,
            r#"
[gateway]
listen = "127.0.0.1:4000"

[gateway.subgraphs.users]
url = "http://localhost:4001/graphql"
"#,
        )
        .unwrap();

        let result = validate(config_path.to_str().unwrap()).unwrap();
        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_validate_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("gateway.toml");
        std::fs::write(
            &config_path,
            r#"
[gateway]
listen = "127.0.0.1:4000"
"#,
        )
        .unwrap();

        let result = validate(config_path.to_str().unwrap()).unwrap();
        assert_eq!(result.status, "validation-failed");
    }

    #[test]
    fn test_extract_fields_from_sdl() {
        let dir = tempfile::tempdir().unwrap();
        let sdl_path = dir.path().join("schema.graphql");
        std::fs::write(
            &sdl_path,
            r#"
type Query {
    users: [User!]!
    user(id: ID!): User
    products: [Product!]!
}

type User @key(fields: "id") {
    id: ID!
    name: String!
}
"#,
        )
        .unwrap();

        let fields = extract_fields_from_sdl(&sdl_path).unwrap();
        assert_eq!(fields, vec!["users", "user", "products"]);
    }

    #[test]
    fn test_extract_fields_no_query_type() {
        let dir = tempfile::tempdir().unwrap();
        let sdl_path = dir.path().join("schema.graphql");
        std::fs::write(
            &sdl_path,
            r"
type User {
    id: ID!
    name: String!
}
",
        )
        .unwrap();

        let fields = extract_fields_from_sdl(&sdl_path).unwrap();
        assert!(fields.is_empty());
    }
}

mod config_tests {
    use super::super::config::*;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_deserialize_minimal_config() {
        let toml_str = r#"
[gateway]
listen = "0.0.0.0:4000"

[gateway.subgraphs.users]
url = "http://localhost:4001/graphql"
"#;
        let file: GatewayConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(file.gateway.listen, "0.0.0.0:4000");
        assert_eq!(file.gateway.subgraphs.len(), 1);
        assert!(file.gateway.subgraphs.contains_key("users"));
    }

    #[test]
    fn test_deserialize_full_config() {
        let toml_str = r#"
[gateway]
listen = "0.0.0.0:4000"
playground = true

[gateway.subgraphs.users]
url = "http://localhost:4001/graphql"
schema = "./schemas/users.graphql"

[gateway.subgraphs.products]
url = "http://localhost:4002/graphql"

[gateway.timeouts]
subgraph_request_ms = 3000
total_request_ms = 15000

[gateway.circuit_breaker]
failure_threshold = 10
recovery_timeout_ms = 60000
"#;
        let file: GatewayConfigFile = toml::from_str(toml_str).unwrap();
        let gw = &file.gateway;

        assert!(gw.playground);
        assert_eq!(gw.subgraphs.len(), 2);
        assert_eq!(gw.timeouts.subgraph_request_ms, 3000);
        assert_eq!(gw.timeouts.total_request_ms, 15000);
        assert_eq!(gw.circuit_breaker.failure_threshold, 10);
        assert_eq!(gw.circuit_breaker.recovery_timeout_ms, 60000);

        let users = &gw.subgraphs["users"];
        assert_eq!(users.url, "http://localhost:4001/graphql");
        assert_eq!(users.schema.as_deref(), Some(Path::new("./schemas/users.graphql")));
    }

    #[test]
    fn test_defaults() {
        let toml_str = r#"
[gateway]

[gateway.subgraphs.svc]
url = "http://localhost:4001/graphql"
"#;
        let file: GatewayConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(file.gateway.listen, "127.0.0.1:4000");
        assert!(!file.gateway.playground);
        assert_eq!(file.gateway.timeouts.subgraph_request_ms, 5000);
        assert_eq!(file.gateway.timeouts.total_request_ms, 30000);
        assert_eq!(file.gateway.circuit_breaker.failure_threshold, 5);
    }

    #[test]
    fn test_validate_no_subgraphs() {
        let config = GatewayConfig {
            listen:          "127.0.0.1:4000".to_string(),
            playground:      false,
            subgraphs:       HashMap::new(),
            timeouts:        TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::NoSubgraphs)));
    }

    #[test]
    fn test_validate_invalid_url() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "bad".to_string(),
            SubgraphConfig {
                url:    "not a url".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::InvalidUrl { .. })));
    }

    #[test]
    fn test_validate_timeout_sanity() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "svc".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig {
                subgraph_request_ms: 10_000,
                total_request_ms:    5_000,
            },
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::TotalTimeoutTooSmall)));
    }

    #[test]
    fn test_validate_total_timeout_too_large() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "svc".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig {
                subgraph_request_ms: 5_000,
                total_request_ms:    999_999,
            },
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::TotalTimeoutTooLarge { .. })));
    }

    #[test]
    fn test_validate_valid_config() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "users".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: None,
            },
        );
        subgraphs.insert(
            "products".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4002/graphql".to_string(),
                schema: None,
            },
        );
        let config = GatewayConfig {
            listen: "0.0.0.0:4000".to_string(),
            playground: true,
            subgraphs,
            timeouts: TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        assert!(validate_config(&config, Path::new(".")).is_ok());
    }

    #[test]
    fn test_validate_schema_file_not_found() {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "svc".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: Some(PathBuf::from("./nonexistent.graphql")),
            },
        );
        let config = GatewayConfig {
            listen: "127.0.0.1:4000".to_string(),
            playground: false,
            subgraphs,
            timeouts: TimeoutConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        };
        let result = validate_config(&config, Path::new("."));
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ConfigError::SchemaFileNotFound { .. })));
    }
}

mod merger_tests {
    use serde_json::json;

    use super::super::merger::*;

    #[test]
    fn test_merge_single_response() {
        let responses = vec![(
            "users".to_string(),
            SubgraphResponse {
                data:   Some(json!({"users": [{"id": 1, "name": "Alice"}]})),
                errors: vec![],
            },
        )];

        let merged = merge_responses(&responses);
        assert_eq!(merged.data["users"][0]["name"], "Alice");
        assert!(merged.errors.is_empty());
    }

    #[test]
    fn test_merge_multiple_responses() {
        let responses = vec![
            (
                "users".to_string(),
                SubgraphResponse {
                    data:   Some(json!({"users": [{"id": 1}]})),
                    errors: vec![],
                },
            ),
            (
                "products".to_string(),
                SubgraphResponse {
                    data:   Some(json!({"products": [{"id": 100}]})),
                    errors: vec![],
                },
            ),
        ];

        let merged = merge_responses(&responses);
        assert!(merged.data["users"].is_array());
        assert!(merged.data["products"].is_array());
    }

    #[test]
    fn test_merge_errors_attributed() {
        let responses = vec![(
            "users".to_string(),
            SubgraphResponse {
                data:   Some(json!({"users": null})),
                errors: vec![GraphQLError {
                    message:    "Not found".to_string(),
                    path:       Some(vec![json!("users")]),
                    locations:  None,
                    extensions: None,
                }],
            },
        )];

        let merged = merge_responses(&responses);
        assert_eq!(merged.errors.len(), 1);
        assert_eq!(merged.errors[0].extensions.as_ref().unwrap()["subgraph"], "users");
    }

    #[test]
    fn test_merge_null_data() {
        let responses = vec![(
            "users".to_string(),
            SubgraphResponse {
                data:   None,
                errors: vec![GraphQLError {
                    message:    "Internal error".to_string(),
                    path:       None,
                    locations:  None,
                    extensions: None,
                }],
            },
        )];

        let merged = merge_responses(&responses);
        assert_eq!(merged.data, json!({}));
        assert_eq!(merged.errors.len(), 1);
    }

    #[test]
    fn test_merge_entity_fields() {
        let mut target = json!({"id": 1, "name": "Alice"});
        let entities = vec![json!({"email": "alice@example.com", "role": "admin"})];

        merge_entity_fields(&mut target, &entities);

        assert_eq!(target["email"], "alice@example.com");
        assert_eq!(target["role"], "admin");
        assert_eq!(target["name"], "Alice"); // preserved
    }

    #[test]
    fn test_merge_empty_responses() {
        let merged = merge_responses(&[]);
        assert_eq!(merged.data, json!({}));
        assert!(merged.errors.is_empty());
    }

    #[test]
    fn test_merge_preserves_error_paths() {
        let responses = vec![(
            "svc".to_string(),
            SubgraphResponse {
                data:   Some(json!({})),
                errors: vec![GraphQLError {
                    message:    "fail".to_string(),
                    path:       Some(vec![json!("users"), json!(0), json!("name")]),
                    locations:  Some(vec![json!({"line": 1, "column": 3})]),
                    extensions: Some(json!({"code": "INTERNAL"})),
                }],
            },
        )];

        let merged = merge_responses(&responses);
        let err = &merged.errors[0];
        assert!(err.path.is_some());
        assert!(err.locations.is_some());
        // Original extension "code" preserved + subgraph added
        assert_eq!(err.extensions.as_ref().unwrap()["code"], "INTERNAL");
        assert_eq!(err.extensions.as_ref().unwrap()["subgraph"], "svc");
    }
}

mod server_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::StatusCode;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::super::config::SubgraphConfig;
    use super::super::planner::FieldOwnership;
    use super::super::server::*;

    fn test_state() -> GatewayState {
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "users".to_string(),
            SubgraphConfig {
                url:    "http://localhost:4001/graphql".to_string(),
                schema: None,
            },
        );

        let mut ownership = FieldOwnership::default();
        ownership.insert("users".to_string(), "users".to_string());

        GatewayState {
            client: reqwest::Client::new(),
            subgraphs,
            ownership: Arc::new(ownership),
            subgraph_timeout: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = build_router(test_state());

        let response = app
            .oneshot(axum::http::Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "healthy");
    }

    #[tokio::test]
    async fn test_ready_endpoint() {
        let app = build_router(test_state());

        let response = app
            .oneshot(axum::http::Request::builder().uri("/ready").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ready");
        assert_eq!(json["subgraphs"], 1);
    }

    #[tokio::test]
    async fn test_graphql_empty_query() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/graphql")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"query": ""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_graphql_unknown_field() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/graphql")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"query": "{ nonexistent }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["errors"][0]["message"].as_str().unwrap().contains("nonexistent"));
    }
}

mod planner_tests {
    use super::super::planner::*;

    fn make_ownership() -> FieldOwnership {
        let mut fo = FieldOwnership::default();
        fo.insert("users".to_string(), "users-svc".to_string());
        fo.insert("user".to_string(), "users-svc".to_string());
        fo.insert("products".to_string(), "products-svc".to_string());
        fo.insert("orders".to_string(), "orders-svc".to_string());
        fo
    }

    #[test]
    fn test_plan_single_subgraph() {
        let ownership = make_ownership();
        let fields = vec!["users".to_string()];
        let plan = plan_query(&fields, &ownership).unwrap();
        assert_eq!(plan.fetches.len(), 1);
        assert_eq!(plan.fetches[0].subgraph, "users-svc");
        assert!(!plan.fetches[0].is_entity_fetch);
    }

    #[test]
    fn test_plan_groups_same_subgraph() {
        let ownership = make_ownership();
        let fields = vec!["users".to_string(), "user".to_string()];
        let plan = plan_query(&fields, &ownership).unwrap();
        assert_eq!(plan.fetches.len(), 1);
        assert_eq!(plan.fetches[0].subgraph, "users-svc");
    }

    #[test]
    fn test_plan_multiple_subgraphs() {
        let ownership = make_ownership();
        let fields = vec!["users".to_string(), "products".to_string()];
        let plan = plan_query(&fields, &ownership).unwrap();
        assert_eq!(plan.fetches.len(), 2);
        let subgraphs: Vec<&str> = plan.fetches.iter().map(|f| f.subgraph.as_str()).collect();
        assert!(subgraphs.contains(&"users-svc"));
        assert!(subgraphs.contains(&"products-svc"));
    }

    #[test]
    fn test_plan_unknown_field() {
        let ownership = make_ownership();
        let fields = vec!["nonexistent".to_string()];
        let result = plan_query(&fields, &ownership);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlanError::UnknownField { .. }));
    }

    #[test]
    fn test_plan_empty_query() {
        let ownership = make_ownership();
        let result = plan_query(&[], &ownership);
        assert!(matches!(result.unwrap_err(), PlanError::EmptyQuery));
    }

    #[test]
    fn test_entity_fetch_depth_exceeded() {
        let result = plan_entity_fetch("svc", &[], "id name", MAX_ENTITY_DEPTH);
        assert!(matches!(result.unwrap_err(), PlanError::DepthExceeded { .. }));
    }

    #[test]
    fn test_entity_fetch_ok() {
        let reps = vec![serde_json::json!({"__typename": "User", "id": "1"})];
        let fetch = plan_entity_fetch("users-svc", &reps, "name email", 0).unwrap();
        assert!(fetch.is_entity_fetch);
        assert_eq!(fetch.subgraph, "users-svc");
        assert!(fetch.query.contains("_entities"));
    }

    #[test]
    fn test_extract_root_fields_simple() {
        let fields = extract_root_fields("{ users products }");
        assert_eq!(fields, vec!["users", "products"]);
    }

    #[test]
    fn test_extract_root_fields_nested() {
        let fields = extract_root_fields("{ users { id name } products }");
        assert_eq!(fields, vec!["users", "products"]);
    }

    #[test]
    fn test_extract_root_fields_with_args() {
        let fields = extract_root_fields("{ user(id: 1) { name } products }");
        assert_eq!(fields, vec!["user", "products"]);
    }

    #[test]
    fn test_extract_root_fields_named_query() {
        let fields = extract_root_fields("query GetStuff { users orders }");
        assert_eq!(fields, vec!["users", "orders"]);
    }

    #[test]
    fn test_extract_root_fields_empty() {
        let fields = extract_root_fields("no braces here");
        assert!(fields.is_empty());
    }
}
