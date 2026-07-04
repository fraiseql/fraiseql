//! Object-safe host context bridge for WASM store data.
//!
//! The [`DynHostContext`] trait now lives in [`crate::host::dyn_context`] so it can
//! be shared by both the WASM and Deno runtime backends. This module re-exports it
//! for the WASM store code (and existing callers) that reference the historical
//! `runtime::wasm::host_bridge` path.

pub use crate::host::dyn_context::{BoxFuture, DynHostContext};
