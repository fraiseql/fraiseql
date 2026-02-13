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

/// Represents a GraphQL field definition with optional scope requirements.
///
/// Fields can have scope-based access control through either a single scope
/// or multiple scopes (all required). Scope format is `action:resource`
/// (e.g., `read:user.email`, `admin:*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Field name (e.g., "email", "password")
    pub name: String,
    /// GraphQL field type (e.g., "String", "Int", "User")
    pub field_type: String,
    /// Whether field is nullable in GraphQL (default: true)
    pub nullable: bool,
    /// Single required scope for field access (e.g., "read:user.email")
    pub requires_scope: Option<String>,
    /// Multiple required scopes (all must be satisfied)
    pub requires_scopes: Option<Vec<String>>,
    /// Optional field description
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
    pub fn with_nullable(mut self, nullable: bool) -> Self {
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

    /// Serializes field to JSON string.
    ///
    /// # Example output:
    /// ```json
    /// {
    ///   "name": "email",
    ///   "type": "String",
    ///   "nullable": false,
    ///   "requiresScope": "read:user.email"
    /// }
    /// ```
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut fields = vec![
            format!("\"name\":\"{name}\"", name = self.name),
            format!("\"type\":\"{field_type}\"", field_type = self.field_type),
            format!("\"nullable\":{nullable}", nullable = self.nullable),
        ];

        if let Some(scope) = &self.requires_scope {
            fields.push(format!("\"requiresScope\":\"{scope}\""));
        }

        if let Some(scopes) = &self.requires_scopes {
            let scopes_json = scopes
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(",");
            fields.push(format!("\"requiresScopes\":[{scopes_json}]"));
        }

        if let Some(desc) = &self.description {
            fields.push(format!("\"description\":\"{desc}\""));
        }

        format!("{{{}}}", fields.join(","))
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
