//! Schema format handling
//!
//! This module handles the intermediate schema format (language-agnostic)
//! and converts it to `CompiledSchema` (Rust-specific).

pub mod converter;
pub mod intermediate;
pub mod optimizer;
pub mod validator;
pub mod merger;
pub mod multi_file_loader;

pub use converter::SchemaConverter;
pub use intermediate::IntermediateSchema;
pub use optimizer::SchemaOptimizer;
pub use validator::SchemaValidator;
pub use merger::SchemaMerger;
pub use multi_file_loader::MultiFileLoader;
