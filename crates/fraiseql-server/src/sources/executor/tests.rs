//! Unit tests for the source query-executor bridge's pure logic: identity
//! resolution (the per-message tenant seam) and reserved-variable splitting.
//!
//! The live `execute_query` round-trip (a connector actually mutating through the
//! server `Executor`) is exercised end-to-end by the scheduler integration test
//! against real PostgreSQL; here we pin the identity/variable
//! logic that has no database dependency.
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
