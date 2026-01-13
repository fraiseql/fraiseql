# Phase 1: Protocol Foundation [RED]

## Objective

Implement the minimal subset of the Postgres wire protocol required to connect, authenticate, and execute a simple query. This phase focuses on **protocol encoding/decoding only** — no streaming, chunking, or high-level API yet.

## Context

fraiseql-wire needs to speak the Postgres wire protocol at the byte level. This phase implements:

* Protocol message types (Startup, Query, RowDescription, DataRow, etc.)
* Message encoding and decoding (pure functions, no I/O)
* Protocol state tracking (startup sequence, query lifecycle)

**Design principle**: Protocol encoding/decoding must be pure (no side effects). All I/O happens in the connection layer (Phase 2).

## Prerequisites

* Phase 0 completed (project structure, error types, utilities)

## Files to Create

* `src/protocol/mod.rs` — protocol module entry point
* `src/protocol/message.rs` — message type definitions
* `src/protocol/encode.rs` — message encoding
* `src/protocol/decode.rs` — message decoding
* `src/protocol/constants.rs` — protocol constants (message tags, etc.)

## Files to Modify

* `src/lib.rs` — add `pub mod protocol;`

## Implementation Steps

### 1. Create src/protocol/constants.rs

```rust
//! Postgres protocol constants

/// Protocol version 3.0
pub const PROTOCOL_VERSION: i32 = 0x0003_0000;

/// Message type tags
pub mod tags {
    /// Authentication request
    pub const AUTHENTICATION: u8 = b'R';

    /// Backend key data
    pub const BACKEND_KEY_DATA: u8 = b'K';

    /// Command complete
    pub const COMMAND_COMPLETE: u8 = b'C';

    /// Data row
    pub const DATA_ROW: u8 = b'D';

    /// Error response
    pub const ERROR_RESPONSE: u8 = b'E';

    /// Notice response
    pub const NOTICE_RESPONSE: u8 = b'N';

    /// Parameter status
    pub const PARAMETER_STATUS: u8 = b'S';

    /// Ready for query
    pub const READY_FOR_QUERY: u8 = b'Z';

    /// Row description
    pub const ROW_DESCRIPTION: u8 = b'T';
}

/// Authentication types
pub mod auth {
    /// Authentication successful
    pub const OK: i32 = 0;

    /// Cleartext password required
    pub const CLEARTEXT_PASSWORD: i32 = 3;

    /// MD5 password required
    pub const MD5_PASSWORD: i32 = 5;
}

/// Transaction status
pub mod tx_status {
    /// Idle (not in transaction)
    pub const IDLE: u8 = b'I';

    /// In transaction block
    pub const IN_TRANSACTION: u8 = b'T';

    /// Failed transaction (queries will be rejected until END)
    pub const FAILED: u8 = b'E';
}
```

### 2. Create src/protocol/message.rs

