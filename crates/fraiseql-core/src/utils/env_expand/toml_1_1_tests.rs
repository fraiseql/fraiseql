//! The configuration files are parsed as TOML 1.1, which accepts multi-line and
//! trailing-comma inline tables and `\e`/`\xHH` escapes. A substituted value can produce
//! none of them: the characters each needs are refused, wherever the placeholder sits.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{UnsetVariable, expand_with};

fn expanded(file: &str, value: &str) -> Result<String, String> {
    expand_with(file, UnsetVariable::KeepLiteral, |_| Some(value.to_owned()))
}

#[test]
fn a_value_cannot_open_or_close_an_inline_table() {
    for value in ["{", "}", "1 }", "{ a = 1 }"] {
        assert!(expanded("t = ${V}", value).is_err(), "bare {value:?} was substituted");
        assert!(expanded("t = { a = ${V} }", value).is_err(), "inline {value:?} was substituted");
        assert!(expanded("t = \"${V}\"", value).is_err(), "quoted {value:?} was substituted");
    }
}

#[test]
fn a_value_cannot_carry_an_inline_table_across_lines() {
    for value in ["1\n", "\n}", "1\r\n"] {
        assert!(expanded("t = { a = ${V} }", value).is_err(), "{value:?} was substituted");
        assert!(expanded("t = \"${V}\"", value).is_err(), "quoted {value:?} was substituted");
    }
}

#[test]
fn a_value_cannot_add_a_trailing_comma_outside_a_string() {
    assert!(expanded("t = { a = ${V} }", "1,").is_err());
    assert_eq!(expanded("t = { a = \"${V}\" }", "1,").unwrap(), "t = { a = \"1,\" }");
}

#[test]
fn a_value_cannot_write_an_escape() {
    for value in ["\\e", "\\x41", "\\u0041"] {
        assert!(expanded("t = \"${V}\"", value).is_err(), "{value:?} was substituted");
    }
}
