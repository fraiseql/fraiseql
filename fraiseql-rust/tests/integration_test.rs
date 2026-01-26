//! Integration tests for FraiseQL Rust security module

use fraiseql_rust::{
    AuthorizeBuilder, RoleRequiredBuilder, AuthzPolicyBuilder,
    RoleMatchStrategy, AuthzPolicyType,
};

// ============================================================================
// AUTHORIZATION TESTS (11 tests)
// ============================================================================

#[test]
fn test_simple_authorization_rule() {
    let config = AuthorizeBuilder::new()
        .rule("isOwner($context.userId, $field.ownerId)")
        .description("Ownership check")
        .build();

    assert_eq!(config.rule, "isOwner($context.userId, $field.ownerId)");
    assert_eq!(config.description, "Ownership check");
}

#[test]
fn test_authorization_with_policy() {
    let config = AuthorizeBuilder::new()
        .policy("ownerOnly")
        .description("References named policy")
        .build();

    assert_eq!(config.policy, "ownerOnly");
}

#[test]
fn test_authorize_fluent_chaining() {
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
fn test_authorize_caching() {
    let config = AuthorizeBuilder::new()
        .rule("checkAccess($context)")
        .cacheable(true)
        .cache_duration_seconds(600)
        .build();

    assert!(config.cacheable);
    assert_eq!(config.cache_duration_seconds, 600);
}

#[test]
fn test_authorize_error_message() {
    let config = AuthorizeBuilder::new()
        .rule("adminOnly($context)")
        .error_message("Only administrators can access this")
        .build();

    assert_eq!(config.error_message, "Only administrators can access this");
}

#[test]
fn test_authorize_recursive() {
    let config = AuthorizeBuilder::new()
        .rule("checkNested($context)")
        .recursive(true)
        .description("Applied to nested types")
        .build();

    assert!(config.recursive);
}

#[test]
fn test_authorize_operation_specific() {
    let config = AuthorizeBuilder::new()
        .rule("canDelete($context)")
        .operations("delete")
        .description("Only applies to delete operations")
        .build();

    assert_eq!(config.operations, "delete");
}

#[test]
fn test_authorize_to_map() {
    let config = AuthorizeBuilder::new()
        .rule("testRule")
        .description("Test")
        .build();

    let map = config.to_map();

    assert_eq!(map.get("rule"), Some(&"testRule".to_string()));
    assert_eq!(map.get("description"), Some(&"Test".to_string()));
}

#[test]
fn test_authorize_multiple_configs() {
    let config1 = AuthorizeBuilder::new()
        .rule("rule1")
        .build();

    let config2 = AuthorizeBuilder::new()
        .rule("rule2")
        .build();

    assert_ne!(config1.rule, config2.rule);
}

#[test]
fn test_authorize_default_cache_settings() {
    let config = AuthorizeBuilder::new()
        .rule("test")
        .build();

    assert!(config.cacheable);
    assert_eq!(config.cache_duration_seconds, 300);
}

#[test]
fn test_authorize_all_options() {
    let config = AuthorizeBuilder::new()
        .rule("complex")
        .policy("policy")
        .description("Complex authorization")
        .error_message("Error")
        .recursive(true)
        .operations("create,read,update,delete")
        .cacheable(false)
        .cache_duration_seconds(1000)
        .build();

    assert_eq!(config.rule, "complex");
    assert!(!config.cacheable);
    assert_eq!(config.cache_duration_seconds, 1000);
}

// ============================================================================
// ROLE BASED ACCESS CONTROL TESTS (18 tests)
// ============================================================================

#[test]
fn test_single_role_requirement() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["admin"])
        .build();

    assert_eq!(config.roles.len(), 1);
    assert_eq!(config.roles[0], "admin");
}

#[test]
fn test_multiple_role_requirements() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["manager", "director"])
        .build();

    assert_eq!(config.roles.len(), 2);
    assert!(config.roles.contains(&"manager".to_string()));
    assert!(config.roles.contains(&"director".to_string()));
}

#[test]
fn test_any_role_strategy() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["viewer", "editor"])
        .strategy(RoleMatchStrategy::Any)
        .description("At least one role")
        .build();

    assert_eq!(config.strategy, RoleMatchStrategy::Any);
    assert_eq!(config.strategy.as_str(), "any");
}

#[test]
fn test_all_role_strategy() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["admin", "auditor"])
        .strategy(RoleMatchStrategy::All)
        .description("All roles required")
        .build();

    assert_eq!(config.strategy, RoleMatchStrategy::All);
    assert_eq!(config.strategy.as_str(), "all");
}

