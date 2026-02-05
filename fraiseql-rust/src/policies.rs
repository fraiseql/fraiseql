//! Authorization policies (RBAC, ABAC, Custom, Hybrid)

use std::collections::HashMap;

/// Authorization policy types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthzPolicyType {
    /// Role-based access control
    Rbac,
    /// Attribute-based access control
    Abac,
    /// Custom authorization rules
    Custom,
    /// Hybrid approach combining multiple models
    Hybrid,
}

impl AuthzPolicyType {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rbac => "rbac",
            Self::Abac => "abac",
            Self::Custom => "custom",
            Self::Hybrid => "hybrid",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rbac" => Some(Self::Rbac),
            "abac" => Some(Self::Abac),
            "custom" => Some(Self::Custom),
            "hybrid" => Some(Self::Hybrid),
            _ => None,
        }
    }
}

impl std::fmt::Display for AuthzPolicyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for authorization policies
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzPolicyConfig {
    /// Policy name
    pub name: String,
    /// Policy type
    pub policy_type: AuthzPolicyType,
    /// Description
    pub description: String,
    /// Authorization rule
    pub rule: String,
    /// ABAC attributes
    pub attributes: Vec<String>,
    /// Enable caching
    pub cacheable: bool,
    /// Cache duration in seconds
    pub cache_duration_seconds: u32,
    /// Apply recursively to nested types
    pub recursive: bool,
    /// Operation-specific rules
    pub operations: String,
    /// Enable audit logging
    pub audit_logging: bool,
    /// Custom error message
    pub error_message: String,
}

impl AuthzPolicyConfig {
    /// Create new policy config with name
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            policy_type: AuthzPolicyType::Custom,
            description: String::new(),
            rule: String::new(),
            attributes: Vec::new(),
            cacheable: true,
            cache_duration_seconds: 300,
            recursive: false,
            operations: String::new(),
            audit_logging: false,
            error_message: String::new(),
        }
    }

    /// Convert to HashMap for serialization
    pub fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("name".to_string(), self.name.clone());
        map.insert("type".to_string(), self.policy_type.as_str().to_string());
        map.insert("description".to_string(), self.description.clone());
        map.insert("rule".to_string(), self.rule.clone());
        map.insert(
            "attributes".to_string(),
            self.attributes.join(","),
        );
        map.insert("cacheable".to_string(), self.cacheable.to_string());
        map.insert(
            "cacheDurationSeconds".to_string(),
            self.cache_duration_seconds.to_string(),
        );
        map.insert("recursive".to_string(), self.recursive.to_string());
        map.insert("operations".to_string(), self.operations.clone());
        map.insert("auditLogging".to_string(), self.audit_logging.to_string());
        map.insert("errorMessage".to_string(), self.error_message.clone());
        map
    }
}

/// Fluent builder for authorization policies
#[derive(Debug)]
pub struct AuthzPolicyBuilder {
    name: String,
    policy_type: AuthzPolicyType,
    description: String,
    rule: String,
    attributes: Vec<String>,
    cacheable: bool,
    cache_duration_seconds: u32,
    recursive: bool,
    operations: String,
    audit_logging: bool,
    error_message: String,
}

impl AuthzPolicyBuilder {
    /// Create a new builder
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            policy_type: AuthzPolicyType::Custom,
            description: String::new(),
            rule: String::new(),
            attributes: Vec::new(),
            cacheable: true,
            cache_duration_seconds: 300,
            recursive: false,
            operations: String::new(),
            audit_logging: false,
            error_message: String::new(),
        }
    }

    /// Set policy type
    pub fn policy_type(mut self, policy_type: AuthzPolicyType) -> Self {
        self.policy_type = policy_type;
        self
    }

    /// Set description
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = description.into();
        self
    }

    /// Set authorization rule
    pub fn rule<S: Into<String>>(mut self, rule: S) -> Self {
        self.rule = rule.into();
        self
    }

    /// Set ABAC attributes (variadic)
    pub fn attributes(mut self, attrs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.attributes = attrs.into_iter().map(|a| a.into()).collect();
        self
    }

    /// Set attributes from vector
    pub fn attributes_vec(mut self, attrs: Vec<String>) -> Self {
        self.attributes = attrs;
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

    /// Enable audit logging
    pub fn audit_logging(mut self, audit_logging: bool) -> Self {
        self.audit_logging = audit_logging;
        self
    }

    /// Set error message
    pub fn error_message<S: Into<String>>(mut self, error_message: S) -> Self {
        self.error_message = error_message.into();
        self
    }

    /// Build the configuration
    pub fn build(self) -> AuthzPolicyConfig {
        AuthzPolicyConfig {
            name: self.name,
            policy_type: self.policy_type,
            description: self.description,
            rule: self.rule,
            attributes: self.attributes,
            cacheable: self.cacheable,
            cache_duration_seconds: self.cache_duration_seconds,
            recursive: self.recursive,
            operations: self.operations,
            audit_logging: self.audit_logging,
            error_message: self.error_message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_policy() {
        let config = AuthzPolicyBuilder::new("adminOnly")
            .policy_type(AuthzPolicyType::Rbac)
            .rule("hasRole($context, 'admin')")
            .description("Admin access")
            .audit_logging(true)
            .build();

        assert_eq!(config.name, "adminOnly");
        assert_eq!(config.policy_type, AuthzPolicyType::Rbac);
        assert!(config.audit_logging);
    }

    #[test]
    fn test_abac_policy() {
        let config = AuthzPolicyBuilder::new("secretClearance")
            .policy_type(AuthzPolicyType::Abac)
            .attributes(vec!["clearance_level >= 3", "background_check == true"])
            .description("Top secret")
            .build();

        assert_eq!(config.name, "secretClearance");
        assert_eq!(config.attributes.len(), 2);
    }

    #[test]
    fn test_custom_policy() {
        let config = AuthzPolicyBuilder::new("custom")
            .policy_type(AuthzPolicyType::Custom)
            .rule("isOwner($context)")
            .build();

        assert_eq!(config.policy_type, AuthzPolicyType::Custom);
    }

    #[test]
    fn test_hybrid_policy() {
        let config = AuthzPolicyBuilder::new("hybrid")
            .policy_type(AuthzPolicyType::Hybrid)
            .rule("hasRole($context, 'auditor')")
            .attributes(vec!["audit_enabled == true"])
            .build();

        assert_eq!(config.policy_type, AuthzPolicyType::Hybrid);
    }

    #[test]
    fn test_fluent_chaining() {
        let config = AuthzPolicyBuilder::new("complex")
            .policy_type(AuthzPolicyType::Hybrid)
            .description("Complex policy")
            .rule("hasRole($context, 'admin')")
            .attributes(vec!["security_clearance >= 3"])
            .cacheable(true)
            .cache_duration_seconds(1800)
            .recursive(false)
            .operations("create,update,delete")
            .audit_logging(true)
            .error_message("Insufficient privileges")
            .build();

        assert_eq!(config.name, "complex");
        assert!(config.cacheable);
        assert!(config.audit_logging);
    }

    #[test]
    fn test_default_values() {
        let config = AuthzPolicyBuilder::new("default").build();

        assert_eq!(config.name, "default");
        assert_eq!(config.policy_type, AuthzPolicyType::Custom);
        assert!(config.cacheable);
        assert_eq!(config.cache_duration_seconds, 300);
    }
}
