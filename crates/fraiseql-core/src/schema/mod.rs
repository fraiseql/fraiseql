//! Compiled Schema Types
//!
//! This module defines the Rust-owned schema representation that is compiled
//! from Python/TypeScript decorators at startup time.
//!
//! # Architecture
//!
//! ```text
//! Python/TypeScript                    Rust
//! ─────────────────                    ────
//! @fraiseql.type    ─┐
//! @fraiseql.query    ├─→ SchemaCompiler ─→ JSON ─→ CompiledSchema
//! @fraiseql.mutation─┘                              (Rust-owned)
//!                                                        │
//!                                                        ▼
//!                                              Axum serves requests
//!                                              (Python/TS irrelevant)
//! ```
//!
//! # Key Invariant
//!
//! After `CompiledSchema::from_json()`, all data is **Rust-owned**.
//! No Python/TypeScript objects are referenced during request handling.
//!
//! # Usage
//!
//! ```ignore
//! use fraiseql_core::schema::CompiledSchema;
//!
//! // From Python-compiled JSON
//! let schema = CompiledSchema::from_json(json_str)?;
//!
//! // From CLI config file
//! let schema = CompiledSchema::from_json(&std::fs::read_to_string("schema.json")?)?;
//!
//! // Access schema info
//! println!("Types: {}", schema.types.len());
//! println!("Queries: {}", schema.queries.len());
//! ```

mod compiled;
mod dependency_graph;
mod field_type;
mod introspection;

pub use dependency_graph::{ChangeImpact, CyclePath, SchemaDependencyGraph};
pub use compiled::{
    ArgumentDefinition, AutoParams, CompiledSchema, DirectiveDefinition, DirectiveLocationKind,
    EnumDefinition, EnumValueDefinition, FilterOperator, InputFieldDefinition,
    InputObjectDefinition, InterfaceDefinition, MutationDefinition, MutationOperation,
    ObserverDefinition, QueryDefinition, RetryConfig, RoleDefinition, SecurityConfig,
    SqlProjectionHint, StaticFilterCondition, SubscriptionDefinition, SubscriptionFilter,
    TypeDefinition, UnionDefinition,
};
pub use field_type::{
    DeprecationInfo, DistanceMetric, FieldDefinition, FieldType, VectorConfig, VectorIndexType,
};
pub use introspection::{
    DirectiveLocation, IntrospectionBuilder, IntrospectionDirective, IntrospectionEnumValue,
    IntrospectionField, IntrospectionInputValue, IntrospectionResponses, IntrospectionSchema,
    IntrospectionType, IntrospectionTypeRef, TypeKind,
};

#[cfg(test)]
mod tests;
