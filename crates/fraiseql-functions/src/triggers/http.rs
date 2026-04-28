//! HTTP triggers: Custom HTTP endpoints backed by functions.
//!
//! HTTP triggers mount custom endpoints on the FraiseQL server that invoke
//! functions to handle requests and generate responses.
//!
//! # Trigger Format
//!
//! ```text
//! http:<METHOD>:<path>
//! http:GET:/hello
//! http:POST:/users/:id/avatar
//! http:DELETE:/cache/:key
//! ```
//!
//! # Request Mapping
//!
//! HTTP requests are mapped to `EventPayload` with:
//! - `trigger_type`: `"http:GET:/hello"`
//! - `entity`: `"HttpRequest"`
//! - `event_kind`: `"request"`
//! - `data`: Contains method, path, headers, query params, path params, body
//!
//! # Response Mapping
//!
//! Functions return `HttpTriggerResponse` JSON with:
//! ```json
//! {
//!   "status": 201,
//!   "headers": {"x-custom": "value"},
//!   "body": {...}
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP method for trigger routes.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct HttpMethod(pub String);

impl HttpMethod {
    /// Create a new HTTP method.
    pub fn new(method: &str) -> Self {
        Self(method.to_uppercase())
    }

    /// Check if this method matches another.
    pub fn matches(&self, other: &str) -> bool {
        self.0.eq_ignore_ascii_case(other)
    }

    /// Get the method as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Route for an HTTP trigger.
///
/// Defines a function that handles requests for a specific HTTP method and path.
///
/// # Execution
///
/// When a request matches this route, the function is invoked with an `EventPayload`
/// containing the request data (method, path, headers, body, params, query).
/// The function returns an HTTP response (status, headers, body).
#[derive(Debug, Clone)]
pub struct HttpTriggerRoute {
    /// Name of the function to invoke.
    pub function_name: String,
    /// HTTP method (GET, POST, etc.).
    pub method: String,
    /// Path pattern (e.g., "/users/:id").
    pub path: String,
    /// Whether authentication is required.
    pub requires_auth: bool,
}

impl HttpTriggerRoute {
    /// Create a new HTTP trigger route.
    pub fn new(function_name: &str, method: &str, path: &str) -> Self {
        Self {
            function_name: function_name.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            requires_auth: false,
        }
    }

    /// Builder method to require authentication.
    pub fn with_auth(mut self) -> Self {
        self.requires_auth = true;
        self
    }

    /// Builder method to not require authentication.
    pub fn without_auth(mut self) -> Self {
        self.requires_auth = false;
        self
    }

    /// Check if this route matches the given method and path.
    pub fn matches(&self, method: &str, path: &str) -> bool {
        self.method.eq_ignore_ascii_case(method) && self.path == path
    }

    /// Check if this route's path pattern matches a request path.
    ///
    /// Simple pattern matching: exact match or `*` for variable segments.
    pub fn pattern_matches(&self, request_path: &str) -> bool {
        let route_parts: Vec<&str> = self.path.split('/').collect();
        let request_parts: Vec<&str> = request_path.split('/').collect();

        if route_parts.len() != request_parts.len() {
            return false;
        }

        route_parts
            .iter()
            .zip(request_parts.iter())
            .all(|(route_part, request_part)| {
                // Exact match or parameter (e.g., ":id")
                route_part == request_part || route_part.starts_with(':')
            })
    }

    /// Extract path parameters from a request path.
    ///
    /// Returns a map of parameter names to values.
    pub fn extract_params(&self, request_path: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();

        let route_parts: Vec<&str> = self.path.split('/').collect();
        let request_parts: Vec<&str> = request_path.split('/').collect();

        for (route_part, request_part) in route_parts.iter().zip(request_parts.iter()) {
            if let Some(param_name) = route_part.strip_prefix(':') {
                params.insert(param_name.to_string(), request_part.to_string());
            }
        }

        params
    }
}

/// Request payload for HTTP trigger functions.
///
/// Passed to function as `EventPayload.data`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpTriggerPayload {
    /// HTTP method (GET, POST, etc.).
    pub method: String,
    /// Request path.
    pub path: String,
    /// Request headers.
    pub headers: serde_json::Value,
    /// Query parameters.
    pub query: serde_json::Value,
    /// Path parameters (extracted from route pattern).
    pub params: serde_json::Value,
    /// Request body (if any).
    pub body: Option<serde_json::Value>,
}

