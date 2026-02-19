//! Wire Protocol Robustness Tests
//!
//! Tests for PostgreSQL wire protocol message decoding:
//! - Malformed message handling (invalid tags, truncation, overflow)
//! - Error field parsing (severity, SQLSTATE, position, hint, detail)
//! - Backend message decoding (ReadyForQuery, CommandComplete, DataRow, etc.)
//! - Edge cases (empty results, large payloads, invalid UTF-8)

use bytes::{BufMut, BytesMut};
use fraiseql_wire::protocol::decode::decode_message;
use fraiseql_wire::protocol::message::BackendMessage;
use std::io;

// ============================================================================
// Malformed Message Handling
// ============================================================================

#[test]
fn test_decode_malformed_message_tag() {
    let mut buf = BytesMut::from(&[b'!', 0, 0, 0, 4][..]);
    let result = decode_message(&mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_decode_truncated_message() {
    // Only 3 bytes when minimum is 5
    let mut buf = BytesMut::from(&[b'Z', 0, 0][..]);
    let result = decode_message(&mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn test_decode_length_field_overflow() {
    // Length field claims ~4GB but only 10 bytes present
    let mut buf = BytesMut::from(&[b'T', 0x7F, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0][..]);
    let result = decode_message(&mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn test_decode_length_too_small() {
    // Length < 4 is invalid (length includes itself but not the tag)
    let mut buf = BytesMut::from(&[b'Z', 0, 0, 0, 3, b'I'][..]);
    let result = decode_message(&mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_decode_zero_length_message() {
    // Length = 0 is invalid
    let mut buf = BytesMut::from(&[b'Z', 0, 0, 0, 0][..]);
    let result = decode_message(&mut buf);
    assert!(result.is_err());
}

#[test]
fn test_decode_empty_buffer() {
    let mut buf = BytesMut::new();
    let result = decode_message(&mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn test_decode_invalid_utf8_in_error_response() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'E');
    // Build error response body: severity + invalid UTF-8 message + terminator
    let body: Vec<u8> = vec![
        b'S', 0xFF, 0xFE, 0x00, // severity with invalid UTF-8 + null
        b'M', b't', b'e', b's', b't', 0x00, // message "test" + null
        0x00, // terminator
    ];
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let result = decode_message(&mut buf);
    // Should succeed with lossy UTF-8 conversion (not panic)
    assert!(result.is_ok());
    if let Ok((BackendMessage::ErrorResponse(fields), _)) = result {
        assert!(fields.severity.is_some());
        assert_eq!(fields.message.as_deref(), Some("test"));
    } else {
        panic!("Expected ErrorResponse");
    }
}

// ============================================================================
// Partial & Incomplete Messages
// ============================================================================

#[test]
fn test_partial_message_buffering() {
    // Header present but body incomplete: ReadyForQuery needs 1 byte of body
    let mut buf = BytesMut::from(&[b'Z', 0, 0, 0, 5][..]); // says 5 bytes but no status byte
    let result = decode_message(&mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);

    // Now append the status byte — should succeed
    buf.put_u8(b'I');
    let result = decode_message(&mut buf);
    assert!(result.is_ok());
}

#[test]
fn test_large_data_row() {
    // 1MB DataRow with 1 field containing 1MB of data
    let field_size: usize = 1_000_000;
    let mut buf = BytesMut::new();
    buf.put_u8(b'D'); // DataRow tag

    // Body: 2 bytes field count + 4 bytes field length + field data
    let body_len = 2 + 4 + field_size;
    buf.put_i32((body_len + 4) as i32); // length includes self
    buf.put_i16(1); // 1 field
    buf.put_i32(field_size as i32); // field length
    buf.extend_from_slice(&vec![b'x'; field_size]); // field data

    let result = decode_message(&mut buf);
    assert!(result.is_ok());
    if let Ok((BackendMessage::DataRow(fields), consumed)) = result {
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].as_ref().unwrap().len(), field_size);
        assert_eq!(consumed, 1 + 4 + body_len);
    } else {
        panic!("Expected DataRow");
    }
}

// ============================================================================
// Error Response Field Parsing
// ============================================================================

#[test]
fn test_error_sqlstate_parsing() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'E');
    let body: Vec<u8> = vec![
        b'S', b'E', b'R', b'R', b'O', b'R', 0, // severity "ERROR"
        b'C', b'2', b'3', b'5', b'0', b'5', 0, // SQLSTATE "23505"
        b'M', b'u', b'n', b'i', b'q', b'u', b'e', 0, // message "unique"
        0, // terminator
    ];
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::ErrorResponse(fields) = msg {
        assert_eq!(fields.code.as_deref(), Some("23505"));
        assert_eq!(fields.severity.as_deref(), Some("ERROR"));
        assert_eq!(fields.message.as_deref(), Some("unique"));
    } else {
        panic!("Expected ErrorResponse");
    }
}

#[test]
fn test_error_position_marker() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'E');
    let body: Vec<u8> = vec![
        b'M', b'e', b'r', b'r', 0, // message "err"
        b'P', b'9', 0, // position "9"
        0, // terminator
    ];
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::ErrorResponse(fields) = msg {
        assert_eq!(fields.position.as_deref(), Some("9"));
    } else {
        panic!("Expected ErrorResponse");
    }
}

#[test]
fn test_error_hint_field() {
    let hint_text = b"Did you mean FROM?";
    let mut buf = BytesMut::new();
    buf.put_u8(b'E');
    let mut body = vec![b'M', b'e', 0]; // message "e"
    body.push(b'H'); // hint field
    body.extend_from_slice(hint_text);
    body.push(0); // null terminator
    body.push(0); // terminator
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::ErrorResponse(fields) = msg {
        assert_eq!(fields.hint.as_deref(), Some("Did you mean FROM?"));
    } else {
        panic!("Expected ErrorResponse");
    }
}

#[test]
fn test_error_detail_field() {
    let detail_text = b"Key (id)=(5) already exists.";
    let mut buf = BytesMut::new();
    buf.put_u8(b'E');
    let mut body = vec![b'M', b'e', 0]; // message "e"
    body.push(b'D'); // detail field
    body.extend_from_slice(detail_text);
    body.push(0); // null terminator
    body.push(0); // terminator
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::ErrorResponse(fields) = msg {
        assert_eq!(
            fields.detail.as_deref(),
            Some("Key (id)=(5) already exists.")
        );
    } else {
        panic!("Expected ErrorResponse");
    }
}

#[test]
fn test_multi_field_error_response() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'E');
    let body: Vec<u8> = vec![
        b'S', b'E', b'R', b'R', b'O', b'R', 0, // severity
        b'C', b'4', b'2', b'P', b'0', b'1', 0, // SQLSTATE
        b'M', b'r', b'e', b'l', b'a', b't', b'i', b'o', b'n', 0, // message
        b'P', b'1', b'5', 0, // position
        b'D', b'd', b'e', b't', 0, // detail
        b'H', b'h', b'n', b't', 0, // hint
        0, // terminator
    ];
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::ErrorResponse(fields) = msg {
        assert_eq!(fields.severity.as_deref(), Some("ERROR"));
        assert_eq!(fields.code.as_deref(), Some("42P01"));
        assert_eq!(fields.message.as_deref(), Some("relation"));
        assert_eq!(fields.position.as_deref(), Some("15"));
        assert_eq!(fields.detail.as_deref(), Some("det"));
        assert_eq!(fields.hint.as_deref(), Some("hnt"));
    } else {
        panic!("Expected ErrorResponse");
    }
}

