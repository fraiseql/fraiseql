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
//! - [`dirs`]: the directories as a deployment resolves them, environment overrides applied.
//! - [`load`]: the one loader, the server's and `fraiseql functions invoke`'s.

pub mod dirs;
#[cfg(feature = "runtime-deno")]
pub mod graph;
pub mod hash;
pub mod layout;
pub mod load;
pub mod resolve;

pub use dirs::apply_env_overrides;
pub use layout::{SharedDirError, check_shared_dir};
pub use load::{LoadError, load_function_module};

/// A function's shared modules, as the isolate runs them (transpiled `JavaScript`), by
/// key.
pub type SharedModules = std::collections::BTreeMap<resolve::ModuleKey, String>;
