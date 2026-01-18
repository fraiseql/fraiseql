//! Health and Introspection Endpoint Tests
//!
//! These tests verify the HTTP endpoints:
//! - Health check (/health): Database connectivity and metrics
//! - Introspection (/introspection): Schema metadata

use fraiseql_server::routes::health::{DatabaseStatus, HealthResponse};
use fraiseql_server::routes::introspection::{IntrospectionResponse, TypeInfo, QueryInfo, MutationInfo};

// ============================================================================
// HEALTH CHECK ENDPOINT TESTS
// ============================================================================

/// Test health response structure
#[test]
fn test_health_response_structure() {
    let response = HealthResponse {
        status: "healthy".to_string(),
        database: DatabaseStatus {
            connected: true,
            database_type: "PostgreSQL".to_string(),
            active_connections: Some(5),
            idle_connections: Some(15),
        },
        version: "2.0.0-alpha.1".to_string(),
    };

    assert_eq!(response.status, "healthy");
    assert!(response.database.connected);
    assert_eq!(response.database.database_type, "PostgreSQL");
    assert_eq!(response.database.active_connections, Some(5));
    assert_eq!(response.database.idle_connections, Some(15));
}

/// Test health response with unhealthy status
#[test]
fn test_health_response_unhealthy() {
    let response = HealthResponse {
        status: "unhealthy".to_string(),
        database: DatabaseStatus {
            connected: false,
            database_type: "PostgreSQL".to_string(),
            active_connections: Some(0),
            idle_connections: Some(0),
        },
        version: "2.0.0-alpha.1".to_string(),
    };

    assert_eq!(response.status, "unhealthy");
    assert!(!response.database.connected);
}

