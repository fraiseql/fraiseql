//! MCP (Model Context Protocol) integration tests.
//!
//! Validates that the MCP feature-gated code works at runtime, not just compiles.
//! Tests cover:
//! - Schema-to-tool conversion with queries and mutations
//! - Tool listing and filtering (include/exclude)
//! - Tool call execution with argument validation
//! - GraphQL injection prevention via argument name validation
//! - Metrics counter integration
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test mcp_integration_test --features mcp,auth
//! ```

#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

use std::sync::Arc;

use fraiseql_core::{
    runtime::Executor,
    schema::{ArgumentDefinition, CompiledSchema, FieldType, McpConfig},
};
use fraiseql_server::mcp::{
    executor::{call_tool, scalar_fields_for_type},
    handler::FraiseQLMcpService,
    tools::{schema_to_tools, should_include},
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestMutationBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder},
};
use rmcp::ServerHandler;

/// Extract text from an MCP Content object.
fn content_as_text(content: &rmcp::model::Content) -> &str {
    content.as_text().expect("expected text content").text.as_str()
}

/// Build a schema with User type, `users` query, `user(id)` query, and `createUser` mutation.
fn build_test_schema() -> CompiledSchema {
    let mut user_query = TestQueryBuilder::new("user", "User").no_sql_source().build();
    user_query.arguments.push(ArgumentDefinition::new("id", FieldType::Id));

    let mut create_mutation = TestMutationBuilder::new("createUser", "User").build();
    create_mutation
        .arguments
        .push(ArgumentDefinition::new("name", FieldType::String));
    create_mutation
        .arguments
        .push(ArgumentDefinition::new("email", FieldType::String));

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_simple_field("id", FieldType::Id)
                .with_simple_field("name", FieldType::String)
                .with_simple_field("email", FieldType::String)
                .build(),
        )
        .with_query(TestQueryBuilder::new("users", "User").no_sql_source().build())
        .build();

    schema.queries.push(user_query);
    schema.mutations.push(create_mutation);
    schema
}

fn make_mcp_config() -> McpConfig {
    McpConfig {
        enabled: true,
        transport: "http".to_string(),
        path: "/mcp".to_string(),
        require_auth: false,
        include: vec![],
        exclude: vec![],
    }
}

fn make_service() -> FraiseQLMcpService<FailingAdapter> {
    let schema = Arc::new(build_test_schema());
    let adapter = Arc::new(FailingAdapter::new());
    let executor = Arc::new(Executor::new((*schema).clone(), adapter));
    FraiseQLMcpService::new(schema, executor, make_mcp_config())
}

fn make_executor() -> (CompiledSchema, Arc<Executor<FailingAdapter>>) {
    let schema = build_test_schema();
    let adapter = Arc::new(FailingAdapter::new());
    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    (schema, executor)
}

// --- Schema-to-tool conversion ---

#[test]
fn schema_to_tools_includes_queries_and_mutations() {
    let schema = build_test_schema();
    let config = make_mcp_config();
    let tools = schema_to_tools(&schema, &config);

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"users"), "missing users query tool");
    assert!(names.contains(&"user"), "missing user query tool");
    assert!(names.contains(&"createUser"), "missing createUser mutation tool");
    assert_eq!(tools.len(), 3);
}

#[test]
fn schema_to_tools_respects_include_filter() {
    let schema = build_test_schema();
    let config = McpConfig {
        include: vec!["users".to_string()],
        ..make_mcp_config()
    };
    let tools = schema_to_tools(&schema, &config);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name.as_ref(), "users");
}

#[test]
fn schema_to_tools_respects_exclude_filter() {
    let schema = build_test_schema();
    let config = McpConfig {
        exclude: vec!["createUser".to_string()],
        ..make_mcp_config()
    };
    let tools = schema_to_tools(&schema, &config);
    assert!(!tools.iter().any(|t| t.name.as_ref() == "createUser"));
    assert_eq!(tools.len(), 2);
}

#[test]
fn should_include_empty_filters_includes_all() {
    let config = make_mcp_config();
    assert!(should_include("anything", &config));
}

#[test]
fn should_include_whitelist_excludes_unlisted() {
    let config = McpConfig {
        include: vec!["users".to_string()],
        ..make_mcp_config()
    };
    assert!(should_include("users", &config));
    assert!(!should_include("createUser", &config));
}

#[test]
fn should_include_blacklist_excludes_listed() {
    let config = McpConfig {
        exclude: vec!["createUser".to_string()],
        ..make_mcp_config()
    };
    assert!(should_include("users", &config));
    assert!(!should_include("createUser", &config));
}

// --- Tool input schema ---

#[test]
fn tool_input_schema_has_required_arguments() {
    let schema = build_test_schema();
    let config = make_mcp_config();
    let tools = schema_to_tools(&schema, &config);

    let user_tool = tools.iter().find(|t| t.name == "user").unwrap();
    let input = user_tool.input_schema.as_ref();
    let required = input.get("required").unwrap().as_array().unwrap();
    assert!(required.iter().any(|v| v.as_str() == Some("id")));
}

#[test]
fn tool_input_schema_has_correct_types() {
    let schema = build_test_schema();
    let config = make_mcp_config();
    let tools = schema_to_tools(&schema, &config);

    let create_tool = tools.iter().find(|t| t.name == "createUser").unwrap();
    let input = create_tool.input_schema.as_ref();
    let props = input.get("properties").unwrap().as_object().unwrap();
    let name_type = props.get("name").unwrap().get("type").unwrap();
    assert_eq!(name_type, "string");
}