#[test]
fn test_exactly_role_strategy() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["exact_role"])
        .strategy(RoleMatchStrategy::Exactly)
        .description("Exactly these roles")
        .build();

    assert_eq!(config.strategy, RoleMatchStrategy::Exactly);
    assert_eq!(config.strategy.as_str(), "exactly");
}

#[test]
fn test_role_hierarchy() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["admin"])
        .hierarchy(true)
        .description("With hierarchy")
        .build();

    assert!(config.hierarchy);
}

#[test]
fn test_role_inheritance() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["editor"])
        .inherit(true)
        .description("Inherits from parent")
        .build();

    assert!(config.inherit);
}

#[test]
fn test_operation_specific_roles() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["editor"])
        .operations("create,update")
        .description("Only for edit operations")
        .build();

    assert_eq!(config.operations, "create,update");
}

#[test]
fn test_role_error_message() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["admin"])
        .error_message("Administrator access required")
        .build();

    assert_eq!(config.error_message, "Administrator access required");
}

#[test]
fn test_role_caching() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["viewer"])
        .cacheable(true)
        .cache_duration_seconds(1800)
        .build();

    assert!(config.cacheable);
    assert_eq!(config.cache_duration_seconds, 1800);
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
fn test_manager_director_pattern() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["manager", "director"])
        .strategy(RoleMatchStrategy::Any)
        .description("Managers and directors")
        .build();

    assert_eq!(config.roles.len(), 2);
    assert_eq!(config.strategy.as_str(), "any");
}

#[test]
fn test_data_scientist_pattern() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["data_scientist", "analyst"])
        .strategy(RoleMatchStrategy::Any)
        .description("Data professionals")
        .build();

    assert_eq!(config.roles.len(), 2);
}

#[test]
fn test_role_to_map() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["admin", "editor"])
        .strategy(RoleMatchStrategy::Any)
        .build();

    let map = config.to_map();

    assert_eq!(map.get("strategy"), Some(&"any".to_string()));
}

#[test]
fn test_role_description() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["viewer"])
        .description("Read-only access")
        .build();

    assert_eq!(config.description, "Read-only access");
}

#[test]
fn test_role_default_values() {
    let config = RoleRequiredBuilder::new()
        .roles(vec!["user"])
        .build();

    assert!(!config.hierarchy);
    assert!(!config.inherit);
    assert!(config.cacheable);
    assert_eq!(config.cache_duration_seconds, 300);
}

// ============================================================================
// ATTRIBUTE BASED ACCESS CONTROL TESTS (16 tests)
// ============================================================================

#[test]
fn test_abac_policy_creation() {
    let config = AuthzPolicyBuilder::new("accessControl")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["clearance_level >= 2"])
        .description("Basic clearance")
        .build();

    assert_eq!(config.name, "accessControl");
    assert_eq!(config.policy_type, AuthzPolicyType::Abac);
}

#[test]
fn test_multiple_attributes() {
    let config = AuthzPolicyBuilder::new("secretAccess")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["clearance_level >= 3", "background_check == true"])
        .build();

    assert_eq!(config.attributes.len(), 2);
}

#[test]
fn test_clearance_level_policy() {
    let config = AuthzPolicyBuilder::new("topSecret")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["clearance_level >= 3"])
        .description("Top secret clearance required")
        .build();

    assert_eq!(config.attributes.len(), 1);
}

#[test]
fn test_department_policy() {
    let config = AuthzPolicyBuilder::new("financeDept")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["department == \"finance\""])
        .description("Finance department only")
        .build();

    assert_eq!(config.name, "financeDept");
}

#[test]
fn test_time_based_policy() {
    let config = AuthzPolicyBuilder::new("businessHours")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["now >= 9:00 AM", "now <= 5:00 PM"])
        .description("During business hours")
        .build();

    assert_eq!(config.attributes.len(), 2);
}

#[test]
fn test_geographic_policy() {
    let config = AuthzPolicyBuilder::new("usOnly")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["country == \"US\""])
        .description("United States only")
        .build();

    assert_eq!(config.attributes.len(), 1);
}

#[test]
fn test_gdpr_policy() {
    let config = AuthzPolicyBuilder::new("gdprCompliance")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["gdpr_compliant == true", "data_residency == \"EU\""])
        .description("GDPR compliance required")
        .build();

    assert_eq!(config.attributes.len(), 2);
}

#[test]
fn test_data_classification_policy() {
    let config = AuthzPolicyBuilder::new("classifiedData")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["classification >= 2"])
        .description("For classified documents")
        .build();

    assert_eq!(config.attributes.len(), 1);
}

#[test]
fn test_abac_caching() {
    let config = AuthzPolicyBuilder::new("cachedAccess")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["role == \"viewer\""])
        .cacheable(true)
        .cache_duration_seconds(600)
        .build();

    assert!(config.cacheable);
    assert_eq!(config.cache_duration_seconds, 600);
}

