//! RBAC and authorization primitives for FraiseQL schema authoring.
//!
//! These types are used at schema-definition time to attach access-control
//! metadata to fields, types, and operations. They are serialised into
//! `schema.json` and enforced at runtime by `fraiseql-server`.

pub mod authorization;
pub mod field;
pub mod policies;
pub mod roles;
pub mod scope;

pub use authorization::{AuthorizeBuilder, AuthorizeConfig};
pub use field::Field;
pub use policies::{AuthzPolicyBuilder, AuthzPolicyConfig, AuthzPolicyType};
pub use roles::{RoleMatchStrategy, RoleRequiredBuilder, RoleRequiredConfig};
pub use scope::{SchemaRegistry, ScopeValidationError, validate_scope};
