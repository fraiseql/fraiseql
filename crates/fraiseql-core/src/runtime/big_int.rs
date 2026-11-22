//! The value space of a `BigInt`: a signed 64-bit integer, which travels as the string of
//! its decimal digits.
//!
//! JSON numbers past 2^53 are not read exactly by a `JavaScript` client, so a `BigInt` is
//! served as a string on every transport. As an input it is that string or a JSON integer
//! in range, which a client holding the value as a native 64-bit integer writes.

use serde_json::Value;

/// Whether `text` writes an integer in its canonical form: `42`, `-7`, `0`. No sign but a
/// leading `-`, no leading zero, no `-0`, nothing around the digits.
#[must_use]
pub fn is_canonical_integer(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && text != "-0"
}

/// The `i64` a `BigInt`'s string holds, if `text` writes one in its canonical form and
/// within range. Any other text would be read as a value its client never wrote.
#[must_use]
pub fn parse_big_int(text: &str) -> Option<i64> {
    if is_canonical_integer(text) {
        text.parse().ok()
    } else {
        None
    }
}

/// The `i64` an input `value` supplies for a `BigInt`: its canonical string, or a JSON
/// integer in range.
///
/// A number written with a fraction or an exponent was read as an `f64` before anything
/// could see its digits, so it is not one.
#[must_use]
pub fn big_int_input(value: &Value) -> Option<i64> {
    match value {
        Value::String(text) => parse_big_int(text),
        Value::Number(number) => number.as_i64(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "big_int_tests.rs"]
mod tests;
