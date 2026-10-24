//! Tests for function type serde, including durable-dispatch config round-trip.

use super::*;

#[test]
fn function_definition_defaults_to_durable_with_no_retry_override() {
    // A compiled-schema entry that omits the durability fields deserializes to
    // the durable default: not re-runnable, no per-function retry override.
    let json = serde_json::json!({
        "name": "onOrderPaid",
        "trigger": "after:mutation:Order:update",
        "runtime": "Wasm"
    });
    let definition: FunctionDefinition = serde_json::from_value(json).expect("valid definition");
    assert!(!definition.re_runnable);
    assert!(definition.retry.is_none());
}

#[test]
fn function_definition_round_trips_re_runnable_and_retry_policy() {
    // A per-trigger retry policy + re_runnable flag round-trip from the
    // compiled schema.
    let json = serde_json::json!({
        "name": "scoreDeal",
        "trigger": "after:mutation:Deal:insert",
        "runtime": "Deno",
        "re_runnable": true,
        "retry": {
            "max_attempts": 7,
            "initial_delay_ms": 250,
            "max_delay_ms": 60_000,
            "backoff_strategy": "exponential"
        }
    });
    let definition: FunctionDefinition = serde_json::from_value(json).expect("valid definition");
    assert!(definition.re_runnable);
    let retry = definition.retry.expect("retry policy present");
    assert_eq!(retry.max_attempts, 7);
    assert_eq!(retry.initial_delay_ms, 250);
    assert_eq!(retry.max_delay_ms, 60_000);
}

#[test]
fn function_definition_defaults_to_no_run_as() {
    // A compiled-schema entry with no `run_as` deserializes to fail-closed: the
    // bridge writes have no authority until an operator grants a ceiling.
    let json = serde_json::json!({
        "name": "notify",
        "trigger": "after:mutation:Order:update",
        "runtime": "Deno"
    });
    let definition: FunctionDefinition = serde_json::from_value(json).expect("valid definition");
    assert!(definition.run_as.is_none(), "absent run_as ⇒ fail-closed");
}

#[test]
fn function_definition_round_trips_run_as_ceiling() {
    // `run_as` round-trips from the compiled schema (#594): roles/scopes/tenant.
    let json = serde_json::json!({
        "name": "recordApproval",
        "trigger": "after:mutation:Order:update",
        "runtime": "Deno",
        "run_as": { "roles": ["order_writer"], "scopes": ["write:order"], "tenant": "acme" }
    });
    let definition: FunctionDefinition = serde_json::from_value(json).expect("valid definition");
    let run_as = definition.run_as.as_ref().expect("run_as present");
    assert_eq!(run_as.roles, vec!["order_writer"]);
    assert_eq!(run_as.scopes, vec!["write:order"]);
    assert_eq!(run_as.tenant.as_deref(), Some("acme"));

    // Empty ceiling fields are skipped on re-serialization (compact wire form).
    let bare = FunctionDefinition::new("f", "after:mutation:X:insert", RuntimeType::Deno);
    let value = serde_json::to_value(&bare).expect("serialize");
    assert!(value.get("run_as").is_none(), "absent run_as is omitted from the wire form");
}

#[cfg(feature = "host-live")]
#[test]
fn identity_is_fail_closed_without_run_as() {
    // No `run_as` ⇒ an anonymous system_job with no roles/scopes/tenant: every
    // RBAC/field-authz check denies and tenant-scoped RLS admits nothing.
    let def = FunctionDefinition::new("purge", "cron:0 2 * * *", RuntimeType::Deno);
    let identity = def.identity("dispatch-token-1");
    assert!(identity.roles.is_empty(), "fail-closed: no roles");
    assert!(identity.scopes.is_empty(), "fail-closed: no scopes");
    assert!(identity.tenant_id.is_none(), "fail-closed: no tenant");
    // Audited as this function's system job.
    assert_eq!(identity.user_id.0, "system_job:purge");
    assert_eq!(identity.request_id, "dispatch-token-1");
}

