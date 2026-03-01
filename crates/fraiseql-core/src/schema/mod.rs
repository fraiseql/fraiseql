//! Compiled Schema Types
//!
//! This module defines the Rust-owned schema representation loaded from a compiled
//! schema at startup time.
//!
//! # Architecture
//!
//! ```text
//! Authoring language                   Rust
//! (Python, TS, Go, …)                 ────
//! @fraiseql.type    ─┐
//! @fraiseql.query    ├─→ schema.json ─→ fraiseql-cli compile ─→ CompiledSchema
//! @fraiseql.mutation─┘                                           (Rust-owned)
//!                                                                      │
//!                                                                      ▼
//!                                                            Axum serves requests
//!                                                    (authoring language not involved)
//! ```
//!
//! # Key Invariant
//!
//! After `CompiledSchema::from_json()`, all data is **Rust-owned**.
//! No authoring-language objects are referenced during request handling.
//!
//! # Usage
//!
//! ```ignore
//! use fraiseql_core::schema::CompiledSchema;
//!
//! // From schema.compiled.json produced by `fraiseql-cli compile`
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
mod config_types;
mod dependency_graph;
mod field_type;
mod introspection;
mod scalar_types;

pub use compiled::{
    ArgumentDefinition, AutoParams, CompiledSchema, CursorType, DirectiveDefinition,
    DirectiveLocationKind, EnumDefinition, EnumValueDefinition, FilterOperator,
    InjectedParamSource, InputFieldDefinition, InputObjectDefinition, InterfaceDefinition,
    MutationDefinition, MutationOperation, ObserverDefinition, QueryDefinition, RetryConfig,
    RoleDefinition, SecurityConfig, SqlProjectionHint, StaticFilterCondition,
    SubscriptionDefinition, SubscriptionFilter, TypeDefinition, UnionDefinition,
};
pub use config_types::{
    AuthorizationPolicy, AuthorizationRule, CircuitBreakerConfig, CompiledSecurityConfig,
    EnterpriseSecurityConfig, EntityCircuitBreakerOverride, EventHandler, FederationConfig,
    FederationEntity, FieldAuthRule, ObserversConfig,
};
pub use dependency_graph::{ChangeImpact, CyclePath, SchemaDependencyGraph};
pub use field_type::{
    DeprecationInfo, DistanceMetric, FieldDefinition, FieldDenyPolicy, FieldEncryptionConfig,
    FieldType, VectorConfig, VectorIndexType,
};
pub use introspection::{
    DirectiveLocation, IntrospectionBuilder, IntrospectionDirective, IntrospectionEnumValue,
    IntrospectionField, IntrospectionInputValue, IntrospectionResponses, IntrospectionSchema,
    IntrospectionType, IntrospectionTypeRef, IntrospectionValidationRule, TypeKind,
};
pub use scalar_types::{BUILTIN_SCALARS, RICH_SCALARS, is_known_scalar};

#[cfg(test)]
mod tests;
