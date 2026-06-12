#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_decode_authentication_ok() {
    let mut data = BytesMut::from(
        &[
            b'R', // Authentication
            0, 0, 0, 8, // Length = 8
            0, 0, 0, 0, // Auth OK
        ][..],
    );

    let (msg, consumed) = decode_message(&mut data).unwrap();
    match msg {
        BackendMessage::Authentication(AuthenticationMessage::Ok) => {}
        _ => panic!("expected Authentication::Ok"),
    }
    assert_eq!(consumed, 9); // 1 tag + 4 len + 4 auth type
}

#[test]
fn test_decode_ready_for_query() {
    let mut data = BytesMut::from(
        &[
            b'Z', // ReadyForQuery
            0, 0, 0, 5,    // Length = 5
            b'I', // Idle
        ][..],
    );

    let (msg, consumed) = decode_message(&mut data).unwrap();
    match msg {
        BackendMessage::ReadyForQuery { status } => assert_eq!(status, b'I'),
        _ => panic!("expected ReadyForQuery"),
    }
    assert_eq!(consumed, 6); // 1 tag + 4 len + 1 status
}

// ── Field-count guard tests ────────────────────────────────────────────────

fn make_data_row_with_count(count: i16) -> BytesMut {
    // DataRow: tag 'D', length (4 bytes), field_count (2 bytes), then `count` null fields.
    // Each null field is represented by length -1 (i32: 0xFF FF FF FF).
    let body_len: u32 = 2 + 4 * u32::from(count.unsigned_abs());
    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"D");
    buf.extend_from_slice(&(body_len + 4).to_be_bytes()); // length includes itself
    buf.extend_from_slice(&count.to_be_bytes());
    for _ in 0..count {
        buf.extend_from_slice(&(-1i32).to_be_bytes()); // NULL field
    }
    buf
}

fn make_row_description_with_count(count: i16) -> BytesMut {
    // RowDescription: tag 'T', length, field_count, then `count` minimal field descriptors.
    // Each descriptor: name (1 null byte) + 18 bytes of OID/size info = 19 bytes.
    let body_len: u32 = 2 + 19 * u32::from(count.unsigned_abs());
    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"T");
    buf.extend_from_slice(&(body_len + 4).to_be_bytes());
    buf.extend_from_slice(&count.to_be_bytes());
    for _ in 0..count {
        buf.extend_from_slice(&[0u8]); // empty name (null terminator)
        buf.extend_from_slice(&[0u8; 18]); // table_oid(4) + col_attr(2) + type_oid(4) + type_size(2) + type_mod(4) + format(2)
    }
    buf
}

#[test]
fn test_data_row_zero_fields_accepted() {
    let mut buf = make_data_row_with_count(0);
    let result = decode_message(&mut buf);
    assert!(result.is_ok(), "zero-field DataRow must be accepted");
    let (msg, _) = result.unwrap();
    assert!(matches!(msg, BackendMessage::DataRow(fields) if fields.is_empty()));
}

#[test]
fn test_data_row_field_count_exceeds_max_is_rejected() {
    // MAX_FIELD_COUNT + 1 = 2049 fields -- must trigger the guard before
    // any field data is read.
    let count: i16 = (MAX_FIELD_COUNT + 1) as i16; // 2049
    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"D");
    // body = 2 (count) + 4 (padding); length field includes itself: 2+4+4 = 10
    buf.extend_from_slice(&10u32.to_be_bytes());
    buf.extend_from_slice(&count.to_be_bytes());
    buf.extend_from_slice(&[0u8; 4]);

    let result = decode_message(&mut buf);
    assert!(result.is_err(), "DataRow with 2049 fields must be rejected");
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    let msg = err.to_string();
    assert!(msg.contains("2048"), "error must mention the limit: {msg}");
}

