//! A structured log entry's timestamp: RFC 3339 in UTC with `Z`, to the millisecond, always
//! three fraction digits.

use super::{LogLevel, StructuredLogEntry};

#[test]
fn a_log_entrys_timestamp_is_utc_with_z_and_three_fraction_digits() {
    let entry = StructuredLogEntry::new(LogLevel::Info, "started".to_owned());
    let shape: String = entry
        .timestamp
        .chars()
        .map(|c| if c.is_ascii_digit() { 'd' } else { c })
        .collect();

    assert_eq!(shape, "dddd-dd-ddTdd:dd:dd.dddZ", "{}", entry.timestamp);
}
