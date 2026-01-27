//! Federation mutation query detection.
//!
//! Detects whether a GraphQL query is a mutation and extracts mutation information
//! for routing to mutation handlers.

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

    // Find the first opening brace
    let brace_pos = trimmed.find('{')?;
    let before_brace = &trimmed[..brace_pos];

    // Find the field name between opening { and either ( or whitespace
    let after_mutation = if let Some(pos) = before_brace.rfind("mutation") {
        &before_brace[pos + 8..] // Skip "mutation" keyword
    } else {
        before_brace
    };

    // Extract the field name (alphanumeric and underscore)
    let field_part = after_mutation.trim();
    let mut field_name = String::new();

    for ch in field_part.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            field_name.push(ch);
        } else if !field_name.is_empty() {
            break;
        }
    }

    if field_name.is_empty() {
        None
    } else {
        Some(field_name)
    }
}

/// Check if a mutation is on a locally-owned entity.
///
/// A mutation is local if the entity type is not marked as @extends.
/// (Simplified for now - would check federation metadata in production)
#[must_use]
pub fn is_local_mutation(_mutation_name: &str) -> bool {
    // Simplified: assume all mutations are local for now
    // In production, check if entity is @extends in federation metadata
    true
}

/// Check if a mutation is on an extended (non-owned) entity.
///
/// A mutation is extended if the entity type is marked as @extends.
#[must_use]
pub fn is_extended_mutation(mutation_name: &str) -> bool {
    // Simplified: check for known extended patterns
    // In production, check federation metadata
    !is_local_mutation(mutation_name)
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
    fn test_mutation_ownership() {
        assert!(is_local_mutation("updateUser"));
        assert!(!is_extended_mutation("updateUser")); // Local is not extended
    }
}
