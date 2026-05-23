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
fn test_registry_loads_multiple_triggers() {
    use crate::{FunctionDefinition, RuntimeType};

    let functions = vec![
        FunctionDefinition::new("onUserCreated", "after:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("validateInput", "before:mutation:createUser", RuntimeType::Deno),
        FunctionDefinition::new("getUser", "http:GET:/users/:id", RuntimeType::Deno),
    ];

    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load registry");

    assert_eq!(registry.function_count, 3);
    assert_eq!(registry.before_mutation_count(), 1);
    assert_eq!(registry.http_route_count(), 1);
}

#[test]
fn test_registry_finds_http_route() {
    use crate::{FunctionDefinition, RuntimeType};

    let functions = vec![
        FunctionDefinition::new("getUser", "http:GET:/users/:id", RuntimeType::Deno),
        FunctionDefinition::new("listUsers", "http:GET:/users", RuntimeType::Deno),
    ];

    let registry = TriggerRegistry::load_from_definitions(&functions).expect("load registry");

    let route = registry.http_routes.find("GET", "/users/123");
    assert!(route.is_some());
    assert_eq!(route.expect("route found").function_name, "getUser");
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
