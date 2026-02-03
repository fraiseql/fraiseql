//! Integration tests for schema API endpoints

#[test]
fn test_graphql_schema_response_structure() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let response = GraphQLSchemaResponse {
        schema: "type Query { hello: String }".to_string(),
    };

    assert_eq!(response.schema, "type Query { hello: String }");
    assert!(!response.schema.is_empty());
}

#[test]
fn test_json_schema_response_structure() {
    use fraiseql_server::routes::api::schema::JsonSchemaResponse;

    let response = JsonSchemaResponse {
        schema: serde_json::json!({
            "types": [],
            "query_type": "Query"
        }),
    };

    assert!(response.schema.is_object());
    assert!(response.schema["types"].is_array());
    assert_eq!(response.schema["query_type"], "Query");
}

#[test]
fn test_graphql_schema_with_multiple_types() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let schema = r#"
type Query {
    users: [User!]!
    user(id: ID!): User
}

type User {
    id: ID!
    name: String!
    email: String!
}

type Mutation {
    createUser(name: String!): User
}
"#;

    let response = GraphQLSchemaResponse {
        schema: schema.to_string(),
    };

    assert!(response.schema.contains("type Query"));
    assert!(response.schema.contains("type User"));
    assert!(response.schema.contains("type Mutation"));
}

#[test]
fn test_json_schema_with_complete_structure() {
    use fraiseql_server::routes::api::schema::JsonSchemaResponse;

    let schema = serde_json::json!({
        "types": [
            {
                "name": "Query",
                "kind": "OBJECT",
                "fields": [
                    {"name": "users", "type": "[User!]!"}
                ]
            },
            {
                "name": "User",
                "kind": "OBJECT",
                "fields": [
                    {"name": "id", "type": "ID!"},
                    {"name": "name", "type": "String!"}
                ]
            }
        ],
        "query_type": "Query",
        "mutation_type": "Mutation"
    });

    let response = JsonSchemaResponse { schema };

    assert!(response.schema["types"].is_array());
    assert_eq!(response.schema["types"].as_array().unwrap().len(), 2);
    assert_eq!(response.schema["query_type"], "Query");
    assert_eq!(response.schema["mutation_type"], "Mutation");
}

#[test]
fn test_graphql_schema_json_serialization() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let response = GraphQLSchemaResponse {
        schema: "type Query { hello: String }".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"schema\":\"type Query { hello: String }\""));
}

#[test]
fn test_json_schema_json_serialization() {
    use fraiseql_server::routes::api::schema::JsonSchemaResponse;

    let response = JsonSchemaResponse {
        schema: serde_json::json!({
            "types": [],
            "query_type": "Query"
        }),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"query_type\":\"Query\""));
    assert!(json.contains("\"types\":[]"));
}

#[test]
fn test_graphql_schema_with_directives() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let schema = r#"
directive @auth(role: String!) on FIELD_DEFINITION
directive @deprecated(reason: String) on FIELD_DEFINITION

type Query {
    users: [User!]! @auth(role: "ADMIN")
    me: User @auth(role: "USER")
}
"#;

    let response = GraphQLSchemaResponse {
        schema: schema.to_string(),
    };

    assert!(response.schema.contains("@auth"));
    assert!(response.schema.contains("@deprecated"));
}

#[test]
fn test_graphql_schema_with_interfaces() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let schema = r#"
interface Node {
    id: ID!
}

type User implements Node {
    id: ID!
    name: String!
}

type Post implements Node {
    id: ID!
    title: String!
}
"#;

    let response = GraphQLSchemaResponse {
        schema: schema.to_string(),
    };

    assert!(response.schema.contains("interface Node"));
    assert!(response.schema.contains("implements Node"));
}

#[test]
fn test_graphql_schema_with_unions() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let schema = r#"
union SearchResult = User | Post | Comment

type Query {
    search(query: String!): [SearchResult!]!
}
"#;

    let response = GraphQLSchemaResponse {
        schema: schema.to_string(),
    };

    assert!(response.schema.contains("union SearchResult"));
    assert!(response.schema.contains("User | Post | Comment"));
}

#[test]
fn test_graphql_schema_with_input_types() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let schema = r#"
input CreateUserInput {
    name: String!
    email: String!
}

type Mutation {
    createUser(input: CreateUserInput!): User
}
"#;

    let response = GraphQLSchemaResponse {
        schema: schema.to_string(),
    };

    assert!(response.schema.contains("input CreateUserInput"));
    assert!(response.schema.contains("CreateUserInput!"));
}

#[test]
fn test_json_schema_federation_metadata() {
    use fraiseql_server::routes::api::schema::JsonSchemaResponse;

    let schema = serde_json::json!({
        "types": [],
        "query_type": "Query",
        "federation": {
            "enabled": true,
            "subgraphs": [
                {"name": "users", "url": "http://users.local"},
                {"name": "posts", "url": "http://posts.local"}
            ]
        }
    });

    let response = JsonSchemaResponse { schema };

    assert!(response.schema["federation"]["enabled"].as_bool().unwrap());
    assert_eq!(
        response.schema["federation"]["subgraphs"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn test_graphql_schema_empty() {
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let response = GraphQLSchemaResponse {
        schema: String::new(),
    };

    assert!(response.schema.is_empty());
}

#[test]
fn test_json_schema_minimal() {
    use fraiseql_server::routes::api::schema::JsonSchemaResponse;

    let response = JsonSchemaResponse {
        schema: serde_json::json!({}),
    };

    assert!(response.schema.is_object());
}

#[test]
fn test_api_response_wrapper_graphql_schema() {
    use fraiseql_server::routes::api::types::ApiResponse;
    use fraiseql_server::routes::api::schema::GraphQLSchemaResponse;

    let response = ApiResponse {
        status: "success".to_string(),
        data: GraphQLSchemaResponse {
            schema: "type Query { hello: String }".to_string(),
        },
    };

    assert_eq!(response.status, "success");
    assert!(!response.data.schema.is_empty());
}

#[test]
fn test_api_response_wrapper_json_schema() {
    use fraiseql_server::routes::api::types::ApiResponse;
    use fraiseql_server::routes::api::schema::JsonSchemaResponse;

    let response = ApiResponse {
        status: "success".to_string(),
        data: JsonSchemaResponse {
            schema: serde_json::json!({"types": []}),
        },
    };

    assert_eq!(response.status, "success");
    assert!(response.data.schema.is_object());
}
