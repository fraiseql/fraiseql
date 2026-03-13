//! Custom type registry for runtime scalar type management.
//!
//! This module provides a thread-safe registry for managing custom scalar types
//! defined in GraphQL schemas. The registry uses `Arc<RwLock<HashMap>>` for
//! concurrent read access with exclusive write access.
//!
//! # Architecture
//!
//! ```text
//! CustomTypeDef (metadata for one custom scalar)
//!     ↓
//! CustomTypeRegistry (manages multiple custom types)
//!     ↓
//! CompiledSchema (contains registry)
//!     ↓
//! Runtime Validation (executes validation rules)
//! ```
//!
//! # Example
//!
//! ```
//! use fraiseql_core::validation::{CustomTypeRegistry, CustomTypeDef};
//!
//! let registry = CustomTypeRegistry::new(Default::default());
//! let email_def = CustomTypeDef {
//!     name: "Email".to_string(),
//!     description: Some("Valid email address".to_string()),
//!     specified_by_url: None,
//!     validation_rules: vec![],
//!     elo_expression: Some("matches(value, /^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$/)".to_string()),
//!     base_type: None,
//! };
//! registry.register("Email".to_string(), email_def).unwrap();
//! assert!(registry.exists("Email"));
//! ```

mod config;
mod registry;

pub use config::{CustomTypeDef, CustomTypeRegistryConfig};
pub use registry::CustomTypeRegistry;

#[cfg(test)]
mod tests;
