//! API endpoints for FraiseQL Server

/// API-key management (create / list / revoke / rotate, #627)
pub mod api_key_management;
/// Role and Permission Management API
pub mod rbac_management;
/// Per-tenant SAML IdP management API (#947)
#[cfg(feature = "auth-saml")]
pub mod saml_idp_management;
/// SCIM 2.0 provisioning endpoints (#946)
#[cfg(feature = "auth")]
pub mod scim;

pub use api_key_management::{ApiKeyManagementState, api_key_management_router};
pub use rbac_management::{RbacManagementState, rbac_management_router};
#[cfg(feature = "auth-saml")]
pub use saml_idp_management::{SamlIdpManagementState, saml_idp_management_router};
#[cfg(feature = "auth")]
pub use scim::{ScimState, ScimTokenManagementState, scim_router, scim_token_management_router};