#[test]
fn test_abac_audit_logging() {
    let config = AuthzPolicyBuilder::new("auditedAccess")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["audit_enabled == true"])
        .audit_logging(true)
        .build();

    assert!(config.audit_logging);
}

#[test]
fn test_abac_recursive() {
    let config = AuthzPolicyBuilder::new("recursiveAccess")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["permission >= 1"])
        .recursive(true)
        .description("Applies to nested types")
        .build();

    assert!(config.recursive);
}

#[test]
fn test_abac_operation_specific() {
    let config = AuthzPolicyBuilder::new("readOnly")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["can_read == true"])
        .operations("read")
        .build();

    assert_eq!(config.operations, "read");
}

#[test]
fn test_abac_attributes_vec() {
    let attrs = vec!["attr1 >= 1".to_string(), "attr2 == true".to_string()];
    let config = AuthzPolicyBuilder::new("arrayTest")
        .policy_type(AuthzPolicyType::Abac)
        .attributes_vec(attrs)
        .build();

    assert_eq!(config.attributes.len(), 2);
}

#[test]
fn test_complex_abac_policy() {
    let config = AuthzPolicyBuilder::new("complex")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["level >= 2", "verified == true", "active == true"])
        .description("Complex attribute rules")
        .audit_logging(true)
        .cacheable(true)
        .build();

    assert_eq!(config.attributes.len(), 3);
    assert!(config.audit_logging);
}

#[test]
fn test_abac_error_message() {
    let config = AuthzPolicyBuilder::new("restricted")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["clearance >= 2"])
        .error_message("Insufficient clearance level")
        .build();

    assert_eq!(config.error_message, "Insufficient clearance level");
}

// ============================================================================
// AUTHORIZATION POLICY TESTS (19 tests)
// ============================================================================

#[test]
fn test_rbac_policy() {
    let config = AuthzPolicyBuilder::new("adminOnly")
        .policy_type(AuthzPolicyType::Rbac)
        .rule("hasRole($context, 'admin')")
        .description("Access restricted to administrators")
        .audit_logging(true)
        .build();

    assert_eq!(config.name, "adminOnly");
    assert_eq!(config.policy_type, AuthzPolicyType::Rbac);
    assert_eq!(config.rule, "hasRole($context, 'admin')");
    assert!(config.audit_logging);
}

#[test]
fn test_abac_policy_full() {
    let config = AuthzPolicyBuilder::new("secretClearance")
        .policy_type(AuthzPolicyType::Abac)
        .description("Requires top secret clearance")
        .attributes(vec!["clearance_level >= 3", "background_check == true"])
        .build();

    assert_eq!(config.name, "secretClearance");
    assert_eq!(config.policy_type, AuthzPolicyType::Abac);
    assert_eq!(config.attributes.len(), 2);
}

#[test]
fn test_custom_policy() {
    let config = AuthzPolicyBuilder::new("customRule")
        .policy_type(AuthzPolicyType::Custom)
        .rule("isOwner($context.userId, $resource.ownerId)")
        .description("Custom ownership rule")
        .build();

    assert_eq!(config.policy_type, AuthzPolicyType::Custom);
}

#[test]
fn test_hybrid_policy() {
    let config = AuthzPolicyBuilder::new("auditAccess")
        .policy_type(AuthzPolicyType::Hybrid)
        .description("Role and attribute-based access")
        .rule("hasRole($context, 'auditor')")
        .attributes(vec!["audit_enabled == true"])
        .build();

    assert_eq!(config.policy_type, AuthzPolicyType::Hybrid);
    assert_eq!(config.rule, "hasRole($context, 'auditor')");
}

#[test]
fn test_multiple_policies() {
    let policy1 = AuthzPolicyBuilder::new("policy1")
        .policy_type(AuthzPolicyType::Rbac)
        .build();

    let policy2 = AuthzPolicyBuilder::new("policy2")
        .policy_type(AuthzPolicyType::Abac)
        .build();

    let policy3 = AuthzPolicyBuilder::new("policy3")
        .policy_type(AuthzPolicyType::Custom)
        .build();

    assert_eq!(policy1.name, "policy1");
    assert_eq!(policy2.name, "policy2");
    assert_eq!(policy3.name, "policy3");
}

#[test]
fn test_pii_access_policy() {
    let config = AuthzPolicyBuilder::new("piiAccess")
        .policy_type(AuthzPolicyType::Rbac)
        .description("Access to Personally Identifiable Information")
        .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
        .build();

    assert_eq!(config.name, "piiAccess");
}

