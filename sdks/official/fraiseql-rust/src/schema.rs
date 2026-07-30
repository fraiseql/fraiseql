//! Schema registry and scope validation for field-level RBAC.
//!
//! Provides:
//! - `SchemaRegistry` for tracking types and their field scopes
//! - `validate_scope()` for validating scope format (action:resource)
//! - JSON export of schema with scope metadata

use crate::field::Field;
use serde::Serialize;
use std::collections::HashMap;

/// Error type for scope validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeValidationError {
    /// Missing colon separator (format must be "action:resource")
    MissingColon,
    /// Empty action part (before colon)
    EmptyAction,
    /// Empty resource part (after colon)
    EmptyResource,
    /// Invalid action format (must be alphanumeric + underscore)
    InvalidAction(String),
    /// Invalid resource format (must be alphanumeric + underscore + dot, or wildcard)
    InvalidResource(String),
    /// Empty scope string
    EmptyScope,
}

/// Validates scope format: action:resource
///
/// # Valid formats
/// - `read:user.email` (specific field)
/// - `read:User.*` (all User fields)
/// - `admin:*` (global admin)
/// - `*` (wildcard)
///
/// # Invalid formats
/// - `readuser` (missing colon)
/// - `read-all:user` (hyphen in action)
/// - `read:user-data` (hyphen in resource)
/// - `:resource` (empty action)
/// - `action:` (empty resource)
///
/// # Errors
/// Returns `ScopeValidationError` for invalid formats.
pub fn validate_scope(scope: &str) -> Result<(), ScopeValidationError> {
    if scope.is_empty() {
        return Err(ScopeValidationError::EmptyScope);
    }

    // Global wildcard is always valid
    if scope == "*" {
        return Ok(());
    }

    // Check for colon separator
    if !scope.contains(':') {
        return Err(ScopeValidationError::MissingColon);
    }

    let parts: Vec<&str> = scope.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(ScopeValidationError::MissingColon);
    }

    let action = parts[0];
    let resource = parts[1];

    // Check for empty parts
    if action.is_empty() {
        return Err(ScopeValidationError::EmptyAction);
    }
    if resource.is_empty() {
        return Err(ScopeValidationError::EmptyResource);
    }

    // Validate action format: [a-zA-Z_][a-zA-Z0-9_]*
    if !is_valid_action(action) {
        return Err(ScopeValidationError::InvalidAction(action.to_string()));
    }

    // Validate resource format: [a-zA-Z_][a-zA-Z0-9_.]*|*
    if !is_valid_resource(resource) {
        return Err(ScopeValidationError::InvalidResource(resource.to_string()));
    }

    Ok(())
}

/// Validates action format: [a-zA-Z_][a-zA-Z0-9_]*
fn is_valid_action(action: &str) -> bool {
    if action.is_empty() {
        return false;
    }

    let first_char = action.chars().next().unwrap();
    if !first_char.is_alphabetic() && first_char != '_' {
        return false;
    }

    action.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Validates resource format: [a-zA-Z_][a-zA-Z0-9_.]*|*
fn is_valid_resource(resource: &str) -> bool {
    if resource == "*" {
        return true;
    }

    if resource.is_empty() {
        return false;
    }

    let first_char = resource.chars().next().unwrap();
    // First character must be letter (upper/lower) or underscore
    if !first_char.is_alphabetic() && first_char != '_' {
        return false;
    }

    // Remaining characters can be alphanumeric, underscore, dot, or asterisk
    resource
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '*')
}

/// Registry for GraphQL schema types and their field scope requirements.
///
/// Tracks all types and fields, extracts scope metadata, and exports
/// to JSON format for the compiler.
#[derive(Debug, Clone)]
pub struct SchemaRegistry {
    types: HashMap<String, RegisteredType>,
}

/// One registered type: its fields and the SQL relation backing it.
#[derive(Debug, Clone)]
struct RegisteredType {
    fields: Vec<Field>,
    sql_source: Option<String>,
}

/// A type as it is written to `schema.json`.
#[derive(Serialize)]
struct SerializedType<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sql_source: Option<&'a str>,
    fields: Vec<Field>,
}

/// The document written to `schema.json`.
///
/// The previous export produced a bare map keyed by type name — `{"User": {...}}` — with
/// no `types` key at all. The compiler reads `types` as an array of objects, so it saw a
/// schema with **zero** types, reported `✓ Schema compiled successfully`, and wrote an
/// artifact containing none of the author's work (#855). A silent success is the one
/// outcome worse than a rejection.
#[derive(Serialize)]
struct SerializedSchema<'a> {
    version: &'a str,
    types: Vec<SerializedType<'a>>,
}