```rust
//! Protocol message types

use bytes::Bytes;

/// Frontend message (client → server)
#[derive(Debug, Clone)]
pub enum FrontendMessage {
    /// Startup message
    Startup {
        /// Protocol version
        version: i32,
        /// Connection parameters
        params: Vec<(String, String)>,
    },

    /// Password message
    Password(String),

    /// Query message
    Query(String),

    /// Terminate message
    Terminate,
}

/// Backend message (server → client)
#[derive(Debug, Clone)]
pub enum BackendMessage {
    /// Authentication request
    Authentication(AuthenticationMessage),

    /// Backend key data (for cancellation)
    BackendKeyData {
        /// Process ID
        process_id: i32,
        /// Secret key
        secret_key: i32,
    },

    /// Command complete
    CommandComplete(String),

    /// Data row
    DataRow(Vec<Option<Bytes>>),

    /// Error response
    ErrorResponse(ErrorFields),

    /// Notice response
    NoticeResponse(ErrorFields),

    /// Parameter status
    ParameterStatus {
        /// Parameter name
        name: String,
        /// Parameter value
        value: String,
    },

    /// Ready for query
    ReadyForQuery {
        /// Transaction status
        status: u8,
    },

    /// Row description
    RowDescription(Vec<FieldDescription>),
}

/// Authentication message types
#[derive(Debug, Clone)]
pub enum AuthenticationMessage {
    /// Authentication OK
    Ok,

    /// Cleartext password required
    CleartextPassword,

    /// MD5 password required
    Md5Password {
        /// Salt for MD5 hash
        salt: [u8; 4],
    },
}

/// Field description (column metadata)
#[derive(Debug, Clone)]
pub struct FieldDescription {
    /// Column name
    pub name: String,
    /// Table OID (0 if not a table column)
    pub table_oid: i32,
    /// Column attribute number (0 if not a table column)
    pub column_attr: i16,
    /// Data type OID
    pub type_oid: u32,
    /// Data type size
    pub type_size: i16,
    /// Type modifier
    pub type_modifier: i32,
    /// Format code (0 = text, 1 = binary)
    pub format_code: i16,
}

/// Error/notice fields
#[derive(Debug, Clone, Default)]
pub struct ErrorFields {
    /// Severity (ERROR, WARNING, etc.)
    pub severity: Option<String>,
    /// SQLSTATE code
    pub code: Option<String>,
    /// Human-readable message
    pub message: Option<String>,
    /// Additional detail
    pub detail: Option<String>,
    /// Hint
    pub hint: Option<String>,
    /// Position in query string
    pub position: Option<String>,
}

impl std::fmt::Display for ErrorFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref msg) = self.message {
            write!(f, "{}", msg)?;
        }
        if let Some(ref code) = self.code {
            write!(f, " ({})", code)?;
        }
        Ok(())
    }
}
```

### 3. Create src/protocol/encode.rs

```rust
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
```

### 4. Create src/protocol/decode.rs

```rust
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
```

### 5. Create src/protocol/mod.rs

```rust
//! Postgres wire protocol implementation
//!
//! This module implements the minimal subset of the Postgres wire protocol
//! needed for fraiseql-wire:
//!
//! * Startup and authentication
//! * Simple Query protocol
//! * Result streaming (RowDescription, DataRow)
//! * Error handling
//!
//! Explicitly NOT supported:
//! * Extended Query protocol (prepared statements)
//! * COPY protocol
//! * Transactions
//! * Multi-statement queries

pub mod constants;
pub mod decode;
pub mod encode;
pub mod message;

pub use decode::decode_message;
pub use encode::encode_message;
pub use message::{
    AuthenticationMessage, BackendMessage, ErrorFields, FieldDescription, FrontendMessage,
};
```

### 6. Update src/lib.rs

```rust
pub mod error;
pub mod protocol;  // ADD THIS LINE
pub mod util;

pub use error::{Error, Result};
```

## Verification Commands

```bash
# Build
cargo build

# Run all tests
cargo test

# Run protocol tests only
cargo test protocol::

# Check test coverage for protocol module
cargo test --test protocol -- --nocapture

# Clippy
cargo clippy -- -D warnings
```

## Expected Output

### cargo test
```
running 4 tests
test protocol::encode::tests::test_encode_query ... ok
test protocol::encode::tests::test_encode_terminate ... ok
test protocol::decode::tests::test_decode_authentication_ok ... ok
test protocol::decode::tests::test_decode_ready_for_query ... ok

test result: ok. 4 passed; 0 failed; 0 ignored
```

## Acceptance Criteria

- [ ] All protocol message types are defined
- [ ] Encoding functions produce correct byte sequences
- [ ] Decoding functions parse byte sequences correctly
- [ ] Round-trip encode/decode tests pass
- [ ] Error handling for malformed messages works
- [ ] No clippy warnings
- [ ] All tests pass
- [ ] Protocol encoding/decoding is pure (no I/O side effects)

## DO NOT

* Implement connection logic (TCP/Unix sockets) — that's Phase 2
* Implement streaming or chunking — that's Phase 3
* Add TLS support (out of scope)
* Implement Extended Query protocol (prepared statements not supported)
* Add COPY protocol support (not needed)

## Next Phase

**Phase 2: Connection Layer** — Implement TCP/Unix socket connections, connection state machine, and integrate protocol encoding/decoding with I/O.
