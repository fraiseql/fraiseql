//! `FRAISEQL_CHANGELOG_ENABLED` is a boolean in the grammar the server's flags use. Set,
//! it replaces the compiled `[changelog] write_enabled`; a value that is neither true nor
//! false refuses the config, naming the variable, rather than turning the outbox on.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::{RuntimeConfig, changelog_precedence};
use crate::schema::CompiledSchema;

#[test]
fn unset_takes_the_compiled_value_or_on() {
    assert!(changelog_precedence(None, None).unwrap());
    assert!(changelog_precedence(None, Some(true)).unwrap());
    assert!(!changelog_precedence(None, Some(false)).unwrap());
}

#[test]
fn the_variable_replaces_the_compiled_value_both_ways() {
    assert!(!changelog_precedence(Some("off"), Some(true)).unwrap());
    assert!(changelog_precedence(Some("yes"), Some(false)).unwrap());
}

/// `f` and `n` used to switch the outbox on: only `false`, `0`, `no` and `off` were
/// read as off.
#[test]
fn every_false_spelling_turns_it_off() {
    for raw in ["0", "false", "no", "off", "f", "n", "FALSE"] {
        assert!(!changelog_precedence(Some(raw), None).unwrap(), "{raw:?}");
    }
}

#[test]
fn a_value_that_is_neither_is_refused_naming_the_variable() {
    for raw in ["disabled", "2", ""] {
        let refusal = changelog_precedence(Some(raw), None).unwrap_err();
        assert!(refusal.contains("FRAISEQL_CHANGELOG_ENABLED"), "{raw:?}: {refusal}");
    }
}

fn changelog_switch(value: &'static str) -> impl Fn(&str) -> Option<String> {
    move |key| (key == "FRAISEQL_CHANGELOG_ENABLED").then(|| value.to_owned())
}

#[test]
fn the_runtime_config_refuses_a_switch_that_is_not_a_boolean() {
    let refusal = RuntimeConfig::default()
        .with_compiled_schema_from(&CompiledSchema::new(), changelog_switch("disabled"))
        .unwrap_err();
    assert!(refusal.contains("FRAISEQL_CHANGELOG_ENABLED"), "{refusal}");
}

#[test]
fn the_runtime_config_takes_the_switch_from_the_environment() {
    let config = RuntimeConfig::default()
        .with_compiled_schema_from(&CompiledSchema::new(), changelog_switch("f"))
        .unwrap();
    assert!(!config.changelog_enabled);
}
