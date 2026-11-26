#![allow(clippy::unwrap_used)] // Reason: test code

use super::{
    Expanded, NotUtf8, UnsetVariable, expand_looked_up, expand_tracked, expand_with,
    without_quoted_spans,
};

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
    for value in ["1#x", "1,x", "1=x", "1 x"] {
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

/// The parse error over `file` expanded with `V` set to `value`.
fn parse_error(file: &str, value: &str) -> String {
    let expanded = expand_tracked(file, UnsetVariable::KeepLiteral, lookup(value)).unwrap();
    let error = toml::from_str::<toml::Table>(&expanded.text).unwrap_err();
    expanded.parse_error(&error, "Bad file")
}

/// With nothing substituted the whole error is kept, the line's text included; an unset
/// placeholder kept as written is nothing substituted.
#[test]
fn a_parse_error_with_nothing_substituted_is_kept_whole() {
    let none = |_: &str| None;
    let expanded =
        expand_tracked("a = 1\nb = plainword", UnsetVariable::KeepLiteral, none).unwrap();
    let error = toml::from_str::<toml::Table>(&expanded.text).unwrap_err();
    let message = expanded.parse_error(&error, "Bad file");
    assert!(message.starts_with("Bad file: "), "{message}");
    assert!(message.contains("b = plainword"), "{message}");

    let kept =
        expand_tracked("a = \"${W}\"\nb = plainword", UnsetVariable::KeepLiteral, none).unwrap();
    let error = toml::from_str::<toml::Table>(&kept.text).unwrap_err();
    assert!(kept.parse_error(&error, "Bad file").contains("b = plainword"));
}

/// With a value substituted the error names the line and the reason, not the line's text.
#[test]
fn a_parse_error_after_substitution_names_the_line_not_its_text() {
    let message = parse_error("a = 1\n\nb = ${V}", "secretSENTINEL");
    assert!(message.starts_with("Bad file at line 3: "), "{message}");
    assert!(!message.contains("secretSENTINEL") && !message.contains("b ="), "{message}");
}

/// A reason that quotes a substituted value is replaced by one that does not.
#[test]
fn a_reason_quoting_the_value_is_replaced() {
    #[derive(Debug, serde::Deserialize)]
    struct Typed {
        #[allow(dead_code)] // Reason: read only to make the parse typed
        n: u32,
    }
    let expanded =
        expand_tracked("n = \"${V}\"", UnsetVariable::KeepLiteral, lookup("secretSENTINEL"))
            .unwrap();
    let error = toml::from_str::<Typed>(&expanded.text).unwrap_err();
    assert!(error.message().contains("secretSENTINEL"), "the reason quotes it: {error}");
    let message = expanded.parse_error(&error, "Bad file");
    assert!(message.starts_with("Bad file at line 1: "), "{message}");
    assert!(!message.contains("secretSENTINEL"), "{message}");
}

/// A typed parse quotes a string value in its escaped form (a tab as `\t`, a zero-width
/// space as `\u{200b}`), which no substring check over the raw value finds. The expander
/// refuses such values, so the `Expanded` here is built by hand: it is what the stripper
/// stands behind, should a value the parser renders differently ever be substituted.
#[test]
fn a_value_quoted_in_an_escaped_form_is_not_shown() {
    #[derive(Debug, serde::Deserialize)]
    struct Typed {
        #[allow(dead_code)] // Reason: read only to make the parse typed
        n: u32,
    }
    for value in [
        "s3cretPART\tSENT",
        "s3cretPART\u{200b}SENT",
        "s3cretPART\u{a0}SENT",
    ] {
        let expanded = Expanded {
            text:   format!("n = \"{value}\""),
            values: vec![value.to_owned()],
        };
        let error = toml::from_str::<Typed>(&expanded.text).unwrap_err();
        assert!(
            error.message().contains("s3cretPART"),
            "{value:?}: the reason quotes it: {error}"
        );
        assert!(!error.message().contains(value), "{value:?}: quoted in another form: {error}");
        let message = expanded.parse_error(&error, "Bad file");
        assert!(!message.contains("s3cretPART"), "{value:?}: {message}");
        assert!(message.contains("invalid type: string <value>, expected u32"), "{message}");
    }
}

/// A value a reason quotes in backticks is not shown either.
#[test]
fn a_value_quoted_in_backticks_is_not_shown() {
    #[derive(Debug, serde::Deserialize)]
    struct Typed {
        #[allow(dead_code)] // Reason: read only to make the parse typed
        mode: Mode,
    }
    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Mode {
        Strict,
    }
    let expanded =
        expand_tracked("mode = \"${V}\"", UnsetVariable::KeepLiteral, lookup("s3cretPART"))
            .unwrap();
    let error = toml::from_str::<Typed>(&expanded.text).unwrap_err();
    assert!(error.message().contains("`s3cretPART`"), "the reason quotes it: {error}");
    let message = expanded.parse_error(&error, "Bad file");
    assert!(!message.contains("s3cretPART"), "{message}");
    assert!(message.contains("unknown variant <value>"), "{message}");
}

/// Every span opened by `"` or a backtick becomes `<value>`; an escaped quote does not close a
/// double-quoted span, and a span never closed is removed to the end.
#[test]
fn every_quoted_span_is_replaced() {
    for (reason, expected) in [
        ("plain reason", "plain reason"),
        ("string \"a b\", expected u32", "string <value>, expected u32"),
        (
            "variant `a`, expected `b` or `c`",
            "variant <value>, expected <value> or <value>",
        ),
        ("string \"a\\\"b\" here", "string <value> here"),
        ("string \"a\\\\\" here", "string <value> here"),
        ("mixed \"a`b\" then `c\"d`", "mixed <value> then <value>"),
        ("path `C:\\` and `d`", "path <value> and <value>"),
        ("open \"never closed", "open <value>"),
        ("open `never closed", "open <value>"),
        ("", ""),
    ] {
        assert_eq!(without_quoted_spans(reason), expected, "{reason:?}");
    }
}

/// One character of each class a value may not hold, wherever its placeholder sits, and the
/// characters either side of each class's edges.
#[test]
fn a_value_that_is_not_printable_text_is_refused_by_class() {
    for (value, class) in [
        ("a\u{1}b", "control"),
        ("a\tb", "control"),
        ("a\u{1f}b", "control"),
        ("a\u{7f}b", "control"),
        ("a\u{80}b", "control"),
        ("a\u{9f}b", "control"),
        ("a\u{ad}b", "format"),
        ("a\u{200b}b", "format"),
        ("a\u{200e}b", "format"),
        ("a\u{200f}b", "format"),
        ("a\u{2060}b", "format"),
        ("a\u{feff}b", "format"),
        ("a\u{a0}b", "separator"),
        ("a\u{2007}b", "separator"),
        ("a\u{200a}b", "separator"),
        ("a\u{202f}b", "separator"),
        ("a\u{3000}b", "separator"),
        ("a\u{2028}b", "separator"),
        ("a\u{2029}b", "separator"),
    ] {
        for file in ["k = \"${V}\"", "k = '${V}'", "# ${V}"] {
            let err = expand_with(file, UnsetVariable::KeepLiteral, lookup(value)).unwrap_err();
            assert!(err.contains("Environment variable V holds"), "{value:?} {file}: {err}");
            assert!(err.contains(class), "{value:?} {file}: {err}");
            assert!(!err.contains(value), "{value:?}: {err}");
        }
    }
}

/// Printable text is not refused: the plain space, accented letters, other scripts, a
/// combining mark, symbols, and the printable characters beside each refused range.
#[test]
fn printable_text_is_kept() {
    for value in [
        "a b", "café", "über", "東京", "e\u{301}", "😀", "~", "\u{a1}", "\u{ae}", "\u{2010}",
        "\u{2030}", "\u{3001}",
    ] {
        let out = expand_with("k = \"${V}\"", UnsetVariable::KeepLiteral, lookup(value)).unwrap();
        assert_eq!(out, format!("k = \"{value}\""), "{value:?}");
    }
}

/// A value that is not UTF-8 is refused naming the variable, set or not under either mode.
#[test]
fn a_value_that_is_not_utf8_is_refused() {
    for unset in [UnsetVariable::KeepLiteral, UnsetVariable::Refuse] {
        let err = expand_looked_up("k = \"${V}\"", unset, |_| Some(Err(NotUtf8))).unwrap_err();
        assert!(err.contains("Environment variable V is not UTF-8"), "{err}");
    }
}

/// Bare, a tab is refused as not printable and a plain space as changing the TOML read there:
/// the two refusals stay distinct.
#[test]
fn the_printable_and_bare_value_refusals_are_distinct() {
    let tab = expand_with("k = ${V}", UnsetVariable::KeepLiteral, lookup("1\t2")).unwrap_err();
    assert!(tab.contains("control character") && !tab.contains("outside a quoted"), "{tab}");
    let space = expand_with("k = ${V}", UnsetVariable::KeepLiteral, lookup("1 2")).unwrap_err();
    assert!(space.contains("outside a quoted") && !space.contains("printable"), "{space}");
}

/// A reason that carries a value bare, outside any quote (as a `regex::Error` prints the
/// pattern it failed on), is replaced whole.
#[test]
fn a_reason_carrying_the_value_unquoted_is_replaced() {
    fn bare_echo<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Err(serde::de::Error::custom(format!("pattern failed: {value}")))
    }
    #[derive(Debug, serde::Deserialize)]
    struct Typed {
        #[allow(dead_code)] // Reason: read only to make the parse fail
        #[serde(deserialize_with = "bare_echo")]
        p: String,
    }
    let expanded =
        expand_tracked("p = \"${V}\"", UnsetVariable::KeepLiteral, lookup("s3cretPART")).unwrap();
    let error = toml::from_str::<Typed>(&expanded.text).unwrap_err();
    assert!(error.message().contains("pattern failed: s3cretPART"), "{error}");
    let message = expanded.parse_error(&error, "Bad file");
    assert!(!message.contains("s3cretPART"), "{message}");
    assert!(
        message.ends_with("a value expanded from an environment variable is not valid there"),
        "{message}"
    );
}
