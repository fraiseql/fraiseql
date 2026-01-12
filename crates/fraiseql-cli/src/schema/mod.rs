//! Schema format handling
//!
//! This module handles the intermediate schema format (language-agnostic)
//! and converts it to CompiledSchema (Rust-specific).

pub mod converter;
pub mod intermediate;

pub use converter::SchemaConverter;
pub use intermediate::IntermediateSchema;