// ============================================================================
// Backend Message Decoding
// ============================================================================

#[test]
fn test_notice_response_handling() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'N'); // Notice (not Error)
    let body: Vec<u8> = vec![
        b'S', b'W', b'A', b'R', b'N', b'I', b'N', b'G', 0, // severity "WARNING"
        b'M', b'c', b'o', b'l', 0, // message "col"
        0, // terminator
    ];
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::NoticeResponse(fields) = msg {
        assert_eq!(fields.severity.as_deref(), Some("WARNING"));
        assert_eq!(fields.message.as_deref(), Some("col"));
    } else {
        panic!("Expected NoticeResponse, not ErrorResponse");
    }
}

#[test]
fn test_parameter_status_updates() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'S'); // ParameterStatus
    let body: Vec<u8> = vec![
        b'c', b'l', b'i', b'e', b'n', b't', b'_', b'e', b'n', b'c', b'o', b'd', b'i', b'n', b'g',
        0, // name "client_encoding"
        b'U', b'T', b'F', b'8', 0, // value "UTF8"
    ];
    let len = (body.len() + 4) as i32;
    buf.put_i32(len);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::ParameterStatus { name, value } = msg {
        assert_eq!(name, "client_encoding");
        assert_eq!(value, "UTF8");
    } else {
        panic!("Expected ParameterStatus");
    }
}

#[test]
fn test_backend_key_data_storage() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'K'); // BackendKeyData
    buf.put_i32(12); // length: 4 (self) + 4 (pid) + 4 (key)
    buf.put_i32(12345); // process_id
    buf.put_i32(0x7FFF_ABCD); // secret_key

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::BackendKeyData {
        process_id,
        secret_key,
    } = msg
    {
        assert_eq!(process_id, 12345);
        assert_eq!(secret_key, 0x7FFF_ABCD);
    } else {
        panic!("Expected BackendKeyData");
    }
}

