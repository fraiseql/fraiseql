//! Validation inheritance for input types.
//!
//! This module provides support for validation rule inheritance, allowing child
//! input types to inherit and override validation rules from parent types.
//!
//! # Examples
//!
//! ```ignore
//! // Parent: UserInput with required email and minLength 5
//! // Child: AdminUserInput extends UserInput with additional admin-only rules
//!
//! let parent_rules = vec![
//!     ValidationRule::Pattern { pattern: "^.+@.+$".to_string(), message: None },
//!     ValidationRule::Length { min: Some(5), max: None }
//! ];
//!
//! let child_rules = vec![
//!     ValidationRule::Required,
//!     ValidationRule::Pattern { pattern: "^admin_.+$".to_string(), message: None }
//! ];
//!
//! let inherited = inherit_validation_rules(&parent_rules, &child_rules, InheritanceMode::Merge);
//! ```

use crate::validation::rules::ValidationRule;
use std::collections::HashMap;

/// Determines how child validation rules interact with parent rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InheritanceMode {
    /// Child rules completely override parent rules (no inheritance)
    Override,
    /// Parent and child rules are merged (all apply)
    Merge,
    /// Child rules are applied first, then parent rules
    ChildFirst,
    /// Parent rules are applied first, then child rules
    ParentFirst,
}

impl InheritanceMode {
    /// Get a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Override => "Child rules override parent rules completely",
            Self::Merge => "All parent and child rules apply (union)",
            Self::ChildFirst => "Child rules applied first, then parent rules",
            Self::ParentFirst => "Parent rules applied first, then child rules",
        }
    }
}

/// Metadata about a validation rule for inheritance tracking.
#[derive(Debug, Clone)]
pub struct RuleMetadata {
    /// The validation rule
    pub rule: ValidationRule,
    /// Whether this rule can be overridden by child types
    pub overrideable: bool,
    /// Whether this rule is inherited from a parent type
    pub inherited: bool,
    /// The source type name for tracking
    pub source: String,
}

impl RuleMetadata {
    /// Create a new rule metadata from a validation rule.
    pub fn new(rule: ValidationRule, source: impl Into<String>) -> Self {
        Self {
            rule,
            overrideable: true,
            inherited: false,
            source: source.into(),
        }
    }

    /// Mark this rule as non-overrideable.
    pub fn non_overrideable(mut self) -> Self {
        self.overrideable = false;
        self
    }

    /// Mark this rule as inherited.
    pub fn as_inherited(mut self) -> Self {
        self.inherited = true;
        self
    }
}

/// Validation rule registry tracking inheritance relationships.
#[derive(Debug, Clone, Default)]
pub struct ValidationRuleRegistry {
    /// Rules by type name
    rules_by_type: HashMap<String, Vec<RuleMetadata>>,
    /// Parent type references
    parent_types: HashMap<String, String>,
}

impl ValidationRuleRegistry {
    /// Create a new validation rule registry.
    pub fn new() -> Self {
        Self {
            rules_by_type: HashMap::new(),
            parent_types: HashMap::new(),
        }
    }

    /// Register rules for a type.
    pub fn register_type(&mut self, type_name: impl Into<String>, rules: Vec<RuleMetadata>) {
        self.rules_by_type.insert(type_name.into(), rules);
    }

    /// Set the parent type for inheritance.
    pub fn set_parent(&mut self, child_type: impl Into<String>, parent_type: impl Into<String>) {
        self.parent_types.insert(child_type.into(), parent_type.into());
    }

    /// Get rules for a type, including inherited rules.
    pub fn get_rules(&self, type_name: &str, mode: InheritanceMode) -> Vec<RuleMetadata> {
        let mut rules = Vec::new();

        // Get parent rules if applicable
        if let Some(parent_name) = self.parent_types.get(type_name) {
            let parent_rules = self.get_rules(parent_name, mode);
            rules.extend(parent_rules.iter().map(|r| r.clone().as_inherited()));
        }

        // Get own rules
        if let Some(own_rules) = self.rules_by_type.get(type_name) {
            match mode {
                InheritanceMode::Override => {
                    // Child rules completely override parent rules
                    return own_rules.clone();
                }
                InheritanceMode::Merge => {
                    // Add all own rules that don't override parent rules
                    for own_rule in own_rules {
                        rules.push(own_rule.clone());
                    }
                }
                InheritanceMode::ChildFirst => {
                    // Keep order: own rules first (reverse order since we prepend)
                    let mut result = own_rules.clone();
                    result.extend(rules);
                    return result;
                }
                InheritanceMode::ParentFirst => {
                    // Keep order: parent rules first
                    rules.extend(own_rules.clone());
                }
            }
        }

        rules
    }

