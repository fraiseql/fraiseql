//! Federation directive parsing and validation
//!
//! Implements parsing and validation for Apollo Federation directives.
//! Week 2 focuses on core directives needed for Federation Standard:
//! - @key: Entity key specification (from Week 1)
//! - @external: External field reference
//! - @requires: Field dependencies
//! - @provides: Eager field loading
//!
//! Advanced directives (Phase 17b):
//! - @shareable, @override, @inaccessible, @tag, @interfaceObject, etc.

use std::collections::HashMap;
use thiserror::Error;

/// Federation directive types (Week 2 core directives)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FederationDirective {
    /// @key(fields: "...") - Specifies entity key fields
    Key { fields: Vec<String> },

    /// @external - Marks field as defined in another subgraph
    External,

    /// @requires(fields: "...") - Marks field dependencies
    Requires { fields: Vec<String> },

    /// @provides(fields: "...") - Marks eager field loading
    Provides { fields: Vec<String> },
}

impl FederationDirective {
    /// Get directive name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Key { .. } => "key",
            Self::External => "external",
            Self::Requires { .. } => "requires",
            Self::Provides { .. } => "provides",
        }
    }

    /// Check if this is an external directive
    pub fn is_external(&self) -> bool {
        matches!(self, Self::External)
    }

    /// Check if this directive has fields
    pub fn has_fields(&self) -> bool {
        matches!(
            self,
            Self::Key { .. } | Self::Requires { .. } | Self::Provides { .. }
        )
    }
}

impl std::fmt::Display for FederationDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Key { fields } => {
                write!(f, "@key(fields: \"{}\")", fields.join(" "))
            }
            Self::External => write!(f, "@external"),
            Self::Requires { fields } => {
                write!(f, "@requires(fields: \"{}\")", fields.join(" "))
            }
            Self::Provides { fields } => {
                write!(f, "@provides(fields: \"{}\")", fields.join(" "))
            }
        }
    }
}

/// Directive parsing errors
#[derive(Debug, Error, Clone)]
pub enum DirectiveError {
    /// Unknown directive
    #[error("Unknown directive: @{name}")]
    UnknownDirective { name: String },

    /// Missing required argument
    #[error("Directive @{directive} missing required argument: {argument}")]
    MissingArgument { directive: String, argument: String },

    /// Invalid field list format
    #[error("Invalid field list format in @{directive}: {reason}")]
    InvalidFieldList { directive: String, reason: String },

    /// Incompatible directive combination
    #[error("Incompatible directives on field: {reason}")]
    IncompatibleDirectives { reason: String },
}

/// Parser for Federation directives
pub struct DirectiveParser;

impl DirectiveParser {
    /// Parse a directive by name and arguments
    ///
    /// # Arguments
    ///
    /// * `name` - Directive name (without @)
    /// * `args` - Map of argument name to value
    ///
    /// # Returns
    ///
    /// `Ok(FederationDirective)` if parsing succeeds
    pub fn parse(
        name: &str,
        args: &HashMap<String, String>,
    ) -> Result<FederationDirective, DirectiveError> {
        match name {
            "key" => Self::parse_key(args),
            "external" => Ok(FederationDirective::External),
            "requires" => Self::parse_requires(args),
            "provides" => Self::parse_provides(args),
            _ => Err(DirectiveError::UnknownDirective {
                name: name.to_string(),
            }),
        }
    }

    /// Parse @key directive
    fn parse_key(args: &HashMap<String, String>) -> Result<FederationDirective, DirectiveError> {
        let fields_str = args.get("fields").ok_or(DirectiveError::MissingArgument {
            directive: "key".to_string(),
            argument: "fields".to_string(),
        })?;

        let fields =
            Self::parse_fields(fields_str).map_err(|_| DirectiveError::InvalidFieldList {
                directive: "key".to_string(),
                reason: "Invalid field list format".to_string(),
            })?;

        if fields.is_empty() {
            return Err(DirectiveError::InvalidFieldList {
                directive: "key".to_string(),
                reason: "Field list cannot be empty".to_string(),
            });
        }

        Ok(FederationDirective::Key { fields })
    }

    /// Parse @requires directive
    fn parse_requires(
        args: &HashMap<String, String>,
    ) -> Result<FederationDirective, DirectiveError> {
        let fields_str = args.get("fields").ok_or(DirectiveError::MissingArgument {
            directive: "requires".to_string(),
            argument: "fields".to_string(),
        })?;

        let fields =
            Self::parse_fields(fields_str).map_err(|_| DirectiveError::InvalidFieldList {
                directive: "requires".to_string(),
                reason: "Invalid field list format".to_string(),
            })?;

        if fields.is_empty() {
            return Err(DirectiveError::InvalidFieldList {
                directive: "requires".to_string(),
                reason: "Field list cannot be empty".to_string(),
            });
        }

        Ok(FederationDirective::Requires { fields })
    }

