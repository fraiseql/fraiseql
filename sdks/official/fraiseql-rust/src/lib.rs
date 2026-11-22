#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! FraiseQL Rust: schema authoring for FraiseQL.
//!
//! Registers types with their fields and field-level authorization, and observers,
//! and exports them as `schema.json` for `fraiseql compile`. What it can author,
//! construct by construct, is the `fraiseql-rust/` row of the support matrix in
//! `sdks/official/README.md`.

pub mod authorization;
pub mod field;
pub mod observer;
pub mod policies;
pub mod roles;
pub mod schema;

pub use authorization::{AuthorizeBuilder, AuthorizeConfig};
pub use field::{Deprecation, Field, VectorConfig, VectorIndex, VectorMetric};
pub use observer::{
    BackoffStrategy, DuplicateObserver, EmailAction, Observer, ObserverAction, ObserverEvent,
    ObserverRetry, SlackAction, SlackDestination, WebhookAction, WebhookDestination,
};
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
