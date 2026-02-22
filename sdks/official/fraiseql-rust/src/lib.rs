//! fraiseql-rust — Rust authoring SDK for FraiseQL.
//!
//! Define GraphQL schemas in Rust using proc-macro attributes, then export
//! to `schema.json` for compilation by `fraiseql-cli`.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use fraiseql_rust::prelude::*;
//!
//! #[fraiseql_type]
//! struct User {
//!     id: i32,
//!     name: String,
//!     #[fraiseql(nullable)]
//!     email: Option<String>,
//! }
//!
//! #[fraiseql_query(sql_source = "v_users")]
//! fn users(limit: Option<i32>) -> Vec<User> {}
//!
//! fn main() {
//!     fraiseql_rust::export::export_schema("schema.json").unwrap();
//! }
//! ```
//!
//! # Modules
//!
//! - [`authz`] — RBAC/ABAC authorization primitives
//! - [`registry`] — global schema registry (populated by macros)
//! - [`export`] — serialize the registry to `schema.json` / `types.json`
//! - [`scalars`] — built-in FraiseQL scalar types

pub mod authz;
pub mod export;
pub mod registry;
pub mod scalars;

// Re-export proc macros at the crate root.
pub use fraiseql_rust_macros::{
    fraiseql_enum, fraiseql_input, fraiseql_mutation, fraiseql_query,
    fraiseql_subscription, fraiseql_type,
};

// Flatten the authz module at the crate root for convenience.
pub use authz::{
    AuthorizeBuilder, AuthorizeConfig, AuthzPolicyBuilder, AuthzPolicyConfig,
    AuthzPolicyType, Field, RoleMatchStrategy, RoleRequiredBuilder,
    RoleRequiredConfig, ScopeValidationError, validate_scope,
};

/// Convenience re-exports for `use fraiseql_rust::prelude::*`.
pub mod prelude {
    pub use crate::{
        fraiseql_enum, fraiseql_input, fraiseql_mutation, fraiseql_query,
        fraiseql_subscription, fraiseql_type,
    };
    pub use crate::authz::{
        AuthorizeBuilder, AuthzPolicyBuilder, AuthzPolicyType, Field,
        RoleMatchStrategy, RoleRequiredBuilder, validate_scope,
    };
    pub use crate::export::{export_schema, export_types};
    pub use crate::registry::SchemaRegistry;
    pub use crate::scalars::{
        Date, DateTime, Decimal, EmailAddress, ID, Json, Time, Uuid, Vector,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authz_exports_accessible() {
        let _authorize = AuthorizeBuilder::new();
        let _role = RoleRequiredBuilder::new();
        let _policy = AuthzPolicyBuilder::new("test");
    }

    #[test]
    fn test_export_produces_valid_json() {
        let json = export::schema_to_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["version"], "2.0");
    }
}