#[test]
fn test_row_description_field_count_exceeds_max_is_rejected() {
    let count: i16 = (MAX_FIELD_COUNT + 1) as i16; // 2049
    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"T");
    buf.extend_from_slice(&10u32.to_be_bytes());
    buf.extend_from_slice(&count.to_be_bytes());
    buf.extend_from_slice(&[0u8; 4]);

    let result = decode_message(&mut buf);
    assert!(
        result.is_err(),
        "RowDescription with 2049 fields must be rejected"
    );
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    let msg = err.to_string();
    assert!(msg.contains("2048"), "error must mention the limit: {msg}");
}

#[test]
fn test_row_description_small_field_count_accepted() {
    let mut buf = make_row_description_with_count(3);
    let result = decode_message(&mut buf);
    assert!(
        result.is_ok(),
        "3-field RowDescription must be accepted: {result:?}"
    );
    let (msg, _) = result.unwrap();
    assert!(matches!(msg, BackendMessage::RowDescription(fields) if fields.len() == 3));
}

// ── Error-field size cap tests (S21-H1) ───────────────────────────────────

fn make_error_response(field_type: u8, field_value: &[u8]) -> BytesMut {
    // ErrorResponse: tag 'E', length (4 bytes), then fields.
    // Each field: 1-byte type + value bytes + null terminator, then a final null byte.
    let body_len = 1 + field_value.len() + 1 + 1; // type + value + null + terminator
    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"E");
    buf.extend_from_slice(&(body_len as u32 + 4).to_be_bytes());
    buf.extend_from_slice(&[field_type]);
    buf.extend_from_slice(field_value);
    buf.extend_from_slice(&[0]); // null terminator for field value
    buf.extend_from_slice(&[0]); // terminating null byte
    buf
}

#[test]
fn error_field_within_limit_is_accepted() {
    let value = vec![b'x'; 1024]; // 1 KiB -- well within 64 KiB limit
    let mut buf = make_error_response(b'M', &value);
    let result = decode_message(&mut buf);
    assert!(
        result.is_ok(),
        "small error field must be accepted: {result:?}"
    );
}

#[test]
fn error_field_exceeding_limit_is_rejected() {
    let value = vec![b'x'; MAX_ERROR_FIELD_BYTES + 1]; // one byte over the cap
    let mut buf = make_error_response(b'M', &value);
    let result = decode_message(&mut buf);
    assert!(result.is_err(), "oversized error field must be rejected");
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    let msg = err.to_string();
    assert!(
        msg.contains("too large") || msg.contains("65536"),
        "error must mention size limit: {msg}"
    );
}

// ── SASL mechanism cap tests (S21-H2) ─────────────────────────────────────

fn make_sasl_auth(mechanisms: &[&str]) -> BytesMut {
    // Authentication SASL: tag 'R', length, auth type (10 = SASL), mechanism list.
    let mut mechanism_bytes: Vec<u8> = Vec::new();
    for m in mechanisms {
        mechanism_bytes.extend_from_slice(m.as_bytes());
        mechanism_bytes.push(0);
    }
    mechanism_bytes.push(0); // final double-null terminator
    let body_len = 4 + mechanism_bytes.len(); // auth type (4) + mechanisms
    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"R");
    buf.extend_from_slice(&(body_len as u32 + 4).to_be_bytes());
    buf.extend_from_slice(&10u32.to_be_bytes()); // SASL auth type
    buf.extend_from_slice(&mechanism_bytes);
    buf
}

#[test]
fn sasl_mechanisms_within_limit_are_accepted() {
    let mechanisms: Vec<&str> = (0..MAX_SASL_MECHANISMS).map(|_| "SCRAM-SHA-256").collect();
    let mut buf = make_sasl_auth(&mechanisms);
    let result = decode_message(&mut buf);
    assert!(
        result.is_ok(),
        "SASL with {MAX_SASL_MECHANISMS} mechanisms must be accepted"
    );
}

