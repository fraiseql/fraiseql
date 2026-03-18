//! MCP end-to-end tests.
//!
//! Exercises the full MCP pipeline: compiled schema → tool listing → tool call →
//! GraphQL execution → JSON-RPC response. Tests the `FraiseQLMcpService`
//! `ServerHandler` implementation directly with a `FailingAdapter` (no real
//! database needed).
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions
#![cfg(feature = "mcp")]

use std::sync::Arc;

use fraiseql_core::runtime::Executor;
use fraiseql_core::schema::{ArgumentDefinition, CompiledSchema, FieldType, McpConfig};
use fraiseql_server::mcp::executor;
use fraiseql_server::mcp::handler::FraiseQLMcpService;
use fraiseql_server::mcp::tools;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use fraiseql_test_utils::schema_builder::{
    TestMutationBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use rmcp::ServerHandler;
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a test schema with a `users` query, a `createUser` mutation, and a `User` type.
fn build_test_schema() -> CompiledSchema {
    let mut query = TestQueryBuilder::new("users", "User")
        .returns_list(true)
        .with_description("List all users")
        .build();
    query.arguments.push(ArgumentDefinition::optional("limit", FieldType::Int));

    let mutation = TestMutationBuilder::new("createUser", "User")
        .with_description("Create a new user")
        .build();

    TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_simple_field("id", FieldType::Id)
                .with_simple_field("name", FieldType::String)
                .with_simple_field("email", FieldType::String)
                .build(),
        )
        .with_query(query)
        .with_mutation(mutation)
        .build()
}

fn mcp_config() -> McpConfig {
    McpConfig {
        enabled: true,
        ..McpConfig::default()
    }
}

/// Create the MCP service backed by a `FailingAdapter`.
fn make_mcp_service() -> (
    FraiseQLMcpService<FailingAdapter>,
    Arc<CompiledSchema>,
    Arc<Executor<FailingAdapter>>,
) {
    let schema = build_test_schema();
    let adapter = Arc::new(FailingAdapter::new());
    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let schema = Arc::new(schema);
    let service = FraiseQLMcpService::new(schema.clone(), executor.clone(), mcp_config());
    (service, schema, executor)
}

// ===========================================================================
// Cycle 1: MCP initialization — ServerHandler::get_info()
// ===========================================================================

/// Verify that `get_info` returns sensible server metadata and capabilities.
#[test]
fn mcp_e2e_server_info_and_capabilities() {
    let (service, _, _) = make_mcp_service();
    let info = service.get_info();

    // Server should identify itself as FraiseQL
    assert!(info.instructions.is_some());
    assert!(
        info.instructions.as_deref().unwrap().contains("FraiseQL"),
        "Server info should mention FraiseQL: {:?}",
        info.instructions,
    );

    // Server should advertise tool capabilities
    let caps = info.capabilities;
    assert!(
        caps.tools.is_some(),
        "Server should advertise tools capability",
    );
}

// ===========================================================================
// Cycle 2: Tool listing — schema_to_tools + get_tool
// ===========================================================================

/// Verify that schema queries and mutations are converted to MCP tools.
#[test]
fn mcp_e2e_tool_listing_from_schema() {
    let schema = build_test_schema();
    let config = mcp_config();
    let tool_list = tools::schema_to_tools(&schema, &config);

    // Should have at least 2 tools: users (query) + createUser (mutation)
    assert!(
        tool_list.len() >= 2,
        "Expected at least 2 tools, got {}",
        tool_list.len(),
    );

    // Verify the `users` query tool
    let users_tool = tool_list.iter().find(|t| t.name == "users").expect("users tool not found");
    assert!(
        users_tool
            .description
            .as_deref()
            .unwrap()
            .contains("List all users"),
    );

    // Verify input schema has the `limit` argument
    let props = users_tool.input_schema.get("properties").unwrap();
    assert!(
        props.get("limit").is_some(),
        "users tool should have 'limit' argument in schema",
    );

    // `limit` is optional → should NOT appear in `required`
    let required = users_tool.input_schema.get("required");
    if let Some(req) = required {
        let arr = req.as_array().unwrap();
        assert!(
            !arr.iter().any(|v| v.as_str() == Some("limit")),
            "limit should not be required",
        );
    }

    // Verify the `createUser` mutation tool
    let create_tool = tool_list
        .iter()
        .find(|t| t.name == "createUser")
        .expect("createUser tool not found");
    assert!(
        create_tool
            .description
            .as_deref()
            .unwrap()
            .contains("Create a new user"),
    );
}

/// Verify `get_tool` returns the correct tool by name.
#[test]
fn mcp_e2e_get_tool_lookup() {
    let (service, _, _) = make_mcp_service();

    let users_tool = service.get_tool("users");
    assert!(users_tool.is_some());
    assert_eq!(users_tool.unwrap().name, "users");

    let create_tool = service.get_tool("createUser");
    assert!(create_tool.is_some());

    let missing = service.get_tool("doesNotExist");
    assert!(missing.is_none());
}

