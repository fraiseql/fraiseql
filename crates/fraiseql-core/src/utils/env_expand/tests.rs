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

/// Outside a quoted string, each character that would end the line early or add a key is
/// refused on its own, naming the variable and not the value.
#[test]
fn every_unsafe_bare_character_is_refused_alone() {
    for value in ["1#x", "1,x", "1=x", "1 x", "1\tx", "1\u{a0}x"] {
        for file in [
            "k = ${V}",
            "t = { k = ${V} }",
            "a = [${V}]",
            "${V} = 1",
            "k = 1 ${V}",
        ] {
            let err = expand_with(file, UnsetVariable::KeepLiteral, lookup(value)).unwrap_err();
            assert!(
                err.contains('V') && err.contains("outside a quoted string"),
                "{file} {value:?}: {err}"
            );
            assert!(!err.contains(value), "{file} {value:?}: {err}");
        }
    }
}

/// Inside every kind of string, and in a comment, the same values are text.
#[test]
fn a_value_inside_a_string_or_comment_is_text() {
    for file in [
        "k = \"${V}\"",
        "k = 'x ${V}'",
        "k = \"\"\"\nline\n${V}\"\"\"",
        "k = '''\n${V}'''",
        "k = \"a\\\"${V}\"",
        "k = \"\\\\\" # ${V}",
        "# ${V}",
        "k = 1 # ${V}",
        "k = \"é\" # ${V}",
    ] {
        let out = expand_with(file, UnsetVariable::KeepLiteral, lookup("1 # x, y = z"));
        assert!(out.is_ok(), "{file}: {out:?}");
    }
}

/// Context follows the written text across lines and placeholders: a string closed earlier
/// leaves the next placeholder bare, and one left open by a backslash-escaped quote does not.
#[test]
fn the_context_of_each_placeholder_follows_the_text_before_it() {
    let bare_after_string = "a = \"${V}\"\nb = ${V}";
    assert!(expand_with(bare_after_string, UnsetVariable::KeepLiteral, lookup("1 x")).is_err());
    for same_line in ["t = { a = \"x\", b = ${V} }", "t = { a = 'x', b = ${V} }"] {
        let out = expand_with(same_line, UnsetVariable::KeepLiteral, lookup("1 x"));
        assert!(out.is_err(), "{same_line}: a string closed on the line leaves it bare");
    }
    let bare_after_comment = "# a ${V}\nb = ${V}";
    assert!(expand_with(bare_after_comment, UnsetVariable::KeepLiteral, lookup("1 x")).is_err());
    let after_multiline = "a = \"\"\"\n\"\" ${V}\n\"\"\"\nb = ${V}";
    assert!(expand_with(after_multiline, UnsetVariable::KeepLiteral, lookup("1 x")).is_err());
    let after_multiline_literal = "a = '''\nx\n'''\nb = ${V}";
    assert!(
        expand_with(after_multiline_literal, UnsetVariable::KeepLiteral, lookup("1 x")).is_err()
    );
    let escaped_quote = "a = \"x\\\" ${V}\"";
    assert!(expand_with(escaped_quote, UnsetVariable::KeepLiteral, lookup("1 x")).is_ok());
    let plain = "a = ${V}\nb = \"${V}\"";
    assert_eq!(
        expand_with(plain, UnsetVariable::KeepLiteral, lookup("12")).unwrap(),
        "a = 12\nb = \"12\""
    );
}