    /// Get the parent type name if one exists.
    pub fn get_parent(&self, type_name: &str) -> Option<&str> {
        self.parent_types.get(type_name).map(|s| s.as_str())
    }

    /// Check if a type has a parent.
    pub fn has_parent(&self, type_name: &str) -> bool {
        self.parent_types.contains_key(type_name)
    }
}

/// Inherit validation rules from parent to child.
///
/// # Arguments
/// * `parent_rules` - Rules from the parent type
/// * `child_rules` - Rules defined on the child type
/// * `mode` - How to combine parent and child rules
///
/// # Returns
/// Combined rules based on the inheritance mode
pub fn inherit_validation_rules(
    parent_rules: &[ValidationRule],
    child_rules: &[ValidationRule],
    mode: InheritanceMode,
) -> Vec<ValidationRule> {
    match mode {
        InheritanceMode::Override => {
            // Child rules completely replace parent rules
            child_rules.to_vec()
        }
        InheritanceMode::Merge => {
            // Combine all rules from both parent and child
            let mut combined = parent_rules.to_vec();
            combined.extend_from_slice(child_rules);
            combined
        }
        InheritanceMode::ChildFirst => {
            // Child rules first, then parent rules
            let mut combined = child_rules.to_vec();
            combined.extend_from_slice(parent_rules);
            combined
        }
        InheritanceMode::ParentFirst => {
            // Parent rules first, then child rules
            let mut combined = parent_rules.to_vec();
            combined.extend_from_slice(child_rules);
            combined
        }
    }
}

