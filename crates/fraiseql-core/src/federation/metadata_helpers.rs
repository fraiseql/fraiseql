//! Helper functions for working with federation metadata.
//!
//! Provides common patterns for metadata lookups and error handling.

use crate::{
    error::{FraiseQLError, Result},
    federation::types::{FederatedType, FederationMetadata, KeyDirective},
};

/// Find a federation type by name in the metadata.
///
/// # Errors
///
/// Returns error if type is not found in metadata.
///
/// # Examples
///
/// ```ignore
/// let fed_type = find_federation_type("User", &metadata)?;
/// ```
pub fn find_federation_type<'a>(
    typename: &str,
    metadata: &'a FederationMetadata,
) -> Result<&'a FederatedType> {
    metadata
        .types
        .iter()
        .find(|t| t.name == typename)
        .ok_or_else(|| FraiseQLError::Validation {
            message: format!("Type '{}' not found in federation metadata", typename),
            path:    None,
        })
}

/// Get the primary key directive for a federation type.
///
/// Uses the first @key directive defined on the type.
///
/// # Errors
///
/// Returns error if type has no @key directive.
pub fn get_key_directive(fed_type: &FederatedType) -> Result<&KeyDirective> {
    fed_type.keys.first().ok_or_else(|| FraiseQLError::Validation {
        message: format!("Type '{}' has no @key directive", fed_type.name),
        path:    None,
    })
}

/// Find a federation type and its primary key directive.
///
/// Convenience function combining find_federation_type and get_key_directive.
///
/// # Errors
///
/// Returns error if type not found or has no @key directive.
pub fn find_type_with_key<'a>(
    typename: &str,
    metadata: &'a FederationMetadata,
) -> Result<(&'a FederatedType, &'a KeyDirective)> {
    let fed_type = find_federation_type(typename, metadata)?;
    let key_directive = get_key_directive(fed_type)?;
    Ok((fed_type, key_directive))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::types::KeyDirective;

    fn make_test_metadata() -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![FederatedType {
                name:             "User".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            }],
        }
    }

    #[test]
    fn test_find_federation_type_success() {
        let metadata = make_test_metadata();
        let result = find_federation_type("User", &metadata);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "User");
    }

    #[test]
    fn test_find_federation_type_not_found() {
        let metadata = make_test_metadata();
        let result = find_federation_type("Order", &metadata);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_key_directive_success() {
        let metadata = make_test_metadata();
        let fed_type = find_federation_type("User", &metadata).unwrap();
        let result = get_key_directive(fed_type);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().fields, vec!["id".to_string()]);
    }

    #[test]
    fn test_find_type_with_key_success() {
        let metadata = make_test_metadata();
        let result = find_type_with_key("User", &metadata);
        assert!(result.is_ok());
        let (fed_type, key_dir) = result.unwrap();
        assert_eq!(fed_type.name, "User");
        assert_eq!(key_dir.fields[0], "id");
    }

    #[test]
    fn test_find_type_with_key_not_found() {
        let metadata = make_test_metadata();
        let result = find_type_with_key("NonExistent", &metadata);
        assert!(result.is_err());
    }
}
