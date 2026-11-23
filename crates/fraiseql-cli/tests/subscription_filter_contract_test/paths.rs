//! #1665: a filter path compiles to the one spelling every runtime reader reads alike.
//!
//! Every SDK documents a filter path as `$.field`. The compiler carried it verbatim, and
//! delivery read `$.id` as the pointer `/$/id`, which names no key: a subscriber who bound
//! the argument received no event. The plan's read gate read it as the key `$`, then `id`.
//! The compiler now writes the pointer, `/id`, for every accepted spelling.

use std::{fs, path::Path, process::Command, sync::Arc};

use fraiseql_core::{
    error::FraiseQLError,
    graphql::parse_query,
    runtime::{
        Executor,
        subscription::{SubscriptionEvent, SubscriptionManager},
    },
    schema::CompiledSchema,
    security::SecurityContext,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::json;
use tempfile::TempDir;

use super::subscription_types_json;

/// Compile `types_json` against `toml`, and the compiled schema, or the compiler's log.
fn compile(types_json: &str, toml: &str) -> Result<CompiledSchema, String> {
    let dir = TempDir::new().unwrap();
    let (types, config, output) = (
        dir.path().join("types.json"),
        dir.path().join("fraiseql.toml"),
        dir.path().join("schema.compiled.json"),
    );
    fs::write(&types, types_json).unwrap();
    fs::write(&config, toml).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "compile",
            arg(&config),
            "--types",
            arg(&types),
            "--output",
            arg(&output),
        ])
        .output()
        .expect("failed to run fraiseql-cli");
    if !out.status.success() {
        return Err(format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(CompiledSchema::from_json(&fs::read_to_string(output).unwrap(), false).unwrap())
}

fn arg(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// `orderUpdated(orderId: ID)`, filtering `orderId` by the authored `path`.
fn by_order_id(path: &str) -> CompiledSchema {
    let types = subscription_types_json(
        r#"{"name": "orderId", "type": "ID", "nullable": true}"#,
        &format!(r#"{{"argument": "orderId", "path": "{path}"}}"#),
    );
    compile(&types, "").unwrap_or_else(|log| panic!("{path} must compile: {log}"))
}

fn order_event(id: &str) -> SubscriptionEvent {
    SubscriptionEvent::new(
        "Order",
        id,
        fraiseql_core::runtime::subscription::SubscriptionOperation::Update,
        json!({"id": id, "total": 1.0}),
    )
}

/// How many of the events for `o1` and then `o2` reach a subscriber bound to `o1`.
fn delivered_to_a_subscriber_of_o1(schema: CompiledSchema) -> (usize, usize) {
    let manager = SubscriptionManager::new(Arc::new(schema));
    manager
        .subscribe("orderUpdated", json!({}), json!({"orderId": "o1"}), "c1")
        .unwrap();
    (
        manager.publish_event(order_event("o1")),
        manager.publish_event(order_event("o2")),
    )
}

/// The issue's case: the documented `$.id` filters, and delivers the bound order only.
#[test]
fn a_documented_dollar_path_delivers_the_bound_event_only() {
    let schema = by_order_id("$.id");
    let paths = &schema.subscriptions[0].filter.as_ref().unwrap().argument_paths;
    assert_eq!(paths.get("orderId").map(String::as_str), Some("/id"));
    assert_eq!(delivered_to_a_subscriber_of_o1(schema), (1, 0));
}

/// The control: a pointer authored today compiles to itself and filters as it did.
#[test]
fn control_a_pointer_path_compiles_to_itself_and_filters_as_before() {
    let schema = by_order_id("/id");
    let paths = &schema.subscriptions[0].filter.as_ref().unwrap().argument_paths;
    assert_eq!(paths.get("orderId").map(String::as_str), Some("/id"));
    assert_eq!(delivered_to_a_subscriber_of_o1(schema), (1, 0));
}

#[test]
fn a_dotted_path_compiles_to_its_pointer() {
    let schema = by_order_id("customer.id");
    let paths = &schema.subscriptions[0].filter.as_ref().unwrap().argument_paths;
    assert_eq!(paths.get("orderId").map(String::as_str), Some("/customer/id"));
}

#[test]
fn a_path_no_reader_reads_is_a_compile_error_naming_the_forms() {
    let types = subscription_types_json(
        r#"{"name": "orderId", "type": "ID", "nullable": true}"#,
        r#"{"argument": "orderId", "path": "$..id"}"#,
    );
    let log = compile(&types, "").expect_err("`$..id` must not compile");
    assert!(log.contains("'$..id'") && log.contains("`$.field`"), "{log}");
}

/// A compiled schema still holding an authored `$.id`, from an earlier CLI or edited by
/// hand, is refused at load, naming the fix: it would match no event.
#[test]
fn a_compiled_dollar_path_is_refused_at_load() {
    let schema = by_order_id("/id");
    let json = schema.to_json().unwrap().replace(r#""orderId":"/id""#, r#""orderId":"$.id""#);
    assert!(json.contains(r#""orderId":"$.id""#), "the artifact carries the old spelling");
    let err = CompiledSchema::from_json(&json, false).unwrap_err().to_string();
    assert!(err.contains("'$.id'") && err.contains("recompile"), "{err}");
}

// ── The plan's read gate reads the same key ───────────────────────────────────────────

const AUDITOR: &str = r#"
[[security.role_definitions]]
name = "auditor"
scopes = ["read:Order.secret"]
"#;

/// `orderUpdated(secret: String)`, filtering by `$.secret`, a field only `auditor` reads.
fn by_secret() -> CompiledSchema {
    let types = r#"{
  "types": [{
    "name": "Order", "sql_source": "v_order",
    "fields": [
      {"name": "id", "type": "ID", "nullable": false},
      {"name": "secret", "type": "String", "nullable": true, "requires_scope": "read:Order.secret"}
    ]
  }],
  "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
               "nullable": false, "sql_source": "v_order"}],
  "subscriptions": [{
    "name": "orderUpdated", "return_type": "Order", "topic": "order_events",
    "arguments": [{"name": "secret", "type": "String", "nullable": true}],
    "filter": {"conditions": [{"argument": "secret", "path": "$.secret"}]},
    "fields": ["id", "secret"]
  }]
}"#;
    compile(types, AUDITOR).unwrap_or_else(|log| panic!("must compile: {log}"))
}

/// A subscriber holding exactly `roles`.
fn subscriber(roles: &[&str]) -> SecurityContext {
    let roles = roles.iter().map(|r| (*r).to_string()).collect();
    SecurityContext::service_account("subscriber", "req-1665", roles, vec![], None)
}

fn plan_filtering_by_secret(roles: &[&str]) -> Result<(), FraiseQLError> {
    let executor = Executor::new(by_secret(), Arc::new(FailingAdapter::new()));
    let document =
        parse_query("subscription($secret: String) { orderUpdated(secret: $secret) { id } }")
            .unwrap();
    executor
        .plan_subscription(&document, Some(&json!({"secret": "s"})), Some(&subscriber(roles)))
        .map(drop)
}

/// A subscriber who may not read `secret` may not filter by it either, written `$.secret`.
#[test]
fn a_dollar_path_on_a_field_the_subscriber_may_not_read_refuses_the_plan() {
    let err = plan_filtering_by_secret(&[]).expect_err("filtering by `secret` reads it");
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
    assert!(err.to_string().contains("Order.secret"), "{err}");
}

#[test]
fn control_a_subscriber_who_reads_the_field_may_filter_by_it() {
    plan_filtering_by_secret(&["auditor"]).expect("`auditor` reads `secret`");
}