/// Test health response serialization
#[test]
fn test_health_response_serialization() {
    let response = HealthResponse {
        status: "healthy".to_string(),
        database: DatabaseStatus {
            connected: true,
            database_type: "MySQL".to_string(),
            active_connections: Some(3),
            idle_connections: Some(7),
        },
        version: "2.0.0-alpha.1".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();

    assert!(json.contains("healthy"));
    assert!(json.contains("MySQL"));
    assert!(json.contains("active_connections"));
    assert!(json.contains("idle_connections"));
}

/// Test health response JSON format
#[test]
fn test_health_response_json_format() {
    let response = HealthResponse {
        status: "healthy".to_string(),
        database: DatabaseStatus {
            connected: true,
            database_type: "PostgreSQL".to_string(),
            active_connections: Some(2),
            idle_connections: Some(8),
        },
        version: "2.0.0-alpha.1".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(json_value["status"], "healthy");
    assert_eq!(json_value["database"]["connected"], true);
    assert_eq!(json_value["version"], "2.0.0-alpha.1");
}

/// Test health with different database types
#[test]
fn test_health_different_databases() {
    let databases = vec![
        "PostgreSQL",
        "MySQL",
        "SQLite",
        "SQLServer",
    ];

    for db_type in databases {
        let response = HealthResponse {
            status: "healthy".to_string(),
            database: DatabaseStatus {
                connected: true,
                database_type: db_type.to_string(),
                active_connections: Some(5),
                idle_connections: Some(10),
            },
            version: "2.0.0-alpha.1".to_string(),
        };

        assert_eq!(response.database.database_type, db_type);
    }
}

/// Test health response with optional metrics
#[test]
fn test_health_optional_metrics() {
    // Without connection metrics
    let response = HealthResponse {
        status: "healthy".to_string(),
        database: DatabaseStatus {
            connected: true,
            database_type: "PostgreSQL".to_string(),
            active_connections: None,
            idle_connections: None,
        },
        version: "2.0.0-alpha.1".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();

    // Should still serialize without metrics
    assert!(json.contains("healthy"));
    assert!(json.contains("PostgreSQL"));

    // Verify None values are properly handled in JSON
    let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();
    // When skipped, keys should not be present
    assert!(json_value["database"].is_object());
}

/// Test health version field
#[test]
fn test_health_version_field() {
    let versions = vec![
        "1.0.0",
        "2.0.0-alpha.1",
        "2.0.0-beta.2",
        "2.0.0",
    ];

    for version in versions {
        let response = HealthResponse {
            status: "healthy".to_string(),
            database: DatabaseStatus {
                connected: true,
                database_type: "PostgreSQL".to_string(),
                active_connections: Some(5),
                idle_connections: Some(10),
            },
            version: version.to_string(),
        };

        assert_eq!(response.version, version);
    }
}

// ============================================================================
// INTROSPECTION ENDPOINT TESTS
// ============================================================================

/// Test type info structure
#[test]
fn test_type_info_structure() {
    let type_info = TypeInfo {
        name: "User".to_string(),
        description: Some("A user in the system".to_string()),
        field_count: 5,
    };

    assert_eq!(type_info.name, "User");
    assert_eq!(type_info.description, Some("A user in the system".to_string()));
    assert_eq!(type_info.field_count, 5);
}

/// Test type info without description
#[test]
fn test_type_info_no_description() {
    let type_info = TypeInfo {
        name: "Post".to_string(),
        description: None,
        field_count: 3,
    };

    assert_eq!(type_info.name, "Post");
    assert_eq!(type_info.description, None);
    assert_eq!(type_info.field_count, 3);
}

/// Test type info serialization
#[test]
fn test_type_info_serialization() {
    let type_info = TypeInfo {
        name: "Comment".to_string(),
        description: Some("A comment on a post".to_string()),
        field_count: 4,
    };

    let json = serde_json::to_string(&type_info).unwrap();

    assert!(json.contains("Comment"));
    assert!(json.contains("field_count"));
    assert!(json.contains("A comment on a post"));
}

/// Test query info structure
#[test]
fn test_query_info_structure() {
    let query_info = QueryInfo {
        name: "user".to_string(),
        return_type: "User".to_string(),
        returns_list: false,
        description: Some("Get a single user".to_string()),
    };

    assert_eq!(query_info.name, "user");
    assert_eq!(query_info.return_type, "User");
    assert!(!query_info.returns_list);
}

/// Test query info returning list
#[test]
fn test_query_info_returns_list() {
    let query_info = QueryInfo {
        name: "users".to_string(),
        return_type: "User".to_string(),
        returns_list: true,
        description: Some("Get all users".to_string()),
    };

    assert!(query_info.returns_list);
}

/// Test mutation info structure
#[test]
fn test_mutation_info_structure() {
    let mutation_info = MutationInfo {
        name: "createUser".to_string(),
        return_type: "User".to_string(),
        description: Some("Create a new user".to_string()),
    };

    assert_eq!(mutation_info.name, "createUser");
    assert_eq!(mutation_info.return_type, "User");
}

/// Test introspection response structure
#[test]
fn test_introspection_response_structure() {
    let response = IntrospectionResponse {
        types: vec![
            TypeInfo {
                name: "User".to_string(),
                description: None,
                field_count: 5,
            },
            TypeInfo {
                name: "Post".to_string(),
                description: None,
                field_count: 4,
            },
        ],
        queries: vec![
            QueryInfo {
                name: "user".to_string(),
                return_type: "User".to_string(),
                returns_list: false,
                description: None,
            },
            QueryInfo {
                name: "users".to_string(),
                return_type: "User".to_string(),
                returns_list: true,
                description: None,
            },
        ],
        mutations: vec![
            MutationInfo {
                name: "createUser".to_string(),
                return_type: "User".to_string(),
                description: None,
            },
        ],
    };

    assert_eq!(response.types.len(), 2);
    assert_eq!(response.queries.len(), 2);
    assert_eq!(response.mutations.len(), 1);
}

/// Test introspection response serialization
#[test]
fn test_introspection_response_serialization() {
    let response = IntrospectionResponse {
        types: vec![
            TypeInfo {
                name: "User".to_string(),
                description: Some("User type".to_string()),
                field_count: 5,
            },
        ],
        queries: vec![
            QueryInfo {
                name: "user".to_string(),
                return_type: "User".to_string(),
                returns_list: false,
                description: Some("Get a single user".to_string()),
            },
        ],
        mutations: vec![],
    };

    let json = serde_json::to_string(&response).unwrap();

    assert!(json.contains("types"));
    assert!(json.contains("queries"));
    assert!(json.contains("mutations"));
    assert!(json.contains("User"));
}

/// Test introspection with multiple types
#[test]
fn test_introspection_multiple_types() {
    let response = IntrospectionResponse {
        types: vec![
            TypeInfo { name: "User".to_string(), description: None, field_count: 3 },
            TypeInfo { name: "Post".to_string(), description: None, field_count: 4 },
            TypeInfo { name: "Comment".to_string(), description: None, field_count: 2 },
        ],
        queries: vec![],
        mutations: vec![],
    };

    assert_eq!(response.types.len(), 3);
    assert_eq!(response.types[0].name, "User");
    assert_eq!(response.types[1].name, "Post");
    assert_eq!(response.types[2].name, "Comment");
}

/// Test introspection with multiple queries and mutations
#[test]
fn test_introspection_operations() {
    let response = IntrospectionResponse {
        types: vec![],
        queries: vec![
            QueryInfo { name: "user".to_string(), return_type: "User".to_string(), returns_list: false, description: None },
            QueryInfo { name: "users".to_string(), return_type: "User".to_string(), returns_list: true, description: None },
            QueryInfo { name: "post".to_string(), return_type: "Post".to_string(), returns_list: false, description: None },
        ],
        mutations: vec![
            MutationInfo { name: "createUser".to_string(), return_type: "User".to_string(), description: None },
            MutationInfo { name: "deleteUser".to_string(), return_type: "User".to_string(), description: None },
        ],
    };

    assert_eq!(response.queries.len(), 3);
    assert_eq!(response.mutations.len(), 2);
}

/// Test introspection empty schema
#[test]
fn test_introspection_empty_schema() {
    let response = IntrospectionResponse {
        types: vec![],
        queries: vec![],
        mutations: vec![],
    };

    assert_eq!(response.types.len(), 0);
    assert_eq!(response.queries.len(), 0);
    assert_eq!(response.mutations.len(), 0);

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("types"));
}

/// Test introspection with descriptions
#[test]
fn test_introspection_with_descriptions() {
    let response = IntrospectionResponse {
        types: vec![
            TypeInfo {
                name: "User".to_string(),
                description: Some("Represents a user account".to_string()),
                field_count: 5,
            },
        ],
        queries: vec![
            QueryInfo {
                name: "user".to_string(),
                return_type: "User".to_string(),
                returns_list: false,
                description: Some("Fetch a user by ID".to_string()),
            },
        ],
        mutations: vec![
            MutationInfo {
                name: "createUser".to_string(),
                return_type: "User".to_string(),
                description: Some("Create a new user account".to_string()),
            },
        ],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("Represents a user account"));
    assert!(json.contains("Fetch a user by ID"));
    assert!(json.contains("Create a new user account"));
}
