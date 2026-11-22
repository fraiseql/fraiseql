use serde_json::json;

use super::*;

#[test]
fn the_canonical_digits_of_an_i64_are_a_big_int() {
    for (text, expected) in [
        ("0", 0),
        ("-7", -7),
        ("9007199254740993", 9_007_199_254_740_993),
        ("9223372036854775807", i64::MAX),
        ("-9223372036854775808", i64::MIN),
    ] {
        assert_eq!(parse_big_int(text), Some(expected), "{text}");
    }
}

#[test]
fn other_text_is_not_a_big_int() {
    for text in [
        "",
        "-",
        "-0",
        "007",
        "+1",
        " 1",
        "1 ",
        "1.0",
        "1e3",
        "9223372036854775808",
        "-9223372036854775809",
        "0x10",
    ] {
        assert_eq!(parse_big_int(text), None, "{text:?}");
    }
}

#[test]
fn an_input_is_the_string_or_an_integer_in_range() {
    assert_eq!(big_int_input(&json!("9007199254740993")), Some(9_007_199_254_740_993));
    assert_eq!(big_int_input(&json!(9_007_199_254_740_993_i64)), Some(9_007_199_254_740_993));
    assert_eq!(big_int_input(&json!(u64::MAX)), None);
    assert_eq!(big_int_input(&json!(3.0)), None);
    assert_eq!(big_int_input(&json!(true)), None);
    assert_eq!(big_int_input(&json!("1.5")), None);
}
