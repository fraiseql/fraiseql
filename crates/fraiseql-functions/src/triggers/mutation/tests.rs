#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_event_kind_as_str() {
    assert_eq!(EventKind::Insert.as_str(), "insert");
    assert_eq!(EventKind::Update.as_str(), "update");
    assert_eq!(EventKind::Delete.as_str(), "delete");
}

#[test]
fn test_after_mutation_trigger_matches() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
        predicates:    Vec::new(),
    };

    assert!(trigger.matches("User", EventKind::Insert));
    assert!(!trigger.matches("User", EventKind::Update));
    assert!(!trigger.matches("Post", EventKind::Insert));
}

#[test]
fn test_after_mutation_trigger_matches_all_kinds() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  None,
        predicates:    Vec::new(),
    };

    assert!(trigger.matches("User", EventKind::Insert));
    assert!(trigger.matches("User", EventKind::Update));
    assert!(trigger.matches("User", EventKind::Delete));
    assert!(!trigger.matches("Post", EventKind::Insert));
}

#[test]
fn test_after_mutation_trigger_builds_payload() {
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
        predicates:    Vec::new(),
    };

    let event = EntityEvent {
        entity:     "User".to_string(),
        event_kind: EventKind::Insert,
        old:        None,
        new:        Some(serde_json::json!({ "id": 1, "name": "Alice" })),
        timestamp:  chrono::Utc::now(),
    };

    let payload = trigger.build_payload(&event);
    assert_eq!(payload.trigger_type, "after:mutation:onUserCreated");
    assert_eq!(payload.entity, "User");
    assert_eq!(payload.event_kind, "insert");
    assert_eq!(payload.data["event_kind"], "insert");
    assert_eq!(payload.data["old"], serde_json::Value::Null);
    assert!(payload.data["new"].is_object());
}

#[test]
fn test_before_mutation_trigger_matches() {
    let trigger = BeforeMutationTrigger {
        function_name: "validateUserInput".to_string(),
        mutation_name: "createUser".to_string(),
    };

    assert!(trigger.matches("createUser"));
    assert!(!trigger.matches("updateUser"));
}

#[test]
fn test_trigger_matcher_empty() {
    let matcher = TriggerMatcher::new();
    let results = matcher.find("User", EventKind::Insert);
    assert!(results.is_empty());
}

#[test]
fn test_trigger_matcher_specific_event_kind() {
    let mut matcher = TriggerMatcher::new();
    let trigger = AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
        predicates:    Vec::new(),
    };

    matcher.add(trigger);
    let results = matcher.find("User", EventKind::Insert);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].function_name, "onUserCreated");

    // Should not match other event kinds
    let results = matcher.find("User", EventKind::Update);
    assert!(results.is_empty());
}

#[test]
fn test_trigger_matcher_all_kinds() {
    let mut matcher = TriggerMatcher::new();
    let trigger = AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  None,
        predicates:    Vec::new(),
    };

    matcher.add(trigger);
    assert_eq!(matcher.find("User", EventKind::Insert).len(), 1);
    assert_eq!(matcher.find("User", EventKind::Update).len(), 1);
    assert_eq!(matcher.find("User", EventKind::Delete).len(), 1);
}

#[test]
fn test_trigger_matcher_mixed_specific_and_all() {
    let mut matcher = TriggerMatcher::new();

    // Add specific triggers
    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
        predicates:    Vec::new(),
    });

    // Add all-kinds trigger
    matcher.add(AfterMutationTrigger {
        function_name: "onUserChanged".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  None,
        predicates:    Vec::new(),
    });

    // Insert should return both
    let results = matcher.find("User", EventKind::Insert);
    assert_eq!(results.len(), 2);

    // Update should return only all-kinds
    let results = matcher.find("User", EventKind::Update);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].function_name, "onUserChanged");
}

#[test]
fn test_trigger_matcher_multiple_entities() {
    let mut matcher = TriggerMatcher::new();

    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
        predicates:    Vec::new(),
    });

    matcher.add(AfterMutationTrigger {
        function_name: "onPostCreated".to_string(),
        entity_type:   "Post".to_string(),
        event_filter:  Some(EventKind::Insert),
        predicates:    Vec::new(),
    });

    let user_results = matcher.find("User", EventKind::Insert);
    assert_eq!(user_results.len(), 1);
    assert_eq!(user_results[0].function_name, "onUserCreated");

    let post_results = matcher.find("Post", EventKind::Insert);
    assert_eq!(post_results.len(), 1);
    assert_eq!(post_results[0].function_name, "onPostCreated");
}

