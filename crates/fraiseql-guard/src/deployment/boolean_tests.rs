//! The one boolean grammar for environment variables read outside clap: clap's
//! `BoolishValueParser`, which the server's flags use. The server pins the two together
//! under its `cli` feature.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::{boolish, env_bool};

const TRUE: [&str; 8] = ["1", "true", "TRUE", "yes", "y", "on", " t ", "On"];
const FALSE: [&str; 8] = ["0", "false", "False", "no", "n", "off", " f ", "OFF"];
const NEITHER: [&str; 5] = ["", "  ", "enabled", "2", "truee"];

#[test]
fn each_true_spelling_is_true() {
    for raw in TRUE {
        assert_eq!(boolish(raw), Some(true), "{raw:?}");
    }
}

#[test]
fn each_false_spelling_is_false() {
    for raw in FALSE {
        assert_eq!(boolish(raw), Some(false), "{raw:?}");
    }
}

#[test]
fn anything_else_is_neither() {
    for raw in NEITHER {
        assert_eq!(boolish(raw), None, "{raw:?}");
    }
}

#[test]
fn an_unset_variable_takes_the_default() {
    assert!(env_bool("FRAISEQL_X", None, true).unwrap());
    assert!(!env_bool("FRAISEQL_X", None, false).unwrap());
}

#[test]
fn a_set_variable_wins_over_the_default() {
    assert!(!env_bool("FRAISEQL_X", Some("off"), true).unwrap());
    assert!(env_bool("FRAISEQL_X", Some("yes"), false).unwrap());
}

#[test]
fn a_value_that_is_neither_is_refused_naming_the_variable() {
    for raw in NEITHER {
        let refusal = env_bool("FRAISEQL_X", Some(raw), true).unwrap_err();
        assert!(refusal.contains("FRAISEQL_X"), "{raw:?}: {refusal}");
    }
}