#[test]
fn test_admin_only_policy() {
    let config = AuthzPolicyBuilder::new("adminOnly")
        .policy_type(AuthzPolicyType::Rbac)
        .description("Admin-only access")
        .rule("hasRole($context, 'admin')")
        .audit_logging(true)
        .build();

    assert!(config.audit_logging);
}

#[test]
fn test_recursive_policy() {
    let config = AuthzPolicyBuilder::new("recursiveProtection")
        .policy_type(AuthzPolicyType::Custom)
        .rule("canAccessNested($context)")
        .recursive(true)
        .description("Recursively applies to nested types")
        .build();

    assert!(config.recursive);
}

#[test]
fn test_operation_specific_policy() {
    let config = AuthzPolicyBuilder::new("readOnly")
        .policy_type(AuthzPolicyType::Custom)
        .rule("hasRole($context, 'viewer')")
        .operations("read")
        .description("Policy applies only to read operations")
        .build();

    assert_eq!(config.operations, "read");
}

#[test]
fn test_cached_policy() {
    let config = AuthzPolicyBuilder::new("cachedAccess")
        .policy_type(AuthzPolicyType::Custom)
        .rule("hasRole($context, 'viewer')")
        .cacheable(true)
        .cache_duration_seconds(3600)
        .description("Access control with result caching")
        .build();

    assert!(config.cacheable);
    assert_eq!(config.cache_duration_seconds, 3600);
}

#[test]
fn test_audited_policy() {
    let config = AuthzPolicyBuilder::new("auditedAccess")
        .policy_type(AuthzPolicyType::Rbac)
        .rule("hasRole($context, 'auditor')")
        .audit_logging(true)
        .description("Access with comprehensive audit logging")
        .build();

    assert!(config.audit_logging);
}

#[test]
fn test_policy_with_error_message() {
    let config = AuthzPolicyBuilder::new("restrictedAccess")
        .policy_type(AuthzPolicyType::Rbac)
        .rule("hasRole($context, 'executive')")
        .error_message("Only executive level users can access this resource")
        .build();

    assert_eq!(config.error_message, "Only executive level users can access this resource");
}

#[test]
fn test_policy_fluent_chaining() {
    let config = AuthzPolicyBuilder::new("complexPolicy")
        .policy_type(AuthzPolicyType::Hybrid)
        .description("Complex hybrid policy")
        .rule("hasRole($context, 'admin')")
        .attributes(vec!["security_clearance >= 3"])
        .cacheable(true)
        .cache_duration_seconds(1800)
        .recursive(false)
        .operations("create,update,delete")
        .audit_logging(true)
        .error_message("Insufficient privileges")
        .build();

    assert_eq!(config.name, "complexPolicy");
    assert_eq!(config.policy_type, AuthzPolicyType::Hybrid);
    assert!(config.cacheable);
    assert!(config.audit_logging);
}

#[test]
fn test_policy_composition() {
    let public_policy = AuthzPolicyBuilder::new("publicAccess")
        .policy_type(AuthzPolicyType::Rbac)
        .rule("true")
        .build();

    let pii_policy = AuthzPolicyBuilder::new("piiAccess")
        .policy_type(AuthzPolicyType::Rbac)
        .rule("hasRole($context, 'data_manager')")
        .build();

    let admin_policy = AuthzPolicyBuilder::new("adminAccess")
        .policy_type(AuthzPolicyType::Rbac)
        .rule("hasRole($context, 'admin')")
        .build();

    assert_eq!(public_policy.name, "publicAccess");
    assert_eq!(pii_policy.name, "piiAccess");
    assert_eq!(admin_policy.name, "adminAccess");
}

#[test]
fn test_financial_data_policy() {
    let config = AuthzPolicyBuilder::new("financialData")
        .policy_type(AuthzPolicyType::Abac)
        .description("Access to financial records")
        .attributes(vec!["clearance_level >= 2", "department == \"finance\""])
        .build();

    assert_eq!(config.name, "financialData");
    assert_eq!(config.attributes.len(), 2);
}

#[test]
fn test_security_clearance_policy() {
    let config = AuthzPolicyBuilder::new("secretClearance")
        .policy_type(AuthzPolicyType::Abac)
        .attributes(vec!["clearance_level >= 3", "background_check == true"])
        .description("Requires top secret clearance")
        .build();

    assert_eq!(config.attributes.len(), 2);
}

#[test]
fn test_default_policy() {
    let config = AuthzPolicyBuilder::new("default").build();

    assert_eq!(config.name, "default");
    assert_eq!(config.policy_type, AuthzPolicyType::Custom);
    assert!(config.cacheable);
    assert_eq!(config.cache_duration_seconds, 300);
}
