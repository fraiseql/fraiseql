//! MCP transport safety regressions — #808, #857, #875.
//!
//! MCP is the AI-agent-facing transport: its callers are language models building
//! arguments from natural language, so adversarial and malformed input is the
//! *normal* case. These tests pin the three properties the transport must hold
//! whatever the caller sends:
//!
//! 1. **The `[mcp] include`/`exclude`/`read_only` allowlist is enforced where the call is
//!    executed**, not only where the tool list is advertised (#808).
//! 2. **No caller-supplied text ever reaches the GraphQL document.** Argument values travel as
//!    GraphQL variables, so a nested object key cannot add a root field to the operation (#808).
//! 3. **The advertised tool name and the executed tool name are one identifier** under every
//!    `naming_convention` (#857).
//!
//! Every assertion that a forbidden operation did not run is made against the
//! adapter's recorded view names, not against the returned error text: an error
//! message is easy to produce, a query that never reached the database is the
//! actual guarantee.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions

use std::sync::Arc;

use fraiseql_core::{
    runtime::Executor,
    schema::{
        ArgumentDefinition, AutoParams, CompiledSchema, FieldType, McpConfig, NamingConvention,
    },
};
use fraiseql_server::{
    config::ErrorSanitizer,
    mcp::{
        executor::{McpCallContext, call_tool},
        tools::schema_to_tools,
    },
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestMutationBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder},
};
use serde_json::json;

/// The view backing the operation an `[mcp]` allowlist is configured to forbid.
///
/// Its presence in [`FailingAdapter::recorded_queries`] is the proof that a
/// forbidden operation actually executed.
const FORBIDDEN_VIEW: &str = "v_api_key";
/// The view backing the operation the allowlist permits.
const ALLOWED_VIEW: &str = "v_user";

/// Build a schema with one allowed list query, one forbidden list query and one
/// mutation, authored `snake_case` so a camelCase convention actually renames them.
///
/// `list_users` declares a `filter` argument (`JSON`), which is what an MCP caller
/// legitimately fills with a nested object — the exact shape #808 injects through.
fn build_schema(naming: NamingConvention) -> CompiledSchema {
    let mut users = TestQueryBuilder::new("list_users", "User")
        .returns_list(true)
        .with_sql_source(ALLOWED_VIEW)
        .build();
    users.arguments.push(ArgumentDefinition::optional("filter", FieldType::Json));
    users.auto_params = AutoParams::all();

    let api_keys = TestQueryBuilder::new("api_keys", "ApiKey")
        .returns_list(true)
        .with_sql_source(FORBIDDEN_VIEW)
        .build();

    let mut create_user = TestMutationBuilder::new("create_user", "User").build();
    create_user
        .arguments
        .push(ArgumentDefinition::optional("name", FieldType::String));

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", ALLOWED_VIEW)
                .with_simple_field("id", FieldType::Id)
                .with_simple_field("name", FieldType::String)
                .build(),
        )
        .with_type(
            TestTypeBuilder::new("ApiKey", FORBIDDEN_VIEW)
                .with_simple_field("id", FieldType::Id)
                .with_simple_field("secret", FieldType::String)
                .build(),
        )
        .with_query(users)
        .with_query(api_keys)
        .with_mutation(create_user)
        .build();

    schema.naming_convention = naming;
    schema.build_indexes();
    schema
}

/// A schema plus the adapter recording which views were actually queried.
struct Fixture {
    schema:    Arc<CompiledSchema>,
    executor:  Arc<Executor<FailingAdapter>>,
    adapter:   Arc<FailingAdapter>,
    sanitizer: ErrorSanitizer,
}

impl Fixture {
    fn ctx<'a>(&'a self, config: &'a McpConfig) -> McpCallContext<'a, FailingAdapter> {
        McpCallContext {
            schema: &self.schema,
            executor: &self.executor,
            config,
            security_context: None,
            error_sanitizer: &self.sanitizer,
        }
    }
}

fn fixture(naming: NamingConvention) -> Fixture {
    let schema = build_schema(naming);
    let adapter = Arc::new(FailingAdapter::new());
    let executor = Arc::new(Executor::new(schema.clone(), Arc::clone(&adapter)));
    Fixture {
        schema: Arc::new(schema),
        executor,
        adapter,
        sanitizer: ErrorSanitizer::disabled(),
    }
}