impl HttpTriggerPayload {
    /// Create a new HTTP trigger payload.
    pub fn new(
        method: &str,
        path: &str,
        headers: serde_json::Value,
        query: serde_json::Value,
        body: Option<serde_json::Value>,
    ) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            headers,
            query,
            params: serde_json::json!({}),
            body,
        }
    }

    /// Get a header value by name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<String> {
        let name_lower = name.to_lowercase();
        if let serde_json::Value::Object(ref obj) = self.headers {
            for (key, value) in obj {
                if key.to_lowercase() == name_lower {
                    return value.as_str().map(|s| s.to_string());
                }
            }
        }
        None
    }

    /// Get a query parameter value.
    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query.get(name).and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// Get a path parameter value.
    pub fn path_param(&self, name: &str) -> Option<String> {
        self.params.get(name).and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// Get the request body as JSON.
    pub fn json_body(&self) -> Option<&serde_json::Value> {
        self.body.as_ref()
    }

    /// Check if this is a GET request.
    pub fn is_get(&self) -> bool {
        self.method.eq_ignore_ascii_case("GET")
    }

    /// Check if this is a POST request.
    pub fn is_post(&self) -> bool {
        self.method.eq_ignore_ascii_case("POST")
    }

    /// Check if this is a PUT request.
    pub fn is_put(&self) -> bool {
        self.method.eq_ignore_ascii_case("PUT")
    }

    /// Check if this is a DELETE request.
    pub fn is_delete(&self) -> bool {
        self.method.eq_ignore_ascii_case("DELETE")
    }

    /// Check if this is a PATCH request.
    pub fn is_patch(&self) -> bool {
        self.method.eq_ignore_ascii_case("PATCH")
    }
}

/// Response from an HTTP trigger function.
///
/// Functions should return this format as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpTriggerResponse {
    /// HTTP status code (default 200).
    pub status: u16,
    /// Response headers.
    pub headers: serde_json::Value,
    /// Response body.
    pub body: serde_json::Value,
}

impl HttpTriggerResponse {
    /// Create a successful response with the given body.
    pub fn ok(body: serde_json::Value) -> Self {
        Self {
            status: 200,
            headers: serde_json::json!({}),
            body,
        }
    }

    /// Create a response with custom status and body.
    pub fn with_status(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            headers: serde_json::json!({}),
            body,
        }
    }

    /// Create a 201 Created response.
    pub fn created(body: serde_json::Value) -> Self {
        Self::with_status(201, body)
    }

    /// Create a 204 No Content response.
    pub fn no_content() -> Self {
        Self {
            status: 204,
            headers: serde_json::json!({}),
            body: serde_json::json!({}),
        }
    }

    /// Create a 400 Bad Request response.
    pub fn bad_request(message: &str) -> Self {
        Self::with_status(400, serde_json::json!({"error": message}))
    }

    /// Create a 401 Unauthorized response.
    pub fn unauthorized() -> Self {
        Self::with_status(401, serde_json::json!({"error": "Unauthorized"}))
    }

    /// Create a 403 Forbidden response.
    pub fn forbidden() -> Self {
        Self::with_status(403, serde_json::json!({"error": "Forbidden"}))
    }

    /// Create a 404 Not Found response.
    pub fn not_found() -> Self {
        Self::with_status(404, serde_json::json!({"error": "Not found"}))
    }

    /// Create a 500 Internal Server Error response.
    pub fn internal_error(message: &str) -> Self {
        Self::with_status(500, serde_json::json!({"error": message}))
    }

    /// Add a header to the response.
    pub fn with_header(mut self, key: String, value: String) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.headers {
            map.insert(key, serde_json::Value::String(value));
        }
        self
    }
}

/// Matcher for efficiently finding HTTP trigger routes.
///
/// Supports path parameter extraction and pattern matching.
#[derive(Debug, Clone, Default)]
pub struct HttpTriggerMatcher {
    /// Routes indexed by (method, path).
    routes: Vec<HttpTriggerRoute>,
}

impl HttpTriggerMatcher {
    /// Create a new empty HTTP trigger matcher.
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
        }
    }

    /// Add a route to the matcher.
    pub fn add(&mut self, route: HttpTriggerRoute) {
        self.routes.push(route);
    }

    /// Find a matching route for the given method and path.
    pub fn find(&self, method: &str, path: &str) -> Option<HttpTriggerRoute> {
        self.routes
            .iter()
            .find(|route| route.method.eq_ignore_ascii_case(method) && route.pattern_matches(path))
            .cloned()
    }

    /// Get all routes.
    pub fn routes(&self) -> &[HttpTriggerRoute] {
        &self.routes
    }
}

#[cfg(test)]
mod tests {
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
}