    /// Parse @provides directive
    fn parse_provides(
        args: &HashMap<String, String>,
    ) -> Result<FederationDirective, DirectiveError> {
        let fields_str = args.get("fields").ok_or(DirectiveError::MissingArgument {
            directive: "provides".to_string(),
            argument: "fields".to_string(),
        })?;

        let fields =
            Self::parse_fields(fields_str).map_err(|_| DirectiveError::InvalidFieldList {
                directive: "provides".to_string(),
                reason: "Invalid field list format".to_string(),
            })?;

        if fields.is_empty() {
            return Err(DirectiveError::InvalidFieldList {
                directive: "provides".to_string(),
                reason: "Field list cannot be empty".to_string(),
            });
        }

        Ok(FederationDirective::Provides { fields })
    }

    /// Parse fields string into vec of field names
    ///
    /// Handles formats like:
    /// - "id name email" (space-separated)
    /// - "id, name, email" (comma-separated)
    /// - "{ id name email }" (GraphQL style)
    fn parse_fields(input: &str) -> Result<Vec<String>, ()> {
        let trimmed = input.trim();

        // Remove GraphQL-style braces if present
        let content = if trimmed.starts_with('{') && trimmed.ends_with('}') {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };

        // Split by space or comma, filter empties
        let fields: Vec<String> = content
            .split([' ', ','])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if fields.is_empty() {
            Err(())
        } else {
            Ok(fields)
        }
    }

    /// Validate directive combination on a field
    ///
    /// Ensures directives used together are compatible
    pub fn validate_combination(directives: &[FederationDirective]) -> Result<(), DirectiveError> {
        // Check for incompatible combinations
        let has_key = directives
            .iter()
            .any(|d| matches!(d, FederationDirective::Key { .. }));
        let has_external = directives
            .iter()
            .any(|d| matches!(d, FederationDirective::External));

        // @key and @external are mutually exclusive on the same field
        if has_key && has_external {
            return Err(DirectiveError::IncompatibleDirectives {
                reason: "@key and @external cannot be used together on the same field".to_string(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_directive() {
        let mut args = HashMap::new();
        args.insert("fields".to_string(), "id".to_string());

        let directive = DirectiveParser::parse("key", &args).unwrap();
        assert_eq!(
            directive,
            FederationDirective::Key {
                fields: vec!["id".to_string()]
            }
        );
    }

    #[test]
    fn test_parse_key_with_multiple_fields() {
        let mut args = HashMap::new();
        args.insert("fields".to_string(), "org_id user_id".to_string());

        let directive = DirectiveParser::parse("key", &args).unwrap();
        assert_eq!(
            directive,
            FederationDirective::Key {
                fields: vec!["org_id".to_string(), "user_id".to_string()]
            }
        );
    }

    #[test]
    fn test_parse_external_directive() {
        let args = HashMap::new();
        let directive = DirectiveParser::parse("external", &args).unwrap();
        assert_eq!(directive, FederationDirective::External);
    }

    #[test]
    fn test_parse_requires_directive() {
        let mut args = HashMap::new();
        args.insert("fields".to_string(), "price weight".to_string());

        let directive = DirectiveParser::parse("requires", &args).unwrap();
        assert_eq!(
            directive,
            FederationDirective::Requires {
                fields: vec!["price".to_string(), "weight".to_string()]
            }
        );
    }

    #[test]
    fn test_parse_provides_directive() {
        let mut args = HashMap::new();
        args.insert("fields".to_string(), "id title".to_string());

        let directive = DirectiveParser::parse("provides", &args).unwrap();
        assert_eq!(
            directive,
            FederationDirective::Provides {
                fields: vec!["id".to_string(), "title".to_string()]
            }
        );
    }

    #[test]
    fn test_parse_fields_space_separated() {
        let fields = DirectiveParser::parse_fields("id name email").unwrap();
        assert_eq!(fields, vec!["id", "name", "email"]);
    }

    #[test]
    fn test_parse_fields_comma_separated() {
        let fields = DirectiveParser::parse_fields("id, name, email").unwrap();
        assert_eq!(fields, vec!["id", "name", "email"]);
    }

    #[test]
    fn test_parse_fields_graphql_style() {
        let fields = DirectiveParser::parse_fields("{ id name email }").unwrap();
        assert_eq!(fields, vec!["id", "name", "email"]);
    }

    #[test]
    fn test_parse_unknown_directive() {
        let args = HashMap::new();
        let result = DirectiveParser::parse("unknown", &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_key_missing_fields_arg() {
        let args = HashMap::new();
        let result = DirectiveParser::parse("key", &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_incompatible_key_and_external() {
        let directives = vec![
            FederationDirective::Key {
                fields: vec!["id".to_string()],
            },
            FederationDirective::External,
        ];

        let result = DirectiveParser::validate_combination(&directives);
        assert!(result.is_err());
    }

    #[test]
    fn test_compatible_requires_and_external() {
        let directives = vec![
            FederationDirective::Requires {
                fields: vec!["price".to_string()],
            },
            FederationDirective::External,
        ];

        let result = DirectiveParser::validate_combination(&directives);
        assert!(result.is_ok());
    }

    #[test]
    fn test_directive_display() {
        let directive = FederationDirective::Key {
            fields: vec!["id".to_string()],
        };
        assert_eq!(directive.to_string(), "@key(fields: \"id\")");
    }

    #[test]
    fn test_directive_name() {
        assert_eq!(FederationDirective::External.name(), "external");
        assert_eq!(
            FederationDirective::Requires { fields: vec![] }.name(),
            "requires"
        );
    }
}
