# Phase 0: Project Setup & Foundation [GREENFIELD]

## Objective

Bootstrap the fraiseql-wire project with proper Rust project structure, dependencies, and foundational utilities before implementing core protocol logic.

## Context

This is a **greenfield project**. The architecture is defined in PRD.md and CLAUDE.md, but no code exists yet. This phase establishes:

* Cargo project structure with proper module organization
* Core dependencies (tokio, futures, serde, bytes)
* Error types and result aliases
* Basic tracing/logging setup
* Project conventions (rustfmt, clippy config)

## Prerequisites

* Rust toolchain installed (edition 2021)
* Postgres 17 available for later integration testing

## Files to Create

* `Cargo.toml` — project manifest with dependencies
* `src/lib.rs` — library entry point
* `src/error.rs` — error types
* `src/util/mod.rs` — utility module entry
* `src/util/oid.rs` — Postgres OID constants
* `src/util/bytes.rs` — byte manipulation helpers
* `.cargo/config.toml` — cargo configuration
* `.gitignore` — ignore target/, Cargo.lock, etc.

## Implementation Steps

### 1. Create Cargo.toml with core dependencies

```toml
[package]
name = "fraiseql-wire"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
futures = "0.3"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Bytes manipulation
bytes = "1"

# Error handling
thiserror = "1"

# Tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tokio-test = "0.4"

[lib]
name = "fraiseql_wire"
path = "src/lib.rs"

[[example]]
name = "basic_stream"
path = "examples/basic_stream.rs"
```

### 2. Create src/lib.rs (library entry point)

```rust
//! fraiseql-wire: Streaming JSON query engine for Postgres 17
//!
//! This crate provides a minimal, async Rust query engine that streams JSON
//! data from Postgres with low latency and bounded memory usage.
//!
//! # Supported Query Shape
//!
//! ```sql
//! SELECT data
//! FROM v_{entity}
//! WHERE predicate
//! [ORDER BY expression]
//! ```

#![warn(missing_docs, rust_2018_idioms)]

pub mod error;
pub mod util;

// Re-export commonly used types
pub use error::{Error, Result};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

### 3. Create src/error.rs (error types)

```rust
//! Error types for fraiseql-wire

use std::io;
use thiserror::Error;

/// Main error type for fraiseql-wire operations
#[derive(Debug, Error)]
pub enum Error {
    /// Connection error
    #[error("connection error: {0}")]
    Connection(String),

    /// Authentication error
    #[error("authentication failed: {0}")]
    Authentication(String),

    /// Protocol violation
    #[error("protocol error: {0}")]
    Protocol(String),

    /// SQL execution error
    #[error("sql error: {0}")]
    Sql(String),

    /// JSON decoding error
    #[error("json decode error: {0}")]
    JsonDecode(#[from] serde_json::Error),

    /// I/O error
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// Invalid configuration
    #[error("invalid configuration: {0}")]
    Config(String),

    /// Query cancelled by client
    #[error("query cancelled")]
    Cancelled,

    /// Invalid result schema (not single `data` column)
    #[error("invalid result schema: {0}")]
    InvalidSchema(String),
}

/// Result type alias using fraiseql-wire Error
pub type Result<T> = std::result::Result<T, Error>;
```

### 4. Create src/util/mod.rs

```rust
//! Utility modules for protocol and data handling

pub mod bytes;
pub mod oid;

pub use self::bytes::BytesExt;
pub use self::oid::{JsonOid, JsonbOid, OID};
```

### 5. Create src/util/oid.rs (Postgres OID constants)

```rust
//! Postgres Object Identifier (OID) constants
//!
//! OIDs identify data types in the Postgres wire protocol.

/// Postgres type OID
pub type OID = u32;

/// JSON type OID
pub const JsonOid: OID = 114;

/// JSONB type OID
pub const JsonbOid: OID = 3802;

/// Check if an OID represents a JSON type
#[inline]
pub fn is_json_oid(oid: OID) -> bool {
    oid == JsonOid || oid == JsonbOid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_oids() {
        assert!(is_json_oid(JsonOid));
        assert!(is_json_oid(JsonbOid));
        assert!(!is_json_oid(23)); // INT4
    }
}
```