impl SchemaRegistry {
    /// Creates a new empty schema registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
        }
    }

    /// Registers a new type with its fields.
    ///
    /// # Arguments
    /// * `type_name` - Name of the type (e.g., "User", "Post")
    /// * `fields` - Vector of Field definitions
    pub fn register_type(&mut self, type_name: &str, fields: Vec<Field>) {
        self.types.insert(
            type_name.to_string(),
            RegisteredType {
                fields,
                sql_source: None,
            },
        );
    }

    /// Registers a type together with the SQL view backing it.
    ///
    /// # Arguments
    /// * `type_name` - Name of the type (e.g., "User", "Post")
    /// * `fields` - Vector of Field definitions
    /// * `sql_source` - The backing view, e.g. `v_user`
    pub fn register_type_with_source(
        &mut self,
        type_name: &str,
        fields: Vec<Field>,
        sql_source: &str,
    ) {
        self.types.insert(
            type_name.to_string(),
            RegisteredType {
                fields,
                sql_source: Some(sql_source.to_string()),
            },
        );
    }

    /// Gets registered fields for a type.
    ///
    /// # Arguments
    /// * `type_name` - Name of the type
    ///
    /// # Returns
    /// Option containing reference to field vector, or None if type not found
    #[must_use]
    pub fn get_type(&self, type_name: &str) -> Option<&Vec<Field>> {
        self.types.get(type_name).map(|registered| &registered.fields)
    }

    /// Extracts all fields that have scope requirements.
    ///
    /// # Returns
    /// `HashMap` mapping type name to vector of field names with scopes
    ///
    /// # Example
    /// ```
    /// // Returns: { "User": ["email", "password"] }
    /// ```
    #[must_use]
    pub fn extract_scoped_fields(&self) -> HashMap<String, Vec<String>> {
        let mut scoped = HashMap::new();

        for (type_name, registered) in &self.types {
            let scoped_fields: Vec<String> = registered
                .fields
                .iter()
                .filter(|f| f.requires_scope.is_some() || f.requires_scopes.is_some())
                .map(|f| f.name.clone())
                .collect();

            if !scoped_fields.is_empty() {
                scoped.insert(type_name.clone(), scoped_fields);
            }
        }

        scoped
    }

    /// Exports the registry as a `schema.json` document for `fraiseql compile`.
    ///
    /// # Returns
    /// A JSON string of the form `{"version": "2.0.0", "types": [...]}`.
    ///
    /// Built with `serde_json`, not string concatenation: the previous implementation
    /// interpolated raw values into JSON string literals, so a single `"` in a name,
    /// scope or description produced output that is not parseable JSON (#855).
    ///
    /// # Panics
    ///
    /// Panics when a field declares more than one required scope; see
    /// [`Field::normalized`].
    #[must_use]
    pub fn export_to_json(&self) -> String {
        // Sorted so the export is byte-stable across runs — `HashMap` iteration order is
        // not, and an unstable artifact makes every diff of a checked-in schema.json noise.
        let mut names: Vec<&String> = self.types.keys().collect();
        names.sort();

        let document = SerializedSchema {
            version: "2.0.0",
            types: names
                .into_iter()
                .map(|name| {
                    let registered = &self.types[name];
                    SerializedType {
                        name,
                        sql_source: registered.sql_source.as_deref(),
                        fields: registered.fields.iter().map(Field::normalized).collect(),
                    }
                })
                .collect(),
        };

        serde_json::to_string_pretty(&document)
            .expect("the schema document contains only strings and bools")
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_scope_valid_specific_field() {
        assert!(validate_scope("read:user.email").is_ok());
    }

    #[test]
    fn test_validate_scope_valid_wildcard_resource() {
        assert!(validate_scope("read:User.*").is_ok());
    }

    #[test]
    fn test_validate_scope_valid_global_wildcard() {
        assert!(validate_scope("admin:*").is_ok());
    }

    #[test]
    fn test_validate_scope_valid_pure_wildcard() {
        assert!(validate_scope("*").is_ok());
    }

    #[test]
    fn test_validate_scope_invalid_missing_colon() {
        assert!(validate_scope("readuser").is_err());
    }

    #[test]
    fn test_validate_scope_invalid_empty_string() {
        assert!(validate_scope("").is_err());
    }

    #[test]
    fn test_validate_scope_invalid_action_with_hyphen() {
        assert!(validate_scope("read-all:user").is_err());
    }

    #[test]
    fn test_validate_scope_invalid_resource_with_hyphen() {
        assert!(validate_scope("read:user-data").is_err());
    }

    #[test]
    fn test_schema_registry_new() {
        let registry = SchemaRegistry::new();
        assert_eq!(registry.types.len(), 0);
    }

    #[test]
    fn test_schema_registry_register_type() {
        let mut registry = SchemaRegistry::new();
        let fields = vec![Field::new("id", "Int")];
        registry.register_type("User", fields);

        assert!(registry.get_type("User").is_some());
        assert_eq!(registry.get_type("User").unwrap().len(), 1);
    }

    #[test]
    fn test_schema_registry_extract_scoped_fields() {
        let mut registry = SchemaRegistry::new();
        let fields = vec![
            Field::new("id", "Int"),
            Field::new("email", "String").with_requires_scope(Some("read:user.email".to_string())),
        ];
        registry.register_type("User", fields);

        let scoped = registry.extract_scoped_fields();
        assert!(scoped.contains_key("User"));
        assert_eq!(scoped["User"].len(), 1);
        assert_eq!(scoped["User"][0], "email");
    }
}
