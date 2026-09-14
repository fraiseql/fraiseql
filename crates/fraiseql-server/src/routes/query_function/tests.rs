//! Tests for the function-backed query seam (#1329).
//!
//! Split the way `before_mutation`'s are: the budget and the diagnosis are tested
//! without a guest runtime, because running a real one needs Deno/WASM and a loaded
//! module — which would leave the arms that matter (the overrun, the ceiling
//! resolution) pinned by nothing.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::time::Duration;

use super::*;

// ── The budget ───────────────────────────────────────────────────────────────

/// An unset environment leaves the documented default in force.
#[test]
fn an_unset_override_leaves_the_default() {
    let budget = QueryFunctionBudget::from_getter(|_| None);
    assert_eq!(budget.duration(), Duration::from_millis(QueryFunctionBudget::DEFAULT_MS));
    assert!(budget.is_enforced());
}

/// The override is read in milliseconds, and `0` disables the ceiling.
#[test]
fn the_override_is_read_and_zero_disables_the_ceiling() {
    let budget = QueryFunctionBudget::from_getter(|key| {
        (key == QueryFunctionBudget::ENV).then(|| "250".into())
    });
    assert_eq!(budget.duration(), Duration::from_millis(250));

    let disabled = QueryFunctionBudget::from_getter(|key| {
        (key == QueryFunctionBudget::ENV).then(|| "0".into())
    });
    assert!(!disabled.is_enforced());
}

/// An unparseable override leaves the default rather than disabling the ceiling.
///
/// The direction matters: parsing `"soon"` as `0` would turn a typo into "no
/// ceiling at all", which is the failure a ceiling exists to prevent.
#[test]
fn an_unparseable_override_leaves_the_default() {
    let budget = QueryFunctionBudget::from_getter(|key| {
        (key == QueryFunctionBudget::ENV).then(|| "soon".into())
    });
    assert_eq!(budget.duration(), Duration::from_millis(QueryFunctionBudget::DEFAULT_MS));
}

/// A function's declared `timeout_ms` wins over the operator's default — in **both**
/// directions.
///
/// Both, deliberately. A rule that only let a declaration *shorten* the ceiling
/// would silently overrule an LLM-backed field's declared 30 s with a 5 s default,
/// which is the case the declaration exists for; and one that only let it lengthen
/// would ignore an author who deliberately wanted a tight field.
#[test]
fn a_declared_timeout_wins_over_the_default_in_both_directions() {
    let budget = QueryFunctionBudget::from_millis(5_000);
    assert_eq!(budget.for_function(Some(30_000)).duration(), Duration::from_millis(30_000));
    assert_eq!(budget.for_function(Some(250)).duration(), Duration::from_millis(250));
    assert_eq!(budget.for_function(None).duration(), Duration::from_millis(5_000));
}

// ── Both sides of the threshold ──────────────────────────────────────────────

/// An invocation inside the budget answers with what it returned.
#[tokio::test(start_paused = true)]
async fn an_invocation_inside_the_budget_answers() {
    let answer = run_within_budget("quotePreview", QueryFunctionBudget::from_millis(500), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(serde_json::json!({"id": "q-1"}))
    })
    .await
    .unwrap();
    assert_eq!(answer, serde_json::json!({"id": "q-1"}));
}

/// An invocation that overruns yields a timeout naming **the field**, not a generic
/// failure.
///
/// The field name is the assertion. An overrun flattened into "the function failed"
/// tells an operator nothing they can act on: a slow field and a broken one need
/// different answers, and only one of them is fixed by raising a number.
#[tokio::test(start_paused = true)]
async fn an_invocation_over_the_budget_times_out_naming_the_field() {
    let error = run_within_budget("quotePreview", QueryFunctionBudget::from_millis(100), async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(serde_json::json!({"id": "q-1"}))
    })
    .await
    .expect_err("an invocation over the ceiling must not answer");

    match error {
        FraiseQLError::Timeout { timeout_ms, query } => {
            assert_eq!(timeout_ms, 100);
            assert_eq!(query.as_deref(), Some("quotePreview"));
        },
        other => panic!("expected a Timeout naming the field, got: {other}"),
    }
}

/// A zero budget awaits the invocation rather than failing it immediately.
///
/// `tokio::time::timeout(Duration::ZERO, …)` elapses at once, so an implementation
/// that skipped the `is_enforced` check would turn "no ceiling" into "always times
/// out" — the exact inversion the flag exists to prevent.
#[tokio::test(start_paused = true)]
async fn a_zero_budget_means_no_ceiling() {
    let answer = run_within_budget("quotePreview", QueryFunctionBudget::from_millis(0), async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(serde_json::json!({"id": "q-1"}))
    })
    .await
    .unwrap();
    assert_eq!(answer, serde_json::json!({"id": "q-1"}));
}

/// The invocation's own error passes through unchanged — the budget wraps it, it
/// does not replace its diagnosis.
#[tokio::test(start_paused = true)]
async fn an_invocation_failure_keeps_its_own_diagnosis() {
    let error = run_within_budget("quotePreview", QueryFunctionBudget::from_millis(500), async {
        Err(FraiseQLError::Validation {
            message: "the quote engine is unavailable".to_string(),
            path:    None,
        })
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("quote engine is unavailable"), "got: {error}");
}

// ── Attribution ──────────────────────────────────────────────────────────────

/// A runtime failure is named for the field and its function, and keeps its own
/// message whole.
///
/// The guest's error arrives as a fact about the guest — "Failed to load WASM
/// component: expected `(`" — with nothing tying it to a GraphQL field. A server may
/// serve several function-backed fields, and which one was asked for is the only
/// thing distinguishing their failures.
#[cfg(feature = "functions-runtime")]
#[test]
fn a_runtime_failure_is_attributed_to_the_field_and_keeps_its_message() {
    let attributed = attribute(
        "quotePreview",
        "preview_quote",
        &FraiseQLError::Validation {
            message: "Failed to load WASM component".to_string(),
            path:    None,
        },
    );
    let message = attributed.to_string();
    assert!(message.contains("quotePreview"), "names the field: {message}");
    assert!(message.contains("preview_quote"), "and the function: {message}");
    assert!(
        message.contains("Failed to load WASM component"),
        "and keeps the only part an author can act on: {message}"
    );
}
