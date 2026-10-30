#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::Value;

use super::{JsonTally, RawRows, compact_json};

/// Texts as PostgreSQL writes them: `json` from `JSON_OBJECT` spaces its separators,
/// `jsonb` spaces after them, and a `->` value inside a `json` object is `jsonb` text.
const PG_TEXTS: &[&str] = &[
    r#"{"id" : "aaaa0000-0000-0000-0000-000000000001", "name" : "Alice"}"#,
    r#"{"name" : "Alice", "profile" : {"tier" : "gold", "address" : {"city" : "Paris"}}}"#,
    r#"{"tags": ["a", "b"], "n": 3, "ok": true, "none": null}"#,
    r#"{"k" : -42, "big" : 123456789012345678, "zero" : 0}"#,
    r#"{"quote" : "a \"b\" \\ c", "lines" : "one\ntwo\tthree\r\b\f"}"#,
    r#"{"space in key" : "a , b : c { } [ ]", "é" : "naïve 漢字"}"#,
    r#"{"empty" : {}, "list" : [], "nested" : [[1, 2], [], [{"a" : null}]]}"#,
    r"{}",
    r"[]",
    r#""just a string""#,
    r"7",
];

/// What a parsed value holds, counted the way the scan counts the text.
fn tally_of(value: &Value, tally: &mut JsonTally) {
    match value {
        Value::Null | Value::Bool(_) => tally.literals += 1,
        Value::Number(_) => tally.numbers += 1,
        Value::String(s) => {
            tally.strings += 1;
            tally.string_bytes += s.len() as u64;
        },
        Value::Array(items) => {
            tally.arrays += 1;
            tally.elements += items.len() as u64;
            for item in items {
                tally_of(item, tally);
            }
        },
        Value::Object(members) => {
            tally.objects += 1;
            tally.members += members.len() as u64;
            for (key, item) in members {
                tally.key_bytes += key.len() as u64;
                tally_of(item, tally);
            }
        },
    }
}

#[test]
fn an_exact_text_compacts_to_what_serde_json_writes() {
    for text in PG_TEXTS {
        let mut out = Vec::new();
        let found = compact_json(text.as_bytes(), &mut out);
        assert!(found.exact, "{text} should be exact");
        let value: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            serde_json::to_string(&value).unwrap(),
            "{text}"
        );
    }
}

#[test]
fn the_tally_counts_what_the_parsed_value_holds() {
    for text in PG_TEXTS {
        let mut out = Vec::new();
        let found = compact_json(text.as_bytes(), &mut out);
        let mut expected = JsonTally::default();
        tally_of(&serde_json::from_str(text).unwrap(), &mut expected);
        assert_eq!(found.tally, expected, "{text}");
    }
}

