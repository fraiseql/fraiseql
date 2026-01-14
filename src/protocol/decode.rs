//! Protocol message decoding

use super::constants::{auth, tags};
use super::message::{
    AuthenticationMessage, BackendMessage, ErrorFields, FieldDescription,
};
use bytes::{BytesMut, Bytes};
use std::io;

/// Decode a backend message from BytesMut without cloning
///
/// This version decodes in-place from a mutable BytesMut buffer and returns
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
    let len = i32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;

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
                        let mechanism = String::from_utf8_lossy(&remaining[offset..offset+end]).to_string();
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
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "backend key data"));
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
        io::Error::new(io::ErrorKind::InvalidData, "missing null terminator in string")
    })?;
    let tag = String::from_utf8_lossy(&data[..end]).to_string();
    Ok(BackendMessage::CommandComplete(tag))
}

fn decode_data_row(data: &[u8]) -> io::Result<BackendMessage> {
    if data.len() < 2 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field count"));
    }
    let field_count = i16::from_be_bytes([data[0], data[1]]) as usize;
    let mut fields = Vec::with_capacity(field_count);
    let mut offset = 2;

    for _ in 0..field_count {
        if offset + 4 > data.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field length"));
        }
        let field_len = i32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]);
        offset += 4;

        let field = if field_len == -1 {
            None
        } else {
            let len = field_len as usize;
            if offset + len > data.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field data"));
            }
            let field_bytes = Bytes::copy_from_slice(&data[offset..offset+len]);
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
            io::Error::new(io::ErrorKind::InvalidData, "missing null terminator in error field")
        })?;
        let value = String::from_utf8_lossy(&data[offset..offset+end]).to_string();
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
        io::Error::new(io::ErrorKind::InvalidData, "missing null terminator in parameter name")
    })?;
    let name = String::from_utf8_lossy(&data[offset..offset+name_end]).to_string();
    offset += name_end + 1;

    if offset >= data.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "parameter value"));
    }
    let value_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing null terminator in parameter value")
    })?;
    let value = String::from_utf8_lossy(&data[offset..offset+value_end]).to_string();

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
    let field_count = i16::from_be_bytes([data[0], data[1]]) as usize;
    let mut fields = Vec::with_capacity(field_count);
    let mut offset = 2;

    for _ in 0..field_count {
        // Read name (null-terminated string)
        let name_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing null terminator in field name")
        })?;
        let name = String::from_utf8_lossy(&data[offset..offset+name_end]).to_string();
        offset += name_end + 1;

        // Read field descriptor (26 bytes: 4+2+4+2+4+2)
        if offset + 18 > data.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "field descriptor"));
        }
        let table_oid = i32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]);
        offset += 4;
        let column_attr = i16::from_be_bytes([data[offset], data[offset+1]]);
        offset += 2;
        let type_oid = i32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as u32;
        offset += 4;
        let type_size = i16::from_be_bytes([data[offset], data[offset+1]]);
        offset += 2;
        let type_modifier = i32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]);
        offset += 4;
        let format_code = i16::from_be_bytes([data[offset], data[offset+1]]);
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
    use super::*;

    #[test]
    fn test_decode_authentication_ok() {
        let mut data = BytesMut::from(&[
            b'R', // Authentication
            0, 0, 0, 8, // Length = 8
            0, 0, 0, 0, // Auth OK
        ][..]);

        let (msg, consumed) = decode_message(&mut data).unwrap();
        match msg {
            BackendMessage::Authentication(AuthenticationMessage::Ok) => {}
            _ => panic!("expected Authentication::Ok"),
        }
        assert_eq!(consumed, 9); // 1 tag + 4 len + 4 auth type
    }

    #[test]
    fn test_decode_ready_for_query() {
        let mut data = BytesMut::from(&[
            b'Z',       // ReadyForQuery
            0, 0, 0, 5, // Length = 5
            b'I',       // Idle
        ][..]);

        let (msg, consumed) = decode_message(&mut data).unwrap();
        match msg {
            BackendMessage::ReadyForQuery { status } => assert_eq!(status, b'I'),
            _ => panic!("expected ReadyForQuery"),
        }
        assert_eq!(consumed, 6); // 1 tag + 4 len + 1 status
    }
}