#[test]
fn test_command_complete_parsing() {
    for (tag, expected) in [
        ("SELECT 5", "SELECT 5"),
        ("INSERT 0 1", "INSERT 0 1"),
        ("UPDATE 10", "UPDATE 10"),
        ("DELETE 3", "DELETE 3"),
    ] {
        let mut buf = BytesMut::new();
        buf.put_u8(b'C'); // CommandComplete
        let body_len = tag.len() + 1; // tag + null terminator
        buf.put_i32((body_len + 4) as i32);
        buf.extend_from_slice(tag.as_bytes());
        buf.put_u8(0); // null terminator

        let (msg, _) = decode_message(&mut buf).unwrap();
        if let BackendMessage::CommandComplete(parsed_tag) = msg {
            assert_eq!(parsed_tag, expected);
        } else {
            panic!("Expected CommandComplete for tag '{tag}'");
        }
    }
}

#[test]
fn test_empty_result_set_messages() {
    // Simulate empty result: RowDescription(0 fields) + CommandComplete("SELECT 0") + ReadyForQuery

    // 1. RowDescription with 0 fields
    let mut buf = BytesMut::new();
    buf.put_u8(b'T');
    buf.put_i32(6); // length: 4 (self) + 2 (field count)
    buf.put_i16(0); // 0 fields

    let (msg, _) = decode_message(&mut buf).unwrap();
    assert!(matches!(msg, BackendMessage::RowDescription(ref fields) if fields.is_empty()));

    // 2. CommandComplete "SELECT 0"
    let tag = b"SELECT 0\0";
    buf = BytesMut::new();
    buf.put_u8(b'C');
    buf.put_i32((tag.len() + 4) as i32);
    buf.extend_from_slice(tag);

    let (msg, _) = decode_message(&mut buf).unwrap();
    assert!(matches!(msg, BackendMessage::CommandComplete(ref t) if t == "SELECT 0"));

    // 3. ReadyForQuery 'I' (idle)
    buf = BytesMut::new();
    buf.put_u8(b'Z');
    buf.put_i32(5);
    buf.put_u8(b'I');

    let (msg, _) = decode_message(&mut buf).unwrap();
    assert!(matches!(
        msg,
        BackendMessage::ReadyForQuery { status: b'I' }
    ));
}

#[test]
fn test_ready_for_query_status() {
    for (status_byte, label) in [(b'I', "idle"), (b'T', "in transaction"), (b'E', "failed")] {
        let mut buf = BytesMut::new();
        buf.put_u8(b'Z');
        buf.put_i32(5);
        buf.put_u8(status_byte);

        let (msg, _) = decode_message(&mut buf).unwrap();
        if let BackendMessage::ReadyForQuery { status } = msg {
            assert_eq!(status, status_byte, "Status mismatch for {label}");
        } else {
            panic!("Expected ReadyForQuery for {label}");
        }
    }
}

#[test]
fn test_data_row_with_null_field() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'D');
    // 2 fields: one NULL (-1), one "hello"
    let mut body = BytesMut::new();
    body.put_i16(2); // 2 fields
    body.put_i32(-1); // NULL field
    body.put_i32(5); // 5 bytes
    body.extend_from_slice(b"hello");

    buf.put_i32((body.len() + 4) as i32);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::DataRow(fields) = msg {
        assert_eq!(fields.len(), 2);
        assert!(fields[0].is_none(), "First field should be NULL");
        assert_eq!(fields[1].as_ref().unwrap().as_ref(), b"hello");
    } else {
        panic!("Expected DataRow");
    }
}

#[test]
fn test_row_description_parsing() {
    let mut buf = BytesMut::new();
    buf.put_u8(b'T');

    let mut body = BytesMut::new();
    body.put_i16(1); // 1 field
                     // Field: name "id" + null + 18 bytes of descriptor
    body.extend_from_slice(b"id\0");
    body.put_i32(0); // table_oid
    body.put_i16(0); // column_attr
    body.put_i32(23); // type_oid (int4 = 23)
    body.put_i16(4); // type_size
    body.put_i32(-1); // type_modifier
    body.put_i16(0); // format_code (text)

    buf.put_i32((body.len() + 4) as i32);
    buf.extend_from_slice(&body);

    let (msg, _) = decode_message(&mut buf).unwrap();
    if let BackendMessage::RowDescription(fields) = msg {
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "id");
        assert_eq!(fields[0].type_oid, 23);
        assert_eq!(fields[0].type_size, 4);
    } else {
        panic!("Expected RowDescription");
    }
}