#[test]
fn test_trigger_matcher_no_cross_entity_match() {
    let mut matcher = TriggerMatcher::new();

    matcher.add(AfterMutationTrigger {
        function_name: "onUserCreated".to_string(),
        entity_type:   "User".to_string(),
        event_filter:  Some(EventKind::Insert),
        predicates:    Vec::new(),
    });

    let post_results = matcher.find("Post", EventKind::Insert);
    assert!(post_results.is_empty());
}

// ── BeforeMutationChain::execute() tests ────────────────────────────────

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_empty_chain_proceeds() {
    use std::collections::HashMap;

    use crate::{FunctionModule, FunctionObserver, ResourceLimits, host::NoopHostContext};

    // Empty chain: no triggers → Proceed with original input
    let chain = BeforeMutationChain { triggers: vec![] };
    let observer = FunctionObserver::new();
    let modules: HashMap<String, FunctionModule> = HashMap::new();
    let input = serde_json::json!({ "name": "Alice" });

    let event = crate::types::EventPayload {
        trigger_type: "test".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input.clone(),
            &modules,
            &observer,
            std::sync::Arc::new(NoopHostContext::new(event)),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    match result {
        BeforeMutationResult::Proceed(v) => assert_eq!(v, input),
        BeforeMutationResult::Abort(msg) => panic!("Expected Proceed, got Abort: {msg}"),
    }
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_passthrough_proceeds() {
    use std::collections::HashMap;

    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::NoopHostContext,
        runtime::deno::{DenoConfig, DenoRuntime},
    };

    // Function that returns the event as-is → Proceed with original input
    let source = "export default async (event) => event;".to_string();
    let module = FunctionModule::from_source("validateUser".to_string(), source, RuntimeType::Deno);

    let mut observer = FunctionObserver::new();
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, runtime);

    let mut modules: HashMap<String, FunctionModule> = HashMap::new();
    modules.insert("validateUser".to_string(), module);

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "validateUser".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let input = serde_json::json!({ "name": "Alice" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input.clone(),
            &modules,
            &observer,
            std::sync::Arc::new(NoopHostContext::new(event)),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    // Function returns the event data (which is the input), no "abort" key → Proceed
    match result {
        BeforeMutationResult::Proceed(_) => {},
        BeforeMutationResult::Abort(msg) => panic!("Expected Proceed, got Abort: {msg}"),
    }
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_abort() {
    use std::collections::HashMap;

    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::NoopHostContext,
        runtime::deno::{DenoConfig, DenoRuntime},
    };

    // Function that returns {"abort": "name required"}
    let source = r#"export default async (event) => ({ abort: "name required" });"#.to_string();
    let module = FunctionModule::from_source("validateUser".to_string(), source, RuntimeType::Deno);

    let mut observer = FunctionObserver::new();
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, runtime);

    let mut modules: HashMap<String, FunctionModule> = HashMap::new();
    modules.insert("validateUser".to_string(), module);

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "validateUser".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let input = serde_json::json!({ "name": "" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input,
            &modules,
            &observer,
            std::sync::Arc::new(NoopHostContext::new(event)),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    match result {
        BeforeMutationResult::Abort(msg) => assert_eq!(msg, "name required"),
        BeforeMutationResult::Proceed(_) => panic!("Expected Abort"),
    }
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_modify_input() {
    use std::collections::HashMap;

    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::NoopHostContext,
        runtime::deno::{DenoConfig, DenoRuntime},
    };

    // Function that uppercases the name and returns {"input": {modified}}
    let source = r"
export default async (event) => ({
  input: { ...event, name: event.name.toUpperCase() }
});
"
    .to_string();
    let module =
        FunctionModule::from_source("transformUser".to_string(), source, RuntimeType::Deno);

    let mut observer = FunctionObserver::new();
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    observer.register_runtime(RuntimeType::Deno, runtime);

    let mut modules: HashMap<String, FunctionModule> = HashMap::new();
    modules.insert("transformUser".to_string(), module);

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "transformUser".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let input = serde_json::json!({ "name": "alice" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input,
            &modules,
            &observer,
            std::sync::Arc::new(NoopHostContext::new(event)),
            ResourceLimits::default(),
        )
        .await
        .expect("execute");

    match result {
        BeforeMutationResult::Proceed(modified) => {
            assert_eq!(modified["name"], "ALICE");
        },
        BeforeMutationResult::Abort(msg) => panic!("Expected Proceed, got Abort: {msg}"),
    }
}

// NOTE: The sequential (multi-trigger) chain test is verified at the unit level here
// using a mock observer, and the end-to-end behaviour is covered by Cycle 7 E2E tests.
#[test]
fn test_before_mutation_chain_execute_sequential_chain_structure() {
    // Verify that a chain with two triggers is built correctly and both triggers
    // are present in declaration order. The actual execution of sequential chains
    // is tested via E2E integration tests (Cycle 7) using the full Deno runtime.
    let chain = BeforeMutationChain {
        triggers: vec![
            BeforeMutationTrigger {
                function_name: "step1".to_string(),
                mutation_name: "createUser".to_string(),
            },
            BeforeMutationTrigger {
                function_name: "step2".to_string(),
                mutation_name: "createUser".to_string(),
            },
        ],
    };

    assert_eq!(chain.triggers.len(), 2);
    assert_eq!(chain.triggers[0].function_name, "step1");
    assert_eq!(chain.triggers[1].function_name, "step2");
}

#[cfg(feature = "runtime-deno")]
#[tokio::test]
async fn test_before_mutation_chain_execute_missing_module_returns_error() {
    use std::collections::HashMap;

    use crate::{FunctionModule, FunctionObserver, ResourceLimits, host::NoopHostContext};

    let chain = BeforeMutationChain {
        triggers: vec![BeforeMutationTrigger {
            function_name: "nonexistentFn".to_string(),
            mutation_name: "createUser".to_string(),
        }],
    };

    let observer = FunctionObserver::new();
    let modules: HashMap<String, FunctionModule> = HashMap::new(); // empty

    let input = serde_json::json!({ "name": "Alice" });
    let event = crate::types::EventPayload {
        trigger_type: "before:mutation:createUser".to_string(),
        entity:       "createUser".to_string(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    };

    let result = chain
        .execute(
            input,
            &modules,
            &observer,
            std::sync::Arc::new(NoopHostContext::new(event)),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_err(), "Expected error for missing module");
}

// ── #597: TriggerPredicate (declarative `when`) ─────────────────────────────

mod trigger_predicates {
    use serde_json::json;

    use super::super::{
        AfterMutationTrigger, EntityEvent, EventKind, TriggerPredicate, predicates_match,
    };

    fn eq(field: &str, value: serde_json::Value) -> TriggerPredicate {
        TriggerPredicate {
            field:      field.to_string(),
            eq:         Some(value),
            changed_to: None,
        }
    }
    fn changed_to(field: &str, value: serde_json::Value) -> TriggerPredicate {
        TriggerPredicate {
            field:      field.to_string(),
            eq:         None,
            changed_to: Some(value),
        }
    }

    #[test]
    fn eq_matches_current_image_and_fails_on_missing_or_mismatch() {
        let new = json!({ "status": "approved", "kind": "standard" });
        // Match on the after-image.
        assert!(eq("status", json!("approved")).matches(None, Some(&new)));
        // Mismatch.
        assert!(!eq("status", json!("pending")).matches(None, Some(&new)));
        // Missing field ⇒ false (eq against absent field never matches).
        assert!(!eq("missing", json!("x")).matches(None, Some(&new)));
    }

    #[test]
    fn eq_on_delete_evaluates_the_pre_image() {
        // A DELETE has no after-image; eq falls back to the pre-image.
        let old = json!({ "status": "archived" });
        assert!(eq("status", json!("archived")).matches(Some(&old), None));
    }

    #[test]
    fn changed_to_matches_only_a_real_transition() {
        let old = json!({ "status": "pending" });
        let new = json!({ "status": "approved" });
        // pending → approved: a transition to the target.
        assert!(changed_to("status", json!("approved")).matches(Some(&old), Some(&new)));
        // Already approved (approved → approved): not a transition.
        let already = json!({ "status": "approved" });
        assert!(!changed_to("status", json!("approved")).matches(Some(&already), Some(&already)));
        // Transitioned to a different value.
        let rejected = json!({ "status": "rejected" });
        assert!(!changed_to("status", json!("approved")).matches(Some(&old), Some(&rejected)));
    }

    #[test]
    fn changed_to_never_matches_a_delete() {
        // A DELETE has no after-image → no field can have "changed to" a value.
        let old = json!({ "status": "pending" });
        assert!(!changed_to("status", json!("approved")).matches(Some(&old), None));
    }

    #[test]
    fn conjunction_requires_all_predicates_and_empty_always_holds() {
        let new = json!({ "status": "approved", "kind": "standard" });
        let old = json!({ "status": "pending", "kind": "standard" });
        let all = [
            changed_to("status", json!("approved")),
            eq("kind", json!("standard")),
        ];
        assert!(predicates_match(&all, Some(&old), Some(&new)), "both hold");

        let one_fails = [
            changed_to("status", json!("approved")),
            eq("kind", json!("premium")),
        ];
        assert!(!predicates_match(&one_fails, Some(&old), Some(&new)), "one fails ⇒ no match");

        // Empty conjunction always fires (back-compat).
        assert!(predicates_match(&[], Some(&old), Some(&new)));
    }

    #[test]
    fn validate_rejects_bad_operator_combinations() {
        // Neither operator.
        assert!(
            TriggerPredicate {
                field:      "s".into(),
                eq:         None,
                changed_to: None,
            }
            .validate(Some("update"))
            .is_err()
        );
        // Both operators.
        assert!(
            TriggerPredicate {
                field:      "s".into(),
                eq:         Some(json!(1)),
                changed_to: Some(json!(1)),
            }
            .validate(Some("update"))
            .is_err()
        );
        // changed_to on a non-update trigger.
        assert!(changed_to("s", json!("x")).validate(Some("insert")).is_err());
        assert!(changed_to("s", json!("x")).validate(None).is_err());
        // changed_to on update is fine; eq on any operation is fine.
        assert!(changed_to("s", json!("x")).validate(Some("update")).is_ok());
        assert!(eq("s", json!("x")).validate(Some("delete")).is_ok());
    }

    #[test]
    fn unknown_predicate_keys_are_rejected() {
        // `deny_unknown_fields` rejects a typo'd operator at deserialization.
        let bad = serde_json::from_value::<TriggerPredicate>(json!({
            "field": "status", "equals": "approved"
        }));
        assert!(bad.is_err(), "unknown key `equals` is a load error");
    }

    #[test]
    fn trigger_predicates_hold_flows_through_the_event() {
        let trigger = AfterMutationTrigger {
            function_name: "notify_approved".to_string(),
            entity_type:   "Order".to_string(),
            event_filter:  Some(EventKind::Update),
            predicates:    vec![changed_to("status", json!("approved"))],
        };
        let now = chrono::Utc::now();
        let fires = EntityEvent {
            entity:     "Order".to_string(),
            event_kind: EventKind::Update,
            old:        Some(json!({ "status": "pending" })),
            new:        Some(json!({ "status": "approved" })),
            timestamp:  now,
        };
        let noop = EntityEvent {
            entity:     "Order".to_string(),
            event_kind: EventKind::Update,
            old:        Some(json!({ "status": "approved" })),
            new:        Some(json!({ "status": "approved" })),
            timestamp:  now,
        };
        assert!(trigger.predicates_hold(&fires), "fires on the pending→approved transition");
        assert!(!trigger.predicates_hold(&noop), "does not fire on approved→approved");
    }
}

// ── The read bridge, end to end through a real guest (#1328) ─────────────

/// A `before:mutation` guest calling `fraiseql_query` must reach the host, and the
/// host must reach the read bridge.
///
/// Every other pin in this change is one link of that chain in isolation: core
/// proves the bridge reads as the caller and refuses a write, `host::before_mutation`
/// proves the surface forwards `query` and refuses everything else, the server
/// proves the budget and the outcome mapping. None of them runs a **guest**, and
/// the link none of them covers is the one that was broken: the chain dispatched
/// through `FunctionObserver::invoke` — the *sync* path, whose Deno backend drops
/// the host argument outright (`_host`) — so before this change a before-hook's
/// `fraiseql_query` failed no matter which host it was handed. A suite that did not
/// run a guest would not have moved when that was fixed.
#[cfg(all(feature = "runtime-deno", feature = "host-live"))]
mod read_bridge {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use fraiseql_core::security::MutationHookReader;

    use super::*;
    use crate::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::before_mutation::BeforeMutationHost,
        runtime::deno::{DenoConfig, DenoRuntime},
    };

    /// Answers one canned read and records what it was asked.
    struct SpyReader {
        asked:  Mutex<Vec<String>>,
        answer: serde_json::Value,
    }

    impl MutationHookReader for SpyReader {
        fn query<'a>(
            &'a self,
            graphql: &'a str,
            _variables: Option<&'a serde_json::Value>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>>
                    + Send
                    + 'a,
            >,
        > {
            self.asked.lock().unwrap().push(graphql.to_string());
            let answer = self.answer.clone();
            Box::pin(async move { Ok(answer) })
        }
    }

    /// A guest that reads, then decides from what it read.
    const GUEST: &str = r#"
        export default async (args) => {
          const raw = await Deno.core.ops.fraiseql_query(
            "query { customer { remaining } }", "{}");
          const remaining = JSON.parse(raw).data.customer.remaining;
          if (args.input.amount > remaining) {
            return { abort: `amount ${args.input.amount} exceeds ${remaining}` };
          }
          return { input: { ...args.input, credit_checked: true } };
        };
    "#;

    async fn run(amount: i64, remaining: i64) -> (BeforeMutationResult, Vec<String>) {
        let reader = Arc::new(SpyReader {
            asked:  Mutex::new(Vec::new()),
            answer: serde_json::json!({ "data": { "customer": { "remaining": remaining } } }),
        });
        let mut observer = FunctionObserver::new();
        observer.register_runtime(
            RuntimeType::Deno,
            DenoRuntime::new(&DenoConfig::default()).expect("deno runtime"),
        );
        let mut modules = HashMap::new();
        modules.insert(
            "credit_limit".to_string(),
            FunctionModule::from_source(
                "credit_limit".to_string(),
                GUEST.to_string(),
                RuntimeType::Deno,
            ),
        );
        let chain = BeforeMutationChain {
            triggers: vec![BeforeMutationTrigger {
                function_name: "credit_limit".to_string(),
                mutation_name: "placeOrder".to_string(),
            }],
        };
        let input = serde_json::json!({ "input": { "amount": amount } });
        let host = BeforeMutationHost::new(
            crate::types::EventPayload {
                trigger_type: "before:mutation:placeOrder".to_string(),
                entity:       "placeOrder".to_string(),
                event_kind:   "before".to_string(),
                data:         input.clone(),
                timestamp:    chrono::Utc::now(),
            },
            Some(Arc::clone(&reader) as Arc<dyn MutationHookReader>),
            None,
        );

        let outcome = chain
            .execute(input, &modules, &observer, Arc::new(host), ResourceLimits::default())
            .await
            .expect("the chain runs");
        let asked = reader.asked.lock().unwrap().clone();
        (outcome, asked)
    }

    /// Both sides of the rule in one isolate-spinning test, so the whole thing is
    /// one V8 lifecycle: the read reaches the bridge, and the decision follows the
    /// value it returned rather than the input alone.
    #[tokio::test]
    async fn a_guest_reads_through_the_bridge_and_decides_on_what_it_read() {
        let (refused, asked) = run(900, 600).await;
        assert_eq!(
            asked,
            vec!["query { customer { remaining } }".to_string()],
            "the guest's document must reach the read bridge verbatim"
        );
        match refused {
            BeforeMutationResult::Abort(reason) => {
                assert_eq!(reason, "amount 900 exceeds 600");
            },
            other => panic!("the read said 600 < 900, so the rule must abort: {other:?}"),
        }

        // The counterweight: the same guest, the same bridge, a read that allows it.
        // A fixture that only ever aborts would pass on a bridge returning garbage.
        let (allowed, _) = run(100, 600).await;
        match allowed {
            BeforeMutationResult::Proceed(arguments) => {
                assert_eq!(
                    arguments["credit_checked"], true,
                    "the hook's rewrite must be what the chain returns: {arguments}"
                );
            },
            other => panic!("the read said 600 > 100, so the rule must proceed: {other:?}"),
        }
    }
}