#[test]
fn sasl_mechanisms_exceeding_limit_are_truncated_not_rejected() {
    // The guard breaks out of the loop rather than erroring; verify it still succeeds
    // with at most MAX_SASL_MECHANISMS entries.
    let mechanisms: Vec<&str> = (0..MAX_SASL_MECHANISMS + 5)
        .map(|_| "SCRAM-SHA-256")
        .collect();
    let mut buf = make_sasl_auth(&mechanisms);
    let result = decode_message(&mut buf);
    assert!(
        result.is_ok(),
        "SASL with excess mechanisms must still parse successfully"
    );
    if let Ok((
        BackendMessage::Authentication(AuthenticationMessage::Sasl { mechanisms: parsed }),
        _,
    )) = result
    {
        assert!(
            parsed.len() <= MAX_SASL_MECHANISMS,
            "parsed mechanisms must not exceed cap: {} > {MAX_SASL_MECHANISMS}",
            parsed.len()
        );
    }
}

// ── Parameter name/value cap tests (S21-H3) ───────────────────────────────

fn make_parameter_status(name: &[u8], value: &[u8]) -> BytesMut {
    let body_len = name.len() + 1 + value.len() + 1; // name + null + value + null
    let mut buf = BytesMut::new();
    buf.extend_from_slice(b"S");
    buf.extend_from_slice(&(body_len as u32 + 4).to_be_bytes());
    buf.extend_from_slice(name);
    buf.extend_from_slice(&[0]);
    buf.extend_from_slice(value);
    buf.extend_from_slice(&[0]);
    buf
}

#[test]
fn parameter_status_normal_is_accepted() {
    let mut buf = make_parameter_status(b"server_version", b"16.0");
    let result = decode_message(&mut buf);
    assert!(
        result.is_ok(),
        "normal ParameterStatus must be accepted: {result:?}"
    );
}

#[test]
fn parameter_name_exceeding_limit_is_rejected() {
    let long_name = vec![b'a'; MAX_PARAMETER_NAME_BYTES + 1];
    let mut buf = make_parameter_status(&long_name, b"value");
    let result = decode_message(&mut buf);
    assert!(result.is_err(), "oversized parameter name must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("too long") || msg.contains("256"),
        "error must mention the name limit: {msg}"
    );
}

#[test]
fn parameter_value_exceeding_limit_is_rejected() {
    let long_value = vec![b'v'; MAX_PARAMETER_VALUE_BYTES + 1];
    let mut buf = make_parameter_status(b"timezone", &long_value);
    let result = decode_message(&mut buf);
    assert!(
        result.is_err(),
        "oversized parameter value must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("too long") || msg.contains("65536"),
        "error must mention the value limit: {msg}"
    );
}

#[test]
fn decode_message_rejects_oversized_declared_length() {
    // A tag plus a declared length far above MAX_MESSAGE_LEN must be a fatal
    // `InvalidData` error, not the `UnexpectedEof` the read loop would treat as
    // "need more bytes" and keep buffering toward ~2 GiB (audit M-wire-msg-cap).
    let mut data = BytesMut::from(
        &[
            b'D', // DataRow
            0x7F, 0xFF, 0xFF, 0xFF, // declared length ~2 GiB, well over MAX_MESSAGE_LEN
        ][..],
    );
    let err = decode_message(&mut data).unwrap_err();
    assert_eq!(
        err.kind(),
        io::ErrorKind::InvalidData,
        "an oversized declared length must be a fatal error"
    );
}

#[test]
fn decode_message_accepts_length_at_the_cap_boundary() {
    // A header declaring exactly MAX_MESSAGE_LEN is within bounds, so the
    // decoder must move past the length cap to the (here unmet) body-length
    // check — i.e. it must NOT reject on the cap.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let len = MAX_MESSAGE_LEN as i32;
    let mut data = BytesMut::new();
    data.extend_from_slice(b"D");
    data.extend_from_slice(&len.to_be_bytes());
    let err = decode_message(&mut data).unwrap_err();
    // Body is absent, so this is the incomplete-body path, NOT the cap.
    assert_eq!(
        err.kind(),
        io::ErrorKind::UnexpectedEof,
        "a length at the cap must not be rejected"
    );
}
