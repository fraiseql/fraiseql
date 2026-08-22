//! Schema loading and management.

pub mod loader;

pub use loader::{CompiledSchemaLoader, ExtendedCompiledSchema, FunctionsConfig};

#[cfg(test)]
mod tests;
