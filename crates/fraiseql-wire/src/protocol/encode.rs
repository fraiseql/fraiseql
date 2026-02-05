//! Protocol message encoding

use super::message::FrontendMessage;
use bytes::{BufMut, BytesMut};
use std::io;

/// Encode a frontend message into bytes
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
    buf[len_pos..len_pos + 4].copy_from_slice(&(len as i32).to_be_bytes());

    Ok(())
}

fn encode_password(buf: &mut BytesMut, password: &str) -> io::Result<()> {
    buf.put_u8(b'p');
    let len_pos = buf.len();
    buf.put_i32(0);

    buf.put(password.as_bytes());
    buf.put_u8(0);

    let len = buf.len() - len_pos;
    buf[len_pos..len_pos + 4].copy_from_slice(&(len as i32).to_be_bytes());

    Ok(())
}

fn encode_query(buf: &mut BytesMut, query: &str) -> io::Result<()> {
    buf.put_u8(b'Q');
    let len_pos = buf.len();
    buf.put_i32(0);

    buf.put(query.as_bytes());
    buf.put_u8(0);

    let len = buf.len() - len_pos;
    buf[len_pos..len_pos + 4].copy_from_slice(&(len as i32).to_be_bytes());

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
    buf[len_pos..len_pos + 4].copy_from_slice(&(len as i32).to_be_bytes());

    Ok(())
}

fn encode_sasl_response(buf: &mut BytesMut, data: &[u8]) -> io::Result<()> {
    buf.put_u8(b'p');
    let len_pos = buf.len();
    buf.put_i32(0);

    // SASL data (length-prefixed)
    buf.put_slice(data);

    let len = buf.len() - len_pos;
    buf[len_pos..len_pos + 4].copy_from_slice(&(len as i32).to_be_bytes());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_query() {
        let msg = FrontendMessage::Query("SELECT 1".to_string());
        let buf = encode_message(&msg).unwrap();

        assert_eq!(buf[0], b'Q');
        let len = i32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
        assert_eq!(len, (buf.len() - 1) as i32);
    }

    #[test]
    fn test_encode_terminate() {
        let msg = FrontendMessage::Terminate;
        let buf = encode_message(&msg).unwrap();

        assert_eq!(buf[0], b'X');
        assert_eq!(buf.len(), 5);
    }
}
