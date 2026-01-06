//! RBAC-specific error types for better error handling.

use std::fmt;

/// Main RBAC error type
#[derive(Debug)]
pub enum RbacError {
    /// Database connection or query errors
    Database(String),

    /// Permission denied for specific resource:action
    PermissionDenied {
        /// Resource name (e.g., "user", "document")
        resource: String,
        /// Action name (e.g., "read", "write")
        action: String,
        /// Optional user ID who was denied
        user_id: Option<String>,
    },

    /// Missing required role
    MissingRole {
        /// Role name that was required
        required_role: String,
        /// Roles that the user currently has
        available_roles: Vec<String>,
    },

    /// User not found in RBAC system
    UserNotFound(String),

    /// Role not found
    RoleNotFound(String),

    /// Permission not found
    PermissionNotFound(String),

    /// Invalid permission format (expected "resource:action")
    InvalidPermissionFormat(String),

    /// Role hierarchy cycle detected
    HierarchyCycle(Vec<String>),

    /// Cache-related errors
    CacheError(String),

    /// Configuration errors
    ConfigError(String),

    /// GraphQL directive parsing errors
    DirectiveError(String),
}

/// Convenience type alias for RBAC operation results
pub type Result<T> = std::result::Result<T, RbacError>;

impl fmt::Display for RbacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(e) => write!(f, "Database error: {e}"),
            Self::PermissionDenied {
                resource,
                action,
                user_id,
            } => {
                if let Some(user) = user_id {
                    write!(f, "Permission denied: {resource}:{action} for user {user}")
                } else {
                    write!(f, "Permission denied: {resource}:{action}")
                }
            }
            Self::MissingRole {
                required_role,
                available_roles,
            } => {
                write!(
                    f,
                    "Missing required role '{required_role}'. Available roles: {available_roles:?}"
                )
            }
            Self::UserNotFound(user_id) => {
                write!(f, "User not found in RBAC system: {user_id}")
            }
            Self::RoleNotFound(role_name) => {
                write!(f, "Role not found: {role_name}")
            }
            Self::PermissionNotFound(perm) => {
                write!(f, "Permission not found: {perm}")
            }
            Self::InvalidPermissionFormat(perm) => {
                write!(
                    f,
                    "Invalid permission format '{perm}'. Expected 'resource:action'"
                )
            }
            Self::HierarchyCycle(roles) => {
                write!(f, "Role hierarchy cycle detected: {roles:?}")
            }
            Self::CacheError(msg) => write!(f, "Cache error: {msg}"),
            Self::ConfigError(msg) => write!(f, "Configuration error: {msg}"),
            Self::DirectiveError(msg) => write!(f, "Directive parsing error: {msg}"),
        }
    }
}

impl std::error::Error for RbacError {}

impl From<uuid::Error> for RbacError {
    fn from(error: uuid::Error) -> Self {
        Self::ConfigError(format!("UUID parsing error: {error}"))
    }
}

