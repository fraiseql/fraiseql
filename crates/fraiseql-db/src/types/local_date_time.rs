//! The value space of a `LocalDateTime`: a date and a time of day in no time zone.
//!
//! It is written `YYYY-MM-DDTHH:MM:SS`, a `T` or `t` between the two, optional fractional
//! seconds of one to nine digits, and never an offset
//! (<https://scalars.graphql.org/chillicream/local-date-time>). An operand is cast
//! `::timestamp`, and PostgreSQL's cast drops an offset or a `Z` without a word, so a value
//! carrying one is refused here rather than compared as a different time.

use fraiseql_error::{FraiseQLError, Result};

/// Why `text` is not a `LocalDateTime`, or `None` when it is one.
///
/// The reason is a phrase that completes "the value …", and never quotes the value.
#[must_use]
pub fn local_date_time_refusal(text: &str) -> Option<&'static str> {
    const NOT_ONE: &str = "is not one";
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        b.get(range).is_some_and(|d| d.iter().all(u8::is_ascii_digit))
    };
    let shaped = digits(0..4)
        && b.get(4) == Some(&b'-')
        && digits(5..7)
        && b.get(7) == Some(&b'-')
        && digits(8..10)
        && matches!(b.get(10), Some(b'T' | b't'))
        && digits(11..13)
        && b.get(13) == Some(&b':')
        && digits(14..16)
        && b.get(16) == Some(&b':')
        && digits(17..19);
    if !shaped {
        return Some(NOT_ONE);
    }
    let mut end = 19;
    if b.get(end) == Some(&b'.') {
        let fraction = b[end + 1..].iter().take_while(|d| d.is_ascii_digit()).count();
        if !(1..=9).contains(&fraction) {
            return Some(NOT_ONE);
        }
        end += 1 + fraction;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().unwrap_or(u32::MAX);
    let real = i32::try_from(number(0..4))
        .ok()
        .and_then(|year| chrono::NaiveDate::from_ymd_opt(year, number(5..7), number(8..10)))
        .is_some()
        && chrono::NaiveTime::from_hms_opt(number(11..13), number(14..16), number(17..19))
            .is_some();
    let tail = &b[end..];
    let offset = matches!(tail, [b'Z' | b'z'])
        || matches!(tail, [b'+' | b'-', h1, h2, b':', m1, m2]
            if [h1, h2, m1, m2].iter().all(|d| d.is_ascii_digit()));
    if !real {
        Some(NOT_ONE)
    } else if tail.is_empty() {
        None
    } else if offset {
        Some("carries a time-zone offset, which a LocalDateTime never has")
    } else {
        Some(NOT_ONE)
    }
}

/// Refuse an operand compared against a `LocalDateTime` that is not one, naming `name`.
///
/// `null` passes (a null test is not a comparison), and a list is checked item by item, as
/// an `in` operand is.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] when the operand, or an item of it, is not a string
/// holding a local date-time.
pub fn check_local_date_time_operand(name: &str, value: &serde_json::Value) -> Result<()> {
    let refusal = match value {
        serde_json::Value::Null => None,
        serde_json::Value::Array(items) => {
            return items.iter().try_for_each(|item| check_local_date_time_operand(name, item));
        },
        serde_json::Value::String(text) => local_date_time_refusal(text),
        _ => Some("is not one"),
    };
    refusal.map_or(Ok(()), |why| {
        Err(FraiseQLError::validation(format!(
            "`{name}` takes a LocalDateTime, a date and time of day in no time zone \
             (YYYY-MM-DDTHH:MM:SS, optional fractional seconds): the value {why}"
        )))
    })
}

#[cfg(test)]
mod tests;
