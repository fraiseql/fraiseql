//! Shared modules: the code a Deno function may import.
//!
//! A function is one file, `<module_dir>/<name>.<ext>`. Code shared between
//! functions lives in one declared directory, `[functions] shared_dir`, and is
//! imported as `@shared/<path>`. Nothing else is importable: not a URL, not a
//! package scheme, not another function's file.
//!
//! - [`resolve`]: the import rule, lexical and then on disk.
//! - `graph`: every shared module a function reaches, read once at load (`runtime-deno`, which
//!   parses the imports).
//! - [`hash`]: the function's integrity hash over its source and those modules.
//! - [`layout`]: where the shared directory may be.

#[cfg(feature = "runtime-deno")]
pub mod graph;
pub mod hash;
pub mod layout;
pub mod resolve;

pub use layout::{SharedDirError, check_shared_dir};

/// A function's shared modules, as the isolate runs them (transpiled `JavaScript`), by
/// key.
pub type SharedModules = std::collections::BTreeMap<resolve::ModuleKey, String>;
