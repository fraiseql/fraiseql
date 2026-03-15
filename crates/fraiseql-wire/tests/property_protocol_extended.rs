#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::cast_possible_wrap)] // Reason: test data casts
#![allow(clippy::items_after_statements)] // Reason: test helper closures defined near use site
#![allow(clippy::cast_precision_loss)] // Reason: test metrics use usize→f64 for reporting
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are small and bounded

//! Extended property-based tests for wire protocol correctness.
//!
//! Builds on `property_protocol.rs` with additional properties covering
//! message framing, encoding consistency, and edge cases.

use bytes::{BufMut, BytesMut};
use fraiseql_wire::protocol::decode::decode_message;
use proptest::prelude::*;

// ============================================================================
// Message Framing Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Property: Messages with length < 4 are rejected (length includes itself).
    #[test]
    fn prop_reject_invalid_length_field(
        tag in prop_oneof![Just(b'Z'), Just(b'C'), Just(b'S'), Just(b'K')],
        bad_len in 0i32..4,
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(tag);
        buf.put_i32(bad_len);
        let result = decode_message(&mut buf);
        prop_assert!(result.is_err(), "Length < 4 should be rejected");
    }

    /// Property: Messages with negative length are rejected.
    #[test]
    fn prop_reject_negative_length(
        tag in prop_oneof![Just(b'Z'), Just(b'C'), Just(b'S')],
        neg_len in i32::MIN..-1,
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(tag);
        buf.put_i32(neg_len);
        // Provide some body bytes in case the decoder tries to read
        buf.extend_from_slice(&[0u8; 16]);
        let result = decode_message(&mut buf);
        prop_assert!(result.is_err(), "Negative length should be rejected");
    }

    /// Property: Exact-length messages decode and consume exactly tag(1) + length(4) + body bytes.
    #[test]
    fn prop_ready_for_query_consumes_exact_bytes(status in any::<u8>()) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'Z');
        buf.put_i32(5); // 4 + 1 byte body
        buf.put_u8(status);
        // Add trailing garbage that should NOT be consumed
        buf.extend_from_slice(b"GARBAGE");

        let result = decode_message(&mut buf);
        let (_, consumed) = result.map_err(|e| TestCaseError::fail(format!("expected Ok for ReadyForQuery exact-bytes: {e}")))?;
        prop_assert_eq!(consumed, 6, "ReadyForQuery should consume exactly 6 bytes");
    }

    /// Property: Truncated message body returns EOF.
    #[test]
    fn prop_truncated_body_returns_eof(
        tag in prop_oneof![Just(b'Z'), Just(b'K'), Just(b'S')],
        declared_body in 10u32..100,
        actual_body in 0u32..9,
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(tag);
        buf.put_i32((declared_body + 4) as i32);
        // Provide fewer bytes than declared
        for _ in 0..actual_body {
            buf.put_u8(0);
        }
        let result = decode_message(&mut buf);
        prop_assert!(result.is_err(), "Truncated body should return error");
    }

    /// Property: Multiple concatenated messages can be decoded sequentially.
    #[test]
    fn prop_sequential_decode(
        status1 in any::<u8>(),
        status2 in any::<u8>(),
    ) {
        let mut buf = BytesMut::new();
        // First ReadyForQuery
        buf.put_u8(b'Z');
        buf.put_i32(5);
        buf.put_u8(status1);
        // Second ReadyForQuery
        buf.put_u8(b'Z');
        buf.put_i32(5);
        buf.put_u8(status2);

        let (msg1, consumed1) = decode_message(&mut buf).unwrap();
        let remaining = buf.split_off(consumed1);
        let mut buf2 = remaining;
        let (msg2, _consumed2) = decode_message(&mut buf2).unwrap();

        match msg1 {
            fraiseql_wire::protocol::message::BackendMessage::ReadyForQuery { status } => {
                prop_assert_eq!(status, status1);
            }
            _ => prop_assert!(false, "Expected ReadyForQuery"),
        }
        match msg2 {
            fraiseql_wire::protocol::message::BackendMessage::ReadyForQuery { status } => {
                prop_assert_eq!(status, status2);
            }
            _ => prop_assert!(false, "Expected ReadyForQuery"),
        }
    }
}

// ============================================================================
// CommandComplete Encoding Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Property: CommandComplete with various SQL tags roundtrips correctly.
    #[test]
    fn prop_command_complete_sql_tags(
        command in prop_oneof![
            Just("SELECT"),
            Just("INSERT"),
            Just("UPDATE"),
            Just("DELETE"),
            Just("CREATE TABLE"),
            Just("DROP TABLE"),
            Just("BEGIN"),
            Just("COMMIT"),
            Just("ROLLBACK"),
        ],
        row_count in 0u32..10000,
    ) {
        let tag = format!("{} {}", command, row_count);
        let mut buf = BytesMut::new();
        buf.put_u8(b'C');
        let body_len = tag.len() + 1;
        buf.put_i32((body_len + 4) as i32);
        buf.extend_from_slice(tag.as_bytes());
        buf.put_u8(0);

        let (msg, _) = decode_message(&mut buf).unwrap();
        match msg {
            fraiseql_wire::protocol::message::BackendMessage::CommandComplete(t) => {
                prop_assert_eq!(t, tag);
            }
            _ => prop_assert!(false, "Expected CommandComplete"),
        }
    }

    /// Property: CommandComplete handles empty tag (null terminator only).
    #[test]
    fn prop_command_complete_empty_tag(_dummy in 0..1u8) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'C');
        buf.put_i32(5); // 4 + 1 null terminator
        buf.put_u8(0);

        let result = decode_message(&mut buf);
        let decoded = result.map_err(|e| TestCaseError::fail(format!("expected Ok for empty CommandComplete tag: {e}")))?;
        match decoded.0 {
            fraiseql_wire::protocol::message::BackendMessage::CommandComplete(t) => {
                prop_assert!(t.is_empty(), "Empty tag should decode as empty string");
            }
            _ => prop_assert!(false, "Expected CommandComplete"),
        }
    }
}

