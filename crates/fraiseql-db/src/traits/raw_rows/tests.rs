#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::Value;

use super::{JsonTally, RawRows, ScalarKind, compact_json, tag};

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

/// The keys of `a`, `b` and `c` a value `kind` does not hold sits under. Each is asked of
/// rows copied without keys and of rows copied knowing the key asked, which must agree.
fn wrong_kinds(text: &str, kind: ScalarKind) -> Vec<&'static str> {
    let mut untold = RawRows::new();
    untold.push(text.as_bytes());
    ["a", "b", "c"]
        .into_iter()
        .filter(|key| {
            let keys = [(key.as_bytes(), kind)];
            let mut told = RawRows::with_scalar_keys(&keys);
            told.push(text.as_bytes());
            let answer = untold.has_wrong_kind_under(&keys);
            assert_eq!(told.has_wrong_kind_under(&keys), answer, "{text} under {key} as {kind:?}");
            answer
        })
        .collect()
}

/// The keys a number an `Int` does not hold as written sits under: one outside the signed
/// 32-bit range, or written with a fraction, an exponent or as `-0`; and a value of any
/// other kind. A value in an array is no scalar position; one under a key holding an escape
/// may be under any.
#[test]
fn a_value_no_int_holds_as_written_is_found_under_its_key() {
    let int = |text: &str| wrong_kinds(text, ScalarKind::Int);
    for n in [
        "0",
        "7",
        "-7",
        "999999999",
        "2147483647",
        "-2147483648",
        "1000000000",
    ] {
        assert!(int(&format!(r#"{{"a" : {n}}}"#)).is_empty(), "{n}");
    }
    for not_int in [
        "2147483648",
        "-2147483649",
        "9999999999",
        "30000000000",
        "1.5",
        "3.0",
        "1e3",
        "-0",
        r#""3000000000""#,
        r#""7""#,
        "true",
        "{}",
        "[1]",
    ] {
        assert_eq!(int(&format!(r#"{{"a" : {not_int}}}"#)), vec!["a"], "{not_int}");
    }
    assert!(int(r#"{"a" : null}"#).is_empty(), "a null is the non-null rule's");
    assert_eq!(int(r#"{"a" : 1, "b" : {"c" : 1.50}}"#), vec!["b", "c"], "nested, rewritten");
    assert_eq!(int(r#"{"a" : [{"b" : 3000000000}], "c" : 2}"#), vec!["a", "b"]);
    assert_eq!(int(r#"{"a" : [{"b" : 2}]}"#), vec!["a"], "only the list under `a` itself");
    assert!(int("2.5").is_empty(), "the text itself is no position");
    assert_eq!(int(r#"{"a\"b" : 1.5}"#), vec!["a"], "a key with an escape, starting `a`");
    assert_eq!(int(r#"{"b" : "x\"y", "a" : 1.5}"#), vec!["a", "b"], "an escaped value");
}

/// Each scalar is held to its own kind: a `Float` to a number, a `String` to a string, a
/// `Boolean` to `true` or `false`.
#[test]
fn each_scalar_finds_a_value_of_another_kind_under_its_key() {
    let row = r#"{"a" : 1.5, "b" : "x", "c" : true}"#;
    assert_eq!(wrong_kinds(row, ScalarKind::Float), vec!["b", "c"]);
    assert_eq!(wrong_kinds(row, ScalarKind::String), vec!["a", "c"]);
    assert_eq!(wrong_kinds(row, ScalarKind::Boolean), vec!["a", "b"]);
    assert_eq!(wrong_kinds(r#"{"a" : 3000000000}"#, ScalarKind::Float), Vec::<&str>::new());
    assert_eq!(wrong_kinds(r#"{"a" : false}"#, ScalarKind::Boolean), Vec::<&str>::new());
    for container in ["{}", "[]", r#"{"a" : 1}"#] {
        for kind in [ScalarKind::Float, ScalarKind::String, ScalarKind::Boolean] {
            assert_eq!(
                wrong_kinds(&format!(r#"{{"a" : {container}}}"#), kind),
                vec!["a"],
                "{container} {kind:?}"
            );
        }
    }
    assert_eq!(
        wrong_kinds(r#"{"c" : [{"b" : 1}, 1.5]}"#, ScalarKind::Int),
        vec!["c"],
        "an element after an object is not under that object's last key"
    );
    let mut rows = RawRows::new();
    rows.push(br#"{"a" : 1}"#);
    assert!(!rows.has_wrong_kind_under(&[]), "a read selecting no scalar");
    assert!(
        rows.has_wrong_kind_under(&[(b"a", ScalarKind::Int), (b"a", ScalarKind::String)]),
        "a key selected as two scalars is held to each"
    );
}

/// A temporal scalar holds a string in its format and refuses anything else: a string
/// outside it, one holding an escape (never read in place), a value of another kind. The
/// string is read only under its own key; a key sharing its tag, or another kind's key, is
/// not held to the format.
#[test]
fn a_temporal_scalar_holds_only_a_string_in_its_format() {
    for (kind, inside, outside) in [
        (
            ScalarKind::DateTime,
            [
                "2026-07-01T12:00:00Z",
                "2026-07-01t12:00:00.123456789+02:00",
            ],
            [
                "2026-07-01T12:00:00",
                "1890-01-01T00:00:00+00:09:21",
                "2026-07-01T12:00:00-00:00",
            ],
        ),
        (
            ScalarKind::LocalDateTime,
            ["2026-07-01T12:00:00", "2026-07-01T12:00:00.5"],
            ["2026-07-01T12:00:00Z", "2026-07-01", "2026-02-30T00:00:00"],
        ),
        (
            ScalarKind::Date,
            ["2026-07-01", "2024-02-29"],
            ["2025-02-29", "now", "2026-07-01T00:00:00"],
        ),
        (
            ScalarKind::Time,
            ["09:30:00", "23:59:59.25"],
            ["24:00:00", "09:30", "09:30:00Z"],
        ),
    ] {
        for text in inside {
            assert!(
                wrong_kinds(&format!(r#"{{"a" : "{text}"}}"#), kind).is_empty(),
                "{kind:?} {text}"
            );
        }
        for text in outside {
            assert_eq!(
                wrong_kinds(&format!(r#"{{"a" : "{text}"}}"#), kind),
                vec!["a"],
                "{kind:?} {text}"
            );
        }
        for other in ["7", "true", "{}", r#"["2026-07-01"]"#] {
            assert_eq!(
                wrong_kinds(&format!(r#"{{"a" : {other}}}"#), kind),
                vec!["a"],
                "{kind:?} {other}"
            );
        }
        assert!(wrong_kinds(r#"{"a" : null}"#, kind).is_empty(), "a null is the non-null rule's");
        assert_eq!(
            wrong_kinds(r#"{"a" : "2026\u002d07-01"}"#, kind),
            vec!["a"],
            "{kind:?}: an escape is never read in place"
        );
        assert_eq!(
            wrong_kinds(r#"{"\u0061" : "x"}"#, kind),
            vec!["a", "b", "c"],
            "{kind:?}: a key holding an escape may be any"
        );
        assert_eq!(
            wrong_kinds(r#"{"b" : "x", "c" : {"d" : "y"}}"#, kind),
            vec!["b", "c"],
            "{kind:?}: each key's own value is read, and `a`, holding none, holds no refusal"
        );
    }
    let mut rows =
        RawRows::with_scalar_keys(&[(b"a", ScalarKind::String), (b"b", ScalarKind::Date)]);
    rows.push(br#"{"a" : "not a date", "b" : "2026-07-01"}"#);
    assert!(
        !rows.has_wrong_kind_under(&[(b"a", ScalarKind::String), (b"b", ScalarKind::Date)]),
        "a String's key is not held to a Date's format"
    );
}

/// A key holding an escape may decode to a selected key: `"\u0061"` is `a`. Compaction
/// keeps the escape, so the key cannot be matched: a value counts when a selected key its
/// kind breaks may be what the key decodes to, any when the key opens with the escape, one
/// starting with the same byte otherwise. Completion reads the decoded name.
#[test]
fn a_key_holding_an_escape_may_be_any_selected_key() {
    assert_eq!(wrong_kinds(r#"{"\u0061" : "x"}"#, ScalarKind::Int), vec!["a", "b", "c"]);
    assert_eq!(wrong_kinds(r#"{"\u0061" : 1}"#, ScalarKind::Int), Vec::<&str>::new(), "an Int");
    assert_eq!(wrong_kinds(r#"{"b\u0061" : "x"}"#, ScalarKind::Int), vec!["b"]);
    assert_eq!(wrong_kinds(r#"{"b\u0061" : true}"#, ScalarKind::Boolean), Vec::<&str>::new());
    let text = br#"{"\u0061" : "x"}"#;
    let mut untold = RawRows::new();
    untold.push(text);
    let mut told = RawRows::with_scalar_keys(&[]);
    told.push(text);
    assert!(!untold.has_wrong_kind_under(&[]), "a read selecting no scalar");
    assert!(!told.has_wrong_kind_under(&[]), "a read naming no scalar key");
}

/// Keys sharing a tag are told apart by their bytes: a value under one is held to the
/// other's scalar only when it is that key, and a value found refused stays so whatever
/// follows it.
#[test]
fn keys_sharing_a_tag_are_told_apart_by_their_bytes() {
    let names: Vec<String> = (0..2000).map(|i| format!("k{i}")).collect();
    let (one, other) = names
        .iter()
        .enumerate()
        .find_map(|(i, one)| {
            names[i + 1..]
                .iter()
                .find(|other| tag(one.as_bytes()) == tag(other.as_bytes()))
                .map(|other| (one, other))
        })
        .expect("2000 keys over 256 tags share one");
    let keys = [(one.as_bytes(), ScalarKind::Int)];
    for (text, answer) in [
        (format!(r#"{{"{other}" : "y"}}"#), false),
        (format!(r#"{{"{one}" : "x", "{other}" : "y"}}"#), true),
        (format!(r#"{{"{other}" : "y", "{one}" : 7}}"#), false),
    ] {
        let mut told = RawRows::with_scalar_keys(&keys);
        told.push(text.as_bytes());
        let mut untold = RawRows::new();
        untold.push(text.as_bytes());
        assert_eq!(told.has_wrong_kind_under(&keys), answer, "{text}");
        assert_eq!(untold.has_wrong_kind_under(&keys), answer, "{text}, read");
    }
}

/// Rows copied knowing some keys answer for those keys as they were copied; asked of other
/// keys, they are read, and answer as rows copied without keys do.
#[test]
fn rows_copied_for_some_scalar_keys_answer_others_by_reading() {
    use ScalarKind::{Int, String};
    let text = br#"{"a" : 1.5, "b" : 1, "c" : {"d" : 3000000000}, "e" : "x"}"#;
    let mut told = RawRows::with_scalar_keys(&[(b"b", Int), (b"e", String)]);
    told.push(text);
    assert!(!told.has_wrong_kind_under(&[(b"b", Int), (b"e", String)]));
    assert!(told.has_wrong_kind_under(&[(b"a", Int)]));
    assert!(told.has_wrong_kind_under(&[(b"d", Int)]));
    assert!(told.has_wrong_kind_under(&[(b"e", Int)]), "the same key as another kind");
    assert!(told.has_wrong_kind_under(&[(b"c", String)]), "an object is no string");

    // A key sharing a selected key's first byte is not that key.
    let mut told = RawRows::with_scalar_keys(&[(b"a", Int)]);
    told.push(br#"{"ab" : "x", "a" : 2}"#);
    assert!(!told.has_wrong_kind_under(&[(b"a", Int)]));
}

/// Asked of keys other than the ones they were copied for (a subset, a superset, the same
/// keys in another order, another kind, none), rows answer by reading, as rows copied
/// without keys do.
#[test]
fn rows_asked_of_other_scalar_keys_answer_by_reading() {
    use ScalarKind::{Float, Int, String};
    let text = br#"{"a" : 1.5, "b" : 1, "c" : "x"}"#;
    let mut untold = RawRows::new();
    untold.push(text);
    let (a, b, c): (&[u8], &[u8], &[u8]) = (b"a", b"b", b"c");
    for (copied_for, asked, answer) in [
        (&[(a, Int), (b, Int)][..], &[(b, Int)][..], false),
        (&[(a, Int), (b, Int)], &[], false),
        (&[(a, Int), (b, Int)], &[(b, Int), (a, Int)], true),
        (&[(a, Int), (b, Int)], &[(a, Int), (b, Int), (c, String)], true),
        (&[(b, Int)], &[(b, Int), (c, Int)], true),
        (&[(a, Float)], &[(a, Int)], true),
        (&[(a, Int)], &[(a, Float)], false),
    ] {
        let mut told = RawRows::with_scalar_keys(copied_for);
        told.push(text);
        assert_eq!(told.has_wrong_kind_under(asked), answer, "{copied_for:?}, asked {asked:?}");
        assert_eq!(untold.has_wrong_kind_under(asked), answer, "untold, asked {asked:?}");
    }
}

/// The rows answer from what copying them counted whether they hold, below a row, a value
/// some scalar refuses: the row itself is no value, and every kind below it is counted,
/// whether a member's or an element's, whatever its key.
#[test]
fn the_kinds_held_below_a_row_answer_without_reading_it() {
    use ScalarKind::{Boolean, Float, Int, String};
    let held = |texts: &[&str], scalars: &[ScalarKind]| {
        let mut rows = RawRows::new();
        for text in texts {
            rows.push(text.as_bytes());
        }
        rows.may_hold_refused_by(scalars)
    };
    assert!(
        !held(&[r#"{"a" : "x", "b" : null}"#, r#"{"c" : "y"}"#], &[String]),
        "strings and nulls"
    );
    assert!(!held(&[r#"["x", "y"]"#], &[String]), "a row that is an array");
    assert!(held(&[r#"{"a" : "x"}"#, r#"{"z" : 1}"#], &[String]), "a number, in a later row");
    assert!(held(&[r#"{"a" : true}"#], &[String]), "a Boolean");
    assert!(held(&[r#"{"a" : {"b" : "x"}}"#], &[String]), "an object below the row");
    assert!(held(&[r#"{"a" : ["x"]}"#], &[String]), "an array below the row");
    assert!(!held(&[r#"{"a" : 1, "b" : 2.5}"#], &[Float]), "numbers, under a Float");
    assert!(held(&[r#"{"a" : 2.5}"#], &[Int]), "a fraction, which an Int refuses");
    assert!(!held(&[r#"{"a" : false}"#], &[Boolean]), "a Boolean, under a Boolean");
    assert!(held(&[r#"{"a" : "x"}"#], &[Boolean]), "a string, which a Boolean refuses");
    assert!(held(&[r#"{"a" : true}"#], &[Float, String]), "refused by one of them");
    assert!(!held(&[r#"{"a" : 7}"#], &[]), "no scalar");
}
