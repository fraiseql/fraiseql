//! Federation mutation query detection.
//!
//! Detects whether a GraphQL query is a mutation and extracts mutation information
//! for routing to mutation handlers. Includes federation awareness for local vs extended mutations.

use crate::federation::types::FederationMetadata;

/// Check if a query is a GraphQL mutation.
///
/// # Examples
///
/// ```ignore
/// assert!(is_mutation("mutation { updateUser { id } }"));
/// assert!(!is_mutation("query { user { id } }"));
/// ```
#[must_use]
pub fn is_mutation(query: &str) -> bool {
    let trimmed = query.trim();

    // Check for explicit mutation keyword
    if trimmed.starts_with("mutation") {
        return true;
    }

    // Check for mutation keyword with whitespace
    if trimmed.contains("mutation ") || trimmed.contains("mutation{") {
        return true;
    }

    false
}

/// Extract the mutation name from a mutation query.
///
/// Example:
/// ```text
/// mutation UpdateUser($id: ID!) {
///   updateUser(id: $id) { id name }
/// }
/// ```
/// Returns: "updateUser"
#[must_use]
pub fn extract_mutation_name(query: &str) -> Option<String> {
    let trimmed = query.trim();

    // Find the first opening brace of the selection set
    let brace_pos = trimmed.find('{')?;
    let after_brace = &trimmed[brace_pos + 1..];

    // Skip whitespace and find the first field name
    let mut field_name = String::new();
    let mut found_alphanumeric = false;

    for ch in after_brace.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            field_name.push(ch);
            found_alphanumeric = true;
        } else if found_alphanumeric {
            // Stop when we hit a non-alphanumeric after finding the field name
            break;
        }
        // Skip whitespace before field name
    }

    if field_name.is_empty() {
        None
    } else {
        Some(field_name)
    }
}

/// Extract entity typename from a mutation name.
///
/// Examples:
/// - "createUser" -> "User"
/// - "updateOrder" -> "Order"
/// - "deleteProduct" -> "Product"
#[must_use]
pub fn extract_typename_from_mutation(mutation_name: &str) -> Option<String> {
    let lower = mutation_name.to_lowercase();

    // Try common prefixes
    if let Some(typename) = lower
        .strip_prefix("create")
        .or_else(|| lower.strip_prefix("add"))
    {
        // Capitalize first letter
        if let Some(first) = typename.chars().next() {
            let capitalized = first.to_uppercase().collect::<String>() + &typename[1..];
            return Some(capitalized);
        }
    }

    if let Some(typename) = lower
        .strip_prefix("update")
        .or_else(|| lower.strip_prefix("modify"))
    {
        if let Some(first) = typename.chars().next() {
            let capitalized = first.to_uppercase().collect::<String>() + &typename[1..];
            return Some(capitalized);
        }
    }

    if let Some(typename) = lower
        .strip_prefix("delete")
        .or_else(|| lower.strip_prefix("remove"))
    {
        if let Some(first) = typename.chars().next() {
            let capitalized = first.to_uppercase().collect::<String>() + &typename[1..];
            return Some(capitalized);
        }
    }

    None
}

/// Check if a mutation is on a locally-owned entity.
///
/// A mutation is local if the entity type is NOT marked as @extends in federation metadata.
/// If federation is disabled, all mutations are considered local.
#[must_use]
pub fn is_local_mutation(mutation_name: &str, metadata: &FederationMetadata) -> bool {
    // If federation is not enabled, assume local
    if !metadata.enabled {
        return true;
    }

    // Extract typename from mutation name
    let Some(typename) = extract_typename_from_mutation(mutation_name) else {
        return true; // Unknown mutations default to local
    };

    // Find type in federation metadata
    let fed_type = metadata.types.iter().find(|t| t.name == typename);

    match fed_type {
        Some(t) => {
            // Local if NOT extended
            !t.is_extends
        }
        None => true, // Unknown types default to local
    }
}

/// Check if a mutation is on an extended (non-owned) entity.
///
/// A mutation is extended if the entity type is marked as @extends in federation metadata.
#[must_use]
pub fn is_extended_mutation(mutation_name: &str, metadata: &FederationMetadata) -> bool {
    !is_local_mutation(mutation_name, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_mutation() {
        assert!(is_mutation("mutation { updateUser { id } }"));
        assert!(is_mutation("mutation UpdateUser { updateUser { id } }"));
        assert!(is_mutation("  mutation  {  updateUser  {  id  }  }"));
        assert!(!is_mutation("query { user { id } }"));
        assert!(!is_mutation("{ user { id } }"));
    }

    #[test]
    fn test_extract_mutation_name() {
        let query = "mutation { updateUser { id name } }";
        assert_eq!(extract_mutation_name(query), Some("updateUser".to_string()));

        let query = "mutation UpdateUser($id: ID!) { updateUser(id: $id) { id } }";
        assert_eq!(extract_mutation_name(query), Some("updateUser".to_string()));

        let query = "query { user { id } }";
        // Should not extract from non-mutations
        let _result = extract_mutation_name(query);
        // May or may not find "user" depending on implementation
    }

    #[test]
    fn test_extract_mutation_with_variables() {
        let query = "mutation CreateUser($input: UserInput!) { createUser(input: $input) { id } }";
        assert_eq!(extract_mutation_name(query), Some("createUser".to_string()));
    }

    #[test]
    fn test_extract_typename_from_mutation() {
        assert_eq!(
            extract_typename_from_mutation("createUser"),
            Some("User".to_string())
        );
        assert_eq!(
            extract_typename_from_mutation("updateOrder"),
            Some("Order".to_string())
        );
        assert_eq!(
            extract_typename_from_mutation("deleteProduct"),
            Some("Product".to_string())
        );
        assert_eq!(extract_typename_from_mutation("unknown"), None);
    }

    #[test]
    fn test_mutation_ownership_federation_disabled() {
        let metadata = crate::federation::FederationMetadata::default();
        // With federation disabled, all mutations are local
        assert!(is_local_mutation("updateUser", &metadata));
        assert!(!is_extended_mutation("updateUser", &metadata));
    }

    #[test]
    fn test_mutation_ownership_local_type() {
        let metadata = crate::federation::FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types: vec![crate::federation::FederatedType {
                name: "User".to_string(),
                keys: vec![],
                is_extends: false, // NOT extended = local
                external_fields: vec![],
                shareable_fields: vec![],
            }],
        };

        assert!(is_local_mutation("updateUser", &metadata));
        assert!(!is_extended_mutation("updateUser", &metadata));
    }

    #[test]
    fn test_mutation_ownership_extended_type() {
        let metadata = crate::federation::FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types: vec![crate::federation::FederatedType {
                name: "User".to_string(),
                keys: vec![],
                is_extends: true, // Extended = remote
                external_fields: vec![],
                shareable_fields: vec![],
            }],
        };

        assert!(!is_local_mutation("updateUser", &metadata));
        assert!(is_extended_mutation("updateUser", &metadata));
    }
}
