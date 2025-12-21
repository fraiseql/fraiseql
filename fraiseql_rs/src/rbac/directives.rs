//! GraphQL directive enforcement (@requiresRole, @requiresPermission).

use super::{
    errors::{RbacError, Result as RbacResult},
    field_auth::FieldPermissions,
};
use crate::graphql::types::{Directive, FieldSelection, ParsedQuery};

/// Extract RBAC directives from parsed query
pub struct DirectiveExtractor;

impl DirectiveExtractor {
    /// Extract all field permissions from parsed query
    pub fn extract_field_permissions(
        query: &ParsedQuery,
    ) -> RbacResult<Vec<(String, FieldPermissions)>> {
        let mut field_permissions = Vec::new();

        for selection in &query.selections {
            Self::extract_from_selection(selection, &mut field_permissions, Vec::new())?;
        }

        Ok(field_permissions)
    }

    /// Recursively extract permissions from field selection
    fn extract_from_selection(
        selection: &FieldSelection,
        permissions: &mut Vec<(String, FieldPermissions)>,
        path: Vec<String>,
    ) -> RbacResult<()> {
        let mut current_path = path;
        current_path.push(selection.name.clone());

        // Extract directives for this field
        let field_perms = Self::extract_directives(&selection.directives)?;
        if field_perms.has_requirements() {
            let field_path = current_path.join(".");
            permissions.push((field_path, field_perms));
        }

        // Recursively process nested fields
        for nested in &selection.nested_fields {
            Self::extract_from_selection(nested, permissions, current_path.clone())?;
        }

        Ok(())
    }

    /// Parse directives into FieldPermissions
    fn extract_directives(directives: &[Directive]) -> RbacResult<FieldPermissions> {
        let mut permissions = FieldPermissions::default();

        for directive in directives {
            match directive.name.as_str() {
                "requiresRole" => {
                    // Parse role argument: @requiresRole(role: "admin")
                    if let Some(role_arg) = directive.arguments.iter().find(|arg| arg.name == "role") {
                        // Extract the role value from the JSON string
                        let role_value = serde_json::from_str::<serde_json::Value>(&role_arg.value_json)
                            .map_err(|_| RbacError::DirectiveError(
                                "Invalid role argument format".to_string(),
                            ))?;

                        if let Some(role_str) = role_value.as_str() {
                            permissions.required_roles.push(role_str.to_string());
                        } else {
                            return Err(RbacError::DirectiveError(
                                "Role argument must be a string".to_string(),
                            ));
                        }
                    } else {
                        return Err(RbacError::DirectiveError(
                            "@requiresRole directive must have a 'role' argument".to_string(),
                        ));
                    }
                }
                "requiresPermission" => {
                    // Parse permission argument: @requiresPermission(permission: "user:read")
                    if let Some(perm_arg) = directive.arguments.iter().find(|arg| arg.name == "permission") {
                        // Extract the permission value from the JSON string
                        let perm_value = serde_json::from_str::<serde_json::Value>(&perm_arg.value_json)
                            .map_err(|_| RbacError::DirectiveError(
                                "Invalid permission argument format".to_string(),
                            ))?;

                        if let Some(perm_str) = perm_value.as_str() {
                            permissions.required_permissions.push(perm_str.to_string());
                        } else {
                            return Err(RbacError::DirectiveError(
                                "Permission argument must be a string".to_string(),
                            ));
                        }
                    } else {
                        return Err(RbacError::DirectiveError(
                            "@requiresPermission directive must have a 'permission' argument".to_string(),
                        ));
                    }
                }
                "requiresAllRoles" => {
                    // Parse multiple roles: @requiresAllRoles(roles: ["admin", "moderator"])
                    if let Some(roles_arg) = directive.arguments.iter().find(|arg| arg.name == "roles") {
                        let roles_value = serde_json::from_str::<serde_json::Value>(&roles_arg.value_json)
                            .map_err(|_| RbacError::DirectiveError(
                                "Invalid roles argument format".to_string(),
                            ))?;

                        if let Some(roles_array) = roles_value.as_array() {
                            for role_value in roles_array {
                                if let Some(role_str) = role_value.as_str() {
                                    permissions.required_roles.push(role_str.to_string());
                                }
                            }
                        } else {
                            return Err(RbacError::DirectiveError(
                                "Roles argument must be an array of strings".to_string(),
                            ));
                        }
                    } else {
                        return Err(RbacError::DirectiveError(
                            "@requiresAllRoles directive must have a 'roles' argument".to_string(),
                        ));
                    }
                }
                "requiresAnyPermission" => {
                    // Parse multiple permissions: @requiresAnyPermission(permissions: ["user:read", "user:write"])
                    if let Some(perms_arg) = directive.arguments.iter().find(|arg| arg.name == "permissions") {
                        let perms_value = serde_json::from_str::<serde_json::Value>(&perms_arg.value_json)
                            .map_err(|_| RbacError::DirectiveError(
                                "Invalid permissions argument format".to_string(),
                            ))?;

                        if let Some(perms_array) = perms_value.as_array() {
                            for perm_value in perms_array {
                                if let Some(perm_str) = perm_value.as_str() {
                                    permissions.required_permissions.push(perm_str.to_string());
                                }
                            }
                        } else {
                            return Err(RbacError::DirectiveError(
                                "Permissions argument must be an array of strings".to_string(),
                            ));
                        }
                    } else {
                        return Err(RbacError::DirectiveError(
                            "@requiresAnyPermission directive must have a 'permissions' argument".to_string(),
                        ));
                    }
                }
                _ => {
                    // Ignore other directives (like @include, @skip, @deprecated)
                }
            }
        }

        Ok(permissions)
    }
}
