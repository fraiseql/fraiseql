//! `FRAISEQL_SOURCES_ENABLED` is a boolean in the grammar the server's flags use: `f` and
//! `n` turn the scheduler off too, and a value that is neither true nor false refuses
//! boot, naming the variable, rather than turning the scheduler on.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::sources_enabled_from;
use crate::server_config::SourcesConfig;

fn set(value: &'static str) -> impl Fn(&str) -> Option<String> {
    move |key| (key == "FRAISEQL_SOURCES_ENABLED").then(|| value.to_owned())
}

#[test]
fn every_false_spelling_turns_the_scheduler_off() {
    for raw in ["0", "false", "no", "off", "f", "n", "FALSE"] {
        assert!(!sources_enabled_from(&SourcesConfig::default(), set(raw)).unwrap(), "{raw:?}");
    }
}

#[test]
fn a_value_that_is_neither_is_refused_naming_the_variable() {
    for raw in ["disabled", "2", ""] {
        let refusal = sources_enabled_from(&SourcesConfig::default(), set(raw)).unwrap_err();
        assert!(refusal.contains("FRAISEQL_SOURCES_ENABLED"), "{raw:?}: {refusal}");
    }
}