/// An `[mcp]` config with authentication off: these tests are about the allowlist
/// and the document builder, and the fail-closed auth gate has its own coverage in
/// `mcp_e2e_test.rs`.
fn config(include: &[&str], exclude: &[&str], read_only: bool) -> McpConfig {
    McpConfig {
        enabled: true,
        require_auth: false,
        include: include.iter().map(|s| (*s).to_string()).collect(),
        exclude: exclude.iter().map(|s| (*s).to_string()).collect(),
        read_only,
        ..McpConfig::default()
    }
}

fn result_text(result: &rmcp::model::CallToolResult) -> String {
    format!("{:?}", result.content)
}

// ---------------------------------------------------------------------------
// #808 — the allowlist is enforced at execution, not only at advertisement
// ---------------------------------------------------------------------------

/// `[mcp] exclude` withholds an operation from the advertised tool list. Naming it
/// directly in `tools/call` must be refused, and the forbidden view must never be
/// queried.
///
/// Before the fix, `call_tool` consulted neither `include`, `exclude` nor
/// `read_only`: an excluded operation executed in full for any caller who guessed
/// its name — the allowlist bypass needed no injection at all.
#[tokio::test]
async fn excluded_operation_is_refused_at_execution() {
    let f = fixture(NamingConvention::Preserve);
    let cfg = config(&[], &["api_keys"], false);

    assert!(
        !schema_to_tools(&f.schema, &cfg).iter().any(|t| t.name == "api_keys"),
        "precondition: the excluded operation is not advertised",
    );

    let result = call_tool("api_keys", None, &f.ctx(&cfg)).await;

    assert_eq!(
        result.is_error,
        Some(true),
        "an excluded operation must be refused at execution: {}",
        result_text(&result),
    );
    assert!(
        result_text(&result).contains("Unknown tool"),
        "the refusal must come from tool resolution: {}",
        result_text(&result),
    );
    assert!(
        !f.adapter.recorded_queries().iter().any(|v| v == FORBIDDEN_VIEW),
        "the excluded operation must never reach the database; queried: {:?}",
        f.adapter.recorded_queries(),
    );
}

/// The same property for `include`: a non-empty allowlist means everything not on
/// it is refused at execution, not merely hidden from `tools/list`.
#[tokio::test]
async fn operation_outside_the_include_allowlist_is_refused_at_execution() {
    let f = fixture(NamingConvention::Preserve);
    let cfg = config(&["list_users"], &[], false);

    let result = call_tool("api_keys", None, &f.ctx(&cfg)).await;

    assert_eq!(
        result.is_error,
        Some(true),
        "an operation outside `include` must be refused at execution: {}",
        result_text(&result),
    );
    assert!(
        !f.adapter.recorded_queries().iter().any(|v| v == FORBIDDEN_VIEW),
        "the withheld operation must never reach the database; queried: {:?}",
        f.adapter.recorded_queries(),
    );
}

/// `read_only` is documented as fail-closed — "no mutation is ever a tool". That
/// guarantee has to survive a caller naming the mutation anyway.
#[tokio::test]
async fn read_only_refuses_a_mutation_named_directly() {
    let f = fixture(NamingConvention::Preserve);
    let cfg = config(&[], &[], true);

    let result = call_tool(
        "create_user",
        Some(&json!({ "name": "mallory" }).as_object().unwrap().clone()),
        &f.ctx(&cfg),
    )
    .await;

    // Asserting `is_error` alone would be a blind control: the backing adapter has no
    // canned response for this mutation, so an *executed* mutation also fails. The
    // refusal must be the tool-resolution refusal.
    assert_eq!(
        result.is_error,
        Some(true),
        "read_only must refuse a mutation named directly: {}",
        result_text(&result),
    );
    assert!(
        result_text(&result).contains("Unknown tool"),
        "read_only must refuse the mutation before executing it, not fail while running \
         it: {}",
        result_text(&result),
    );
}

// ---------------------------------------------------------------------------
// #808 — nested object keys cannot inject GraphQL structure
// ---------------------------------------------------------------------------

