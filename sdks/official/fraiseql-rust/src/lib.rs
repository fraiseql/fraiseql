#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! FraiseQL Rust - Security module with 100% feature parity
//!
//! Provides declarative, type-safe authorization and security configuration
//! across 10 authoring languages.

pub mod authorization;
pub mod field;
pub mod policies;
pub mod roles;
pub mod schema;

pub use authorization::{AuthorizeBuilder, AuthorizeConfig};
pub use field::Field;
pub use policies::{AuthzPolicyBuilder, AuthzPolicyConfig, AuthzPolicyType};
pub use roles::{RoleMatchStrategy, RoleRequiredBuilder, RoleRequiredConfig};
pub use schema::{validate_scope, SchemaRegistry, ScopeValidationError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        let _authorize = AuthorizeBuilder::new();
        let _role = RoleRequiredBuilder::new();
        let _policy = AuthzPolicyBuilder::new("test");
    }
}
