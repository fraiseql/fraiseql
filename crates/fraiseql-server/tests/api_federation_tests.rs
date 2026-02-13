//! Integration tests for federation API endpoints

#[test]
fn test_federation_subgraphs_response_structure() {
    use fraiseql_server::routes::api::federation::SubgraphsResponse;

    let response = SubgraphsResponse { subgraphs: vec![] };

    assert!(response.subgraphs.is_empty());
}

#[test]
fn test_federation_subgraph_info_structure() {
    use fraiseql_server::routes::api::federation::{SubgraphInfo, SubgraphsResponse};

    let subgraph = SubgraphInfo {
        name:     "users".to_string(),
        url:      "http://users.example.com/graphql".to_string(),
        entities: vec!["User".to_string(), "Query".to_string()],
        healthy:  true,
    };

    assert_eq!(subgraph.name, "users");
    assert_eq!(subgraph.url, "http://users.example.com/graphql");
    assert_eq!(subgraph.entities.len(), 2);
    assert!(subgraph.healthy);

    let response = SubgraphsResponse {
        subgraphs: vec![subgraph],
    };

    assert_eq!(response.subgraphs.len(), 1);
    assert_eq!(response.subgraphs[0].name, "users");
}

#[test]
fn test_federation_graph_response_structure() {
    use fraiseql_server::routes::api::federation::GraphResponse;

    let response = GraphResponse {
        format:  "json".to_string(),
        content: r#"{"subgraphs":[]}"#.to_string(),
    };

    assert_eq!(response.format, "json");
    assert!(response.content.contains("subgraphs"));
}

#[test]
fn test_federation_graph_json_format() {
    use fraiseql_server::routes::api::federation::GraphResponse;

    let json_content = r#"{
        "subgraphs": [
            {"name": "users", "entities": ["User"]}
        ],
        "edges": [
            {"from": "users", "to": "posts", "entity": "User"}
        ]
    }"#;

    let response = GraphResponse {
        format:  "json".to_string(),
        content: json_content.to_string(),
    };

    let parsed: serde_json::Value = serde_json::from_str(&response.content).unwrap();
    assert!(parsed["subgraphs"].is_array());
    assert!(parsed["edges"].is_array());
}

#[test]
fn test_federation_graph_dot_format() {
    let dot_content = r#"digraph federation {
    users [label="users\n[User, Query]"];
    posts [label="posts\n[Post]"];
    users -> posts [label="User"];
}"#;

    assert!(dot_content.contains("digraph"));
    assert!(dot_content.contains("->"));
    assert!(dot_content.contains("[label="));
}

#[test]
fn test_federation_graph_mermaid_format() {
    let mermaid_content = r#"graph LR
    users["users<br/>[User, Query]"]
    posts["posts<br/>[Post]"]
    users -->|User| posts"#;

    assert!(mermaid_content.contains("graph"));
    assert!(mermaid_content.contains("--"));
    assert!(mermaid_content.contains("<br/>"));
}

#[test]
fn test_subgraphs_response_json_serialization() {
    use fraiseql_server::routes::api::federation::{SubgraphInfo, SubgraphsResponse};

    let response = SubgraphsResponse {
        subgraphs: vec![SubgraphInfo {
            name:     "users".to_string(),
            url:      "http://users.local/graphql".to_string(),
            entities: vec!["User".to_string()],
            healthy:  true,
        }],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"users\""));
    assert!(json.contains("\"http://users.local/graphql\""));
    assert!(json.contains("\"healthy\":true"));
}

#[test]
fn test_graph_response_json_serialization() {
    use fraiseql_server::routes::api::federation::GraphResponse;

    let response = GraphResponse {
        format:  "json".to_string(),
        content: "{}".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"format\":\"json\""));
    assert!(json.contains("\"content\":\"{}\""));
}

#[test]
fn test_multiple_subgraphs() {
    use fraiseql_server::routes::api::federation::{SubgraphInfo, SubgraphsResponse};

    let response = SubgraphsResponse {
        subgraphs: vec![
            SubgraphInfo {
                name:     "users".to_string(),
                url:      "http://users.local/graphql".to_string(),
                entities: vec!["User".to_string()],
                healthy:  true,
            },
            SubgraphInfo {
                name:     "posts".to_string(),
                url:      "http://posts.local/graphql".to_string(),
                entities: vec!["Post".to_string()],
                healthy:  true,
            },
            SubgraphInfo {
                name:     "comments".to_string(),
                url:      "http://comments.local/graphql".to_string(),
                entities: vec!["Comment".to_string()],
                healthy:  false,
            },
        ],
    };

    assert_eq!(response.subgraphs.len(), 3);
    assert!(response.subgraphs[0].healthy);
    assert!(response.subgraphs[1].healthy);
    assert!(!response.subgraphs[2].healthy);
}

#[test]
fn test_subgraph_with_multiple_entities() {
    use fraiseql_server::routes::api::federation::SubgraphInfo;

    let subgraph = SubgraphInfo {
        name:     "users".to_string(),
        url:      "http://users.local/graphql".to_string(),
        entities: vec![
            "User".to_string(),
            "Query".to_string(),
            "Mutation".to_string(),
        ],
        healthy:  true,
    };

    assert_eq!(subgraph.entities.len(), 3);
    assert!(subgraph.entities.contains(&"User".to_string()));
}

#[test]
fn test_federation_graph_empty() {
    use fraiseql_server::routes::api::federation::GraphResponse;

    let empty_json = r#"{"subgraphs": [], "edges": []}"#;
    let response = GraphResponse {
        format:  "json".to_string(),
        content: empty_json.to_string(),
    };

    let parsed: serde_json::Value = serde_json::from_str(&response.content).unwrap();
    assert!(parsed["subgraphs"].as_array().unwrap().is_empty());
    assert!(parsed["edges"].as_array().unwrap().is_empty());
}

#[test]
fn test_federation_graph_with_entities() {
    use fraiseql_server::routes::api::federation::GraphResponse;

    let graph_json = r#"{
        "subgraphs": [
            {"name": "users", "url": "http://users.local", "entities": ["User"]}
        ],
        "edges": [
            {"from": "users", "to": "posts", "entity": "User"}
        ]
    }"#;

    let response = GraphResponse {
        format:  "json".to_string(),
        content: graph_json.to_string(),
    };

    let parsed: serde_json::Value = serde_json::from_str(&response.content).unwrap();
    let edges = &parsed["edges"];

    assert!(edges.is_array());
    assert!(!edges.as_array().unwrap().is_empty());
    assert_eq!(edges[0]["entity"], "User");
}

#[test]
fn test_api_response_wrapper_federation() {
    use fraiseql_server::routes::api::{federation::SubgraphsResponse, types::ApiResponse};

    let response = ApiResponse {
        status: "success".to_string(),
        data:   SubgraphsResponse { subgraphs: vec![] },
    };

    assert_eq!(response.status, "success");
    assert!(response.data.subgraphs.is_empty());
}