#[cfg(feature = "host-live")]
#[test]
fn identity_carries_the_granted_ceiling() {
    // A granted ceiling flows into the background identity verbatim.
    let def =
        FunctionDefinition::new("recordApproval", "after:mutation:Order:update", RuntimeType::Deno)
            .with_run_as(RunAs {
                roles:   vec!["order_writer".to_string()],
                scopes:  vec!["write:order".to_string()],
                tenant:  Some("acme".to_string()),
                user_id: None,
            });
    let identity = def.identity("token-2");
    assert_eq!(identity.roles, vec!["order_writer"]);
    assert_eq!(identity.scopes, vec!["write:order"]);
    assert_eq!(identity.tenant_id.as_ref().map(|t| t.0.as_str()), Some("acme"));
    assert_eq!(identity.user_id.0, "system_job:recordApproval");
}

/// #1700: `run_as.user_id` is what the identity answers to `jwt:sub` and
/// `jwt:user_id`; the provenance stays the function's system job.
#[cfg(feature = "host-live")]
#[test]
fn identity_answers_sub_with_the_run_as_user_id() {
    let principal = "6f1c2a9e-3b4d-4e5f-8a7b-9c0d1e2f3a4b";
    let def =
        FunctionDefinition::new("approveOrder", "after:mutation:Order:insert", RuntimeType::Deno)
            .with_run_as(RunAs {
                roles: vec!["order_approver".to_string()],
                user_id: Some(principal.to_string()),
                ..RunAs::default()
            });
    let identity = def.identity("token-3");
    for claim in ["sub", "user_id"] {
        assert_eq!(
            identity.jwt_claim(claim, "tenant_id"),
            Some(serde_json::json!(principal)),
            "jwt:{claim}"
        );
    }
    assert_eq!(identity.user_id.0, "system_job:approveOrder", "provenance is unchanged");
    assert_eq!(identity.actor_type(), fraiseql_core::security::ActorType::SystemJob);
}

/// #1700: a `run_as` key the ceiling does not carry is refused, never dropped: an
/// ignored `user_id` was how an inject silently kept receiving `system_job:<name>`.
#[test]
fn run_as_refuses_an_unknown_key() {
    let json = serde_json::json!({
        "name": "approveOrder",
        "trigger": "after:mutation:Order:insert",
        "runtime": "Deno",
        "run_as": { "roles": ["order_approver"], "claims": { "sub": "x" } }
    });
    let error = serde_json::from_value::<FunctionDefinition>(json).expect_err("unknown key");
    assert!(error.to_string().contains("claims"), "{error}");
}

#[test]
fn function_definition_round_trips_when_predicates() {
    // The `when` conjunction round-trips from the compiled schema (#597).
    let json = serde_json::json!({
        "name": "notify_approved",
        "trigger": "after:mutation:Order:update",
        "runtime": "Deno",
        "when": [
            { "field": "status", "changed_to": "approved" },
            { "field": "kind", "eq": "standard" }
        ]
    });
    let def: FunctionDefinition = serde_json::from_value(json).expect("valid when");
    assert_eq!(def.when.len(), 2);
    assert_eq!(def.when[0].field, "status");
    assert_eq!(def.when[0].changed_to, Some(serde_json::json!("approved")));
    assert_eq!(def.when[1].eq, Some(serde_json::json!("standard")));

    // Absent `when` deserializes to an empty conjunction (always fires) and is
    // omitted from the compact wire form.
    let bare = FunctionDefinition::new("f", "after:mutation:X:insert", RuntimeType::Deno);
    assert!(bare.when.is_empty());
    let value = serde_json::to_value(&bare).expect("serialize");
    assert!(value.get("when").is_none(), "empty when is omitted from the wire form");
}

/// #1627: the one rule every trigger kind derives its limits from.
#[test]
fn resource_limits_for_a_function_take_its_declared_timeout() {
    let declared =
        FunctionDefinition::new("onOrder", "after:mutation:Order:insert", RuntimeType::Wasm)
            .with_timeout(100);
    let limits = ResourceLimits::for_function(&declared);
    assert_eq!(limits.max_duration, Duration::from_millis(100));
    assert_eq!(limits.max_memory_bytes, ResourceLimits::default().max_memory_bytes);
}

#[test]
fn resource_limits_for_a_function_without_a_timeout_are_the_defaults() {
    let undeclared =
        FunctionDefinition::new("onOrder", "after:mutation:Order:insert", RuntimeType::Wasm);
    let limits = ResourceLimits::for_function(&undeclared);
    assert_eq!(limits.max_duration, ResourceLimits::default().max_duration);
}
