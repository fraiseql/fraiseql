//! Validation inheritance for input types.
//!
//! This module provides support for validation rule inheritance, allowing child
//! input types to inherit and override validation rules from parent types.
//!
//! # Examples
//!
//! ```
//! use fraiseql_core::validation::{ValidationRule, InheritanceMode, inherit_validation_rules};
//!
//! // Parent: UserInput with required email and minLength 5
//! // Child: AdminUserInput extends UserInput with additional admin-only rules
//!
//! let parent_rules = vec![
//!     ValidationRule::Pattern { pattern: "^.+@.+$".to_string(), message: None },
//!     ValidationRule::Length { min: Some(5), max: None },
//! ];
//!
//! let child_rules = vec![
//!     ValidationRule::Required,
//!     ValidationRule::Pattern { pattern: "^admin_.+$".to_string(), message: None },
//! ];
//!
//! let inherited = inherit_validation_rules(&parent_rules, &child_rules, InheritanceMode::Merge);
//! assert_eq!(inherited.len(), 4);
//! ```

use std::collections::HashMap;

use crate::validation::rules::ValidationRule;

/// Determines how child validation rules interact with parent rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    #[must_use]
    pub const fn description(&self) -> &'static str {
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
    pub rule:         ValidationRule,
    /// Whether this rule can be overridden by child types
    pub overrideable: bool,
    /// Whether this rule is inherited from a parent type
    pub inherited:    bool,
    /// The source type name for tracking
    pub source:       String,
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
    #[must_use]
    pub const fn non_overrideable(mut self) -> Self {
        self.overrideable = false;
        self
    }

    /// Mark this rule as inherited.
    #[must_use]
    pub const fn as_inherited(mut self) -> Self {
        self.inherited = true;
        self
    }
}

/// Validation rule registry tracking inheritance relationships.
#[derive(Debug, Clone, Default)]
pub struct ValidationRuleRegistry {
    /// Rules by type name
    pub(crate) rules_by_type: HashMap<String, Vec<RuleMetadata>>,
    /// Parent type references
    parent_types:             HashMap<String, String>,
}

impl ValidationRuleRegistry {
    /// Create a new validation rule registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules_by_type: HashMap::new(),
            parent_types:  HashMap::new(),
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
    #[must_use]
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
                },
                InheritanceMode::Merge => {
                    // Add all own rules that don't override parent rules
                    for own_rule in own_rules {
                        rules.push(own_rule.clone());
                    }
                },
                InheritanceMode::ChildFirst => {
                    // Keep order: own rules first (reverse order since we prepend)
                    let mut result = own_rules.clone();
                    result.extend(rules);
                    return result;
                },
                InheritanceMode::ParentFirst => {
                    // Keep order: parent rules first
                    rules.extend(own_rules.clone());
                },
            }
        }

        rules
    }

    /// Get the parent type name if one exists.
    #[must_use]
    pub fn get_parent(&self, type_name: &str) -> Option<&str> {
        self.parent_types.get(type_name).map(|s| s.as_str())
    }

    /// Check if a type has a parent.
    #[must_use]
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
#[must_use]
pub fn inherit_validation_rules(
    parent_rules: &[ValidationRule],
    child_rules: &[ValidationRule],
    mode: InheritanceMode,
) -> Vec<ValidationRule> {
    match mode {
        InheritanceMode::Override => {
            // Child rules completely replace parent rules
            child_rules.to_vec()
        },
        InheritanceMode::Merge => {
            // Combine all rules from both parent and child
            let mut combined = parent_rules.to_vec();
            combined.extend_from_slice(child_rules);
            combined
        },
        InheritanceMode::ChildFirst => {
            // Child rules first, then parent rules
            let mut combined = child_rules.to_vec();
            combined.extend_from_slice(parent_rules);
            combined
        },
        InheritanceMode::ParentFirst => {
            // Parent rules first, then child rules
            let mut combined = parent_rules.to_vec();
            combined.extend_from_slice(child_rules);
            combined
        },
    }
}

/// Check if child type has valid inheritance from parent type.
///
/// # Arguments
/// * `_child_name` - Name of the child type
/// * `parent_name` - Name of the parent type
/// * `registry` - The validation rule registry
///
/// # Errors
///
/// Returns an error string if the parent type is not found or contains circular inheritance.
pub fn validate_inheritance(
    _child_name: &str,
    parent_name: &str,
    registry: &ValidationRuleRegistry,
) -> Result<(), String> {
    // Check that parent type exists in registry
    if !registry.rules_by_type.contains_key(parent_name) {
        return Err(format!("Parent type '{}' not found in validation registry", parent_name));
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
