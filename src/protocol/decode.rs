//! Protocol message decoding

use super::constants::{auth, tags};
use super::message::{
    AuthenticationMessage, BackendMessage, ErrorFields, FieldDescription,
};
use crate::util::BytesExt;
use bytes::{Buf, Bytes};
use std::io;

/// Decode a backend message from bytes
///
/// Returns the message and remaining bytes
pub fn decode_message(mut data: Bytes) -> io::Result<(BackendMessage, Bytes)> {
    if data.len() < 5 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "incomplete message header",
        ));
    }

    let tag = data.get_u8();
    let len = data.get_i32() as usize;

    if data.len() < len - 4 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "incomplete message body",
        ));
    }

    let mut msg_data = data.split_to(len - 4);

    let msg = match tag {
        tags::AUTHENTICATION => decode_authentication(&mut msg_data)?,
        tags::BACKEND_KEY_DATA => decode_backend_key_data(&mut msg_data)?,
        tags::COMMAND_COMPLETE => decode_command_complete(&mut msg_data)?,
        tags::DATA_ROW => decode_data_row(&mut msg_data)?,
        tags::ERROR_RESPONSE => decode_error_response(&mut msg_data)?,
        tags::NOTICE_RESPONSE => decode_notice_response(&mut msg_data)?,
        tags::PARAMETER_STATUS => decode_parameter_status(&mut msg_data)?,
        tags::READY_FOR_QUERY => decode_ready_for_query(&mut msg_data)?,
        tags::ROW_DESCRIPTION => decode_row_description(&mut msg_data)?,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown message tag: {}", tag),
            ))
        }
    };

    Ok((msg, data))
}

fn decode_authentication(data: &mut Bytes) -> io::Result<BackendMessage> {
    let auth_type = data.read_i32_be()?;

    let auth_msg = match auth_type {
        auth::OK => AuthenticationMessage::Ok,
        auth::CLEARTEXT_PASSWORD => AuthenticationMessage::CleartextPassword,
        auth::MD5_PASSWORD => {
            let mut salt = [0u8; 4];
            data.copy_to_slice(&mut salt);
            AuthenticationMessage::Md5Password { salt }
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

fn decode_backend_key_data(data: &mut Bytes) -> io::Result<BackendMessage> {
    let process_id = data.read_i32_be()?;
    let secret_key = data.read_i32_be()?;
    Ok(BackendMessage::BackendKeyData {
        process_id,
        secret_key,
    })
}

fn decode_command_complete(data: &mut Bytes) -> io::Result<BackendMessage> {
    let tag = data.read_cstr()?;
    Ok(BackendMessage::CommandComplete(tag))
}

fn decode_data_row(data: &mut Bytes) -> io::Result<BackendMessage> {
    let field_count = data.read_i16_be()? as usize;
    let mut fields = Vec::with_capacity(field_count);

    for _ in 0..field_count {
        let field_len = data.read_i32_be()?;
        let field = if field_len == -1 {
            None
        } else {
            Some(data.split_to(field_len as usize))
        };
        fields.push(field);
    }

    Ok(BackendMessage::DataRow(fields))
}

fn decode_error_response(data: &mut Bytes) -> io::Result<BackendMessage> {
    let fields = decode_error_fields(data)?;
    Ok(BackendMessage::ErrorResponse(fields))
}

fn decode_notice_response(data: &mut Bytes) -> io::Result<BackendMessage> {
    let fields = decode_error_fields(data)?;
    Ok(BackendMessage::NoticeResponse(fields))
}

fn decode_error_fields(data: &mut Bytes) -> io::Result<ErrorFields> {
    let mut fields = ErrorFields::default();

    loop {
        let field_type = data.get_u8();
        if field_type == 0 {
            break;
        }

        let value = data.read_cstr()?;

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

fn decode_parameter_status(data: &mut Bytes) -> io::Result<BackendMessage> {
    let name = data.read_cstr()?;
    let value = data.read_cstr()?;
    Ok(BackendMessage::ParameterStatus { name, value })
}

fn decode_ready_for_query(data: &mut Bytes) -> io::Result<BackendMessage> {
    let status = data.get_u8();
    Ok(BackendMessage::ReadyForQuery { status })
}

fn decode_row_description(data: &mut Bytes) -> io::Result<BackendMessage> {
    let field_count = data.read_i16_be()? as usize;
    let mut fields = Vec::with_capacity(field_count);

    for _ in 0..field_count {
        let name = data.read_cstr()?;
        let table_oid = data.read_i32_be()?;
        let column_attr = data.read_i16_be()?;
        let type_oid = data.read_i32_be()? as u32;
        let type_size = data.read_i16_be()?;
        let type_modifier = data.read_i32_be()?;
        let format_code = data.read_i16_be()?;

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
        let data = Bytes::from_static(&[
            b'R', // Authentication
            0, 0, 0, 8, // Length = 8
            0, 0, 0, 0, // Auth OK
        ]);

        let (msg, _) = decode_message(data).unwrap();
        match msg {
            BackendMessage::Authentication(AuthenticationMessage::Ok) => {}
            _ => panic!("expected Authentication::Ok"),
        }
    }

    #[test]
    fn test_decode_ready_for_query() {
        let data = Bytes::from_static(&[
            b'Z',       // ReadyForQuery
            0, 0, 0, 5, // Length = 5
            b'I',       // Idle
        ]);

        let (msg, _) = decode_message(data).unwrap();
        match msg {
            BackendMessage::ReadyForQuery { status } => assert_eq!(status, b'I'),
            _ => panic!("expected ReadyForQuery"),
        }
    }
}
