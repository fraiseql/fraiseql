//! Role-Based Access Control (RBAC) system for GraphQL operations.
//!
//! This module provides:
//! - Role and permission models
//! - Hierarchical role resolution with PostgreSQL CTEs
//! - Multi-layer permission caching
//! - Field-level authorization enforcement
//! - GraphQL directive parsing

pub mod errors;
pub mod models;
pub mod hierarchy;
pub mod resolver;
pub mod cache;
pub mod directives;
pub mod field_auth;
pub mod py_bindings;

pub use errors::{RbacError, Result};
pub use models::{Role, Permission, UserRole, RolePermission};
pub use hierarchy::RoleHierarchy;
pub use resolver::PermissionResolver;
pub use cache::{PermissionCache, CacheInvalidation, CacheStats};
pub use directives::DirectiveExtractor;
pub use field_auth::{FieldAuthChecker, FieldPermissions};
pub use py_bindings::{PyPermissionResolver, PyFieldAuthChecker};
