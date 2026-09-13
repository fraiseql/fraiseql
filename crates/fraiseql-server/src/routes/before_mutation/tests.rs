//! Unit tests for the `before:mutation` chain gate (#1327).
//!
//! [`map_chain_outcome`] is tested directly: executing a real chain needs a guest
//! runtime and a loaded module, and the arms that matter (abort, fail-closed) would
//! otherwise be pinned by nothing. The one arm that *is* reachable without a
//! runtime — a registered trigger whose module is absent — is exercised through
//! the gate itself, so the wiring between the two is covered too.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    error::FraiseQLError,
    security::{BeforeMutationGate, BeforeMutationOutcome, BeforeMutationRequest},
};
use fraiseql_functions::{
    BeforeMutationResult, BeforeMutationTrigger, FunctionObserver, TriggerRegistry,
};

use super::{FunctionChainGate, map_chain_outcome};
use crate::subsystems::BeforeMutationHooks;

/// A hook bundle whose registry declares `before:mutation` for each named mutation,
/// with no modules loaded.
fn hooks_for(mutations: &[&str]) -> Arc<BeforeMutationHooks> {
    let mut registry = TriggerRegistry::new();
    for mutation in mutations {
        registry.before_mutation_triggers.push(BeforeMutationTrigger {
            function_name: format!("guard_{mutation}"),
            mutation_name: (*mutation).to_string(),
        });
    }
    Arc::new(BeforeMutationHooks::new(
        registry,
        HashMap::new(),
        Arc::new(FunctionObserver::new()),
    ))
}

// ---- outcome mapping ------------------------------------------------------

/// The chain's threaded input becomes the arguments the write binds from — always,
/// not only when it differs. The handler this replaced treated a `null` result as
/// "no change" and dropped the arguments entirely, so a chain returning
/// `{"input": null}` ran the write with no input at all instead of failing its
/// required-argument check.
#[test]
fn proceed_carries_the_chains_arguments_to_the_write() {
    let threaded = serde_json::json!({ "input": { "name": "REWRITTEN" } });
    let mapped = map_chain_outcome("createUser", Ok(BeforeMutationResult::Proceed(threaded)))
        .expect("Proceed must not refuse");
    match mapped {
        BeforeMutationOutcome::ProceedWith { arguments } => {
            assert_eq!(arguments["input"]["name"], "REWRITTEN");
        },
        other => panic!("expected ProceedWith, got {other:?}"),
    }
}

#[test]
fn proceed_with_null_is_carried_through_rather_than_dropped() {
    let mapped =
        map_chain_outcome("createUser", Ok(BeforeMutationResult::Proceed(serde_json::Value::Null)))
            .expect("Proceed must not refuse");
    match mapped {
        BeforeMutationOutcome::ProceedWith { arguments } => {
            assert_eq!(
                arguments,
                serde_json::Value::Null,
                "a null rewrite must reach the write's required-argument check, not vanish"
            );
        },
        other => panic!("expected ProceedWith, got {other:?}"),
    }
}

#[test]
fn abort_carries_the_rules_own_message() {
    let mapped = map_chain_outcome(
        "pay",
        Ok(BeforeMutationResult::Abort("needs two approvals".to_string())),
    )
    .expect("an abort is a decision, not a gate failure");
    match mapped {
        BeforeMutationOutcome::Abort { reason } => assert_eq!(reason, "needs two approvals"),
        other => panic!("expected Abort, got {other:?}"),
    }
}

/// Fail-closed: a chain that could not run refuses the write. It must not fall
/// through to "proceed with the original input" — that would execute a mutation the
/// chain never approved, which is the whole point of the hook.
#[test]
fn a_chain_failure_refuses_the_write() {
    let err = map_chain_outcome(
        "pay",
        Err(FraiseQLError::Validation {
            message: "before:mutation function 'guard_pay' not found in module registry"
                .to_string(),
            path:    None,
        }),
    )
    .expect_err("a chain failure must refuse the write");
    assert!(
        matches!(&err, FraiseQLError::Internal { message, .. }
            if message == "before:mutation hook execution failed"),
        "the chain's own text is withheld from the client: {err:?}"
    );
}

// ---- the gate itself ------------------------------------------------------

/// A mutation with no declared `before:mutation` trigger proceeds untouched — and
/// does *not* report a rewrite, so the engine keeps the arguments it already
/// resolved.
#[tokio::test]
async fn a_mutation_with_no_trigger_proceeds_untouched() {
    let gate = FunctionChainGate::new(hooks_for(&["guarded"]));
    let arguments = serde_json::json!({ "id": 1 });
    let request = BeforeMutationRequest::new(None, "harmless", "harmless", &arguments);
    let mapped = gate.before_mutation(&request).await.expect("no trigger cannot fail");
    assert!(
        matches!(mapped, BeforeMutationOutcome::Proceed),
        "an unhooked mutation must proceed with the arguments the engine resolved"
    );
}

/// The registry is keyed on the field name, so a declared trigger is found for the
/// mutation regardless of the alias the document gave it — and the chain then fails
/// closed because its module was never loaded.
#[tokio::test]
async fn a_declared_trigger_is_found_under_an_alias_and_fails_closed() {
    let gate = FunctionChainGate::new(hooks_for(&["guarded"]));
    let arguments = serde_json::json!({ "id": 1 });
    let request = BeforeMutationRequest::new(None, "guarded", "aliasedAs", &arguments);
    let err = gate
        .before_mutation(&request)
        .await
        .expect_err("a declared trigger with no module must refuse the write");
    assert!(
        matches!(&err, FraiseQLError::Internal { message, .. }
            if message == "before:mutation hook execution failed"),
        "lookup must use `mutation`, not `response_key`: {err:?}"
    );
}

/// The alias is never the lookup key: a document aliasing some *other* mutation to
/// `guarded` must not pick up `guarded`'s chain.
#[tokio::test]
async fn the_alias_is_never_the_lookup_key() {
    let gate = FunctionChainGate::new(hooks_for(&["guarded"]));
    let arguments = serde_json::json!({ "id": 1 });
    let request = BeforeMutationRequest::new(None, "harmless", "guarded", &arguments);
    let mapped = gate.before_mutation(&request).await.expect("no trigger for `harmless`");
    assert!(
        matches!(mapped, BeforeMutationOutcome::Proceed),
        "an alias spelled like a guarded mutation must not run that mutation's chain"
    );
}
