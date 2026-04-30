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
//! ```no_run
//! // Requires: a compiled schema JSON file from `fraiseql-cli compile`.
//! use fraiseql_core::schema::CompiledSchema;
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let json_str = r#"{"types":[],"queries":[]}"#;
//! // From schema.compiled.json produced by `fraiseql-cli compile`
//! let schema = CompiledSchema::from_json(json_str)?;
//!
//! // From CLI config file
//! let schema = CompiledSchema::from_json(&std::fs::read_to_string("schema.json")?)?;
//!
//! // Access schema info
//! println!("Types: {}", schema.types.len());
//! println!("Queries: {}", schema.queries.len());
//! # Ok(())
//! # }
//! ```

mod compiled;
mod config_types;
mod dependency_graph;
pub mod domain_types;
mod field_type;
mod graphql_type_defs;
pub mod graphql_value;
mod introspection;
pub mod introspection_types;
mod observer_types;
mod scalar_types;
pub mod security_config;
mod subscription_types;

pub use compiled::{
    ArgumentDefinition, AutoParams, CURRENT_SCHEMA_FORMAT_VERSION, CompiledSchema, CursorType,
    DirectiveDefinition, DirectiveLocationKind, MutationDefinition, MutationOperation,
    QueryDefinition, is_safe_sql_identifier,
};
pub use config_types::{
    AuthorizationPolicy, AuthorizationRule, Cardinality, CircuitBreakerConfig,
    CompiledSecurityConfig, CrudNamingConfig, CrudNamingPreset, DebugConfig, DeleteResponse,
    EnterpriseSecurityConfig, EntityCircuitBreakerOverride, EventHandler, FederationConfig,
    FederationEntity, FieldAuthRule, GrpcConfig, McpConfig, NamingConvention, ObserversConfig,
    Relationship, RestConfig, SessionVariableMapping, SessionVariableSource,
    SessionVariablesConfig, SubscriptionHooksConfig, SubscriptionsConfig, ValidationConfig,
};
pub use dependency_graph::{ChangeImpact, CyclePath, SchemaDependencyGraph};
pub use field_type::{
    DeprecationInfo, DistanceMetric, FieldDefinition, FieldDenyPolicy, FieldEncryptionConfig,
    FieldType, VectorConfig, VectorIndexType,
};
pub use graphql_type_defs::{
    EnumDefinition, EnumValueDefinition, InputFieldDefinition, InputObjectDefinition,
    InterfaceDefinition, SqlProjectionHint, TypeDefinition, UnionDefinition,
};
pub use graphql_value::GraphQLValue;
pub use introspection::{
    DirectiveLocation, IntrospectionBuilder, IntrospectionDirective, IntrospectionEnumValue,
    IntrospectionField, IntrospectionInputValue, IntrospectionResponses, IntrospectionSchema,
    IntrospectionType, IntrospectionTypeRef, IntrospectionValidationRule, TypeKind,
};
pub use observer_types::{ObserverDefinition, RetryConfig};
pub use scalar_types::{BUILTIN_SCALARS, RICH_SCALARS, is_known_scalar};
pub use security_config::{
    InjectedParamSource, RoleDefinition, SecurityConfig, TenancyConfig, TenancyMode,
};
pub use subscription_types::{
    FilterOperator, StaticFilterCondition, SubscriptionDefinition, SubscriptionFilter,
};

#[cfg(test)]
mod tests;
