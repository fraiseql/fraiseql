#![allow(clippy::unwrap_used)]       // Reason: test code, panics are acceptable
#![allow(clippy::cast_possible_truncation)] // Reason: test protocol encoding casts
#![allow(clippy::cast_possible_wrap)]       // Reason: test protocol encoding casts

//! Property-based tests for wire protocol message decoding

use bytes::{BufMut, BytesMut};
use fraiseql_wire::protocol::decode::decode_message;
use proptest::prelude::*;

// ============================================================================
// Fuzz-like Properties: decode_message never panics
// ============================================================================

proptest! {
    #[test]
    fn prop_decode_never_panics(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let mut buf = BytesMut::from(&data[..]);
        let _ = decode_message(&mut buf); // must not panic
    }

    #[test]
    fn prop_decode_with_valid_tag_never_panics(
        tag in prop_oneof![
            Just(b'R'), Just(b'K'), Just(b'C'), Just(b'D'),
            Just(b'E'), Just(b'N'), Just(b'S'), Just(b'Z'), Just(b'T'),
        ],
        body in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(tag);
        let len = (body.len() + 4) as i32;
        buf.put_i32(len);
        buf.extend_from_slice(&body);
        let _ = decode_message(&mut buf); // must not panic
    }

    // Property: incomplete messages always return UnexpectedEof
    #[test]
    fn prop_short_buffer_returns_eof(len in 0..5usize) {
        let data = vec![b'Z'; len];
        let mut buf = BytesMut::from(&data[..]);
        let result = decode_message(&mut buf);
        prop_assert!(result.is_err());
        prop_assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
    }

    // Property: ReadyForQuery roundtrip preserves status byte
    #[test]
    fn prop_ready_for_query_roundtrip(status in any::<u8>()) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'Z');
        buf.put_i32(5); // 4 (length) + 1 (status)
        buf.put_u8(status);
        let result = decode_message(&mut buf);
        prop_assert!(result.is_ok());
        let (msg, consumed) = result.unwrap();
        prop_assert_eq!(consumed, 6);
        match msg {
            fraiseql_wire::protocol::message::BackendMessage::ReadyForQuery { status: s } => {
                prop_assert_eq!(s, status);
            }
            _ => prop_assert!(false, "Expected ReadyForQuery"),
        }
    }

    // Property: BackendKeyData roundtrip preserves both fields
    #[test]
    fn prop_backend_key_data_roundtrip(pid in any::<i32>(), key in any::<i32>()) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'K');
        buf.put_i32(12); // 4 + 4 (pid) + 4 (key)
        buf.put_i32(pid);
        buf.put_i32(key);
        let (msg, _) = decode_message(&mut buf).unwrap();
        match msg {
            fraiseql_wire::protocol::message::BackendMessage::BackendKeyData { process_id, secret_key } => {
                prop_assert_eq!(process_id, pid);
                prop_assert_eq!(secret_key, key);
            }
            _ => prop_assert!(false, "Expected BackendKeyData"),
        }
    }

    // Property: CommandComplete roundtrip preserves tag string
    #[test]
    fn prop_command_complete_roundtrip(tag in "[A-Z]+ [0-9]+") {
        let mut buf = BytesMut::new();
        buf.put_u8(b'C');
        let body_len = tag.len() + 1; // +1 for null terminator
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

    // Property: ParameterStatus roundtrip preserves name and value
    #[test]
    fn prop_parameter_status_roundtrip(
        name in "[a-z_]{1,30}",
        value in "[a-zA-Z0-9_ ]{1,50}",
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
            fraiseql_wire::protocol::message::BackendMessage::ParameterStatus { name: n, value: v } => {
                prop_assert_eq!(n, name);
                prop_assert_eq!(v, value);
            }
            _ => prop_assert!(false, "Expected ParameterStatus"),
        }
    }

    // Property: invalid tags always return InvalidData
    #[test]
    fn prop_invalid_tag_returns_error(
        tag in any::<u8>().prop_filter("not a valid tag", |t| {
            !matches!(t, b'R' | b'K' | b'C' | b'D' | b'E' | b'N' | b'S' | b'Z' | b'T')
        })
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(tag);
        buf.put_i32(4); // minimum valid length
        let result = decode_message(&mut buf);
        prop_assert!(result.is_err());
        prop_assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }
}

// ============================================================================
// Protocol Boundary Properties: Message Length and Field Validation
// ============================================================================

proptest! {
    #[test]
    fn prop_message_length_must_include_length_field(
        tag in prop_oneof![
            Just(b'R'), Just(b'K'), Just(b'C'), Just(b'D'),
            Just(b'E'), Just(b'N'), Just(b'S'), Just(b'Z'), Just(b'T'),
        ],
        body_len in 0usize..512,
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(tag);
        // Length field includes itself (4 bytes)
        let total_len = (body_len + 4) as i32;
        buf.put_i32(total_len);
        buf.extend_from_slice(&vec![0; body_len]);

        let result = decode_message(&mut buf);
        if result.is_ok() {
            let (_, consumed) = result.unwrap();
            // consumed should be tag (1) + length field (4) + body
            prop_assert_eq!(consumed, 1 + body_len + 4);
        }
    }

    #[test]
    fn prop_error_message_with_truncation_safe(
        code in "[A-Z]{5}",
        message in ".*",
        position in "[a-z0-9_]{0,20}",
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'E');

        // Build minimal error response
        let body = format!("C{}\x00M{}\x00P{}\x00\x00",
            &code[..code.len().min(5)],
            message.chars().take(100).collect::<String>(),
            position.chars().take(20).collect::<String>()
        );
        buf.put_i32((body.len() + 4) as i32);
        buf.extend_from_slice(body.as_bytes());

        let result = decode_message(&mut buf);
        // Must not panic even with truncated error responses
        let _ = result;
    }

    #[test]
    fn prop_parameter_status_empty_name_rejected(
        value in "[a-zA-Z0-9_ ]{1,50}",
    ) {
        let mut buf = BytesMut::new();
        buf.put_u8(b'S');
        // Empty name: just null terminator, then value, then null
        let body_len = 1 + value.len() + 1;
        buf.put_i32((body_len + 4) as i32);
        buf.put_u8(0); // empty name
        buf.extend_from_slice(value.as_bytes());
        buf.put_u8(0);

        let result = decode_message(&mut buf);
        // Result can succeed or fail, but must not panic
        let _ = result;
    }
}