/// The headline repro: a legal top-level argument name whose *value* is an object
/// with a key crafted to close the argument list and open a second root field.
///
/// `build_graphql_query` validated top-level argument names ("to prevent injection
/// via malformed argument names") and then rendered the value with `graphql_value`,
/// whose object arm interpolated keys raw. The result parsed as a valid multi-root
/// document and the runtime fans multi-root queries out in parallel, so the
/// injected root executed and returned fields the tool's own projection would never
/// have emitted.
#[tokio::test]
async fn nested_object_keys_cannot_add_a_root_field() {
    let f = fixture(NamingConvention::Preserve);
    let cfg = config(&[], &["api_keys"], false);

    // The nested key closes `filter: {`, closes the argument list and the selection
    // set, opens `api_keys { id secret }`, then re-opens a `list_users(filter: {`
    // so the remainder of the generated text still parses.
    let injection = json!({
        "filter": { "a: 1}) { id } api_keys { id secret } alias: list_users(filter: {b": 1 }
    });

    let result = call_tool("list_users", Some(injection.as_object().unwrap()), &f.ctx(&cfg)).await;

    assert!(
        !f.adapter.recorded_queries().iter().any(|v| v == FORBIDDEN_VIEW),
        "a nested argument key must not be able to add a root field; queried: {:?} \
         (result: {})",
        f.adapter.recorded_queries(),
        result_text(&result),
    );
}

/// The same class one level deeper. `graphql_value` recursed into nested objects
/// and arrays with the same unvalidated key interpolation, so a fix applied only to
/// the outermost object would leave this open.
#[tokio::test]
async fn deeply_nested_object_keys_cannot_add_a_root_field() {
    let f = fixture(NamingConvention::Preserve);
    let cfg = config(&[], &["api_keys"], false);

    let injection = json!({
        "filter": {
            "inner": {
                "a: 1}}) { id } api_keys { id secret } alias: list_users(filter: {x: {b": 1
            }
        }
    });

    let result = call_tool("list_users", Some(injection.as_object().unwrap()), &f.ctx(&cfg)).await;

    assert!(
        !f.adapter.recorded_queries().iter().any(|v| v == FORBIDDEN_VIEW),
        "a deeply nested argument key must not be able to add a root field; \
         queried: {:?} (result: {})",
        f.adapter.recorded_queries(),
        result_text(&result),
    );
}

/// A well-formed nested filter must still work — the fix must close the injection
/// without breaking the legitimate use of the same shape.
#[tokio::test]
async fn a_well_formed_nested_filter_still_executes() {
    let f = fixture(NamingConvention::Preserve);
    let cfg = config(&[], &[], false);

    let args = json!({ "filter": { "name": { "eq": "alice" } }, "limit": 5 });

    let result = call_tool("list_users", Some(args.as_object().unwrap()), &f.ctx(&cfg)).await;

    assert!(
        result.is_error != Some(true),
        "a legitimate nested filter must still execute: {}",
        result_text(&result),
    );
    assert!(
        f.adapter.recorded_queries().iter().any(|v| v == ALLOWED_VIEW),
        "the allowed view should have been queried; queried: {:?}",
        f.adapter.recorded_queries(),
    );
}

// ---------------------------------------------------------------------------
// #857 — one identifier for advertisement and execution
// ---------------------------------------------------------------------------

/// List the tools, then call every one of them by the name it was advertised
/// under. Parameterised over every supported `naming_convention`.
///
/// Under `camelCase` — the compiler default since #456 — tools were advertised as
/// `listUsers`/`createUser` (`display_name`) while `call_tool` looked the operation
/// up by the raw compiled name (`list_users`), so every call returned
/// `Unknown operation: listUsers`. The whole MCP surface was unusable while
/// `tools/list` reported it as available.
#[tokio::test]
async fn every_advertised_tool_is_callable_by_its_advertised_name() {
    for naming in [NamingConvention::Preserve, NamingConvention::CamelCase] {
        let f = fixture(naming);
        let cfg = config(&[], &[], false);

        let tools = schema_to_tools(&f.schema, &cfg);
        assert!(!tools.is_empty(), "{naming:?}: precondition — tools are advertised");

        for tool in &tools {
            let result = call_tool(&tool.name, None, &f.ctx(&cfg)).await;
            let text = result_text(&result);
            assert!(
                !text.contains("Unknown tool") && !text.contains("Unknown operation"),
                "{naming:?}: tool '{}' was advertised but is not executable: {text}",
                tool.name,
            );
        }

        // Resolving the name is only half of it: the emitted document carries the
        // advertised name too, so assert the read actually reached its view rather
        // than merely surviving the lookup.
        assert!(
            f.adapter.recorded_queries().iter().any(|v| v == ALLOWED_VIEW),
            "{naming:?}: calling the advertised query tool must reach its view; \
             queried: {:?}",
            f.adapter.recorded_queries(),
        );
    }
}

