//! Integration tests for OpenAPI specification

#[test]
fn test_openapi_spec_is_valid_json() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&spec_json).expect("OpenAPI spec should be valid JSON");

    assert!(parsed.is_object(), "OpenAPI spec should be a JSON object");
}

#[test]
fn test_openapi_spec_has_required_fields() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    // Required OpenAPI 3.0 fields
    assert!(spec.get("openapi").is_some(), "Should have openapi version");
    assert!(spec.get("info").is_some(), "Should have info object");
    assert!(spec.get("paths").is_some(), "Should have paths object");
    assert!(spec.get("components").is_some(), "Should have components object");
}

#[test]
fn test_openapi_spec_has_correct_version() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let version = spec.get("openapi").and_then(|v| v.as_str());
    assert_eq!(version, Some("3.0.0"), "Should be OpenAPI 3.0.0");
}

#[test]
fn test_openapi_spec_has_info() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let info = &spec["info"];
    assert!(info.get("title").is_some(), "Info should have title");
    assert!(info.get("version").is_some(), "Info should have version");
}

#[test]
fn test_openapi_spec_documents_query_endpoints() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let paths = &spec["paths"];
    assert!(
        paths.get("/api/v1/query/explain").is_some(),
        "Should document /api/v1/query/explain"
    );
    assert!(
        paths.get("/api/v1/query/validate").is_some(),
        "Should document /api/v1/query/validate"
    );
}

#[test]
fn test_openapi_spec_documents_federation_endpoints() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let paths = &spec["paths"];
    assert!(
        paths.get("/api/v1/federation/subgraphs").is_some(),
        "Should document /api/v1/federation/subgraphs"
    );
    assert!(
        paths.get("/api/v1/federation/graph").is_some(),
        "Should document /api/v1/federation/graph"
    );
}

#[test]
fn test_openapi_spec_documents_schema_endpoints() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let paths = &spec["paths"];
    assert!(
        paths.get("/api/v1/schema.graphql").is_some(),
        "Should document /api/v1/schema.graphql"
    );
    assert!(
        paths.get("/api/v1/schema.json").is_some(),
        "Should document /api/v1/schema.json"
    );
}

#[test]
fn test_openapi_spec_documents_admin_endpoints() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let paths = &spec["paths"];
    assert!(
        paths.get("/api/v1/admin/reload-schema").is_some(),
        "Should document /api/v1/admin/reload-schema"
    );
    assert!(
        paths.get("/api/v1/admin/cache/clear").is_some(),
        "Should document /api/v1/admin/cache/clear"
    );
    assert!(
        paths.get("/api/v1/admin/config").is_some(),
        "Should document /api/v1/admin/config"
    );
}

#[test]
fn test_openapi_spec_has_request_schemas() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let schemas = &spec["components"]["schemas"];
    assert!(!schemas.is_null(), "Should have component schemas defined");

    // Check for some key request types
    assert!(
        schemas.get("ExplainRequest").is_some() || schemas.get("QueryExplainRequest").is_some(),
        "Should document query explain request schema"
    );
}

#[test]
fn test_openapi_spec_has_response_schemas() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let schemas = &spec["components"]["schemas"];
    assert!(!schemas.is_null(), "Should have component schemas defined");

    // Check for some key response types
    assert!(
        schemas.get("ExplainResponse").is_some() || schemas.get("QueryExplainResponse").is_some(),
        "Should document query explain response schema"
    );
}

#[test]
fn test_openapi_spec_documents_http_methods() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let paths = &spec["paths"];

    // Check explain endpoint uses POST
    let explain_post = paths["_api_v1_query_explain"]
        .get("post")
        .or_else(|| paths["/api/v1/query/explain"].get("post"));
    assert!(explain_post.is_some(), "Explain should use POST method");

    // Check subgraphs endpoint uses GET
    let subgraphs_get = paths["_api_v1_federation_subgraphs"]
        .get("get")
        .or_else(|| paths["/api/v1/federation/subgraphs"].get("get"));
    assert!(subgraphs_get.is_some(), "Subgraphs should use GET method");
}

#[test]
fn test_openapi_spec_has_all_10_endpoints() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let paths = &spec["paths"];

    // Count all endpoints
    let endpoint_count = paths.as_object().map(|m| m.len()).unwrap_or(0);

    // Should have 10 path entries for the API endpoints
    // (query: 3, federation: 2, schema: 2, admin: 3)
    assert!(
        endpoint_count >= 10,
        "Should document all API endpoints, found {}",
        endpoint_count
    );
}

#[test]
fn test_openapi_spec_has_security_scheme() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let security_schemes = &spec["components"]["securitySchemes"];
    assert!(!security_schemes.is_null(), "Should define security schemes for authentication");
}

#[test]
fn test_openapi_spec_endpoints_have_descriptions() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    let paths = &spec["paths"];

    // Check that at least one endpoint has description
    let has_descriptions = paths
        .as_object()
        .map(|paths| {
            paths.iter().any(|(_, endpoint)| {
                endpoint
                    .get("post")
                    .or_else(|| endpoint.get("get"))
                    .and_then(|op| op.get("description"))
                    .is_some()
            })
        })
        .unwrap_or(false);

    assert!(has_descriptions, "Endpoints should have descriptions");
}

#[test]
fn test_openapi_spec_is_valid_structure() {
    use fraiseql_server::routes::api::openapi::get_openapi_spec;

    let spec_json = get_openapi_spec();
    let spec: serde_json::Value = serde_json::from_str(&spec_json).unwrap();

    // Validate basic structure
    assert!(spec["openapi"].is_string(), "openapi field should be string");
    assert!(spec["info"].is_object(), "info should be object");
    assert!(spec["paths"].is_object(), "paths should be object");
    assert!(spec["components"].is_object(), "components should be object");
}
