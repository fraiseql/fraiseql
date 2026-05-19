//! WASM component bindings for `fraiseql:host`.
//!
//! This module contains types corresponding to the `wit/fraiseql-host.wit` interface definition.
//!
//! The actual trait implementations (for logging, context, and I/O interfaces) are provided by
//! `StoreData` and registered via `wasmtime::component::bindgen!`.
//! This module defines the type mappings between WIT and Rust.

use serde::{Deserialize, Serialize};

/// Log level enum matching the WIT definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    /// Debug level.
    Debug,
    /// Info level.
    Info,
    /// Warn level.
    Warn,
    /// Error level.
    Error,
}

/// HTTP response structure matching the WIT definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    pub body: Vec<u8>,
}

/// Generated bindings module documentation.
///
/// Type mappings are defined in this module. Trait implementations are in `store.rs`.
///
/// The `wasmtime::component::bindgen!` macro generates:
///
/// ```ignore
/// pub mod fraiseql_host {
///     pub trait Host: Send {
///         // Logging interface
///         type Logging: logging::Host;
///         fn logging(&mut self) -> &mut Self::Logging;
///
///         // Context interface
///         type Context: context::Host;
///         fn context(&mut self) -> &mut Self::Context;
///
///         // I/O interface (stubs)
///         type Io: io::Host;
///         fn io(&mut self) -> &mut Self::Io;
///     }
/// }
/// ```
///
/// These traits are implemented by `StoreData` to delegate to host functions
/// defined in `store.rs`. Host function implementations are complete for logging and context;
/// I/O operations are stubs pending full host bridge wiring.
mod _bindings_doc {}
