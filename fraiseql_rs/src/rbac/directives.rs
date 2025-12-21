//! GraphQL directive enforcement (@requiresRole, @requiresPermission).

use crate::graphql::types::{ParsedQuery, FieldSelection};
use super::{errors::{Result as RbacResult, RbacError}, field_auth::FieldPermissions};

/// Extract RBAC directives from parsed query
pub struct DirectiveExtractor;

impl DirectiveExtractor {
    /// Extract all field permissions from parsed query
    pub fn extract_field_permissions(query: &ParsedQuery) -> RbacResult<Vec<(String, FieldPermissions)>> {
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
    fn extract_directives(directives: &[String]) -> RbacResult<FieldPermissions> {
        let mut permissions = FieldPermissions::default();

        // Note: Current FieldSelection.directives only contains names.
        // This is a simplified implementation. Full implementation would need
        // to extend the GraphQL parser to capture directive arguments.

        // For Phase 11, we'll implement a basic version that assumes
        // directives are applied at schema level, not query level.
        // Phase 12 will add full directive parsing with arguments.

        for directive in directives {
            match directive.as_str() {
                "requiresRole" => {
                    // TODO: Parse role argument from directive
                    // For now, this is a placeholder for schema-level directives
                    // In full implementation: @requiresRole(role: "admin")
                    return Err(RbacError::DirectiveError(
                        "requiresRole directive parsing not implemented yet".to_string()
                    ));
                }
                "requiresPermission" => {
                    // TODO: Parse permission argument from directive
                    // For now, this is a placeholder for schema-level directives
                    // In full implementation: @requiresPermission(permission: "user:read")
                    return Err(RbacError::DirectiveError(
                        "requiresPermission directive parsing not implemented yet".to_string()
                    ));
                }
                _ => {
                    // Ignore other directives (like @include, @skip)
                }
            }
        }

        Ok(permissions)
    }
}
