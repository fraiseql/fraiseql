#![allow(clippy::unwrap_used)] // Reason: test code

use super::{UnsetVariable, expand_with};

fn lookup(value: &str) -> impl Fn(&str) -> Option<String> {
    let value = value.to_string();
    move |name| (name == "V").then(|| value.clone())
}

#[test]
fn a_set_variable_is_substituted() {
    let out =
        expand_with("url = \"${V}/app\"", UnsetVariable::Refuse, lookup("postgres://db")).unwrap();
    assert_eq!(out, "url = \"postgres://db/app\"");
}

#[test]
fn an_unset_variable_is_kept_or_refused_as_asked() {
    let none = |_: &str| None;
    assert_eq!(
        expand_with("a = \"${W}\"", UnsetVariable::KeepLiteral, none).unwrap(),
        "a = \"${W}\""
    );
    let err = expand_with("a = \"${W}\"", UnsetVariable::Refuse, none).unwrap_err();
    assert!(err.contains('W') && err.contains("not set"), "{err}");
}

/// Each character that could end the TOML string the value sits in, or start a new line, is
/// refused on its own.
#[test]
fn every_unsafe_character_is_refused_alone() {
    for value in [
        "a\nb", "a\rb", "a\0b", "a\"b", "a'b", "a\\b", "a]b", "a[b", "a{b", "a}b",
    ] {
        let err =
            expand_with("k = \"${V}\"", UnsetVariable::KeepLiteral, lookup(value)).unwrap_err();
        assert!(err.contains('V'), "{value:?}: {err}");
    }
}
