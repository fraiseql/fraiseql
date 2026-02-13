//! API endpoints for FraiseQL Server

/// Role and Permission Management API
pub mod rbac_management;

pub use rbac_management::{RbacManagementState, rbac_management_router};
