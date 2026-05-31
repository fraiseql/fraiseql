//! Code generation for FraiseQL.
//!
//! This crate turns a [`fraiseql_core::schema::CompiledSchema`] into source-code
//! artefacts that *consumers* of a FraiseQL API use to call it in a type-safe way.
//!
//! # Scope
//!
//! - [`client`] — generates code consumed *by callers of* a FraiseQL API (typed
//!   query/mutation/subscription clients from the compiled schema).
//!
//! The crate is deliberately **filesystem-free**: every generator takes a
//! `&CompiledSchema` and returns a [`Generated`] map of relative path → file
//! content. Callers (the CLI, IDE extensions, build plugins) decide where to write.
//!
//! # Why this is separate from `fraiseql generate`
//!
//! The CLI's `generate <language>` command emits **authoring** code — FraiseQL
//! type/query definitions in another language, fed *back into* the compiler. It
//! operates on the CLI's compile-pipeline IR (`IntermediateSchema`), not on a
//! `CompiledSchema`. That family of generators stays in `fraiseql-cli` because it
//! is part of the compile pipeline, not the consumer-client domain. This crate
//! consumes the *output* of compilation (`CompiledSchema`) to build clients —
//! the inverse direction. Keeping the two apart avoids coupling a
//! `CompiledSchema`-based crate to the compiler's internal IR.

use std::{collections::BTreeMap, path::PathBuf};

pub use fraiseql_error::FraiseQLError;

/// Result alias for code-generation operations.
pub type Result<T> = std::result::Result<T, FraiseQLError>;

pub mod client;

/// A bundle of generated files: relative path → file content.
///
/// Callers decide where to write them; the generators in this crate never touch
/// the filesystem. The map is ordered (`BTreeMap`) so generated output is
/// deterministic across runs — important for snapshot tests and CI diffing.
pub type Generated = BTreeMap<PathBuf, String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_is_an_ordered_path_to_content_map() {
        let mut generated = Generated::new();
        generated.insert(PathBuf::from("b.ts"), "second".to_string());
        generated.insert(PathBuf::from("a.ts"), "first".to_string());

        // BTreeMap iteration order is sorted by key, giving deterministic output.
        let paths: Vec<_> = generated.keys().map(|p| p.to_string_lossy().into_owned()).collect();
        assert_eq!(paths, vec!["a.ts", "b.ts"]);
    }
}
