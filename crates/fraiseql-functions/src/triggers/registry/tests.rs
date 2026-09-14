#![allow(clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_parse_after_mutation_trigger() {
    let parsed = ParsedTrigger::parse("after:mutation:createUser").expect("parse");
    match parsed {
        ParsedTrigger::AfterMutation {
            entity_type,
            operation,
        } => {
            assert_eq!(entity_type, "createUser");
            assert_eq!(operation, None);
        },
        _ => panic!("Wrong trigger type"),
    }
}

#[test]
fn test_parse_before_mutation_trigger() {
    let parsed = ParsedTrigger::parse("before:mutation:validateUser").expect("parse");
    match parsed {
        ParsedTrigger::BeforeMutation { mutation_name } => {
            assert_eq!(mutation_name, "validateUser");
        },
        _ => panic!("Wrong trigger type"),
    }
}

#[test]
fn test_parse_http_trigger() {
    let parsed = ParsedTrigger::parse("http:GET:/users/:id").expect("parse");
    match parsed {
        ParsedTrigger::Http { method, path } => {
            assert_eq!(method, "GET");
            assert_eq!(path, "/users/:id");
        },
        _ => panic!("Wrong trigger type"),
    }
}

#[test]
fn test_parse_cron_trigger() {
    let parsed = ParsedTrigger::parse("cron:0 2 * * *").expect("parse");
    match parsed {
        ParsedTrigger::Cron { expression } => {
            assert_eq!(expression, "0 2 * * *");
        },
        _ => panic!("Wrong trigger type"),
    }
}

#[test]
fn test_parse_invalid_trigger() {
    let result = ParsedTrigger::parse("invalid:format:here");
    assert!(result.is_err());
}

#[test]
fn test_parse_after_ingest_triggers() {
    // Bare: matches every source.
    match ParsedTrigger::parse("after:ingest").expect("parse") {
        ParsedTrigger::AfterIngest { source } => assert_eq!(source, None),
        _ => panic!("Wrong trigger type"),
    }
    // Simple source.
    match ParsedTrigger::parse("after:ingest:email").expect("parse") {
        ParsedTrigger::AfterIngest { source } => assert_eq!(source.as_deref(), Some("email")),
        _ => panic!("Wrong trigger type"),
    }
    // Colon-bearing source (webhook:<provider>) is rejoined intact.
    match ParsedTrigger::parse("after:ingest:webhook:stripe").expect("parse") {
        ParsedTrigger::AfterIngest { source } => {
            assert_eq!(source.as_deref(), Some("webhook:stripe"));
        },
        _ => panic!("Wrong trigger type"),
    }
    assert_eq!(
        ParsedTrigger::parse("after:ingest").expect("parse").trigger_type(),
        "after:ingest"
    );
}

// ── #842: an unrecognized operation token must fail the load, never widen ────

/// Test: #842 — a typo'd/wrong-case/past-tense operation token is a load
/// error, not a silent widening of the trigger to every event kind.
#[test]
fn test_registry_rejects_unknown_after_mutation_operation_token() {
    for trigger in [
        "after:mutation:User:created",
        "after:mutation:User:INSERT",
        "after:mutation:User:inserts",
        "after:capture:User:updated",
    ] {
        let defs = vec![crate::FunctionDefinition::new(
            "onUserCreated",
            trigger,
            crate::RuntimeType::Deno,
        )];
        let error = TriggerRegistry::load_from_definitions(&defs)
            .expect_err(&format!("`{trigger}` must abort the load, not register all-kinds"));
        assert!(
            error.message.contains("onUserCreated") && error.message.contains("insert"),
            "the error must name the function and the valid tokens, got: {}",
            error.message
        );
    }
}

