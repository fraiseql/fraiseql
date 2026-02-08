//! Schema format handling
//!
//! This module handles the intermediate schema format (language-agnostic)
//! and converts it to `CompiledSchema` (Rust-specific).

pub mod advanced_types;
pub mod converter;
pub mod intermediate;
pub mod lookup_data;
pub mod merger;
pub mod multi_file_loader;
pub mod optimizer;
pub mod rich_filters;
pub mod sql_templates;
pub mod validator;

pub use converter::SchemaConverter;
pub use intermediate::IntermediateSchema;
pub use merger::SchemaMerger;
pub use multi_file_loader::MultiFileLoader;
pub use optimizer::SchemaOptimizer;
pub use validator::SchemaValidator;
