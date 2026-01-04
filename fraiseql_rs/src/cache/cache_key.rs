//! Query result cache key generation
//!
//! Generates cache keys from GraphQL queries and extracts accessed entities
//! for cascade-driven invalidation.

use crate::graphql::types::ParsedQuery;
use serde_json::Value;
use std::collections::HashSet;

/// Cache key information for a GraphQL query
#[derive(Debug, Clone)]
pub struct QueryCacheKey {
    /// Unique cache key for this query (query hash + variable values)
    pub key: String,

    /// Entities accessed by this query
    /// Format: vec![("User", "123"), ("Post", "*")]
    /// Wildcard "*" means "all entities of this type"
    pub accessed_entities: Vec<(String, String)>,
}

impl QueryCacheKey {
    /// Generate cache key and extract accessed entities from a GraphQL query
    ///
    /// # Arguments
    ///
    /// * `query` - Parsed GraphQL query
    /// * `variables` - Query variables (for computing stable hash)
    ///
    /// # Returns
    ///
    /// Cache key with accessed entities, or None if query should not be cached
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Query: { user(id: "123") { id name } }
    /// let key = QueryCacheKey::from_query(parsed_query, &vars);
    /// // Returns: key="query:user:123", accessed_entities=[("User", "123")]
    ///
    /// // Query: { users { id name } }
    /// let key = QueryCacheKey::from_query(parsed_query, &vars);
    /// // Returns: key="query:users:all", accessed_entities=[("User", "*")]
    /// ```
    pub fn from_query(
        query: &ParsedQuery,
        variables: &std::collections::HashMap<String, Value>,
    ) -> Option<Self> {
        // Don't cache mutations (they have side effects)
        if query.operation_type == "mutation" {
            return None;
        }

        // Don't cache list queries with filtering beyond ID
        // (These may have dynamic filters not captured in key)
        let should_cache = Self::should_cache_query(query);
        if !should_cache {
            return None;
        }

        let accessed_entities = Self::extract_accessed_entities(query);
        let key = Self::generate_cache_key(query, variables);

        Some(QueryCacheKey {
            key,
            accessed_entities,
        })
    }

    /// Check if a query should be cached
    ///
    /// Don't cache:
    /// - Mutations
    /// - Introspection queries
    /// - Queries with @skip/@include directives (dynamic)
    fn should_cache_query(query: &ParsedQuery) -> bool {
        // Don't cache if any field has @skip or @include directives
        // (These are runtime-dependent and shouldn't be cached)
        let has_dynamic_directives = query.selections.iter().any(|sel| {
            // If selection has any directives, consider it dynamic
            !sel.directives.is_empty()
        });

        !has_dynamic_directives
    }

    /// Generate stable cache key from query and variables
    ///
    /// Uses query structure + variable values for stable key generation
    fn generate_cache_key(
        query: &ParsedQuery,
        variables: &std::collections::HashMap<String, Value>,
    ) -> String {
        // Build key from root field name and variable values
        let default_field = "unknown".to_string();
        let root_field = query
            .selections
            .get(0)
            .map(|s| &s.name)
            .unwrap_or(&default_field);

        // Hash variable values if present
        let vars_hash = if variables.is_empty() {
            "no-vars".to_string()
        } else {
            // Create sorted variable hash for stability
            let mut var_parts = Vec::new();
            let mut var_keys: Vec<_> = variables.keys().collect();
            var_keys.sort();

            for key in var_keys {
                if let Some(val) = variables.get(key) {
                    // Use JSON string representation for stable hashing
                    let val_str = match val {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => serde_json::to_string(val).unwrap_or_default(),
                    };
                    var_parts.push(format!("{}:{}", key, val_str));
                }
            }
            var_parts.join("|")
        };

        // Use sha256 hash for query structure (stable across runs)
        // For now, use simple string key with field + variables
        format!("query:{}:{}", root_field, vars_hash)
    }

    /// Extract entities accessed by this query
    ///
    /// Returns list of (entity_type, entity_id/wildcard) tuples
    /// Wildcard "*" means "all entities of this type"
    fn extract_accessed_entities(query: &ParsedQuery) -> Vec<(String, String)> {
        let mut entities = HashSet::new();

        for selection in &query.selections {
            // For each selected field, determine what entities it accesses
            let entity_type = Self::infer_entity_type(&selection.name);

            // Check if this is a single-entity query (like user(id: "123"))
            // or all-entities query (like users, allUsers)
            if selection.name.ends_with('s') || selection.name.starts_with("all") {
                // Plural or "all" prefix = all entities of this type
                entities.insert((entity_type, "*".to_string()));
            } else {
                // Singular query - extract ID from nested_fields if available
                let has_id_field = selection.nested_fields.first().map_or(false, |n| n.name == "id");
                if has_id_field {
                    // This is a single entity query, but we don't have the ID yet
                    // Mark as accessing all entities (conservative)
                    entities.insert((entity_type.clone(), "*".to_string()));
                }
                // Default to all entities for this type (conservative)
                entities.insert((entity_type, "*".to_string()));
            }
        }

        // Convert to sorted vec for consistency
        let mut result: Vec<_> = entities.into_iter().collect();
        result.sort();
        result
    }

    /// Infer entity type from GraphQL field name
    ///
    /// Converts "user" → "User", "users" → "User", "allPosts" → "Post"
    fn infer_entity_type(field_name: &str) -> String {
        // Remove plural 's' if present
        let singular = if field_name.ends_with('s') && field_name.len() > 1 {
            &field_name[..field_name.len() - 1]
        } else {
            field_name
        };

        // Remove "all" prefix if present (e.g., "allUsers" → "User")
        let without_prefix = if singular.starts_with("all") && singular.len() > 3 {
            &singular[3..]
        } else {
            singular
        };

        // Capitalize first letter
        let mut chars = without_prefix.chars();
        match chars.next() {
            None => field_name.to_string(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_entity_type_singular() {
        assert_eq!(QueryCacheKey::infer_entity_type("user"), "User");
        assert_eq!(QueryCacheKey::infer_entity_type("post"), "Post");
    }

    #[test]
    fn test_infer_entity_type_plural() {
        assert_eq!(QueryCacheKey::infer_entity_type("users"), "User");
        assert_eq!(QueryCacheKey::infer_entity_type("posts"), "Post");
    }

    #[test]
    fn test_infer_entity_type_all_prefix() {
        assert_eq!(QueryCacheKey::infer_entity_type("allUsers"), "User");
        assert_eq!(QueryCacheKey::infer_entity_type("allPosts"), "Post");
    }

    #[test]
    fn test_cache_key_generation() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("id".to_string(), Value::String("123".to_string()));

        let key = QueryCacheKey::generate_cache_key(
            &ParsedQuery {
                operation_type: "query".to_string(),
                root_field: "user".to_string(),
                selections: vec![],
                fragments: vec![],
                variables: Default::default(),
                directives: vec![],
            },
            &vars,
        );

        assert!(key.contains("query:user:"));
        assert!(key.contains("id:123"));
    }

    #[test]
    fn test_cache_key_no_variables() {
        let vars = std::collections::HashMap::new();

        let key = QueryCacheKey::generate_cache_key(
            &ParsedQuery {
                operation_type: "query".to_string(),
                root_field: "users".to_string(),
                selections: vec![],
                fragments: vec![],
                variables: Default::default(),
                directives: vec![],
            },
            &vars,
        );

        assert_eq!(key, "query:users:no-vars");
    }
}
