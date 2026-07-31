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
        error.message.contains("avatarUpload") && error.message.contains("not"),
        "the error names the function and says the surface is unavailable, got: {}",
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
