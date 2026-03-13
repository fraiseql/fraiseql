//! Protocol message decoding

use super::constants::{auth, tags};
use super::message::{AuthenticationMessage, BackendMessage, ErrorFields, FieldDescription};
use bytes::{Bytes, BytesMut};
use std::io;

/// Maximum number of fields accepted in a single DataRow or RowDescription message.
///
/// PostgreSQL's protocol allows up to 1600 columns per table (hard limit enforced by
/// the server), so 2048 is a generous cap that prevents an attacker-supplied message
/// from triggering a huge `Vec::with_capacity` before any bounds are checked.
const MAX_FIELD_COUNT: usize = 2048;

/// Decode a backend message from `BytesMut` without cloning
///
/// This version decodes in-place from a mutable `BytesMut` buffer and returns
/// the number of bytes consumed. The caller must advance the buffer after calling this.
///
/// # Returns
/// `Ok((msg, consumed))` - Message and number of bytes consumed
/// `Err(e)` - IO error if message is incomplete or invalid
///
/// # Performance
/// This version avoids the expensive `buf.clone().freeze()` call by working directly
/// with references, reducing allocations and copies in the hot path.
pub fn decode_message(data: &mut BytesMut) -> io::Result<(BackendMessage, usize)> {
    if data.len() < 5 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "incomplete message header",
        ));
    }

    let tag = data[0];
    let len_i32 = i32::from_be_bytes([data[1], data[2], data[3], data[4]]);

    // PostgreSQL message length includes the 4 length bytes but not the tag byte.
    // Minimum valid length is 4 (just the length field itself).
    if len_i32 < 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message length too small",
        ));
    }

    let len = len_i32 as usize;

    if data.len() < len + 1 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "incomplete message body",
        ));
    }

    // Create a temporary slice starting after the tag and length
    let msg_start = 5;
    let msg_end = len + 1;
    let msg_data = &data[msg_start..msg_end];

    let msg = match tag {
        tags::AUTHENTICATION => decode_authentication(msg_data)?,
        tags::BACKEND_KEY_DATA => decode_backend_key_data(msg_data)?,
        tags::COMMAND_COMPLETE => decode_command_complete(msg_data)?,
        tags::DATA_ROW => decode_data_row(msg_data)?,
        tags::ERROR_RESPONSE => decode_error_response(msg_data)?,
        tags::NOTICE_RESPONSE => decode_notice_response(msg_data)?,
        tags::PARAMETER_STATUS => decode_parameter_status(msg_data)?,
        tags::READY_FOR_QUERY => decode_ready_for_query(msg_data)?,
        tags::ROW_DESCRIPTION => decode_row_description(msg_data)?,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown message tag: {}", tag),
            ))
        }
    };

    Ok((msg, len + 1))
}

fn decode_authentication(data: &[u8]) -> io::Result<BackendMessage> {
    if data.len() < 4 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "auth type"));
    }
    let auth_type = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);

    let auth_msg = match auth_type {
        auth::OK => AuthenticationMessage::Ok,
        auth::CLEARTEXT_PASSWORD => AuthenticationMessage::CleartextPassword,
        auth::MD5_PASSWORD => {
            if data.len() < 8 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "salt data"));
            }
            let mut salt = [0u8; 4];
            salt.copy_from_slice(&data[4..8]);
            AuthenticationMessage::Md5Password { salt }
        }
        auth::SASL => {
            // SASL: read mechanism list (null-terminated strings)
            let mut mechanisms = Vec::new();
            let remaining = &data[4..];
            let mut offset = 0;
            loop {
                if offset >= remaining.len() {
                    break;
                }
                match remaining[offset..].iter().position(|&b| b == 0) {
                    Some(end) => {
                        let mechanism =
                            String::from_utf8_lossy(&remaining[offset..offset + end]).to_string();
                        if mechanism.is_empty() {
                            break;
                        }
                        mechanisms.push(mechanism);
                        offset += end + 1;
                    }
                    None => break,
                }
            }
            AuthenticationMessage::Sasl { mechanisms }
        }
        auth::SASL_CONTINUE => {
            // SASL continue: read remaining data as bytes
            let data_vec = data[4..].to_vec();
            AuthenticationMessage::SaslContinue { data: data_vec }
        }
        auth::SASL_FINAL => {
            // SASL final: read remaining data as bytes
            let data_vec = data[4..].to_vec();
            AuthenticationMessage::SaslFinal { data: data_vec }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported auth type: {}", auth_type),
            ))
        }
    };

    Ok(BackendMessage::Authentication(auth_msg))
}

fn decode_backend_key_data(data: &[u8]) -> io::Result<BackendMessage> {
    if data.len() < 8 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "backend key data",
        ));
    }
    let process_id = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let secret_key = i32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    Ok(BackendMessage::BackendKeyData {
        process_id,
        secret_key,
    })
}

fn decode_command_complete(data: &[u8]) -> io::Result<BackendMessage> {
    let end = data.iter().position(|&b| b == 0).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing null terminator in string",
        )
    })?;
    let tag = String::from_utf8_lossy(&data[..end]).to_string();
    Ok(BackendMessage::CommandComplete(tag))
}

