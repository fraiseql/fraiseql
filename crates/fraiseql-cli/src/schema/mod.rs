//! Schema format handling
//!
//! This module handles the intermediate schema format (language-agnostic)
//! and converts it to `CompiledSchema` (Rust-specific).

pub mod accent_insensitive;
pub mod artifact;
pub mod converter;
pub mod database_validator;
pub mod function_root_contract;
pub mod intermediate;
pub mod merger;
pub mod multi_file_loader;
pub mod mutation_contract;
pub mod native_joins;
pub mod optimizer;
pub mod pg_catalog;
pub mod seam;
pub mod update_input_defaults;
pub mod validator;

pub use artifact::CompiledArtifact;
pub use converter::{ConvertOptions, SchemaConverter};
pub use intermediate::{IntermediateScalar, IntermediateSchema};
pub use merger::SchemaMerger;
pub use multi_file_loader::MultiFileLoader;
pub use optimizer::{OptimizationReport, SchemaOptimizer};
pub use validator::SchemaValidator;

/// Every scalar type name the compiler recognizes, for the validator and the
/// converter to seed their known-type registries with.
///
/// Derived from `fraiseql_core::schema::BUILTIN_SCALARS` and `RICH_SCALARS`, the
/// tables `SchemaConverter::parse_field_type` and `FieldType::parse` read: a name
/// is a scalar exactly when `fraiseql_core::schema::is_known_scalar` says so. The two were
/// separately hand-maintained lists and drifted twice. First over spelling
/// (#724 item 2): this list held `"JSON"`, which the converter does not
/// recognize, while `"Json"` — the spelling every SDK emits — was missing here,
/// so one name compiled to a reference to a type that does not exist and the
/// other was reported as undeclared. Then over the vector types added in #959,
/// which the converter parsed and this list did not carry, so every schema
/// authoring a `BitVector` compiled with a warning advising the author to
/// declare a custom scalar shadowing a built-in. And a third time over the rich
/// scalars, which one of the two registries carried and the other did not, so
/// `Email` compiled as a field and was refused as a query argument.
pub(crate) fn known_scalar_names() -> impl Iterator<Item = &'static str> {
    fraiseql_core::schema::BUILTIN_SCALARS
        .iter()
        .map(|(name, _)| *name)
        .chain(fraiseql_core::schema::RICH_SCALARS.iter().copied())
}

#[cfg(test)]
mod tests;