/// Verify include/exclude filters work.
#[test]
fn mcp_e2e_tool_filtering() {
    let schema = build_test_schema();

    // Include filter: only expose `users`
    let config_include = McpConfig {
        enabled: true,
        include: vec!["users".to_string()],
        ..McpConfig::default()
    };
    let filtered = tools::schema_to_tools(&schema, &config_include);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "users");

    // Exclude filter: hide `createUser`
    let config_exclude = McpConfig {
        enabled: true,
        exclude: vec!["createUser".to_string()],
        ..McpConfig::default()
    };
    let filtered = tools::schema_to_tools(&schema, &config_exclude);
    assert!(filtered.iter().all(|t| t.name != "createUser"));
    assert!(filtered.iter().any(|t| t.name == "users"));
}

// ===========================================================================
// Cycle 3: Tool execution (query) — call_tool through Executor
// ===========================================================================

/// Call the `users` tool through the full MCP executor pipeline.
///
/// `FailingAdapter` returns an empty result set for any query, so we verify
/// the response structure and that no error occurred.
#[tokio::test]
async fn mcp_e2e_tool_call_query() {
    let (_, schema, executor) = make_mcp_service();

    let args = json!({ "limit": 10 });
    let args_map = args.as_object().unwrap();

    let result = executor::call_tool("users", Some(args_map), &schema, &executor).await;

    // Should NOT be an error
    assert!(
        result.is_error != Some(true),
        "Expected successful call_tool for 'users', got error: {:?}",
        result.content,
    );

    // Should have content
    assert!(!result.content.is_empty(), "Expected non-empty content");
}

/// Call the `users` tool with no arguments (all optional).
#[tokio::test]
async fn mcp_e2e_tool_call_query_no_args() {
    let (_, schema, executor) = make_mcp_service();

    let result = executor::call_tool("users", None, &schema, &executor).await;

    assert!(
        result.is_error != Some(true),
        "Expected successful call_tool with no args: {:?}",
        result.content,
    );
    assert!(!result.content.is_empty());
}

// ===========================================================================
// Cycle 4: Error cases
// ===========================================================================

/// Calling a non-existent tool returns an error result.
#[tokio::test]
async fn mcp_e2e_tool_call_unknown_tool() {
    let (_, schema, executor) = make_mcp_service();

    let result = executor::call_tool("nonExistentTool", None, &schema, &executor).await;

    assert_eq!(result.is_error, Some(true), "Expected is_error for unknown tool");

    // Error message should mention the unknown operation
    let text = format!("{:?}", result.content);
    assert!(
        text.contains("Unknown operation") || text.contains("nonExistentTool"),
        "Error should reference unknown tool: {text}",
    );
}

/// Argument names containing special characters are rejected (injection prevention).
#[tokio::test]
async fn mcp_e2e_tool_call_invalid_argument_name() {
    let (_, schema, executor) = make_mcp_service();

    let args = json!({ "limit: 99) { __typename } #": 1 });
    let args_map = args.as_object().unwrap();

    let result = executor::call_tool("users", Some(args_map), &schema, &executor).await;

    assert_eq!(
        result.is_error,
        Some(true),
        "Expected is_error for injection attempt",
    );

    let text = format!("{:?}", result.content);
    assert!(
        text.contains("Invalid argument name"),
        "Expected injection rejection: {text}",
    );
}

/// Calling a mutation tool also works through the executor.
#[tokio::test]
async fn mcp_e2e_tool_call_mutation() {
    let (_, schema, executor) = make_mcp_service();

    let args = json!({ "name": "Alice", "email": "alice@example.com" });
    let args_map = args.as_object().unwrap();

    let result = executor::call_tool("createUser", Some(args_map), &schema, &executor).await;

    // FailingAdapter may return an error for mutations (no canned response),
    // but the MCP layer should handle it gracefully (not panic).
    // We just verify the pipeline didn't crash.
    assert!(!result.content.is_empty(), "Expected content (success or error)");
}

/// Verify the GraphQL query built by the executor has correct structure.
///
/// Tests the `scalar_fields_for_type` helper to ensure field selection
/// only includes scalar fields (not nested objects).
#[test]
fn mcp_e2e_scalar_field_selection() {
    let schema = build_test_schema();
    let fields = executor::scalar_fields_for_type("User", &schema);

    assert!(fields.contains(&"id".to_string()));
    assert!(fields.contains(&"name".to_string()));
    assert!(fields.contains(&"email".to_string()));
}

/// Verify field selection returns empty for unknown types.
#[test]
fn mcp_e2e_scalar_fields_unknown_type() {
    let schema = build_test_schema();
    let fields = executor::scalar_fields_for_type("NonExistentType", &schema);
    assert!(fields.is_empty());
}
