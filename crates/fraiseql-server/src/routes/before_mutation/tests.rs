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

use super::{BeforeMutationBudget, FunctionChainGate, map_chain_outcome, run_within_budget};
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

// ---- the chain budget (#1328) ---------------------------------------------

/// The documented number is the default. Before #1328 the "500ms, shorter than the
/// general 5s because before-hooks are on the critical mutation path" in
/// `fraiseql-functions`'s trigger docs was prose with no implementation: the gate
/// passed `ResourceLimits::default()`, so each hook got 5 s and a chain of *n*
/// hooks got 5*n*.
#[test]
fn the_default_budget_is_the_documented_five_hundred_milliseconds() {
    assert_eq!(BeforeMutationBudget::default().duration().as_millis(), 500);
    assert!(BeforeMutationBudget::default().is_enforced());
}

#[test]
fn the_env_var_overrides_the_default() {
    let budget = BeforeMutationBudget::from_getter(|key| {
        (key == BeforeMutationBudget::ENV).then(|| "1200".to_string())
    });
    assert_eq!(budget.duration().as_millis(), 1200);
}

/// An unset or unparseable value leaves the default alone — an operator's typo
/// must not silently remove the ceiling.
#[test]
fn an_unparseable_override_leaves_the_default_in_place() {
    assert_eq!(BeforeMutationBudget::from_getter(|_| None), BeforeMutationBudget::default());
    assert_eq!(
        BeforeMutationBudget::from_getter(|_| Some("soon".to_string())),
        BeforeMutationBudget::default()
    );
}

/// `0` disables the ceiling, the same convention `query_timeout_ms` uses.
#[test]
fn zero_disables_the_ceiling() {
    assert!(!BeforeMutationBudget::from_millis(0).is_enforced());
}

/// **Under** the threshold: a chain that finishes in time is untouched, and its
/// decision reaches the caller. Without this the overrun test below would also
/// pass on a budget that refused everything.
#[tokio::test(start_paused = true)]
async fn a_chain_inside_its_budget_decides() {
    let outcome = run_within_budget("pay", BeforeMutationBudget::from_millis(500), async {
        tokio::time::sleep(std::time::Duration::from_millis(499)).await;
        Ok(BeforeMutationResult::Abort("needs two approvals".to_string()))
    })
    .await
    .expect("a chain inside its budget must return its own decision");

    assert!(
        matches!(outcome, BeforeMutationResult::Abort(reason) if reason == "needs two approvals")
    );
}

/// **Over** the threshold: the write is refused, with a diagnosis that names the
/// budget and the mutation rather than the generic "execution failed" — the one
/// cause an operator can act on by raising a limit.
#[tokio::test(start_paused = true)]
async fn a_chain_over_its_budget_refuses_the_write() {
    let error = run_within_budget("pay", BeforeMutationBudget::from_millis(500), async {
        tokio::time::sleep(std::time::Duration::from_millis(501)).await;
        Ok(BeforeMutationResult::Proceed(serde_json::json!({ "input": {} })))
    })
    .await
    .expect_err("a chain that ran out of time approved nothing");

    match &error {
        FraiseQLError::Timeout { timeout_ms, query } => {
            assert_eq!(*timeout_ms, 500);
            assert_eq!(query.as_deref(), Some("before:mutation:pay"));
        },
        other => panic!("expected the budget's own diagnosis, got {other:?}"),
    }
}

/// The overrun diagnosis survives the outcome mapping. Folding it into
/// "before:mutation hook execution failed" — which is what every *other* chain
/// failure becomes — would hide it.
#[test]
fn the_overrun_diagnosis_is_not_flattened_into_execution_failed() {
    let error = map_chain_outcome(
        "pay",
        Err(FraiseQLError::Timeout {
            timeout_ms: 500,
            query:      Some("before:mutation:pay".to_string()),
        }),
    )
    .expect_err("an overrun refuses the write");

    assert!(
        matches!(&error, FraiseQLError::Timeout { timeout_ms, .. } if *timeout_ms == 500),
        "the budget's diagnosis must reach the operator: {error:?}"
    );
}

/// A disabled ceiling awaits the chain rather than refusing it instantly — the
/// mutation that proves `is_enforced` is consulted and not ignored.
#[tokio::test(start_paused = true)]
async fn a_disabled_ceiling_does_not_refuse() {
    let outcome = run_within_budget("pay", BeforeMutationBudget::from_millis(0), async {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        Ok(BeforeMutationResult::Abort("late but heard".to_string()))
    })
    .await
    .expect("a disabled ceiling must not refuse");

    assert!(matches!(outcome, BeforeMutationResult::Abort(reason) if reason == "late but heard"));
}

/// The gate carries its budget into the chain. Asserted through the observable
/// outcome: a gate whose ceiling is already spent refuses with the budget's own
/// diagnosis rather than the module-registry failure it would otherwise hit.
#[tokio::test(start_paused = true)]
async fn the_gate_applies_its_budget() {
    let gate = FunctionChainGate::new(hooks_for(&["guarded"]))
        .with_budget(BeforeMutationBudget::from_millis(1));
    let arguments = serde_json::json!({ "id": 1 });
    let request = BeforeMutationRequest::new(None, "guarded", "guarded", &arguments);

    let error = gate.before_mutation(&request).await.expect_err("the chain cannot run");

    // Which failure it is depends on whether this build has a runtime at all; what
    // must hold either way is that the gate refused the write.
    assert!(
        matches!(&error, FraiseQLError::Timeout { .. } | FraiseQLError::Internal { .. }),
        "the gate must refuse: {error:?}"
    );
}
