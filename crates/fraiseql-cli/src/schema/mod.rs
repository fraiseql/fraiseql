//! Schema format handling
//!
//! This module handles the intermediate schema format (language-agnostic)
//! and converts it to CompiledSchema (Rust-specific).

pub mod converter;
pub mod intermediate;
pub mod optimizer;
pub mod validator;

pub use converter::SchemaConverter;
pub use intermediate::IntermediateSchema;
pub use optimizer::{OptimizationReport, SchemaOptimizer};
pub use validator::{SchemaValidator, ValidationReport};