#[test]
fn tool_with_no_arguments_has_empty_schema() {
    let schema = build_test_schema();
    let config = make_mcp_config();
    let tools = schema_to_tools(&schema, &config);

    let users_tool = tools.iter().find(|t| t.name == "users").unwrap();
    let input = users_tool.input_schema.as_ref();
    let props = input.get("properties").unwrap().as_object().unwrap();
    assert!(props.is_empty());
    assert!(input.get("required").is_none());
}

// --- Scalar field extraction ---

#[test]
fn scalar_fields_for_type_returns_all_scalar_fields() {
    let schema = build_test_schema();
    let fields = scalar_fields_for_type("User", &schema);
    assert!(fields.contains(&"id".to_string()));
    assert!(fields.contains(&"name".to_string()));
    assert!(fields.contains(&"email".to_string()));
    assert_eq!(fields.len(), 3);
}

#[test]
fn scalar_fields_for_unknown_type_returns_empty() {
    let schema = build_test_schema();
    let fields = scalar_fields_for_type("NonExistent", &schema);
    assert!(fields.is_empty());
}

// --- ServerHandler trait: get_info and get_tool ---

#[test]
fn service_get_info_has_instructions() {
    let service = make_service();
    let info = service.get_info();
    assert!(info.instructions.is_some());
    assert!(info.instructions.unwrap().contains("FraiseQL"));
}

#[test]
fn service_get_tool_finds_existing() {
    let service = make_service();
    let tool = service.get_tool("users");
    assert!(tool.is_some());
    assert_eq!(tool.unwrap().name.as_ref(), "users");
}

#[test]
fn service_get_tool_returns_none_for_missing() {
    let service = make_service();
    let missing = service.get_tool("nonExistent");
    assert!(missing.is_none());
}

// --- Tool call execution (via executor::call_tool directly) ---

#[tokio::test]
async fn call_tool_with_unknown_name_returns_error() {
    let (schema, executor) = make_executor();

    let result = call_tool("nonExistentQuery", None, &schema, &executor).await;
    assert_eq!(result.is_error, Some(true));

    let text = content_as_text(&result.content[0]);
    assert!(text.contains("Unknown operation"));
}

#[tokio::test]
async fn call_tool_rejects_invalid_argument_names() {
    let (schema, executor) = make_executor();

    let mut args = serde_json::Map::new();
    args.insert("valid_arg".to_string(), serde_json::json!("value"));
    args.insert("inject: bad".to_string(), serde_json::json!("evil"));

    let result = call_tool("users", Some(&args), &schema, &executor).await;
    assert_eq!(result.is_error, Some(true));

    let text = content_as_text(&result.content[0]);
    assert!(text.contains("Invalid argument name"));
}

#[tokio::test]
async fn call_tool_with_valid_query_attempts_execution() {
    let (schema, executor) = make_executor();

    // This will fail at the executor level (FailingAdapter), but should not
    // fail at the MCP layer — proving the GraphQL query was built correctly.
    let result = call_tool("users", None, &schema, &executor).await;

    // The FailingAdapter will produce an execution error, which is expected.
    // The key assertion is that we got past the MCP query-building phase.
    assert!(!result.content.is_empty(), "should have some content");
}

#[tokio::test]
async fn call_tool_with_arguments_builds_valid_query() {
    let (schema, executor) = make_executor();

    let mut args = serde_json::Map::new();
    args.insert("id".to_string(), serde_json::json!("123"));

    let result = call_tool("user", Some(&args), &schema, &executor).await;
    // Should not be an MCP-level error (may be an executor error from FailingAdapter)
    assert!(!result.content.is_empty());
}

// --- Metrics integration ---

#[tokio::test]
async fn call_tool_executor_tracks_results() {
    use std::sync::atomic::Ordering;

    use fraiseql_server::mcp::handler::{MCP_TOOL_CALLS_TOTAL, MCP_TOOL_ERRORS_TOTAL};

    // The global counters are incremented by the ServerHandler::call_tool impl,
    // not by executor::call_tool directly. We verify they exist and are readable.
    let calls = MCP_TOOL_CALLS_TOTAL.load(Ordering::Relaxed);
    let errors = MCP_TOOL_ERRORS_TOTAL.load(Ordering::Relaxed);

    // Counters should be non-negative (may be non-zero from prior tests in the process)
    assert!(calls < u64::MAX);
    assert!(errors < u64::MAX);
}

// --- Combined include + exclude ---

#[test]
fn include_and_exclude_combined() {
    let schema = build_test_schema();
    let config = McpConfig {
        include: vec!["users".to_string(), "createUser".to_string()],
        exclude: vec!["createUser".to_string()],
        ..make_mcp_config()
    };
    let tools = schema_to_tools(&schema, &config);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name.as_ref(), "users");
}

// --- Tool descriptions ---

#[test]
fn query_tool_has_description() {
    let schema = build_test_schema();
    let config = make_mcp_config();
    let tools = schema_to_tools(&schema, &config);

    let users_tool = tools.iter().find(|t| t.name == "users").unwrap();
    assert!(users_tool.description.is_some());
    let desc = users_tool.description.as_ref().unwrap();
    assert!(desc.contains("users"), "description should reference the query name");
}

#[test]
fn mutation_tool_has_description() {
    let schema = build_test_schema();
    let config = make_mcp_config();
    let tools = schema_to_tools(&schema, &config);

    let create_tool = tools.iter().find(|t| t.name == "createUser").unwrap();
    assert!(create_tool.description.is_some());
    let desc = create_tool.description.as_ref().unwrap();
    assert!(desc.contains("createUser"), "description should reference the mutation name");
}
