//! Field-level RBAC support for schema definition.
//!
//! Provides `Field` struct with scope metadata for field-level access control.
//! Each field can specify required scopes (permissions) for GraphQL field access.
//!
//! # Example
//! ```
//! use fraiseql_rust::Field;
//!
//! let field = Field::new("email", "String")
//!     .with_nullable(false)
//!     .with_requires_scope(Some("read:user.email".to_string()));
//! ```

use serde::Serialize;

/// Represents a GraphQL field definition with optional scope requirements.
///
/// Fields can have scope-based access control through either a single scope
/// or multiple scopes (all required). Scope format is `action:resource`
/// (e.g., `read:user.email`, `admin:*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    /// Field name (e.g., "email", "password")
    pub name: String,
    /// GraphQL field type (e.g., "String", "Int", "User")
    ///
    /// The wire key is `type`, not `field_type` — the intermediate format is
    /// language-agnostic and every other SDK spells it that way.
    #[serde(rename = "type")]
    pub field_type: String,
    /// Whether field is nullable in GraphQL (default: true)
    pub nullable: bool,
    /// Single required scope for field access (e.g., "read:user.email")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,
    /// Multiple required scopes (all must be satisfied)
    ///
    /// Never serialized directly: the compiled schema and the runtime field filter
    /// represent exactly one required scope, so [`Field::normalized`] folds a singleton
    /// into `requires_scope` and refuses anything longer.
    #[serde(skip)]
    pub requires_scopes: Option<Vec<String>>,
    /// Optional field description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Field {
    /// Creates a new field with given name and type.
    ///
    /// # Arguments
    /// * `name` - Field name
    /// * `field_type` - GraphQL field type
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("email", "String");
    /// assert_eq!(field.name, "email");
    /// assert!(field.nullable); // default
    /// ```
    #[must_use]
    pub fn new(name: &str, field_type: &str) -> Self {
        Self {
            name: name.to_string(),
            field_type: field_type.to_string(),
            nullable: true,
            requires_scope: None,
            requires_scopes: None,
            description: None,
        }
    }

    /// Sets nullable property (fluent API).
    ///
    /// # Arguments
    /// * `nullable` - Whether field is nullable
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("id", "Int").with_nullable(false);
    /// assert!(!field.nullable);
    /// ```
    #[must_use]
    pub const fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Sets single required scope (fluent API).
    ///
    /// # Arguments
    /// * `scope` - Scope in format `action:resource` (e.g., `read:user.email`)
    ///
    /// Use this when field requires a single permission scope.
    /// Cannot be used together with `with_requires_scopes()`.
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("email", "String")
    ///     .with_requires_scope(Some("read:user.email".to_string()));
    /// ```
    #[must_use]
    pub fn with_requires_scope(mut self, scope: Option<String>) -> Self {
        self.requires_scope = scope;
        self
    }

    /// Sets multiple required scopes (fluent API).
    ///
    /// # Arguments
    /// * `scopes` - Vector of scopes (all must be satisfied)
    ///
    /// Use this when field requires multiple permission scopes.
    /// Cannot be used together with `with_requires_scope()`.
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let scopes = vec!["read:user.email".to_string(), "write:user.*".to_string()];
    /// let field = Field::new("email", "String")
    ///     .with_requires_scopes(Some(scopes));
    /// ```
    #[must_use]
    pub fn with_requires_scopes(mut self, scopes: Option<Vec<String>>) -> Self {
        self.requires_scopes = scopes;
        self
    }

    /// Sets field description (fluent API).
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("email", "String")
    ///     .with_description(Some("User email address".to_string()));
    /// ```
    #[must_use]
    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.description = description;
        self
    }

    /// The field as it is written to `schema.json`, with scopes normalized.
    ///
    /// # Panics
    ///
    /// Panics when the field declares more than one required scope. The compiled schema
    /// and the runtime field filter represent exactly one `requires_scope`, so a
    /// multi-scope declaration cannot be honoured; emitting it produced a field with no
    /// scope at all — silently public (#807). Failing loudly at authoring time is the
    /// only outcome that does not ship an ungated field.
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut field = self.clone();
        if let Some(scopes) = self.requires_scopes.as_deref() {
            match scopes {
                [] => {},
                [only] => field.requires_scope = Some(only.clone()),
                many => panic!(
                    "field `{}` declares {} required scopes; multiple required scopes are not \
                     supported — use `with_requires_scope` with a single scope",
                    self.name,
                    many.len()
                ),
            }
        }
        field.requires_scopes = None;
        field
    }

    /// Serializes field to JSON string.
    ///
    /// # Example output:
    /// ```json
    /// {
    ///   "name": "email",
    ///   "type": "String",
    ///   "nullable": false,
    ///   "requires_scope": "read:user.email"
    /// }
    /// ```
    ///
    /// Built with `serde_json`, not string concatenation. The previous implementation
    /// interpolated raw values into JSON string literals, so a `"` or `\` anywhere in a
    /// name, scope or description produced text that is not parseable JSON — a
    /// description as ordinary as `the user's "display" name` broke the export, and the
    /// CLI failed at a byte offset rather than naming a field (#855).
    ///
    /// # Panics
    ///
    /// Panics under the same condition as [`Field::normalized`].
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.normalized())
            .expect("a Field contains only strings and bools, which always serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_new() {
        let field = Field::new("id", "Int");
        assert_eq!(field.name, "id");
        assert_eq!(field.field_type, "Int");
        assert!(field.nullable);
    }

    #[test]
    fn test_field_builder_chain() {
        let field = Field::new("email", "String")
            .with_nullable(false)
            .with_requires_scope(Some("read:user.email".to_string()));

        assert_eq!(field.name, "email");
        assert!(!field.nullable);
        assert_eq!(field.requires_scope, Some("read:user.email".to_string()));
    }
}
