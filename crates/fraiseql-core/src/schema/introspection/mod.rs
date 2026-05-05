//! GraphQL introspection types per GraphQL spec §4.1-4.2.
//!
//! This module provides standard GraphQL introspection support, enabling
//! tools like Apollo Sandbox, `GraphiQL`, and Altair to query the schema.
//!
//! # Architecture
//!
//! FraiseQL generates introspection responses at **compile time** for performance.
//! The `IntrospectionSchema` is built from `CompiledSchema` and cached.
//!
//! # Supported Queries
//!
//! - `__schema` - Returns the full schema introspection
//! - `__type(name: String!)` - Returns a specific type's introspection
//! - `__typename` - Handled at projection level, not here
//!
//! # Module Layout
//!
//! | Sub-module | Responsibility |
//! |---|---|
//! | `types` | All `__*` introspection structs and enums |
//! | `field_resolver` | `FieldType` → `IntrospectionType` conversion, validation rules |
//! | `type_resolver` | Per-type builders (object, enum, input, interface, union, scalars) |
//! | `directive_builder` | Built-in and custom directive definitions |
//! | `schema_builder` | Root type builders, `IntrospectionBuilder`, `IntrospectionResponses` |

mod directive_builder;
mod field_resolver;
mod schema_builder;
mod type_resolver;
mod types;

// Re-export the complete public API (unchanged from the old flat module).
pub use schema_builder::{IntrospectionBuilder, IntrospectionResponses};
pub use types::{
    DirectiveLocation, IntrospectionDirective, IntrospectionEnumValue, IntrospectionField,
    IntrospectionInputValue, IntrospectionSchema, IntrospectionType, IntrospectionTypeRef,
    IntrospectionValidationRule, TypeKind,
};

#[cfg(test)]
mod tests;
