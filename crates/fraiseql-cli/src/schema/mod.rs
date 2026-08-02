//! Schema format handling
//!
//! This module handles the intermediate schema format (language-agnostic)
//! and converts it to `CompiledSchema` (Rust-specific).

pub mod advanced_types;
pub mod converter;
pub mod database_validator;
pub mod intermediate;
pub mod merger;
pub mod multi_file_loader;
pub mod mutation_contract;
pub mod optimizer;
pub mod pg_catalog;
pub mod seam;
pub mod validator;

pub use converter::{ConvertOptions, SchemaConverter};
pub use intermediate::{IntermediateScalar, IntermediateSchema};
pub use merger::SchemaMerger;
pub use multi_file_loader::MultiFileLoader;
pub use optimizer::{OptimizationReport, SchemaOptimizer};
pub use validator::SchemaValidator;

/// GraphQL built-in scalar type names.
///
/// Used by the validator and converter to seed the known-type registry so
/// fields typed as these names are never flagged as unknown.
/// Every scalar type name the compiler recognizes.
///
/// **This must agree with `SchemaConverter::parse_field_type`**, which is the only other
/// place that decides whether a name is a scalar or an object-type reference. The two were
/// separately hand-maintained and had drifted: this list held six names including `"JSON"`,
/// while the converter matched twelve including `"Json"` — so a field typed `Json` (the
/// spelling every SDK emits) was not a known scalar here, and a field typed `JSON` compiled
/// to `FieldType::Object("JSON")`, a reference to a type that does not exist. Both were
/// masked because the validator used to register every field's type name as an implicit
/// custom scalar (#724 item 2).
///
/// `builtin_scalar_names_match_the_converter` in `converter::tests` fails the build if the
/// two ever disagree again.
pub(crate) const BUILTIN_SCALAR_NAMES: &[&str] = &[
    "String", "Int", "Float", "Boolean", "ID", "DateTime", "Date", "Time", "Json", "UUID",
    "Decimal", "Vector",
];

#[cfg(test)]
mod tests;