/// Check if child type has valid inheritance from parent type.
///
/// # Arguments
/// * `_child_name` - Name of the child type
/// * `parent_name` - Name of the parent type
/// * `registry` - The validation rule registry
///
/// # Returns
/// Ok(()) if inheritance is valid, Err with message if invalid
pub fn validate_inheritance(
    _child_name: &str,
    parent_name: &str,
    registry: &ValidationRuleRegistry,
) -> Result<(), String> {
    // Check that parent type exists in registry
    if !registry.rules_by_type.contains_key(parent_name) {
        return Err(format!(
            "Parent type '{}' not found in validation registry",
            parent_name
        ));
    }

    // Check for circular inheritance
    let mut visited = std::collections::HashSet::new();
    let mut current = Some(parent_name.to_string());

    while let Some(type_name) = current {
        if visited.contains(&type_name) {
            return Err(format!(
                "Circular inheritance detected: '{}' inherits from itself",
                type_name
            ));
        }
        visited.insert(type_name.clone());

        current = registry.get_parent(&type_name).map(|s| s.to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_override_mode() {
        let parent = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
        ];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Override);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], ValidationRule::Pattern { .. }));
    }

    #[test]
    fn test_merge_mode() {
        let parent = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
        ];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_child_first_mode() {
        let parent = vec![ValidationRule::Required];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::ChildFirst);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], ValidationRule::Pattern { .. }));
        assert!(matches!(result[1], ValidationRule::Required));
    }

    #[test]
    fn test_parent_first_mode() {
        let parent = vec![ValidationRule::Required];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::ParentFirst);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], ValidationRule::Required));
        assert!(matches!(result[1], ValidationRule::Pattern { .. }));
    }

    #[test]
    fn test_registry_register_type() {
        let mut registry = ValidationRuleRegistry::new();
        let rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", rules);

        assert!(registry.rules_by_type.contains_key("UserInput"));
    }

    #[test]
    fn test_registry_set_parent() {
        let mut registry = ValidationRuleRegistry::new();
        registry.set_parent("AdminUserInput", "UserInput");

        assert_eq!(
            registry.get_parent("AdminUserInput"),
            Some("UserInput")
        );
    }

    #[test]
    fn test_registry_has_parent() {
        let mut registry = ValidationRuleRegistry::new();
        registry.set_parent("ChildType", "ParentType");

        assert!(registry.has_parent("ChildType"));
        assert!(!registry.has_parent("ParentType"));
    }

    #[test]
    fn test_registry_get_rules_with_merge() {
        let mut registry = ValidationRuleRegistry::new();

        let parent_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", parent_rules);

        let child_rules = vec![RuleMetadata::new(
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", child_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited =
            registry.get_rules("AdminUserInput", InheritanceMode::Merge);
        assert_eq!(inherited.len(), 2);
    }

    #[test]
    fn test_registry_get_rules_with_override() {
        let mut registry = ValidationRuleRegistry::new();

        let parent_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", parent_rules);

        let child_rules = vec![RuleMetadata::new(
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", child_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited = registry.get_rules("AdminUserInput", InheritanceMode::Override);
        assert_eq!(inherited.len(), 1);
        assert!(matches!(inherited[0].rule, ValidationRule::Length { .. }));
    }

    #[test]
    fn test_validate_inheritance_success() {
        let mut registry = ValidationRuleRegistry::new();
        let parent_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", parent_rules);

        let result = validate_inheritance("AdminUserInput", "UserInput", &registry);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_inheritance_parent_not_found() {
        let registry = ValidationRuleRegistry::new();
        let result = validate_inheritance("AdminUserInput", "NonExistent", &registry);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_validate_inheritance_circular() {
        let mut registry = ValidationRuleRegistry::new();

        let user_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", user_rules);

        let admin_rules = vec![RuleMetadata::new(ValidationRule::Required, "AdminUserInput")];
        registry.register_type("AdminUserInput", admin_rules);

        registry.set_parent("UserInput", "AdminUserInput");
        registry.set_parent("AdminUserInput", "UserInput");

        let result = validate_inheritance("UserInput", "AdminUserInput", &registry);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular"));
    }

    #[test]
    fn test_multi_level_inheritance() {
        let mut registry = ValidationRuleRegistry::new();

        // GrandParent
        let grandparent_rules = vec![RuleMetadata::new(ValidationRule::Required, "BaseInput")];
        registry.register_type("BaseInput", grandparent_rules);

        // Parent
        let parent_rules = vec![RuleMetadata::new(
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
            "UserInput",
        )];
        registry.register_type("UserInput", parent_rules);
        registry.set_parent("UserInput", "BaseInput");

        // Child
        let child_rules = vec![RuleMetadata::new(
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", child_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited = registry.get_rules("AdminUserInput", InheritanceMode::Merge);
        // Should have: grandparent rule + parent rule + child rule
        assert_eq!(inherited.len(), 3);
    }

    #[test]
    fn test_rule_metadata_non_overrideable() {
        let rule = RuleMetadata::new(ValidationRule::Required, "UserInput")
            .non_overrideable();
        assert!(!rule.overrideable);
        assert!(rule.inherited == false);
    }

    #[test]
    fn test_rule_metadata_as_inherited() {
        let mut rule = RuleMetadata::new(ValidationRule::Required, "UserInput");
        rule = rule.as_inherited();
        assert!(rule.inherited);
        assert!(rule.overrideable);
    }

    #[test]
    fn test_inheritance_mode_description() {
        assert!(!InheritanceMode::Override.description().is_empty());
        assert!(!InheritanceMode::Merge.description().is_empty());
        assert!(!InheritanceMode::ChildFirst.description().is_empty());
        assert!(!InheritanceMode::ParentFirst.description().is_empty());
    }

    #[test]
    fn test_complex_inheritance_scenario() {
        let mut registry = ValidationRuleRegistry::new();

        // Base: email + minLength 5
        let base_rules = vec![
            RuleMetadata::new(ValidationRule::Required, "BaseInput"),
            RuleMetadata::new(
                ValidationRule::Length {
                    min: Some(5),
                    max: None,
                },
                "BaseInput",
            ),
        ];
        registry.register_type("BaseInput", base_rules);

        // User extends Base: adds pattern
        let user_rules = vec![RuleMetadata::new(
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            "UserInput",
        )];
        registry.register_type("UserInput", user_rules);
        registry.set_parent("UserInput", "BaseInput");

        // Admin extends User: adds enum constraint
        let admin_rules = vec![RuleMetadata::new(
            ValidationRule::Enum {
                values: vec!["admin".to_string(), "moderator".to_string()],
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", admin_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited = registry.get_rules("AdminUserInput", InheritanceMode::Merge);
        // Should have all rules: 2 from base + 1 from user + 1 from admin = 4
        assert_eq!(inherited.len(), 4);
    }

    #[test]
    fn test_empty_child_rules() {
        let parent = vec![ValidationRule::Required];
        let child: Vec<ValidationRule> = vec![];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_empty_parent_rules() {
        let parent: Vec<ValidationRule> = vec![];
        let child = vec![ValidationRule::Required];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_empty_both_rules() {
        let parent: Vec<ValidationRule> = vec![];
        let child: Vec<ValidationRule> = vec![];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert!(result.is_empty());
    }
}
