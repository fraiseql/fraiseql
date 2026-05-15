//! WASM component bindings for `fraiseql:host`.
//!
//! This module invokes `wasmtime::component::bindgen!` to generate Rust traits
//! from the WIT interface definition (`wit/fraiseql-host.wit`). The generated
//! traits are implemented by `StoreData` to provide host services to WASM guests.

#[allow(missing_docs)] // Reason: wasmtime bindgen-generated types do not have docs
#[allow(clippy::all, clippy::pedantic)] // Reason: generated code
#[allow(unsafe_code)] // Reason: wasmtime bindgen generates unsafe component model glue
mod generated {
    wasmtime::component::bindgen!({
        path: "wit/fraiseql-host.wit",
        imports: {
            default: async,
        },
    });
}

pub use generated::*;
