#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! FraiseQL Rust - Security module with 100% feature parity
//!
//! Provides declarative, type-safe authorization and security configuration
//! across 10 authoring languages.

pub mod authorization;
pub mod roles;
pub mod policies;

pub use authorization::{AuthorizeConfig, AuthorizeBuilder};
pub use roles::{RoleMatchStrategy, RoleRequiredConfig, RoleRequiredBuilder};
pub use policies::{AuthzPolicyType, AuthzPolicyConfig, AuthzPolicyBuilder};

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
