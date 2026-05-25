//! Protocol message decoding

use super::constants::{auth, tags};
use super::message::{AuthenticationMessage, BackendMessage, ErrorFields, FieldDescription};
use bytes::{Bytes, BytesMut};
use std::io;

/// Bounds-checked read cursor over a byte slice.
///
/// All accessors return `io::Result` so this whole file can stay panic-free
/// under `#![deny(clippy::indexing_slicing)]`. Each method advances `offset`
/// only on success.
struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn remaining(&self) -> &'a [u8] {
        // `self.offset` is monotonically advanced only by successful reads,
        // each of which ensures the offset stays `<= self.data.len()`.
        self.data.get(self.offset..).unwrap_or(&[])
    }

    const fn is_empty(&self) -> bool {
        self.offset >= self.data.len()
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        let byte = *self
            .data
            .get(self.offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "byte"))?;
        self.offset += 1;
        Ok(byte)
    }

    fn read_i16_be(&mut self) -> io::Result<i16> {
        let bytes: [u8; 2] = self
            .data
            .get(self.offset..self.offset + 2)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "i16"))?
            .try_into()
            // Reason: provably-safe — `.get(offset..offset+2)` returned a
            // 2-byte slice, and `<[u8; 2]>::try_from(&[u8])` cannot fail on
            // a slice of the exact length.
            .expect("slice of length 2 always converts to [u8; 2]");
        self.offset += 2;
        Ok(i16::from_be_bytes(bytes))
    }

    fn read_i32_be(&mut self) -> io::Result<i32> {
        let bytes: [u8; 4] = self
            .data
            .get(self.offset..self.offset + 4)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "i32"))?
            .try_into()
            // Reason: provably-safe — slice length 4 always converts to [u8; 4].
            .expect("slice of length 4 always converts to [u8; 4]");
        self.offset += 4;
        Ok(i32::from_be_bytes(bytes))
    }

    fn read_slice(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let slice = self
            .data
            .get(self.offset..self.offset + n)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "slice"))?;
        self.offset += n;
        Ok(slice)
    }

    /// Read until the next `0x00` byte (exclusive), advancing past the null.
    /// Returns the bytes before the null terminator.
    fn read_until_null(&mut self) -> io::Result<&'a [u8]> {
        let tail = self.remaining();
        let end = tail.iter().position(|&b| b == 0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing null terminator in string",
            )
        })?;
        let bytes = tail.get(..end).unwrap_or(&[]);
        // Advance past `end` bytes plus the null terminator.
        self.offset += end + 1;
        Ok(bytes)
    }

    /// Find the next `0x00` byte in the remaining slice without advancing.
    fn position_of_null(&self) -> Option<usize> {
        self.remaining().iter().position(|&b| b == 0)
    }
}

/// Maximum number of fields accepted in a single DataRow or RowDescription message.
///
/// PostgreSQL's protocol allows up to 1600 columns per table (hard limit enforced by
/// the server), so 2048 is a generous cap that prevents an attacker-supplied message
/// from triggering a huge `Vec::with_capacity` before any bounds are checked.
pub(crate) const MAX_FIELD_COUNT: usize = 2048;

/// Maximum byte length of a single error/notice field string (severity, message, etc.).
///
/// A 64 KiB cap is generous for any human-readable error message. Without this limit a
/// malicious server can send a single oversized field and drive unbounded allocation
/// in `String::from_utf8_lossy` before the string is ever stored.
pub(crate) const MAX_ERROR_FIELD_BYTES: usize = 64 * 1024; // 64 KiB

/// Maximum number of SASL mechanism names accepted in an Authentication message.
///
/// Real providers offer one or two mechanisms (e.g. SCRAM-SHA-256).  Capping at 32
/// prevents a rogue server from flooding the `Vec<String>` until memory is exhausted.
pub(crate) const MAX_SASL_MECHANISMS: usize = 32;

/// Maximum byte length of a ParameterStatus name (e.g. `"server_version"`).
///
/// PostgreSQL parameter names are short identifiers; 256 bytes is more than enough.
pub(crate) const MAX_PARAMETER_NAME_BYTES: usize = 256;

/// Maximum byte length of a ParameterStatus value.
///
/// 64 KiB covers realistic values (long `TimeZone` strings, etc.) while preventing
/// a malicious server from inflating memory with an oversized value string.
pub(crate) const MAX_PARAMETER_VALUE_BYTES: usize = 64 * 1024; // 64 KiB