impl From<tokio_postgres::Error> for RbacError {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for RbacError {
    fn from(error: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        Self::Database(error.to_string())
    }
}

#[cfg(feature = "python")]
impl From<RbacError> for pyo3::PyErr {
    fn from(error: RbacError) -> Self {
        use pyo3::exceptions::{PyPermissionError, PyRuntimeError};

        match error {
            RbacError::PermissionDenied { .. } => PyPermissionError::new_err(error.to_string()),
            // All other variants use PyRuntimeError
            _ => PyRuntimeError::new_err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test Suite 1: Permission Denied Errors
    // ========================================================================

    #[test]
    fn test_permission_denied_error_with_user_id() {
        let error = RbacError::PermissionDenied {
            resource: "document".to_string(),
            action: "delete".to_string(),
            user_id: Some("user-123".to_string()),
        };

        let message = error.to_string();
        assert!(message.contains("Permission denied"));
        assert!(message.contains("document"));
        assert!(message.contains("delete"));
        assert!(message.contains("user-123"));
    }

    #[test]
    fn test_permission_denied_error_without_user_id() {
        let error = RbacError::PermissionDenied {
            resource: "document".to_string(),
            action: "delete".to_string(),
            user_id: None,
        };

        let message = error.to_string();
        assert!(message.contains("Permission denied"));
        assert!(message.contains("document"));
        assert!(message.contains("delete"));
        assert!(!message.contains("user-"), "Should not include user ID");
    }

    // ========================================================================
    // Test Suite 2: Missing Role Errors
    // ========================================================================

    #[test]
    fn test_missing_role_error() {
        let error = RbacError::MissingRole {
            required_role: "admin".to_string(),
            available_roles: vec!["user".to_string(), "viewer".to_string()],
        };

        let message = error.to_string();
        assert!(message.contains("Missing required role"));
        assert!(message.contains("admin"));
        assert!(message.contains("user"));
        assert!(message.contains("viewer"));
    }

    #[test]
    fn test_missing_role_with_no_available_roles() {
        let error = RbacError::MissingRole {
            required_role: "admin".to_string(),
            available_roles: vec![],
        };

        let message = error.to_string();
        assert!(message.contains("Missing required role"));
        assert!(message.contains("admin"));
    }

    // ========================================================================
    // Test Suite 3: Resource Not Found Errors
    // ========================================================================

    #[test]
    fn test_user_not_found_error() {
        let error = RbacError::UserNotFound("user-uuid-123".to_string());
        let message = error.to_string();

        assert!(message.contains("User not found"));
        assert!(message.contains("user-uuid-123"));
    }

    #[test]
    fn test_role_not_found_error() {
        let error = RbacError::RoleNotFound("admin".to_string());
        let message = error.to_string();

        assert!(message.contains("Role not found"));
        assert!(message.contains("admin"));
    }

    #[test]
    fn test_permission_not_found_error() {
        let error = RbacError::PermissionNotFound("document:delete".to_string());
        let message = error.to_string();

        assert!(message.contains("Permission not found"));
        assert!(message.contains("document:delete"));
    }

    // ========================================================================
    // Test Suite 4: Invalid Permission Format Errors
    // ========================================================================

    #[test]
    fn test_invalid_permission_format_error() {
        let error = RbacError::InvalidPermissionFormat("invalid-format".to_string());
        let message = error.to_string();

        assert!(message.contains("Invalid permission format"));
        assert!(message.contains("invalid-format"));
        assert!(message.contains("Expected 'resource:action'"));
    }

    // ========================================================================
    // Test Suite 5: Hierarchy Cycle Errors
    // ========================================================================

    #[test]
    fn test_hierarchy_cycle_error() {
        let cycle = vec![
            "admin".to_string(),
            "manager".to_string(),
            "admin".to_string(),
        ];
        let error = RbacError::HierarchyCycle(cycle.clone());
        let message = error.to_string();

        assert!(message.contains("Role hierarchy cycle detected"));
        assert!(message.contains("admin"));
        assert!(message.contains("manager"));
    }

    // ========================================================================
    // Test Suite 6: Cache & Config Errors
    // ========================================================================

    #[test]
    fn test_cache_error() {
        let error = RbacError::CacheError("Cache capacity exceeded".to_string());
        let message = error.to_string();

        assert!(message.contains("Cache error"));
        assert!(message.contains("Cache capacity exceeded"));
    }

    #[test]
    fn test_config_error() {
        let error = RbacError::ConfigError("Invalid JWT configuration".to_string());
        let message = error.to_string();

        assert!(message.contains("Configuration error"));
        assert!(message.contains("Invalid JWT configuration"));
    }

    #[test]
    fn test_database_error() {
        let error = RbacError::Database("Connection timeout".to_string());
        let message = error.to_string();

        assert!(message.contains("Database error"));
        assert!(message.contains("Connection timeout"));
    }

    // ========================================================================
    // Test Suite 7: Error Type Safety
    // ========================================================================

    #[test]
    fn test_error_implements_std_error_trait() {
        let error: Box<dyn std::error::Error> = Box::new(RbacError::PermissionDenied {
            resource: "document".to_string(),
            action: "read".to_string(),
            user_id: None,
        });

        // Should be able to call Error trait methods
        let _message = error.to_string();
    }

    #[test]
    fn test_multiple_error_types_can_be_collected() {
        let errors: Vec<RbacError> = vec![
            RbacError::UserNotFound("user1".to_string()),
            RbacError::RoleNotFound("admin".to_string()),
            RbacError::PermissionDenied {
                resource: "doc".to_string(),
                action: "read".to_string(),
                user_id: None,
            },
        ];

        assert_eq!(errors.len(), 3);
    }

    // ========================================================================
    // Test Suite 8: UUID Parsing Error Conversion
    // ========================================================================

    #[test]
    fn test_uuid_error_conversion_to_rbac_error() {
        let uuid_result = uuid::Uuid::parse_str("invalid-uuid");
        let error = uuid_result.err().unwrap();
        let rbac_error = RbacError::from(error);

        let message = rbac_error.to_string();
        assert!(message.contains("UUID parsing error"));
    }
}
