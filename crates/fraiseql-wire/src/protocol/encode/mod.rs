//! Protocol message encoding

use super::message::FrontendMessage;
use bytes::{BufMut, BytesMut};
use std::io;

/// Write a 4-byte big-endian length back into a previously-reserved slot.
///
/// Every encoder writes a placeholder `put_i32(0)` early, captures the offset
/// in `len_pos`, then continues writing the body. This helper backfills the
/// real length using a bounds-checked `.get_mut()` so the file stays panic-free
/// under `#![deny(clippy::indexing_slicing)]`.
fn fill_length(buf: &mut BytesMut, len_pos: usize, len: usize) {
    let bytes = (len as i32).to_be_bytes();
    if let Some(slot) = buf.get_mut(len_pos..len_pos + 4) {
        slot.copy_from_slice(&bytes);
    }
    // If the slot is missing the buffer was corrupted upstream; the encoder
    // contract guarantees the caller reserved exactly 4 bytes at `len_pos`
    // before appending the body, so the `if let` always succeeds in practice.
}

/// Encode a frontend message into bytes
///
/// # Errors
///
/// Returns `io::Error` if the message contains invalid UTF-8 or cannot be serialized.
pub fn encode_message(msg: &FrontendMessage) -> io::Result<BytesMut> {
    let mut buf = BytesMut::new();

    match msg {
        FrontendMessage::Startup { version, params } => {
            encode_startup(&mut buf, *version, params)?;
        }
        FrontendMessage::Password(password) => {
            encode_password(&mut buf, password)?;
        }
        FrontendMessage::Query(query) => {
            encode_query(&mut buf, query)?;
        }
        FrontendMessage::Terminate => {
            encode_terminate(&mut buf)?;
        }
        FrontendMessage::SaslInitialResponse { mechanism, data } => {
            encode_sasl_initial_response(&mut buf, mechanism, data)?;
        }
        FrontendMessage::SaslResponse { data } => {
            encode_sasl_response(&mut buf, data)?;
        }
    }

    Ok(buf)
}

fn encode_startup(buf: &mut BytesMut, version: i32, params: &[(String, String)]) -> io::Result<()> {
    // Startup messages don't have a type byte
    // Reserve space for length (will be filled at end)
    let len_pos = buf.len();
    buf.put_i32(0);

    // Protocol version
    buf.put_i32(version);

    // Parameters (key-value pairs, null-terminated)
    for (key, value) in params {
        buf.put(key.as_bytes());
        buf.put_u8(0);
        buf.put(value.as_bytes());
        buf.put_u8(0);
    }

    // Final null terminator
    buf.put_u8(0);

    // Fill in length
    let len = buf.len() - len_pos;
    fill_length(buf, len_pos, len);

    Ok(())
}

fn encode_password(buf: &mut BytesMut, password: &str) -> io::Result<()> {
    buf.put_u8(b'p');
    let len_pos = buf.len();
    buf.put_i32(0);

    buf.put(password.as_bytes());
    buf.put_u8(0);

    let len = buf.len() - len_pos;
    fill_length(buf, len_pos, len);

    Ok(())
}

fn encode_query(buf: &mut BytesMut, query: &str) -> io::Result<()> {
    buf.put_u8(b'Q');
    let len_pos = buf.len();
    buf.put_i32(0);

    buf.put(query.as_bytes());
    buf.put_u8(0);

    let len = buf.len() - len_pos;
    fill_length(buf, len_pos, len);

    Ok(())
}

fn encode_terminate(buf: &mut BytesMut) -> io::Result<()> {
    buf.put_u8(b'X');
    buf.put_i32(4); // Length includes itself
    Ok(())
}

fn encode_sasl_initial_response(
    buf: &mut BytesMut,
    mechanism: &str,
    data: &[u8],
) -> io::Result<()> {
    buf.put_u8(b'p');
    let len_pos = buf.len();
    buf.put_i32(0);

    // Mechanism name (null-terminated)
    buf.put(mechanism.as_bytes());
    buf.put_u8(0);

    // SASL data (as length-prefixed bytes)
    buf.put_i32(data.len() as i32);
    buf.put_slice(data);

    let len = buf.len() - len_pos;
    fill_length(buf, len_pos, len);

    Ok(())
}

fn encode_sasl_response(buf: &mut BytesMut, data: &[u8]) -> io::Result<()> {
    buf.put_u8(b'p');
    let len_pos = buf.len();
    buf.put_i32(0);

    // SASL data (length-prefixed)
    buf.put_slice(data);

    let len = buf.len() - len_pos;
    fill_length(buf, len_pos, len);

    Ok(())
}

#[cfg(test)]
mod tests;
