//! Shared modules: the code a Deno function may import.
//!
//! A function is one file, `<module_dir>/<name>.<ext>`. Code shared between
//! functions lives in one declared directory, `[functions] shared_dir`, and is
//! imported as `@shared/<path>`. Nothing else is importable: not a URL, not a
//! package scheme, not another function's file.

pub mod layout;

pub use layout::{SharedDirError, check_shared_dir};
