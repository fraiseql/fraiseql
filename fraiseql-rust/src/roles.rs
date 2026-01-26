//! Role-based access control (RBAC)

use std::collections::HashMap;

/// Role matching strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleMatchStrategy {
    /// At least one role must match
    Any,
    /// All roles must match
    All,
    /// Exactly these roles
    Exactly,
}

impl RoleMatchStrategy {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::All => "all",
            Self::Exactly => "exactly",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "any" => Some(Self::Any),
            "all" => Some(Self::All),
            "exactly" => Some(Self::Exactly),
            _ => None,
        }
    }
}

impl std::fmt::Display for RoleMatchStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for role-based access control
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleRequiredConfig {
    /// Required roles
    pub roles: Vec<String>,
    /// Role matching strategy
    pub strategy: RoleMatchStrategy,
    /// Support role hierarchy
    pub hierarchy: bool,
    /// Description
    pub description: String,
    /// Custom error message
    pub error_message: String,
    /// Operation-specific rules
    pub operations: String,
    /// Inherit from parent
    pub inherit: bool,
    /// Enable caching
    pub cacheable: bool,
    /// Cache duration in seconds
    pub cache_duration_seconds: u32,
}

impl Default for RoleRequiredConfig {
    fn default() -> Self {
        Self {
            roles: Vec::new(),
            strategy: RoleMatchStrategy::Any,
            hierarchy: false,
            description: String::new(),
            error_message: String::new(),
            operations: String::new(),
            inherit: false,
            cacheable: true,
            cache_duration_seconds: 300,
        }
    }
}

impl RoleRequiredConfig {
    /// Convert to HashMap for serialization
    pub fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert(
            "roles".to_string(),
            self.roles.join(","),
        );
        map.insert("strategy".to_string(), self.strategy.as_str().to_string());
        map.insert("hierarchy".to_string(), self.hierarchy.to_string());
        map.insert("description".to_string(), self.description.clone());
        map.insert("errorMessage".to_string(), self.error_message.clone());
        map.insert("operations".to_string(), self.operations.clone());
        map.insert("inherit".to_string(), self.inherit.to_string());
        map.insert("cacheable".to_string(), self.cacheable.to_string());
        map.insert(
            "cacheDurationSeconds".to_string(),
            self.cache_duration_seconds.to_string(),
        );
        map
    }
}

/// Fluent builder for role-based access control
#[derive(Debug, Default)]
pub struct RoleRequiredBuilder {
    roles: Vec<String>,
    strategy: RoleMatchStrategy,
    hierarchy: bool,
    description: String,
    error_message: String,
    operations: String,
    inherit: bool,
    cacheable: bool,
    cache_duration_seconds: u32,
}

impl RoleRequiredBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set required roles (variadic)
    pub fn roles(mut self, roles: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.roles = roles.into_iter().map(|r| r.into()).collect();
        self
    }

    /// Set required roles from vector
    pub fn roles_vec(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    /// Set role matching strategy
    pub fn strategy(mut self, strategy: RoleMatchStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Enable role hierarchy
    pub fn hierarchy(mut self, hierarchy: bool) -> Self {
        self.hierarchy = hierarchy;
        self
    }

    /// Set description
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = description.into();
        self
    }

    /// Set error message
    pub fn error_message<S: Into<String>>(mut self, error_message: S) -> Self {
        self.error_message = error_message.into();
        self
    }

    /// Set operation-specific rules
    pub fn operations<S: Into<String>>(mut self, operations: S) -> Self {
        self.operations = operations.into();
        self
    }

    /// Enable role inheritance
    pub fn inherit(mut self, inherit: bool) -> Self {
        self.inherit = inherit;
        self
    }

    /// Enable caching
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
    pub fn build(self) -> RoleRequiredConfig {
        RoleRequiredConfig {
            roles: self.roles,
            strategy: self.strategy,
            hierarchy: self.hierarchy,
            description: self.description,
            error_message: self.error_message,
            operations: self.operations,
            inherit: self.inherit,
            cacheable: self.cacheable,
            cache_duration_seconds: self.cache_duration_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_role_requirement() {
        let config = RoleRequiredBuilder::new()
            .roles(vec!["admin"])
            .build();

        assert_eq!(config.roles.len(), 1);
        assert_eq!(config.roles[0], "admin");
    }

    #[test]
    fn test_multiple_roles() {
        let config = RoleRequiredBuilder::new()
            .roles(vec!["manager", "director"])
            .build();

        assert_eq!(config.roles.len(), 2);
        assert!(config.roles.contains(&"manager".to_string()));
    }

    #[test]
    fn test_role_strategies() {
        let any_config = RoleRequiredBuilder::new()
            .roles(vec!["viewer", "editor"])
            .strategy(RoleMatchStrategy::Any)
            .build();

        assert_eq!(any_config.strategy, RoleMatchStrategy::Any);
        assert_eq!(any_config.strategy.as_str(), "any");
    }

    #[test]
    fn test_admin_pattern() {
        let config = RoleRequiredBuilder::new()
            .roles(vec!["admin"])
            .strategy(RoleMatchStrategy::Any)
            .description("Admin access")
            .build();

        assert_eq!(config.roles.len(), 1);
        assert_eq!(config.roles[0], "admin");
    }

    #[test]
    fn test_default_values() {
        let config = RoleRequiredBuilder::new()
            .roles(vec!["user"])
            .build();

        assert!(!config.hierarchy);
        assert!(!config.inherit);
        assert!(config.cacheable);
        assert_eq!(config.cache_duration_seconds, 300);
    }
}
