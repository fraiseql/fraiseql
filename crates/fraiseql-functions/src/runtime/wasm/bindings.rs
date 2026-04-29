//! WASM component bindings for `fraiseql:host`.
//!
//! This module contains types corresponding to the `wit/fraiseql-host.wit` interface definition.
//!
//! The actual trait implementations (for logging, context, and I/O interfaces) are provided by
//! `StoreData` and will be registered via `wasmtime::component::bindgen!` in Phase 5B.
//! For now, this module defines the type mappings between WIT and Rust.

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

/// # Generated Bindings (Phase 5B)
///
/// **Cycle 5 Status**: Type mappings are defined here. Trait implementations are in `store.rs`.
///
/// The `wasmtime::component::bindgen!` macro (to be invoked in Phase 5B) will generate:
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
///         // I/O interface (stubs for now)
///         type Io: io::Host;
///         fn io(&mut self) -> &mut Self::Io;
///     }
/// }
/// ```
///
/// These traits will be implemented by `StoreData` to delegate to host functions
/// defined in `store.rs`. Host function implementations are complete for logging and context;
/// I/O operations remain stubs for Phase 5B.
mod _phase_5b {}
