//! Schema loading and management.

pub mod loader;

pub use loader::CompiledSchemaLoader;

#[cfg(test)]
mod tests;
