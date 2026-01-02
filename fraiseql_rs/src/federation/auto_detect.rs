//! Auto-key detection for Federation Lite
//!
//! Automatically detects entity key fields using a priority-based algorithm:
//! 1. Field named 'id' (most common, ~90% of cases)
//! 2. Field with `@primary_key` annotation
//! 3. First field with ID scalar type
//! 4. None - returns clear error
//!
//! This enables the `@entity` decorator to work without explicit key specification
//! for the vast majority of users.

use std::collections::HashMap;
use thiserror::Error;

/// Information about a field in a type definition
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// GraphQL type name (e.g., "String", "ID", "User")
    pub type_name: String,

    /// Whether field is required (non-null)
    pub is_required: bool,

    /// Annotations on this field (e.g., [`primary_key`, `indexed`])
    pub annotations: Vec<String>,

    /// Whether field is a list type
    pub is_list: bool,
}

impl FieldInfo {
    /// Create a new field info
    #[must_use]
    pub fn new(type_name: &str, is_required: bool) -> Self {
        Self {
            type_name: type_name.to_string(),
            is_required,
            annotations: Vec::new(),
            is_list: false,
        }
    }

    /// Add an annotation to this field
    #[must_use]
    pub fn with_annotation(mut self, annotation: &str) -> Self {
        self.annotations.push(annotation.to_string());
        self
    }

    /// Mark field as list type
    #[must_use]
    pub const fn with_list(mut self) -> Self {
        self.is_list = true;
        self
    }

    /// Check if this field is an ID type
    #[must_use]
    pub fn is_id_type(&self) -> bool {
        self.type_name == "ID" || self.type_name == "ID!"
    }

    /// Check if this field has a `primary_key` annotation
    #[must_use]
    pub fn has_primary_key_annotation(&self) -> bool {
        self.annotations
            .iter()
            .any(|a| a == "primary_key" || a == "primary key")
    }
}

/// Error types for auto-detection
#[derive(Debug, Error, Clone)]
pub enum AutoDetectError {
    /// No suitable key field found in type definition
    #[error("Auto-detect failed for type '{type_name}': no 'id' field found. Specify key explicitly: @entity(key='field_name')")]
    NoKeyFound {
        /// GraphQL type name
        type_name: String,
    },

    /// Multiple potential keys found (ambiguous)
    #[error("Auto-detect ambiguous for type '{type_name}': found multiple primary_key annotations. Specify key explicitly.")]
    AmbiguousKey {
        /// GraphQL type name
        type_name: String,
    },

    /// Invalid type definition
    #[error("Invalid type definition for '{type_name}': {reason}")]
    InvalidType {
        /// GraphQL type name
        type_name: String,
        /// Error description
        reason: String,
    },
}