// ============================================================================
// BackendKeyData Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Property: BackendKeyData with extreme values roundtrips correctly.
    #[test]
    fn prop_backend_key_data_extremes(
        pid in prop_oneof![
            Just(0i32), Just(1), Just(-1), Just(i32::MAX), Just(i32::MIN),
            any::<i32>(),
        ],
        key in prop_oneof![
            Just(0i32), Just(1), Just(-1), Just(i32::MAX), Just(i32::MIN),
            any::<i32>(),
        ],
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'K');
        buf.put_i32(12);
        buf.put_i32(pid);
        buf.put_i32(key);

        let (msg, consumed) = decode_message(&mut buf).unwrap();
        prop_assert_eq!(consumed, 13); // 1 tag + 4 len + 4 pid + 4 key

        match msg {
            fraiseql_wire::protocol::message::BackendMessage::BackendKeyData {
                process_id, secret_key,
            } => {
                prop_assert_eq!(process_id, pid);
                prop_assert_eq!(secret_key, key);
            }
            _ => prop_assert!(false, "Expected BackendKeyData"),
        }
    }
}

// ============================================================================
// ParameterStatus Edge Cases
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Property: ParameterStatus with special characters in value roundtrips.
    #[test]
    fn prop_parameter_status_special_chars(
        name in "[a-z_]{1,20}",
        value in "[a-zA-Z0-9 =,;:/.+-]{1,50}",
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'S');
        let body_len = name.len() + 1 + value.len() + 1;
        buf.put_i32((body_len + 4) as i32);
        buf.extend_from_slice(name.as_bytes());
        buf.put_u8(0);
        buf.extend_from_slice(value.as_bytes());
        buf.put_u8(0);

        let (msg, _) = decode_message(&mut buf).unwrap();
        match msg {
            fraiseql_wire::protocol::message::BackendMessage::ParameterStatus {
                name: n, value: v,
            } => {
                prop_assert_eq!(n, name);
                prop_assert_eq!(v, value);
            }
            _ => prop_assert!(false, "Expected ParameterStatus"),
        }
    }

    /// Property: ParameterStatus with common PostgreSQL parameters roundtrips.
    #[test]
    fn prop_parameter_status_common_params(
        name in prop_oneof![
            Just("server_version"),
            Just("server_encoding"),
            Just("client_encoding"),
            Just("DateStyle"),
            Just("TimeZone"),
            Just("integer_datetimes"),
        ],
        value in "[a-zA-Z0-9./ -]{1,30}",
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'S');
        let body_len = name.len() + 1 + value.len() + 1;
        buf.put_i32((body_len + 4) as i32);
        buf.extend_from_slice(name.as_bytes());
        buf.put_u8(0);
        buf.extend_from_slice(value.as_bytes());
        buf.put_u8(0);

        let (msg, _) = decode_message(&mut buf).unwrap();
        match msg {
            fraiseql_wire::protocol::message::BackendMessage::ParameterStatus {
                name: n, value: v,
            } => {
                prop_assert_eq!(n, name);
                prop_assert_eq!(v, value);
            }
            _ => prop_assert!(false, "Expected ParameterStatus"),
        }
    }
}

// ============================================================================
// Error Message Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: Error messages with valid structure decode without panic.
    #[test]
    fn prop_error_message_valid_structure(
        severity in prop_oneof![Just("ERROR"), Just("FATAL"), Just("WARNING")],
        code in "[0-9A-Z]{5}",
        message in "[a-zA-Z0-9 .,!?:;-]{1,100}",
    ) {
        let mut body = BytesMut::new();
        // Severity field
        body.put_u8(b'S');
        body.extend_from_slice(severity.as_bytes());
        body.put_u8(0);
        // Code field
        body.put_u8(b'C');
        body.extend_from_slice(code.as_bytes());
        body.put_u8(0);
        // Message field
        body.put_u8(b'M');
        body.extend_from_slice(message.as_bytes());
        body.put_u8(0);
        // Terminator
        body.put_u8(0);

        let mut buf = BytesMut::new();
        buf.put_u8(b'E');
        buf.put_i32((body.len() + 4) as i32);
        buf.extend_from_slice(&body);

        let result = decode_message(&mut buf);
        // Should not panic — may succeed or return an error depending on parser
        let _ = result;
    }
}