### 6. Create src/util/bytes.rs (byte helpers)

```rust
//! Byte manipulation utilities for protocol parsing

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io;

/// Extension trait for Bytes operations
pub trait BytesExt {
    /// Read a null-terminated string
    fn read_cstr(&mut self) -> io::Result<String>;

    /// Read a 32-bit big-endian integer
    fn read_i32_be(&mut self) -> io::Result<i32>;

    /// Read a 16-bit big-endian integer
    fn read_i16_be(&mut self) -> io::Result<i16>;
}

impl BytesExt for Bytes {
    fn read_cstr(&mut self) -> io::Result<String> {
        let null_pos = self
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no null terminator"))?;

        let s = String::from_utf8(self.slice(..null_pos).to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.advance(null_pos + 1);
        Ok(s)
    }

    fn read_i32_be(&mut self) -> io::Result<i32> {
        if self.remaining() < 4 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "not enough bytes"));
        }
        Ok(self.get_i32())
    }

    fn read_i16_be(&mut self) -> io::Result<i16> {
        if self.remaining() < 2 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "not enough bytes"));
        }
        Ok(self.get_i16())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_cstr() {
        let mut data = Bytes::from_static(b"hello\0world");
        assert_eq!(data.read_cstr().unwrap(), "hello");
        assert_eq!(data.read_cstr().unwrap(), "world");
    }

    #[test]
    fn test_read_i32() {
        let mut data = Bytes::from_static(&[0x00, 0x00, 0x01, 0x00]);
        assert_eq!(data.read_i32_be().unwrap(), 256);
    }
}
```

### 7. Create .gitignore

```
/target/
Cargo.lock
*.swp
*.swo
*~
.DS_Store
.idea/
.vscode/
```

### 8. Create .cargo/config.toml

```toml
[build]
rustflags = ["-D", "warnings"]

[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
```

### 9. Create examples/basic_stream.rs (skeleton)

```rust
//! Basic streaming example (placeholder for later phases)

use fraiseql_wire::{Error, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("fraiseql-wire v{}", fraiseql_wire::VERSION);
    println!("Example will be implemented in later phases");

    Ok(())
}
```

## Verification Commands

```bash
# Build the project
cargo build

# Run tests
cargo test

# Check formatting
cargo fmt -- --check

# Run clippy
cargo clippy -- -D warnings

# Run example
cargo run --example basic_stream

# Check documentation
cargo doc --no-deps --open
```

## Expected Output

### cargo build

```
   Compiling fraiseql-wire v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in X.XXs
```

### cargo test

```
running 3 tests
test util::oid::tests::test_json_oids ... ok
test util::bytes::tests::test_read_cstr ... ok
test util::bytes::tests::test_read_i32 ... ok

test result: ok. 3 passed; 0 failed; 0 ignored
```

### cargo clippy

```
    Checking fraiseql-wire v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in X.XXs
```

### cargo run --example basic_stream

```
fraiseql-wire v0.1.0
Example will be implemented in later phases
```

## Acceptance Criteria

* [ ] Project compiles without errors
* [ ] All unit tests pass
* [ ] No clippy warnings
* [ ] Code is formatted per rustfmt.toml
* [ ] Error types are defined and properly derive Debug/Display
* [ ] Utility functions have unit tests
* [ ] Example runs without panic
* [ ] Documentation builds without warnings

## DO NOT

* Add dependencies not listed (keep minimal)
* Implement protocol or streaming logic yet (later phases)
* Add TLS/SSL support (out of scope for MVP)
* Create database test fixtures (Phase 1+ concern)
* Write integration tests yet (no connection logic exists)

## Next Phase

**Phase 1: Protocol Foundation** — Implement minimal Postgres wire protocol (startup, authentication, simple query messages).
