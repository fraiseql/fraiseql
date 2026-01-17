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
