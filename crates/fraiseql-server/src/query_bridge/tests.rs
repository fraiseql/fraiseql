//! Unit tests for the shared query-bridge's pure logic: identity resolution (the
//! per-message tenant seam) and reserved-variable splitting.
//!
//! The live `execute_query` round-trip (a guest actually mutating through the
//! server `Executor`) is exercised end-to-end by the scheduler integration test
//! against real PostgreSQL. Here: the identity/variable logic, and the dispatch depth a
//! bridge write carries, over a mock writer.
#![allow(clippy::unwrap_used)] // Reason: test module

use fraiseql_core::{security::SecurityContext, types::TenantId};
use serde_json::json;

use super::{SOURCE_TENANT_VAR, resolve_identity, split_tenant_override};

/// The base `run_as` identity: a `SystemJob` context with a role ceiling and,
/// optionally, a pinned tenant.
fn base(tenant: Option<&str>) -> SecurityContext {
    SecurityContext::system_job(
        "orders",
        "fire-1",
        vec!["ingest_writer".to_string()],
        vec!["write:order".to_string()],
        tenant.map(TenantId::from),
    )
}

#[test]
fn multi_tenant_source_scopes_to_the_per_message_tenant() {
    // Base has no pinned tenant (multi-tenant source) → a per-message tenant scopes
    // this write, and the role/scope ceiling is preserved.
    let ctx = resolve_identity(&base(None), Some("acme"));
    assert_eq!(ctx.tenant_id.as_ref().map(TenantId::as_str), Some("acme"));
    assert!(ctx.has_role("ingest_writer"));
    assert!(ctx.has_scope("write:order"));
}

#[test]
fn pinned_source_ignores_a_tenant_override() {
    // Base is pinned to "corp" (single-tenant source) → an override cannot forge a
    // write for another tenant; the identity stays scoped to "corp".
    let ctx = resolve_identity(&base(Some("corp")), Some("acme"));
    assert_eq!(ctx.tenant_id.as_ref().map(TenantId::as_str), Some("corp"));
}

#[test]
fn global_source_without_override_stays_global() {
    let ctx = resolve_identity(&base(None), None);
    assert!(ctx.tenant_id.is_none());
    assert!(ctx.has_role("ingest_writer"));
}

#[test]
fn split_extracts_and_strips_the_reserved_tenant() {
    let vars = json!({ SOURCE_TENANT_VAR: "acme", "id": 1 });
    let (cleaned, tenant) = split_tenant_override(Some(&vars));
    assert_eq!(tenant.as_deref(), Some("acme"));
    // The reserved key never reaches the mutation; the real variables survive.
    let cleaned = cleaned.unwrap();
    assert!(cleaned.get(SOURCE_TENANT_VAR).is_none());
    assert_eq!(cleaned.get("id"), Some(&json!(1)));
}

#[test]
fn split_passes_variables_through_untouched_when_absent() {
    let vars = json!({ "id": 1 });
    let (cleaned, tenant) = split_tenant_override(Some(&vars));
    assert!(tenant.is_none());
    assert_eq!(cleaned, Some(json!({ "id": 1 })));

    // No variables at all → nothing to split.
    let (cleaned, tenant) = split_tenant_override(None);
    assert!(cleaned.is_none());
    assert!(tenant.is_none());
}

#[test]
fn split_strips_a_blank_tenant_without_scoping() {
    // A blank reserved value is a no-op tenant, but the key is still stripped.
    let vars = json!({ SOURCE_TENANT_VAR: "   ", "id": 1 });
    let (cleaned, tenant) = split_tenant_override(Some(&vars));
    assert!(tenant.is_none(), "a blank tenant scopes nothing");
    assert!(cleaned.unwrap().get(SOURCE_TENANT_VAR).is_none(), "but the key is stripped");
}

/// A write made through the bridge is observed at dispatch depth 1, so the
/// after-mutation observer never dispatches on it (M-bridge, #1340, #1440). The same
/// write made directly, as a request makes it, is depth 0: the control.
#[tokio::test]
async fn a_bridge_write_runs_at_dispatch_depth_one() {
    use std::sync::{Arc, Mutex};

    use arc_swap::ArcSwap;
    use fraiseql_core::{
        runtime::{AfterMutationObserver, CommittedMutation, Executor, RuntimeConfig},
        schema::CompiledSchema,
    };
    use fraiseql_functions::host::live::QueryExecutor as _;
    use fraiseql_test_utils::failing_adapter::FailingAdapter;

    #[derive(Default)]
    struct Depths(Mutex<Vec<u8>>);
    impl AfterMutationObserver for Depths {
        fn on_committed(&self, mutation: &CommittedMutation<'_>) {
            self.0.lock().unwrap().push(mutation.dispatch_depth);
        }
    }

    let schema: CompiledSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "fields": [ { "name": "id", "field_type": "ID" } ]
        }],
        "mutations": [{
            "name": "updateOrder",
            "return_type": "Order",
            "sql_source": "fn_update_order",
            "operation": { "Update": { "table": "tb_order" } },
            "arguments": []
        }]
    }))
    .unwrap();
    let row = std::collections::HashMap::from([
        ("succeeded".to_string(), json!(true)),
        ("state_changed".to_string(), json!(true)),
        ("entity".to_string(), json!({ "id": "o1" })),
        ("entity_type".to_string(), json!("Order")),
        ("cascade".to_string(), serde_json::Value::Null),
    ]);
    let adapter = FailingAdapter::new().with_function_response("fn_update_order", vec![row]);
    let depths = Arc::new(Depths::default());
    let config = RuntimeConfig::default().with_after_mutation_observer(depths.clone());
    let executor = Arc::new(Executor::with_config(schema, Arc::new(adapter), config));
    let mutation = "mutation { updateOrder { id } }";

    executor.execute(mutation, None).await.unwrap();
    let bridge =
        super::RunAsQueryExecutor::new(Arc::new(ArcSwap::from(Arc::clone(&executor))), base(None));
    bridge.execute_query(mutation, None).await.unwrap();

    assert_eq!(*depths.0.lock().unwrap(), [0, 1]);
}
