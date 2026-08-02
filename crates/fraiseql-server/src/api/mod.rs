//! API endpoints for FraiseQL Server

/// API-key management (create / list / revoke / rotate, #627)
pub mod api_key_management;
/// Role and Permission Management API
pub mod rbac_management;

pub use api_key_management::{ApiKeyManagementState, api_key_management_router};
pub use rbac_management::{RbacManagementState, rbac_management_router};
