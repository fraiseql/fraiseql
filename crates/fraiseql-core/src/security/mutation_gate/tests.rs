//! Unit tests for the `before:mutation` enforcement helper.
//!
//! These pin the *helper's* contract (what each outcome does to the write's
//! arguments, and that `Err` refuses). That the helper is actually **reached**
//! from every mutation entry path — the three #1327 bypasses — is pinned in
//! `runtime::executor::runners::mutation::tests`, against a real executor.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::{
    BeforeMutationGate, BeforeMutationOutcome, BeforeMutationRequest, enforce_before_mutation,
};
use crate::error::{FraiseQLError, Result};

/// Records what the gate was handed, and answers with a fixed outcome.
struct Spy {
    outcome:  fn() -> Result<BeforeMutationOutcome>,
    observed: std::sync::Mutex<Vec<(String, String, serde_json::Value)>>,
}

impl Spy {
    fn new(outcome: fn() -> Result<BeforeMutationOutcome>) -> Self {
        Self {
            outcome,
            observed: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn observed(&self) -> Vec<(String, String, serde_json::Value)> {
        self.observed.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl BeforeMutationGate for Spy {
    async fn before_mutation(
        &self,
        request: &BeforeMutationRequest<'_>,
    ) -> Result<BeforeMutationOutcome> {
        self.observed.lock().unwrap().push((
            request.mutation.to_string(),
            request.response_key.to_string(),
            request.arguments.clone(),
        ));
        (self.outcome)()
    }
}

/// A reader double: answers every read with a fixed value and counts the reads.
///
/// The helper's contract is that a gate *receives* one, not what it returns —
/// what the engine's real reader does (refuse a write, scope to the caller) is
/// pinned against a real executor in
/// `runtime::executor::runners::mutation::tests::before_mutation_read_bridge`.
struct StubReader {
    reads: std::sync::Mutex<Vec<String>>,
}

impl crate::security::MutationHookReader for StubReader {
    fn query<'a>(
        &'a self,
        graphql: &'a str,
        _variables: Option<&'a serde_json::Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        self.reads.lock().unwrap().push(graphql.to_string());
        Box::pin(async { Ok(serde_json::json!({ "data": {} })) })
    }
}

/// A reader factory that panics when called — the mutation that proves
/// `enforce_before_mutation` builds the reader **after** the no-gate early
/// return, and not on every write.
fn reader_must_not_be_built() -> std::sync::Arc<dyn crate::security::MutationHookReader> {
    panic!("the read bridge must not be built when no gate is configured");
}

/// A reader factory for the paths that do reach a gate.
fn stub_reader() -> std::sync::Arc<dyn crate::security::MutationHookReader> {
    std::sync::Arc::new(StubReader {
        reads: std::sync::Mutex::new(Vec::new()),
    })
}

/// A gate that panics if consulted — proves the no-gate path never calls one.
struct PanicIfCalled;

#[async_trait::async_trait]
impl BeforeMutationGate for PanicIfCalled {
    async fn before_mutation(
        &self,
        _request: &BeforeMutationRequest<'_>,
    ) -> Result<BeforeMutationOutcome> {
        panic!("gate must not be consulted");
    }
}

#[tokio::test]
async fn no_gate_configured_is_a_no_op() {
    let args = serde_json::json!({ "input": { "name": "Bob" } });
    let out = enforce_before_mutation(
        None,
        None,
        "createUser",
        "createUser",
        Some(&args),
        reader_must_not_be_built,
    )
    .await
    .expect("no gate cannot fail");
    assert!(out.is_none(), "no gate must leave the arguments alone");
}

/// The `PanicIfCalled` gate is only silent because it is never passed in — this
/// is the mutation that proves the previous test is not vacuous.
#[tokio::test]
#[should_panic(expected = "gate must not be consulted")]
async fn a_configured_gate_is_consulted() {
    let gate = PanicIfCalled;
    let _ =
        enforce_before_mutation(Some(&gate), None, "createUser", "createUser", None, stub_reader)
            .await;
}

#[tokio::test]
async fn proceed_leaves_the_arguments_unchanged() {
    let gate = Spy::new(|| Ok(BeforeMutationOutcome::Proceed));
    let args = serde_json::json!({ "input": { "name": "Bob" } });
    let out = enforce_before_mutation(
        Some(&gate),
        None,
        "createUser",
        "created",
        Some(&args),
        stub_reader,
    )
    .await
    .unwrap();
    assert!(out.is_none(), "Proceed must not replace the arguments");
}

#[tokio::test]
async fn proceed_with_replaces_the_arguments() {
    let gate = Spy::new(|| {
        Ok(BeforeMutationOutcome::ProceedWith {
            arguments: serde_json::json!({ "input": { "name": "REWRITTEN" } }),
        })
    });
    let args = serde_json::json!({ "input": { "name": "Bob" } });
    let out = enforce_before_mutation(
        Some(&gate),
        None,
        "createUser",
        "created",
        Some(&args),
        stub_reader,
    )
    .await
    .unwrap()
    .expect("ProceedWith must return the replacement");
    assert_eq!(
        out["input"]["name"], "REWRITTEN",
        "the rewrite must be what the write binds from"
    );
}

#[tokio::test]
async fn abort_refuses_the_write_with_the_rules_own_message() {
    let gate = Spy::new(|| {
        Ok(BeforeMutationOutcome::Abort {
            reason: "amount exceeds the approval ceiling".to_string(),
        })
    });
    let err = enforce_before_mutation(Some(&gate), None, "pay", "pay", None, stub_reader)
        .await
        .expect_err("Abort must refuse the write");
    assert!(
        matches!(&err, FraiseQLError::Validation { message, .. }
            if message == "amount exceeds the approval ceiling"),
        "the rule's own message must reach the client verbatim: {err:?}"
    );
}

/// Fail-closed: a gate that cannot decide must not fall through to "proceed with
/// the original input" — that would run a write the chain never approved.
#[tokio::test]
async fn a_gate_error_refuses_the_write() {
    let gate = Spy::new(|| {
        Err(FraiseQLError::Internal {
            message: "before:mutation hook execution failed".to_string(),
            source:  None,
        })
    });
    let err = enforce_before_mutation(Some(&gate), None, "pay", "pay", None, stub_reader)
        .await
        .expect_err("a gate error must refuse the write, not proceed");
    assert!(
        matches!(err, FraiseQLError::Internal { .. }),
        "the gate's error stands: {err:?}"
    );
}

/// The chain is keyed on the field name; the alias travels separately so a rule
/// can report it without ever being looked up by it.
#[tokio::test]
async fn the_gate_sees_the_field_name_and_the_alias_separately() {
    let gate = Spy::new(|| Ok(BeforeMutationOutcome::Proceed));
    let args = serde_json::json!({ "id": 1 });
    enforce_before_mutation(Some(&gate), None, "archiveUser", "archived", Some(&args), stub_reader)
        .await
        .unwrap();
    assert_eq!(
        gate.observed(),
        vec![("archiveUser".to_string(), "archived".to_string(), args)],
        "mutation must be the field name and response_key the alias"
    );
}

/// An argument-less write is handed `Null`, the payload shape the `before:mutation`
/// chain has always received — not `{}`.
#[tokio::test]
async fn an_argumentless_write_is_handed_null() {
    let gate = Spy::new(|| Ok(BeforeMutationOutcome::Proceed));
    enforce_before_mutation(Some(&gate), None, "reindex", "reindex", None, stub_reader)
        .await
        .unwrap();
    assert_eq!(gate.observed()[0].2, serde_json::Value::Null);
}