/// The converse direction: under `camelCase`, the *raw* compiled name is not an
/// advertised tool, so calling it must be refused. Accepting both spellings would
/// re-open the allowlist bypass — `exclude = ["apiKeys"]` is written against the
/// advertised name, and a raw-name alias would walk straight past it.
#[tokio::test]
async fn the_raw_compiled_name_is_not_an_alias_for_the_advertised_tool() {
    let f = fixture(NamingConvention::CamelCase);
    let cfg = config(&[], &["apiKeys"], false);

    let result = call_tool("api_keys", None, &f.ctx(&cfg)).await;

    assert_eq!(
        result.is_error,
        Some(true),
        "the raw compiled name must not bypass an exclude written against the \
         advertised name: {}",
        result_text(&result),
    );
    assert!(
        !f.adapter.recorded_queries().iter().any(|v| v == FORBIDDEN_VIEW),
        "queried: {:?}",
        f.adapter.recorded_queries(),
    );
}

// ---------------------------------------------------------------------------
// #875 item 1 — MCP errors go through the configured error sanitizer
// ---------------------------------------------------------------------------

/// Every other transport runs execution errors through the configured
/// [`ErrorSanitizer`] — the documented "hide implementation details in error
/// messages" control. The MCP path returned `e.to_string()` raw, so a
/// `FraiseQLError::Database` handed an AI agent the driver message and SQLSTATE
/// verbatim, internal relation names included.
#[tokio::test]
async fn database_errors_are_sanitized_before_reaching_the_mcp_client() {
    let leaky = "relation \"tenant_acme.v_customer_secret\" does not exist";

    let schema = build_schema(NamingConvention::Preserve);
    let adapter = Arc::new(FailingAdapter::new().fail_on_query(0).fail_with_error(
        fraiseql_test_utils::failing_adapter::FailError::Database {
            message:   leaky.to_string(),
            sql_state: Some("42P01".to_string()),
        },
    ));
    let executor = Arc::new(Executor::new(schema.clone(), Arc::clone(&adapter)));
    let schema = Arc::new(schema);
    let cfg = config(&[], &[], false);
    let sanitizer = ErrorSanitizer::new(fraiseql_server::config::ErrorSanitizationConfig {
        enabled:                     true,
        hide_implementation_details: true,
        sanitize_database_errors:    true,
        custom_error_message:        None,
    });

    let result = call_tool(
        "list_users",
        None,
        &McpCallContext {
            schema:           &schema,
            executor:         &executor,
            config:           &cfg,
            security_context: None,
            error_sanitizer:  &sanitizer,
        },
    )
    .await;

    let text = result_text(&result);
    assert_eq!(
        result.is_error,
        Some(true),
        "the failing query must surface as an error: {text}"
    );
    assert!(
        !text.contains("v_customer_secret") && !text.contains("42P01"),
        "#875: the raw driver message reached the MCP client: {text}",
    );
}

/// The converse: with sanitization off, the message is still useful. A sanitizer
/// that swallowed everything unconditionally would pass the test above while
/// making the transport undebuggable.
#[tokio::test]
async fn errors_are_left_intact_when_sanitization_is_disabled() {
    let detail = "no such relation v_probe";

    let schema = build_schema(NamingConvention::Preserve);
    let adapter = Arc::new(FailingAdapter::new().fail_on_query(0).fail_with_error(
        fraiseql_test_utils::failing_adapter::FailError::Database {
            message:   detail.to_string(),
            sql_state: None,
        },
    ));
    let executor = Arc::new(Executor::new(schema.clone(), Arc::clone(&adapter)));
    let schema = Arc::new(schema);
    let cfg = config(&[], &[], false);
    let sanitizer = ErrorSanitizer::disabled();

    let result = call_tool(
        "list_users",
        None,
        &McpCallContext {
            schema:           &schema,
            executor:         &executor,
            config:           &cfg,
            security_context: None,
            error_sanitizer:  &sanitizer,
        },
    )
    .await;

    assert!(result_text(&result).contains(detail), "{}", result_text(&result));
}
