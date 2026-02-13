//! Custom authorization rules and builder pattern

use std::collections::HashMap;

/// Configuration for custom authorization rules
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizeConfig {
    /// Authorization rule expression
    pub rule: String,
    /// Named policy reference
    pub policy: String,
    /// Configuration description
    pub description: String,
    /// Custom error message on denial
    pub error_message: String,
    /// Apply rule recursively to nested types
    pub recursive: bool,
    /// Operation-specific rules (e.g., "read,create,update,delete")
    pub operations: String,
    /// Enable result caching
    pub cacheable: bool,
    /// Cache duration in seconds
    pub cache_duration_seconds: u32,
}

impl Default for AuthorizeConfig {
    fn default() -> Self {
        Self {
            rule: String::new(),
            policy: String::new(),
            description: String::new(),
            error_message: String::new(),
            recursive: false,
            operations: String::new(),
            cacheable: true,
            cache_duration_seconds: 300,
        }
    }
}

impl AuthorizeConfig {
    /// Convert to HashMap for serialization
    pub fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("rule".to_string(), self.rule.clone());
        map.insert("policy".to_string(), self.policy.clone());
        map.insert("description".to_string(), self.description.clone());
        map.insert("errorMessage".to_string(), self.error_message.clone());
        map.insert("recursive".to_string(), self.recursive.to_string());
        map.insert("operations".to_string(), self.operations.clone());
        map.insert("cacheable".to_string(), self.cacheable.to_string());
        map.insert(
            "cacheDurationSeconds".to_string(),
            self.cache_duration_seconds.to_string(),
        );
        map
    }
}

/// Fluent builder for custom authorization rules
#[derive(Debug, Default)]
pub struct AuthorizeBuilder {
    rule: String,
    policy: String,
    description: String,
    error_message: String,
    recursive: bool,
    operations: String,
    cacheable: bool,
    cache_duration_seconds: u32,
}

impl AuthorizeBuilder {
    /// Create a new builder instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the authorization rule expression
    pub fn rule<S: Into<String>>(mut self, rule: S) -> Self {
        self.rule = rule.into();
        self
    }

    /// Reference a named policy
    pub fn policy<S: Into<String>>(mut self, policy: S) -> Self {
        self.policy = policy.into();
        self
    }

    /// Set the description
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = description.into();
        self
    }

    /// Set the custom error message
    pub fn error_message<S: Into<String>>(mut self, error_message: S) -> Self {
        self.error_message = error_message.into();
        self
    }

    /// Set recursive application
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Set operation-specific rules
    pub fn operations<S: Into<String>>(mut self, operations: S) -> Self {
        self.operations = operations.into();
        self
    }

    /// Enable or disable caching
    pub fn cacheable(mut self, cacheable: bool) -> Self {
        self.cacheable = cacheable;
        self
    }

    /// Set cache duration in seconds
    pub fn cache_duration_seconds(mut self, duration: u32) -> Self {
        self.cache_duration_seconds = duration;
        self
    }

    /// Build the configuration
    pub fn build(self) -> AuthorizeConfig {
        AuthorizeConfig {
            rule: self.rule,
            policy: self.policy,
            description: self.description,
            error_message: self.error_message,
            recursive: self.recursive,
            operations: self.operations,
            cacheable: self.cacheable,
            cache_duration_seconds: self.cache_duration_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_authorization_rule() {
        let config = AuthorizeBuilder::new()
            .rule("isOwner($context.userId, $field.ownerId)")
            .description("Ownership check")
            .build();

        assert_eq!(
            config.rule,
            "isOwner($context.userId, $field.ownerId)"
        );
        assert_eq!(config.description, "Ownership check");
    }

    #[test]
    fn test_fluent_chaining() {
        let config = AuthorizeBuilder::new()
            .rule("hasPermission($context)")
            .description("Complex rule")
            .error_message("Access denied")
            .recursive(true)
            .operations("read")
            .build();

        assert_eq!(config.rule, "hasPermission($context)");
        assert!(config.recursive);
        assert_eq!(config.operations, "read");
    }

    #[test]
    fn test_caching_configuration() {
        let config = AuthorizeBuilder::new()
            .rule("checkAccess($context)")
            .cacheable(true)
            .cache_duration_seconds(600)
            .build();

        assert!(config.cacheable);
        assert_eq!(config.cache_duration_seconds, 600);
    }

    #[test]
    fn test_default_values() {
        let config = AuthorizeBuilder::new().rule("test").build();

        assert!(config.cacheable);
        assert_eq!(config.cache_duration_seconds, 300);
        assert!(!config.recursive);
    }

    #[test]
    fn test_to_map_serialization() {
        let config = AuthorizeBuilder::new()
            .rule("testRule")
            .description("Test")
            .build();

        let map = config.to_map();

        assert_eq!(map.get("rule"), Some(&"testRule".to_string()));
        assert_eq!(map.get("description"), Some(&"Test".to_string()));
    }
}