/// Test: #842 — the documented `after:mutation:<Entity>:*` wildcard keeps
/// working (docs/examples advertise it; a naive strict reject would regress
/// it) and fires on every event kind, exactly like the token-less form.
#[test]
fn test_registry_wildcard_operation_matches_every_event_kind() {
    use crate::triggers::mutation::EventKind;

    for trigger in ["after:mutation:User:*", "after:mutation:User"] {
        let defs = vec![crate::FunctionDefinition::new(
            "onAnyUserChange",
            trigger,
            crate::RuntimeType::Deno,
        )];
        let registry = TriggerRegistry::load_from_definitions(&defs)
            .unwrap_or_else(|e| panic!("`{trigger}` must load: {e}"));
        for kind in [EventKind::Insert, EventKind::Update, EventKind::Delete] {
            assert_eq!(
                registry.after_mutation_triggers.find("User", kind).len(),
                1,
                "`{trigger}` must fire on {kind:?}"
            );
        }
    }
}

/// Test: #842 — a valid explicit token still narrows to exactly its kind.
#[test]
fn test_registry_explicit_operation_narrows_to_one_kind() {
    use crate::triggers::mutation::EventKind;

    let defs = vec![crate::FunctionDefinition::new(
        "onUserInsert",
        "after:mutation:User:insert",
        crate::RuntimeType::Deno,
    )];
    let registry = TriggerRegistry::load_from_definitions(&defs).expect("valid token loads");
    assert_eq!(registry.after_mutation_triggers.find("User", EventKind::Insert).len(), 1);
    assert_eq!(registry.after_mutation_triggers.find("User", EventKind::Update).len(), 0);
    assert_eq!(registry.after_mutation_triggers.find("User", EventKind::Delete).len(), 0);
}

/// #871 item 2: an `http:` trigger is accepted at load and never mounted — no
/// server code consumes `http_routes`, and `POST /functions/v1/{name}`
/// dispatches by function name, ignoring the trigger entirely. Until routes are
/// actually mounted, a declared `http:` function must abort startup with the
/// same loud error `after:storage` gets, not silently never serve.
#[test]
fn test_registry_rejects_unmounted_http_triggers() {
    let defs = vec![crate::FunctionDefinition::new(
        "avatarUpload",
        "http:POST:/users/:id/avatar",
        crate::RuntimeType::Deno,
    )];
    let error = TriggerRegistry::load_from_definitions(&defs)
        .expect_err("an http: trigger must abort the load until routes are actually mounted");
    assert!(
        error.message.contains("avatarUpload") && error.message.contains("not mounted"),
        "the error names the function and says the surface is unavailable, got: {}",
        error.message
    );
    // The remedy is part of the refusal, and it changed with #1329: the
    // name-dispatched invoke route it used to name was library-only and is retired,
    // so an author following the old advice would reach a door that no longer exists.
    assert!(
        error.message.contains("request:query") && error.message.contains("function ="),
        "the refusal must point at the surface that does serve requests, got: {}",
        error.message
    );
}

