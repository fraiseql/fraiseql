#![allow(clippy::unwrap_used)] // Reason: test code

use super::{UnaccentFunction, check_fold_safe_pattern};

#[test]
fn a_schema_qualified_lower_case_name_is_accepted_and_quoted_on_use() {
    let function = UnaccentFunction::parse("search.fold_v2").unwrap();
    assert_eq!(function.call("$1"), "\"search\".\"fold_v2\"($1)");
    assert_eq!(function.to_string(), "search.fold_v2");
}

#[test]
fn the_default_is_fraiseql_unaccent() {
    assert_eq!(
        UnaccentFunction::default(),
        UnaccentFunction::parse("fraiseql.unaccent").unwrap()
    );
}

#[test]
fn a_name_that_is_not_two_lower_case_identifiers_is_refused() {
    for name in [
        "unaccent",
        "public.unaccent.x",
        "Public.unaccent",
        "fraiseql.Unaccent",
        "fraiseql.1unaccent",
        "fraiseql.",
        ".unaccent",
        "fraiseql.un accent",
        "fraiseql.unaccent\"; select 1; --",
        "fraiseql.unaccent()",
    ] {
        let err = UnaccentFunction::parse(name).unwrap_err();
        assert!(err.to_string().contains("schema-qualified lower-case"), "{name}: {err}");
    }
    let long = format!("fraiseql.{}", "a".repeat(64));
    assert!(UnaccentFunction::parse(&long).is_err());
}

#[test]
fn a_name_read_back_from_json_is_held_to_the_same_shape() {
    let read: UnaccentFunction = serde_json::from_str("\"fraiseql.unaccent\"").unwrap();
    assert_eq!(read, UnaccentFunction::default());
    assert!(serde_json::from_str::<UnaccentFunction>("\"x\\\"; drop\"").is_err());
}

#[test]
fn a_pattern_of_ascii_accented_letters_and_combining_marks_is_fold_safe() {
    for pattern in [
        "^cr[éè]me",
        "(a|b)+c",
        "Straße",
        "ÆSIR",
        "e\u{301}cole",
        "日本",
    ] {
        check_fold_safe_pattern(pattern).unwrap();
    }
}

#[test]
fn a_pattern_holding_a_symbol_the_fold_turns_into_regex_syntax_is_refused() {
    for (pattern, shown) in [
        ("（a＋）＋", "（"),
        ("a＊", "＊"),
        ("⑴", "⑴"),
        ("x\u{2c6}", "\u{2c6}"),
        ("［a］", "［"),
    ] {
        let err = check_fold_safe_pattern(pattern).unwrap_err();
        assert!(err.to_string().contains(&format!("`{shown}`")), "{pattern}: {err}");
    }
}
