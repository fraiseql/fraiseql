//! Schema loading and management.

pub mod loader;

pub use loader::{
    CompiledSchemaLoader, ExtendedCompiledSchema, FunctionsConfig, SchemaBucketDef,
    SchemaStorageConfig,
};

#[cfg(test)]
mod tests;
