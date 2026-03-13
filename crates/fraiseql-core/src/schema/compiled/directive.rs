use serde::{Deserialize, Serialize};

use super::argument::ArgumentDefinition;

/// A custom directive definition for schema extension.
///
/// Allows defining custom directives beyond the built-in `@skip`, `@include`,
/// and `@deprecated` directives. Custom directives are exposed via introspection
/// and can be evaluated at runtime via registered handlers.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{DirectiveDefinition, DirectiveLocationKind, ArgumentDefinition, FieldType};
///
/// let rate_limit = DirectiveDefinition {
///     name: "rateLimit".to_string(),
///     description: Some("Apply rate limiting to this field".to_string()),
///     locations: vec![DirectiveLocationKind::FieldDefinition],
///     arguments: vec![
///         ArgumentDefinition::new("limit", FieldType::Int),
///         ArgumentDefinition::optional("window", FieldType::String),
///     ],
///     is_repeatable: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectiveDefinition {
    /// Directive name (e.g., "rateLimit", "auth").
    pub name: String,

    /// Description of what this directive does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Valid locations where this directive can be applied.
    pub locations: Vec<DirectiveLocationKind>,

    /// Arguments this directive accepts.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// Whether this directive can be applied multiple times to the same location.
    #[serde(default)]
    pub is_repeatable: bool,
}

impl DirectiveDefinition {
    /// Create a new directive definition.
    #[must_use]
    pub fn new(name: impl Into<String>, locations: Vec<DirectiveLocationKind>) -> Self {
        Self {
            name: name.into(),
            description: None,
            locations,
            arguments: Vec::new(),
            is_repeatable: false,
        }
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an argument to this directive.
    #[must_use]
    pub fn with_argument(mut self, arg: ArgumentDefinition) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Add multiple arguments to this directive.
    #[must_use]
    pub fn with_arguments(mut self, args: Vec<ArgumentDefinition>) -> Self {
        self.arguments = args;
        self
    }

    /// Mark this directive as repeatable.
    #[must_use]
    pub const fn repeatable(mut self) -> Self {
        self.is_repeatable = true;
        self
    }

    /// Check if this directive can be applied at the given location.
    #[must_use]
    pub fn valid_at(&self, location: DirectiveLocationKind) -> bool {
        self.locations.contains(&location)
    }

    /// Find an argument by name.
    #[must_use]
    pub fn find_argument(&self, name: &str) -> Option<&ArgumentDefinition> {
        self.arguments.iter().find(|a| a.name == name)
    }
}

/// Directive location kinds for custom directive definitions.
///
/// This mirrors `DirectiveLocation` in introspection but is used for
/// compiled schema definitions. The two types can be converted between
/// each other for introspection purposes.
///
/// Per GraphQL spec §3.13, directive locations fall into two categories:
/// - Executable locations (operations, fields, fragments)
/// - Type system locations (schema definitions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectiveLocationKind {
    // Executable directive locations
    /// Directive on query operation.
    Query,
    /// Directive on mutation operation.
    Mutation,
    /// Directive on subscription operation.
    Subscription,
    /// Directive on field selection.
    Field,
    /// Directive on fragment definition.
    FragmentDefinition,
    /// Directive on fragment spread.
    FragmentSpread,
    /// Directive on inline fragment.
    InlineFragment,
    /// Directive on variable definition.
    VariableDefinition,

    // Type system directive locations
    /// Directive on schema definition.
    Schema,
    /// Directive on scalar type definition.
    Scalar,
    /// Directive on object type definition.
    Object,
    /// Directive on field definition.
    FieldDefinition,
    /// Directive on argument definition.
    ArgumentDefinition,
    /// Directive on interface definition.
    Interface,
    /// Directive on union definition.
    Union,
    /// Directive on enum definition.
    Enum,
    /// Directive on enum value definition.
    EnumValue,
    /// Directive on input object definition.
    InputObject,
    /// Directive on input field definition.
    InputFieldDefinition,
}

impl DirectiveLocationKind {
    /// Check if this is an executable directive location.
    #[must_use]
    pub const fn is_executable(&self) -> bool {
        matches!(
            self,
            Self::Query
                | Self::Mutation
                | Self::Subscription
                | Self::Field
                | Self::FragmentDefinition
                | Self::FragmentSpread
                | Self::InlineFragment
                | Self::VariableDefinition
        )
    }

    /// Check if this is a type system directive location.
    #[must_use]
    pub const fn is_type_system(&self) -> bool {
        !self.is_executable()
    }
}