#[test]
fn test_registry_registers_ingest_triggers() {
    use crate::{FunctionDefinition, InboundMessage, IngestSource, RuntimeType};

    let functions = vec![
        FunctionDefinition::new("onAnyInbound", "after:ingest", RuntimeType::Deno),
        FunctionDefinition::new("onStripe", "after:ingest:webhook:stripe", RuntimeType::Deno),
        FunctionDefinition::new("onEmail", "after:ingest:email", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load registry");
    assert_eq!(registry.ingest_trigger_count(), 3);

    let stripe_msg = InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        "evt_1",
        chrono::Utc::now(),
    );
    // The stripe message fires the source-agnostic trigger and the stripe-specific
    // one, but not the email trigger.
    let matched: Vec<_> = registry
        .find_ingest_triggers(&stripe_msg)
        .into_iter()
        .map(|t| t.function_name)
        .collect();
    assert_eq!(matched.len(), 2);
    assert!(matched.contains(&"onAnyInbound".to_string()));
    assert!(matched.contains(&"onStripe".to_string()));
    assert!(!matched.contains(&"onEmail".to_string()));
}

#[test]
fn test_registry_rejects_unknown_ingest_source() {
    use crate::{FunctionDefinition, RuntimeType};

    let functions = vec![FunctionDefinition::new(
        "onBad",
        "after:ingest:carrier-pigeon",
        RuntimeType::Deno,
    )];
    let error = TriggerRegistry::load_from_definitions(&functions)
        .expect_err("unknown source must fail loud");
    assert!(error.message.contains("carrier-pigeon"));
}

#[test]
fn test_registry_loads_multiple_triggers() {
    use crate::{FunctionDefinition, RuntimeType};

    let functions = vec![
        FunctionDefinition::new("onUserCreated", "after:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("validateInput", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("dailyReport", "cron:0 2 * * *", RuntimeType::Deno),
    ];

    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load registry");

    assert_eq!(registry.function_count, 3);
    assert_eq!(registry.before_mutation_count(), 1);
    assert_eq!(registry.cron_trigger_count(), 1);
}

#[test]
fn test_parsed_trigger_type_detection() {
    let after_mut = ParsedTrigger::parse("after:mutation:createUser").expect("parse");
    assert!(after_mut.is_after_mutation());
    assert_eq!(after_mut.trigger_type(), "after:mutation");

    let http = ParsedTrigger::parse("http:POST:/data").expect("parse");
    assert!(http.is_http());
    assert_eq!(http.trigger_type(), "http");
}

#[test]
fn test_registry_before_mutation_lookup() {
    use crate::{FunctionDefinition, RuntimeType};

    let functions = vec![
        FunctionDefinition::new("validate1", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("validate2", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("validate3", "before:mutation:deleteUser", RuntimeType::Deno),
    ];

    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load registry");

    assert_eq!(registry.before_mutation_count(), 3);
    assert!(registry.has_before_mutation_triggers("createUser"));
    assert!(registry.has_before_mutation_triggers("deleteUser"));
    assert!(!registry.has_before_mutation_triggers("updateUser"));

    let create_user_triggers = registry.before_mutation_triggers_for("createUser");
    assert_eq!(create_user_triggers.len(), 2);
}

#[test]
fn test_registry_before_chain_returns_none_for_unknown_mutation() {
    use crate::{FunctionDefinition, RuntimeType};

    let functions = vec![FunctionDefinition::new(
        "validate",
        "before:mutation:createUser",
        RuntimeType::Deno,
    )];
    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load");

    // Unknown mutation → None (zero overhead fast path)
    assert!(registry.before_chain("updateUser").is_none());
    assert!(registry.before_chain("deleteUser").is_none());
}

#[test]
fn test_registry_before_chain_returns_chain_for_known_mutation() {
    use crate::{FunctionDefinition, RuntimeType};

    let functions = vec![
        FunctionDefinition::new("validate1", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("validate2", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("other", "before:mutation:deleteUser", RuntimeType::Deno),
    ];
    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load");

    let chain = registry.before_chain("createUser").expect("chain present");
    assert_eq!(chain.triggers.len(), 2);
    assert_eq!(chain.triggers[0].function_name, "validate1");
    assert_eq!(chain.triggers[1].function_name, "validate2");

    // deleteUser chain has only 1 trigger
    let del_chain = registry.before_chain("deleteUser").expect("chain present");
    assert_eq!(del_chain.triggers.len(), 1);
}

// ── #1329: `request:query`, and the pairing it only half-declares ────────────
//
// The trigger names a capability, not an event, so nothing in this module can
// tell whether a `request:query` function will ever be invoked — the query that
// declares `function = "<name>"` decides, and it lives in the compiled schema.
// `validate_query_bindings` is where the two halves meet, and each test below
// mutates exactly one half so a check that fired for the other reason fails.

use crate::{FunctionDefinition, RuntimeType, triggers::registry::QueryFunctionBinding};

/// `request:query` parses to the kind, carrying nothing.
#[test]
fn a_request_query_trigger_parses_to_the_kind() {
    let parsed = ParsedTrigger::parse("request:query").expect("parse");
    assert_eq!(parsed, ParsedTrigger::RequestQuery);
    assert!(parsed.is_request_query());
    assert_eq!(parsed.trigger_type(), "request:query");
}

/// A selector after `request:query` is refused rather than ignored.
///
/// Every other trigger takes one, so writing `request:query:quotePreview` is the
/// natural mistake. Parsing it to a bare `RequestQuery` would drop the name the
/// author wrote while the declaration still reads as if it bound something.
#[test]
fn a_request_query_trigger_refuses_a_selector() {
    let error = ParsedTrigger::parse("request:query:quotePreview")
        .expect_err("a selector must not be silently dropped");
    assert!(
        error.message.contains("request:query:quotePreview"),
        "the refusal must quote what was written; got: {}",
        error.message
    );
}

/// `request` alone is not a trigger — the kind is required.
#[test]
fn a_bare_request_trigger_is_refused() {
    ParsedTrigger::parse("request").expect_err("`request` names no kind");
}

/// Loading records the function under its own list and adds no matcher entry.
///
/// A request-serving function is reached from a compiled query, never from an
/// event, so an entry in `before_mutation_triggers` or `ingest_triggers` would
/// mean something dispatches it twice.
#[test]
fn loading_a_request_query_function_registers_no_event_trigger() {
    let functions = vec![FunctionDefinition::new(
        "preview_quote",
        "request:query",
        RuntimeType::Deno,
    )];
    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load");

    assert_eq!(registry.request_query_functions, vec!["preview_quote".to_string()]);
    assert_eq!(registry.before_mutation_count(), 0);
    assert_eq!(registry.cron_trigger_count(), 0);
    assert_eq!(registry.ingest_trigger_count(), 0);
}

/// The fixture both directions of the pairing are mutated against.
fn preview_quote() -> Vec<FunctionDefinition> {
    vec![FunctionDefinition::new(
        "preview_quote",
        "request:query",
        RuntimeType::Deno,
    )]
}

/// A query naming a declared `request:query` function pairs.
#[test]
fn a_query_naming_a_request_function_pairs() {
    let bindings = [QueryFunctionBinding {
        query:    "quotePreview",
        function: "preview_quote",
    }];
    TriggerRegistry::validate_query_bindings(&bindings, &preview_quote())
        .expect("the declared pair must validate");
}

/// A query naming a function nobody declared is refused, and the refusal names
/// the query, the missing name, and what *is* declared.
#[test]
fn a_query_naming_an_undeclared_function_is_refused() {
    let bindings = [QueryFunctionBinding {
        query:    "quotePreview",
        function: "preview_qoute",
    }];
    let error = TriggerRegistry::validate_query_bindings(&bindings, &preview_quote())
        .expect_err("a query bound to nothing must be refused");
    assert!(
        error.message.contains("quotePreview")
            && error.message.contains("preview_qoute")
            && error.message.contains("preview_quote"),
        "the refusal must name the query, the typo and the real name; got: {}",
        error.message
    );
}

/// A query naming an **event**-triggered function is refused.
///
/// This is the case a presence-only check passes: `notify_approved` is declared,
/// so "does the name resolve?" says yes. It would be invoked with an event
/// payload it will never receive, and would answer no read.
#[test]
fn a_query_naming_an_event_triggered_function_is_refused() {
    let functions = vec![FunctionDefinition::new(
        "notify_approved",
        "after:mutation:Order:update",
        RuntimeType::Deno,
    )];
    let bindings = [QueryFunctionBinding {
        query:    "quotePreview",
        function: "notify_approved",
    }];
    let error = TriggerRegistry::validate_query_bindings(&bindings, &functions)
        .expect_err("an event-triggered function may not back a query");
    assert!(
        error.message.contains("after:mutation:Order:update")
            && error.message.contains("request:query"),
        "the refusal must name the trigger it found and the one it needs; got: {}",
        error.message
    );
}

/// A `request:query` function no query names is refused — #871's rule, applied to
/// the kind that fails most quietly.
#[test]
fn a_request_function_no_query_names_is_refused() {
    let error = TriggerRegistry::validate_query_bindings(&[], &preview_quote())
        .expect_err("a function nothing can invoke must be refused");
    assert!(
        error.message.contains("preview_quote") && error.message.contains("function ="),
        "the refusal must name the function and the declaration that would bind it; got: {}",
        error.message
    );
}

/// Both directions fail at once, and one message reports **all three**.
///
/// The count is asserted, not merely the names: rule 1's own diagnostic lists the
/// declared request functions, so `preview_quote` and `score_lead` appear in the
/// message whether or not rule 3 ran. A names-only assertion here passes with the
/// unreferenced-function rule deleted — which is the whole reason this test exists.
#[test]
fn every_failing_pair_is_reported_in_one_message() {
    let functions = vec![
        FunctionDefinition::new("preview_quote", "request:query", RuntimeType::Deno),
        FunctionDefinition::new("score_lead", "request:query", RuntimeType::Deno),
    ];
    let bindings = [QueryFunctionBinding {
        query:    "quotePreview",
        function: "missing",
    }];
    let error = TriggerRegistry::validate_query_bindings(&bindings, &functions)
        .expect_err("three failures, one message");
    assert!(
        error.message.starts_with("3 function-backed query binding(s)"),
        "one unbound query plus two uninvokable functions is three failures; got: {}",
        error.message
    );
    assert!(
        error.message.contains("quotePreview")
            && error.message.contains("preview_quote")
            && error.message.contains("score_lead"),
        "one compile must name the whole list; got: {}",
        error.message
    );
}

/// A function serving two queries is not a failure.
///
/// The binding is one-way — the query names the function — so nothing stops two
/// queries from naming one aggregator, and nothing should: the alternative is a
/// rule that exists only to be worked around by copying a module.
#[test]
fn one_function_may_back_two_queries() {
    let bindings = [
        QueryFunctionBinding {
            query:    "quotePreview",
            function: "preview_quote",
        },
        QueryFunctionBinding {
            query:    "bulkQuotePreview",
            function: "preview_quote",
        },
    ];
    TriggerRegistry::validate_query_bindings(&bindings, &preview_quote())
        .expect("two queries may share one function");
}

/// Every dispatch setting is refused on a request-serving function, by name.
///
/// One case per setting, each built from the declaration it must reject, and each
/// asserting the refusal names **that** setting. A shared fixture declaring all four
/// would still be refused with any one arm deleted, because the other three fire.
///
/// `run_as` is the one that matters most: it is an authority ceiling, and a security
/// setting accepted and never applied reads as a granted authority.
#[test]
fn a_dispatch_setting_on_a_request_query_function_is_refused_by_name() {
    fn base() -> FunctionDefinition {
        FunctionDefinition::new("preview_quote", "request:query", RuntimeType::Deno)
    }

    let mut with_run_as = base();
    with_run_as.run_as = Some(crate::RunAs::default());

    let mut with_when = base();
    with_when.when = vec![crate::triggers::mutation::TriggerPredicate {
        field:      "status".to_string(),
        eq:         Some(serde_json::json!("approved")),
        changed_to: None,
    }];

    let mut with_re_runnable = base();
    with_re_runnable.re_runnable = true;

    let mut with_retry = base();
    with_retry.retry = Some(fraiseql_observers::RetryConfig::default());

    for (setting, definition) in [
        ("run_as", with_run_as),
        ("when", with_when),
        ("re_runnable", with_re_runnable),
        ("retry", with_retry),
    ] {
        let error = TriggerRegistry::validate_definitions(std::slice::from_ref(&definition))
            .expect_err(&format!("`{setting}` must be refused on a request-serving function"));
        assert!(
            error.message.contains(setting) && error.message.contains("preview_quote"),
            "the refusal must name the setting and the function; got: {}",
            error.message
        );
    }
}

/// A request-serving function declaring none of them loads — the negative direction,
/// which is what shows the refusals are about those four keys and not about the
/// trigger kind.
#[test]
fn a_plain_request_query_function_loads() {
    TriggerRegistry::validate_definitions(&[FunctionDefinition::new(
        "preview_quote",
        "request:query",
        RuntimeType::Deno,
    )])
    .expect("a request-serving function with no dispatch settings must load");
}

/// A `timeout_ms` **is** allowed: it is the author's statement about their own
/// function's latency, which is the one dispatch-adjacent number that means
/// something on the read path.
#[test]
fn a_timeout_on_a_request_query_function_is_allowed() {
    let mut definition =
        FunctionDefinition::new("preview_quote", "request:query", RuntimeType::Deno);
    definition.timeout_ms = Some(30_000);
    TriggerRegistry::validate_definitions(&[definition])
        .expect("a declared timeout is the author's own ceiling, not a dispatch setting");
}