/// Auto-detect entity key field from type definition
///
/// Uses priority-based algorithm:
/// 1. Field named 'id' (most common, ~90% of cases)
/// 2. Field with `@primary_key` annotation
/// 3. First field with ID scalar type
/// 4. None - returns error
///
/// # Arguments
///
/// * `type_name` - GraphQL type name (e.g., "User")
/// * `fields` - Map of field names to field info
///
/// # Returns
///
/// `Ok(key_field_name)` if key detected, `Err(AutoDetectError)` otherwise
///
/// # Errors
///
/// Returns `AutoDetectError` if:
/// - Type has no fields
/// - Multiple fields have `@primary_key` annotation
/// - No suitable key field found
///
/// # Panics
///
/// Never panics. The `.unwrap()` on line 144 is guaranteed safe because
/// we check `primary_key_fields.len() == 1` before calling it.
///
/// # Examples
///
/// ```
/// use fraiseql_rs::federation::auto_detect::{auto_detect_key, FieldInfo};
/// use std::collections::HashMap;
///
/// let mut fields = HashMap::new();
/// fields.insert("id".to_string(), FieldInfo::new("ID!", true));
/// fields.insert("name".to_string(), FieldInfo::new("String", true));
///
/// let key = auto_detect_key("User", &fields).unwrap();
/// assert_eq!(key, "id");
/// ```
pub fn auto_detect_key<S: std::hash::BuildHasher>(
    type_name: &str,
    fields: &HashMap<String, FieldInfo, S>,
) -> Result<String, AutoDetectError> {
    // Empty type definition check
    if fields.is_empty() {
        return Err(AutoDetectError::InvalidType {
            type_name: type_name.to_string(),
            reason: "Type has no fields".to_string(),
        });
    }

    // Priority 1: Field named 'id' (most common, ~90% of cases)
    if fields.contains_key("id") {
        return Ok("id".to_string());
    }

    // Priority 2: Field with @primary_key annotation
    let mut primary_key_fields = Vec::new();
    for (field_name, field_info) in fields {
        if field_info.has_primary_key_annotation() {
            primary_key_fields.push(field_name.clone());
        }
    }

    match primary_key_fields.len() {
        1 => {
            // Safe: we know there's exactly one element
            if let Some(field) = primary_key_fields.into_iter().next() {
                return Ok(field);
            }
        }
        n if n > 1 => {
            return Err(AutoDetectError::AmbiguousKey {
                type_name: type_name.to_string(),
            })
        }
        _ => {} // Continue to next priority
    }

    // Priority 3: First field with ID scalar type
    for (field_name, field_info) in fields {
        if field_info.is_id_type() {
            return Ok(field_name.clone());
        }
    }

    // Priority 4: Not found
    Err(AutoDetectError::NoKeyFound {
        type_name: type_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_field(type_name: &str, is_required: bool) -> FieldInfo {
        FieldInfo::new(type_name, is_required)
    }

    #[test]
    fn test_auto_detect_id_field() {
        let mut fields = HashMap::new();
        fields.insert("id".to_string(), create_field("ID!", true));
        fields.insert("name".to_string(), create_field("String", true));

        let key = auto_detect_key("User", &fields).unwrap();
        assert_eq!(key, "id");
    }

    #[test]
    fn test_auto_detect_uuid_as_id() {
        let mut fields = HashMap::new();
        fields.insert("uuid".to_string(), create_field("ID!", true));
        fields.insert("name".to_string(), create_field("String", true));

        let key = auto_detect_key("User", &fields).unwrap();
        assert_eq!(key, "uuid");
    }

    #[test]
    fn test_auto_detect_primary_key_annotation() {
        let mut fields = HashMap::new();
        fields.insert(
            "user_id".to_string(),
            create_field("String", true).with_annotation("primary_key"),
        );
        fields.insert("name".to_string(), create_field("String", true));

        let key = auto_detect_key("User", &fields).unwrap();
        assert_eq!(key, "user_id");
    }

    #[test]
    fn test_auto_detect_no_key_error() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), create_field("String", true));
        fields.insert("email".to_string(), create_field("String", false));

        let result = auto_detect_key("User", &fields);
        assert!(result.is_err());
        match result {
            Err(AutoDetectError::NoKeyFound { type_name }) => {
                assert_eq!(type_name, "User");
            }
            _ => panic!("Expected NoKeyFound error"),
        }
    }

    #[test]
    fn test_auto_detect_ambiguous_key() {
        let mut fields = HashMap::new();
        fields.insert(
            "user_id".to_string(),
            create_field("String", true).with_annotation("primary_key"),
        );
        fields.insert(
            "org_id".to_string(),
            create_field("String", true).with_annotation("primary_key"),
        );

        let result = auto_detect_key("OrgUser", &fields);
        assert!(result.is_err());
        match result {
            Err(AutoDetectError::AmbiguousKey { type_name }) => {
                assert_eq!(type_name, "OrgUser");
            }
            _ => panic!("Expected AmbiguousKey error"),
        }
    }

    #[test]
    fn test_auto_detect_priority_id_before_primary_key() {
        // When both 'id' and primary_key annotation exist, 'id' wins
        let mut fields = HashMap::new();
        fields.insert("id".to_string(), create_field("ID!", true));
        fields.insert(
            "user_id".to_string(),
            create_field("String", true).with_annotation("primary_key"),
        );

        let key = auto_detect_key("User", &fields).unwrap();
        assert_eq!(key, "id");
    }

    #[test]
    fn test_auto_detect_empty_type() {
        let fields = HashMap::new();
        let result = auto_detect_key("EmptyType", &fields);
        assert!(result.is_err());
        match result {
            Err(AutoDetectError::InvalidType { .. }) => {}
            _ => panic!("Expected InvalidType error"),
        }
    }
}