#[test]
fn a_null_is_reported_only_where_there_is_one() {
    let null = |text: &str| compact_json(text.as_bytes(), &mut Vec::new()).null;
    assert!(null(r#"{"a" : null}"#));
    assert!(null(r"[1, null]"));
    assert!(!null(r#"{"a" : "null", "n" : 1, "t" : true, "f" : false}"#));
}

/// Numbers as `jsonb` and `json` hold them, each of which `serde_json` writes back with
/// other digits: through an `f64` unless it is an integer that fits `i64` or `u64`.
const REWRITTEN_NUMBERS: &[&str] = &[
    r#"{"price" : 1.50}"#,
    r#"{"f" : 1.0, "g" : 100.00}"#,
    r#"{"e" : 1e2, "E" : 2E-3, "p" : 1e+3, "big" : 1.5e300}"#,
    r#"{"z" : -0, "zf" : -0.0, "zero" : 0.0}"#,
    r#"{"wide" : 1234567890123456789, "u64" : 18446744073709551615}"#,
    r#"{"wider" : 18446744073709551616, "neg" : -9223372036854775809}"#,
    r#"{"decimal" : 12345678901234567890.12, "cents" : 123456789012345678.99}"#,
    r#"{"tiny" : 0.000000000000000000001, "third" : 0.3333333333333333333333}"#,
    r"[0.5, -2.25, 19.99, 1e-7]",
    r"3.25",
    r"-0",
];

#[test]
fn a_number_is_written_as_serde_json_writes_it() {
    for text in REWRITTEN_NUMBERS {
        let mut out = Vec::new();
        let found = compact_json(text.as_bytes(), &mut out);
        let value: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            serde_json::to_string(&value).unwrap(),
            "{text}"
        );
        assert!(found.exact, "{text} should be exact");
        let mut expected = JsonTally::default();
        tally_of(&value, &mut expected);
        assert_eq!(found.tally, expected, "{text}");
    }
}

/// Each would serialize differently after a round trip through `serde_json`, or would not
/// parse at all.
#[test]
fn a_value_serde_json_would_rewrite_is_not_exact() {
    for text in [
        r#"{"esc" : "\u00e9"}"#,
        r#"{"ctl" : "\u001f"}"#,
        // Past `f64`: `serde_json` refuses it, so the `Value` path must be the one to.
        r#"{"huge" : 1e400}"#,
        r"[-1e400]",
    ] {
        let found = compact_json(text.as_bytes(), &mut Vec::new());
        assert!(!found.exact, "{text} should not be exact");
    }
}

#[test]
fn rows_are_joined_by_commas_and_each_can_be_read_back() {
    let mut rows = RawRows::new();
    rows.push(br#"{"a" : 1}"#);
    rows.push(br#"{"b" : [1, 2]}"#);
    rows.push(br#"{"c" : null}"#);
    assert_eq!(rows.bytes(), br#"{"a":1},{"b":[1,2]},{"c":null}"#);
    assert_eq!(rows.row(0).unwrap(), br#"{"a":1}"#);
    assert_eq!(rows.row(1).unwrap(), br#"{"b":[1,2]}"#);
    assert_eq!(rows.row(2).unwrap(), br#"{"c":null}"#);
    assert_eq!(rows.row(3), None);
    assert!(rows.exact());
    assert!(rows.has_null());
    assert_eq!(
        rows.parse().unwrap(),
        vec![
            serde_json::json!({"a": 1}),
            serde_json::json!({"b": [1, 2]}),
            serde_json::json!({"c": null}),
        ]
    );
}

#[test]
fn one_inexact_row_makes_the_rows_inexact() {
    let mut rows = RawRows::new();
    rows.push(br#"{"a" : 1}"#);
    assert!(rows.exact());
    rows.push(br#"{"a" : 1e400}"#);
    assert!(!rows.exact());
}

/// The key each `null` is a member's value under, and a `null` no key names.
#[test]
fn a_null_is_reported_under_the_key_it_is_the_value_of() {
    let nulls = |texts: &[&str]| {
        let mut rows = RawRows::new();
        for text in texts {
            rows.push(text.as_bytes());
        }
        let keys: Vec<String> = ["a", "b", "c", "d", "null"]
            .into_iter()
            .filter(|key| rows.has_null_where(|at| at == Some(key.as_bytes())))
            .map(str::to_string)
            .collect();
        (keys, rows.has_null_where(|at| at.is_none()))
    };
    let keys = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();

    assert_eq!(nulls(&[r#"{"a" : null, "b" : {"c" : null}}"#]), (keys(&["a", "c"]), false));
    assert_eq!(nulls(&[r#"{"a" : {"b" : 1}, "c" : null}"#]), (keys(&["c"]), false));
    assert_eq!(nulls(&[r#"{"a" : 1}"#, r#"{"d" : null}"#]), (keys(&["d"]), false));
    assert_eq!(nulls(&[r#"{"a" : "null", "null" : 1}"#]), (keys(&[]), false));
    assert_eq!(nulls(&[r#"{"a" : [1, null]}"#]), (keys(&[]), true), "an element");
    assert_eq!(nulls(&["null"]), (keys(&[]), true), "the text itself");
    assert_eq!(nulls(&[r#"{"a" : [{"b" : null}]}"#]), (keys(&["b"]), false));
    assert_eq!(nulls(&[r#"{"a\"b" : null}"#]), (keys(&[]), true), "a key with a quote");
    assert_eq!(nulls(&[r#"{"x" : 1, "a,\"b" : null}"#]), (keys(&[]), true), "and a comma");
    assert_eq!(nulls(&[r#"{"a\\" : null}"#]), (keys(&[]), false), "an escaped backslash");
}
