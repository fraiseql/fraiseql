use super::*;

#[test]
fn test_http_method_parsing() {
    let method = HttpMethod::new("GET");
    assert_eq!(method.as_str(), "GET");
    assert!(method.matches("get"));
    assert!(method.matches("GET"));
}

#[test]
fn test_http_route_exact_match() {
    let route = HttpTriggerRoute {
        function_name: "hello".to_string(),
        method: "GET".to_string(),
        path: "/hello".to_string(),
        requires_auth: false,
    };

    assert!(route.matches("GET", "/hello"));
    assert!(!route.matches("POST", "/hello"));
    assert!(!route.matches("GET", "/goodbye"));
}

#[test]
fn test_http_route_pattern_match() {
    let route = HttpTriggerRoute {
        function_name: "getUser".to_string(),
        method: "GET".to_string(),
        path: "/users/:id".to_string(),
        requires_auth: false,
    };

    assert!(route.pattern_matches("/users/123"));
    assert!(route.pattern_matches("/users/abc"));
    assert!(!route.pattern_matches("/users/123/posts"));
    assert!(!route.pattern_matches("/posts/123"));
}

#[test]
fn test_http_route_extract_params() {
    let route = HttpTriggerRoute {
        function_name: "getUser".to_string(),
        method: "GET".to_string(),
        path: "/users/:id/posts/:post_id".to_string(),
        requires_auth: false,
    };

    let params = route.extract_params("/users/123/posts/456");
    assert_eq!(params.get("id"), Some(&"123".to_string()));
    assert_eq!(params.get("post_id"), Some(&"456".to_string()));
}

#[test]
fn test_http_trigger_matcher() {
    let mut matcher = HttpTriggerMatcher::new();

    matcher.add(HttpTriggerRoute {
        function_name: "getUser".to_string(),
        method: "GET".to_string(),
        path: "/users/:id".to_string(),
        requires_auth: true,
    });

    matcher.add(HttpTriggerRoute {
        function_name: "createUser".to_string(),
        method: "POST".to_string(),
        path: "/users".to_string(),
        requires_auth: true,
    });

    // GET /users/:id should match
    let route = matcher.find("GET", "/users/123");
    assert!(route.is_some());
    assert_eq!(route.expect("route matched").function_name, "getUser");

    // POST /users should match
    let route = matcher.find("POST", "/users");
    assert!(route.is_some());
    assert_eq!(route.expect("route matched").function_name, "createUser");

    // GET /posts should not match
    let route = matcher.find("GET", "/posts");
    assert!(route.is_none());
}

#[test]
fn test_http_response_builder() {
    let response = HttpTriggerResponse::ok(serde_json::json!({"ok": true}));
    assert_eq!(response.status, 200);

    let response = HttpTriggerResponse::with_status(201, serde_json::json!({"id": 1}));
    assert_eq!(response.status, 201);
}