fn decode_data_row(data: &[u8]) -> io::Result<BackendMessage> {
    if data.len() < 2 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field count"));
    }
    let field_count_i16 = i16::from_be_bytes([data[0], data[1]]);
    if field_count_i16 < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "negative field count",
        ));
    }
    let field_count = field_count_i16 as usize;
    if field_count > MAX_FIELD_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("DataRow field count {field_count} exceeds maximum {MAX_FIELD_COUNT}"),
        ));
    }
    let mut fields = Vec::with_capacity(field_count);
    let mut offset = 2;

    for _ in 0..field_count {
        if offset + 4 > data.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field length"));
        }
        let field_len = i32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        let field = if field_len == -1 {
            None
        } else if field_len < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "negative field length",
            ));
        } else {
            let len = field_len as usize;
            if offset + len > data.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field data"));
            }
            let field_bytes = Bytes::copy_from_slice(&data[offset..offset + len]);
            offset += len;
            Some(field_bytes)
        };
        fields.push(field);
    }

    Ok(BackendMessage::DataRow(fields))
}

fn decode_error_response(data: &[u8]) -> io::Result<BackendMessage> {
    let fields = decode_error_fields(data)?;
    Ok(BackendMessage::ErrorResponse(fields))
}

fn decode_notice_response(data: &[u8]) -> io::Result<BackendMessage> {
    let fields = decode_error_fields(data)?;
    Ok(BackendMessage::NoticeResponse(fields))
}

fn decode_error_fields(data: &[u8]) -> io::Result<ErrorFields> {
    let mut fields = ErrorFields::default();
    let mut offset = 0;

    loop {
        if offset >= data.len() {
            break;
        }
        let field_type = data[offset];
        offset += 1;
        if field_type == 0 {
            break;
        }

        let end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing null terminator in error field",
            )
        })?;
        let value = String::from_utf8_lossy(&data[offset..offset + end]).to_string();
        offset += end + 1;

        match field_type {
            b'S' => fields.severity = Some(value),
            b'C' => fields.code = Some(value),
            b'M' => fields.message = Some(value),
            b'D' => fields.detail = Some(value),
            b'H' => fields.hint = Some(value),
            b'P' => fields.position = Some(value),
            _ => {} // Ignore unknown fields
        }
    }

    Ok(fields)
}

fn decode_parameter_status(data: &[u8]) -> io::Result<BackendMessage> {
    let mut offset = 0;

    let name_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing null terminator in parameter name",
        )
    })?;
    let name = String::from_utf8_lossy(&data[offset..offset + name_end]).to_string();
    offset += name_end + 1;

    if offset >= data.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "parameter value",
        ));
    }
    let value_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing null terminator in parameter value",
        )
    })?;
    let value = String::from_utf8_lossy(&data[offset..offset + value_end]).to_string();

    Ok(BackendMessage::ParameterStatus { name, value })
}

fn decode_ready_for_query(data: &[u8]) -> io::Result<BackendMessage> {
    if data.is_empty() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "status byte"));
    }
    let status = data[0];
    Ok(BackendMessage::ReadyForQuery { status })
}

fn decode_row_description(data: &[u8]) -> io::Result<BackendMessage> {
    if data.len() < 2 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field count"));
    }
    let field_count_i16 = i16::from_be_bytes([data[0], data[1]]);
    if field_count_i16 < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "negative field count",
        ));
    }
    let field_count = field_count_i16 as usize;
    if field_count > MAX_FIELD_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("RowDescription field count {field_count} exceeds maximum {MAX_FIELD_COUNT}"),
        ));
    }
    let mut fields = Vec::with_capacity(field_count);
    let mut offset = 2;

    for _ in 0..field_count {
        // Read name (null-terminated string)
        let name_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing null terminator in field name",
            )
        })?;
        let name = String::from_utf8_lossy(&data[offset..offset + name_end]).to_string();
        offset += name_end + 1;

        // Read field descriptor (26 bytes: 4+2+4+2+4+2)
        if offset + 18 > data.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "field descriptor",
            ));
        }
        let table_oid = i32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;
        let column_attr = i16::from_be_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        let type_oid = i32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as u32;
        offset += 4;
        let type_size = i16::from_be_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        let type_modifier = i32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;
        let format_code = i16::from_be_bytes([data[offset], data[offset + 1]]);
        offset += 2;

        fields.push(FieldDescription {
            name,
            table_oid,
            column_attr,
            type_oid,
            type_size,
            type_modifier,
            format_code,
        });
    }

    Ok(BackendMessage::RowDescription(fields))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
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
        buf.extend_from_slice(&[b'D']);
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
        buf.extend_from_slice(&[b'T']);
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
        // MAX_FIELD_COUNT + 1 = 2049 fields — must trigger the guard before
        // any field data is read.
        let count: i16 = (MAX_FIELD_COUNT + 1) as i16; // 2049
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&[b'D']);
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
        buf.extend_from_slice(&[b'T']);
        buf.extend_from_slice(&10u32.to_be_bytes());
        buf.extend_from_slice(&count.to_be_bytes());
        buf.extend_from_slice(&[0u8; 4]);

        let result = decode_message(&mut buf);
        assert!(result.is_err(), "RowDescription with 2049 fields must be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = err.to_string();
        assert!(msg.contains("2048"), "error must mention the limit: {msg}");
    }

    #[test]
    fn test_row_description_small_field_count_accepted() {
        let mut buf = make_row_description_with_count(3);
        let result = decode_message(&mut buf);
        assert!(result.is_ok(), "3-field RowDescription must be accepted: {result:?}");
        let (msg, _) = result.unwrap();
        assert!(matches!(msg, BackendMessage::RowDescription(fields) if fields.len() == 3));
    }
}