/// Decode a backend message from `BytesMut` without cloning
///
/// This version decodes in-place from a mutable `BytesMut` buffer and returns
/// the number of bytes consumed. The caller must advance the buffer after calling this.
///
/// # Errors
///
/// Returns `io::Error` with `UnexpectedEof` if the buffer does not contain a complete
/// message. Returns `io::Error` with `InvalidData` if the message tag or length is invalid.
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

    let mut header = Cursor::new(data);
    let tag = header.read_u8()?;
    let len_i32 = header.read_i32_be()?;

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
    let msg_data = data
        .get(msg_start..msg_end)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "message body slice"))?;

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
    let mut cur = Cursor::new(data);
    let auth_type = cur
        .read_i32_be()
        .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "auth type"))?;

    let auth_msg = match auth_type {
        auth::OK => AuthenticationMessage::Ok,
        auth::CLEARTEXT_PASSWORD => AuthenticationMessage::CleartextPassword,
        auth::MD5_PASSWORD => {
            let salt_slice = cur
                .read_slice(4)
                .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "salt data"))?;
            let salt: [u8; 4] = salt_slice
                .try_into()
                // Reason: provably-safe — `read_slice(4)` returns a 4-byte slice.
                .expect("slice of length 4 always converts to [u8; 4]");
            AuthenticationMessage::Md5Password { salt }
        }
        auth::SASL => {
            // SASL: read mechanism list (null-terminated strings)
            let mut mechanisms = Vec::new();
            loop {
                if cur.is_empty() {
                    break;
                }
                let Some(end) = cur.position_of_null() else {
                    break;
                };
                let mech_bytes = cur.read_slice(end).unwrap_or(&[]);
                let mechanism = String::from_utf8_lossy(mech_bytes).to_string();
                // Skip the null terminator we just located.
                let _ = cur.read_u8();
                if mechanism.is_empty() {
                    break;
                }
                if mechanisms.len() >= MAX_SASL_MECHANISMS {
                    break;
                }
                mechanisms.push(mechanism);
            }
            AuthenticationMessage::Sasl { mechanisms }
        }
        auth::SASL_CONTINUE => {
            // SASL continue: read remaining data as bytes
            let data_vec = cur.remaining().to_vec();
            AuthenticationMessage::SaslContinue { data: data_vec }
        }
        auth::SASL_FINAL => {
            // SASL final: read remaining data as bytes
            let data_vec = cur.remaining().to_vec();
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
    let mut cur = Cursor::new(data);
    let process_id = cur
        .read_i32_be()
        .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "backend key data"))?;
    let secret_key = cur
        .read_i32_be()
        .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "backend key data"))?;
    Ok(BackendMessage::BackendKeyData {
        process_id,
        secret_key,
    })
}

fn decode_command_complete(data: &[u8]) -> io::Result<BackendMessage> {
    let mut cur = Cursor::new(data);
    let tag_bytes = cur.read_until_null()?;
    let tag = String::from_utf8_lossy(tag_bytes).to_string();
    Ok(BackendMessage::CommandComplete(tag))
}

fn decode_data_row(data: &[u8]) -> io::Result<BackendMessage> {
    let mut cur = Cursor::new(data);
    let field_count_i16 = cur
        .read_i16_be()
        .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field count"))?;
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

    for _ in 0..field_count {
        let field_len = cur
            .read_i32_be()
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field length"))?;

        let field = if field_len == -1 {
            None
        } else if field_len < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "negative field length",
            ));
        } else {
            let len = field_len as usize;
            let field_slice = cur
                .read_slice(len)
                .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field data"))?;
            Some(Bytes::copy_from_slice(field_slice))
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
    let mut cur = Cursor::new(data);

    loop {
        if cur.is_empty() {
            break;
        }
        let field_type = cur.read_u8()?;
        if field_type == 0 {
            break;
        }

        let end = cur.position_of_null().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing null terminator in error field",
            )
        })?;
        if end > MAX_ERROR_FIELD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Error field too large ({end} bytes, max {MAX_ERROR_FIELD_BYTES})"),
            ));
        }
        let value_bytes = cur.read_slice(end).unwrap_or(&[]);
        let value = String::from_utf8_lossy(value_bytes).to_string();
        // Skip the null terminator.
        let _ = cur.read_u8();

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
    let mut cur = Cursor::new(data);

    let name_end = cur.position_of_null().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing null terminator in parameter name",
        )
    })?;
    if name_end > MAX_PARAMETER_NAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Parameter name too long ({name_end} bytes, max {MAX_PARAMETER_NAME_BYTES})"),
        ));
    }
    let name_bytes = cur.read_slice(name_end).unwrap_or(&[]);
    let name = String::from_utf8_lossy(name_bytes).to_string();
    // Skip null terminator.
    let _ = cur.read_u8();

    if cur.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "parameter value",
        ));
    }
    let value_end = cur.position_of_null().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing null terminator in parameter value",
        )
    })?;
    if value_end > MAX_PARAMETER_VALUE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Parameter value too long ({value_end} bytes, max {MAX_PARAMETER_VALUE_BYTES})"
            ),
        ));
    }
    let value_bytes = cur.read_slice(value_end).unwrap_or(&[]);
    let value = String::from_utf8_lossy(value_bytes).to_string();

    Ok(BackendMessage::ParameterStatus { name, value })
}

fn decode_ready_for_query(data: &[u8]) -> io::Result<BackendMessage> {
    let status = *data
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "status byte"))?;
    Ok(BackendMessage::ReadyForQuery { status })
}

fn decode_row_description(data: &[u8]) -> io::Result<BackendMessage> {
    let mut cur = Cursor::new(data);
    let field_count_i16 = cur
        .read_i16_be()
        .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field count"))?;
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

    for _ in 0..field_count {
        // Read name (null-terminated string)
        let name_end = cur.position_of_null().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing null terminator in field name",
            )
        })?;
        let name_bytes = cur.read_slice(name_end).unwrap_or(&[]);
        let name = String::from_utf8_lossy(name_bytes).to_string();
        // Skip null terminator.
        let _ = cur.read_u8();

        // Read field descriptor (18 bytes: 4+2+4+2+4+2)
        let table_oid = cur
            .read_i32_be()
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field descriptor"))?;
        let column_attr = cur
            .read_i16_be()
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field descriptor"))?;
        let type_oid = cur
            .read_i32_be()
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field descriptor"))?
            as u32;
        let type_size = cur
            .read_i16_be()
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field descriptor"))?;
        let type_modifier = cur
            .read_i32_be()
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field descriptor"))?;
        let format_code = cur
            .read_i16_be()
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "field descriptor"))?;

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
mod tests;
