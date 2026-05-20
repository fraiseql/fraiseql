//! Helper functions for working with federation metadata.
//!
//! Provides common patterns for metadata lookups and error handling.

use fraiseql_error::{FraiseQLError, Result};

use crate::types::{FederatedType, FederationMetadata, KeyDirective};

/// Find a federation type by name in the metadata.
///
/// # Errors
///
/// Returns error if type is not found in metadata.
///
/// # Examples
///
/// ```text
/// // Requires: a populated FederationMetadata struct.
/// // See: tests/integration/ for runnable examples.
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
            path: None,
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
        path: None,
    })
}

/// Find a federation type and its primary key directive.
///
/// Convenience function combining `find_federation_type` and `get_key_directive`.
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
mod tests;
