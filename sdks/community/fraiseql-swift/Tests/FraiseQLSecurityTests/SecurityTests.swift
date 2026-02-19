import XCTest
@testable import FraiseQLSecurity

// MARK: - Authorization Tests (11 tests)

final class AuthorizationTests: XCTestCase {
    func testSimpleAuthorizationRule() {
        let config = AuthorizeBuilder()
            .rule("isOwner($context.userId, $field.ownerId)")
            .description("Ownership check")
            .build()

        XCTAssertEqual(config.rule, "isOwner($context.userId, $field.ownerId)")
        XCTAssertEqual(config.description, "Ownership check")
    }

    func testAuthorizationWithPolicy() {
        let config = AuthorizeBuilder()
            .policy("ownerOnly")
            .description("References named policy")
            .build()

        XCTAssertEqual(config.policy, "ownerOnly")
    }

    func testFluentChaining() {
        let config = AuthorizeBuilder()
            .rule("hasPermission($context)")
            .description("Complex rule")
            .errorMessage("Access denied")
            .recursive(true)
            .operations("read")
            .build()

        XCTAssertEqual(config.rule, "hasPermission($context)")
        XCTAssertTrue(config.recursive)
        XCTAssertEqual(config.operations, "read")
    }

    func testCachingConfiguration() {
        let config = AuthorizeBuilder()
            .rule("checkAccess($context)")
            .cacheable(true)
            .cacheDurationSeconds(600)
            .build()

        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 600)
    }

    func testErrorMessage() {
        let config = AuthorizeBuilder()
            .rule("adminOnly($context)")
            .errorMessage("Only administrators can access this")
            .build()

        XCTAssertEqual(config.errorMessage, "Only administrators can access this")
    }

    func testRecursive() {
        let config = AuthorizeBuilder()
            .rule("checkNested($context)")
            .recursive(true)
            .description("Applied to nested types")
            .build()

        XCTAssertTrue(config.recursive)
    }

    func testOperationSpecific() {
        let config = AuthorizeBuilder()
            .rule("canDelete($context)")
            .operations("delete")
            .description("Only applies to delete operations")
            .build()

        XCTAssertEqual(config.operations, "delete")
    }

    func testToDictionary() {
        let config = AuthorizeBuilder()
            .rule("testRule")
            .description("Test")
            .build()

        let dict = config.toDictionary()

        XCTAssertEqual(dict["rule"] as? String, "testRule")
        XCTAssertEqual(dict["description"] as? String, "Test")
    }

    func testMultipleConfigurations() {
        let config1 = AuthorizeBuilder()
            .rule("rule1")
            .build()

        let config2 = AuthorizeBuilder()
            .rule("rule2")
            .build()

        XCTAssertNotEqual(config1.rule, config2.rule)
    }

    func testDefaultCacheSettings() {
        let config = AuthorizeBuilder()
            .rule("test")
            .build()

        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 300)
    }

    func testAllOptions() {
        let config = AuthorizeBuilder()
            .rule("complex")
            .policy("policy")
            .description("Complex authorization")
            .errorMessage("Error")
            .recursive(true)
            .operations("create,read,update,delete")
            .cacheable(false)
            .cacheDurationSeconds(1000)
            .build()

        XCTAssertEqual(config.rule, "complex")
        XCTAssertFalse(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 1000)
    }
}

// MARK: - Role Based Access Control Tests (18 tests)

final class RoleBasedAccessControlTests: XCTestCase {
    func testSingleRoleRequirement() {
        let config = RoleRequiredBuilder()
            .roles(["admin"])
            .build()

        XCTAssertEqual(config.roles.count, 1)
        XCTAssertEqual(config.roles[0], "admin")
    }

    func testMultipleRoleRequirements() {
        let config = RoleRequiredBuilder()
            .roles(["manager", "director"])
            .build()

        XCTAssertEqual(config.roles.count, 2)
        XCTAssertTrue(config.roles.contains("manager"))
        XCTAssertTrue(config.roles.contains("director"))
    }

    func testAnyRoleStrategy() {
        let config = RoleRequiredBuilder()
            .roles(["viewer", "editor"])
            .strategy(.any)
            .description("At least one role")
            .build()

        XCTAssertEqual(config.strategy, .any)
    }

    func testAllRoleStrategy() {
        let config = RoleRequiredBuilder()
            .roles(["admin", "auditor"])
            .strategy(.all)
            .description("All roles required")
            .build()

        XCTAssertEqual(config.strategy, .all)
    }

    func testExactlyRoleStrategy() {
        let config = RoleRequiredBuilder()
            .roles(["exact_role"])
            .strategy(.exactly)
            .description("Exactly these roles")
            .build()

        XCTAssertEqual(config.strategy, .exactly)
    }

    func testRoleHierarchy() {
        let config = RoleRequiredBuilder()
            .roles(["admin"])
            .hierarchy(true)
            .description("With hierarchy")
            .build()

        XCTAssertTrue(config.hierarchy)
    }

    func testRoleInheritance() {
        let config = RoleRequiredBuilder()
            .roles(["editor"])
            .inherit(true)
            .description("Inherits from parent")
            .build()

        XCTAssertTrue(config.inherit)
    }

    func testOperationSpecificRoles() {
        let config = RoleRequiredBuilder()
            .roles(["editor"])
            .operations("create,update")
            .description("Only for edit operations")
            .build()

        XCTAssertEqual(config.operations, "create,update")
    }

    func testRoleErrorMessage() {
        let config = RoleRequiredBuilder()
            .roles(["admin"])
            .errorMessage("Administrator access required")
            .build()

        XCTAssertEqual(config.errorMessage, "Administrator access required")
    }

    func testRoleCaching() {
        let config = RoleRequiredBuilder()
            .roles(["viewer"])
            .cacheable(true)
            .cacheDurationSeconds(1800)
            .build()

        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 1800)
    }

    func testAdminPattern() {
        let config = RoleRequiredBuilder()
            .roles(["admin"])
            .strategy(.any)
            .description("Admin access")
            .build()

        XCTAssertEqual(config.roles.count, 1)
        XCTAssertEqual(config.roles[0], "admin")
    }

    func testManagerDirectorPattern() {
        let config = RoleRequiredBuilder()
            .roles(["manager", "director"])
            .strategy(.any)
            .description("Managers and directors")
            .build()

        XCTAssertEqual(config.roles.count, 2)
        XCTAssertEqual(config.strategy, .any)
    }

    func testDataScientistPattern() {
        let config = RoleRequiredBuilder()
            .roles(["data_scientist", "analyst"])
            .strategy(.any)
            .description("Data professionals")
            .build()

        XCTAssertEqual(config.roles.count, 2)
    }

    func testRoleToDictionary() {
        let config = RoleRequiredBuilder()
            .roles(["admin", "editor"])
            .strategy(.any)
            .build()

        let dict = config.toDictionary()

        XCTAssertEqual(dict["strategy"] as? String, "any")
    }

    func testRoleDescription() {
        let config = RoleRequiredBuilder()
            .roles(["viewer"])
            .description("Read-only access")
            .build()

        XCTAssertEqual(config.description, "Read-only access")
    }

    func testRoleDefaultValues() {
        let config = RoleRequiredBuilder()
            .roles(["user"])
            .build()

        XCTAssertFalse(config.hierarchy)
        XCTAssertFalse(config.inherit)
        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 300)
    }
}

// MARK: - Attribute Based Access Control Tests (16 tests)

final class AttributeBasedAccessControlTests: XCTestCase {
    func testABACPolicyCreation() {
        let config = AuthzPolicyBuilder("accessControl")
            .type(.abac)
            .attributes(["clearance_level >= 2"])
            .description("Basic clearance")
            .build()

        XCTAssertEqual(config.name, "accessControl")
        XCTAssertEqual(config.type, .abac)
    }

    func testMultipleAttributes() {
        let config = AuthzPolicyBuilder("secretAccess")
            .type(.abac)
            .attributes(["clearance_level >= 3", "background_check == true"])
            .build()

        XCTAssertEqual(config.attributes.count, 2)
    }

    func testClearanceLevelPolicy() {
        let config = AuthzPolicyBuilder("topSecret")
            .type(.abac)
            .attributes(["clearance_level >= 3"])
            .description("Top secret clearance required")
            .build()

        XCTAssertEqual(config.attributes.count, 1)
    }

    func testDepartmentPolicy() {
        let config = AuthzPolicyBuilder("financeDept")
            .type(.abac)
            .attributes(["department == \"finance\""])
            .description("Finance department only")
            .build()

        XCTAssertEqual(config.name, "financeDept")
    }

    func testTimeBasedPolicy() {
        let config = AuthzPolicyBuilder("businessHours")
            .type(.abac)
            .attributes(["now >= 9:00 AM", "now <= 5:00 PM"])
            .description("During business hours")
            .build()

        XCTAssertEqual(config.attributes.count, 2)
    }

    func testGeographicPolicy() {
        let config = AuthzPolicyBuilder("usOnly")
            .type(.abac)
            .attributes(["country == \"US\""])
            .description("United States only")
            .build()

        XCTAssertEqual(config.attributes.count, 1)
    }

    func testGDPRPolicy() {
        let config = AuthzPolicyBuilder("gdprCompliance")
            .type(.abac)
            .attributes(["gdpr_compliant == true", "data_residency == \"EU\""])
            .description("GDPR compliance required")
            .build()

        XCTAssertEqual(config.attributes.count, 2)
    }

    func testDataClassificationPolicy() {
        let config = AuthzPolicyBuilder("classifiedData")
            .type(.abac)
            .attributes(["classification >= 2"])
            .description("For classified documents")
            .build()

        XCTAssertEqual(config.attributes.count, 1)
    }

    func testABACCaching() {
        let config = AuthzPolicyBuilder("cachedAccess")
            .type(.abac)
            .attributes(["role == \"viewer\""])
            .cacheable(true)
            .cacheDurationSeconds(600)
            .build()

        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 600)
    }

    func testABACAuditLogging() {
        let config = AuthzPolicyBuilder("auditedAccess")
            .type(.abac)
            .attributes(["audit_enabled == true"])
            .auditLogging(true)
            .build()

        XCTAssertTrue(config.auditLogging)
    }

    func testABACRecursive() {
        let config = AuthzPolicyBuilder("recursiveAccess")
            .type(.abac)
            .attributes(["permission >= 1"])
            .recursive(true)
            .description("Applies to nested types")
            .build()

        XCTAssertTrue(config.recursive)
    }

    func testABACOperationSpecific() {
        let config = AuthzPolicyBuilder("readOnly")
            .type(.abac)
            .attributes(["can_read == true"])
            .operations("read")
            .build()

        XCTAssertEqual(config.operations, "read")
    }

    func testComplexABACPolicy() {
        let config = AuthzPolicyBuilder("complex")
            .type(.abac)
            .attributes(["level >= 2", "verified == true", "active == true"])
            .description("Complex attribute rules")
            .auditLogging(true)
            .cacheable(true)
            .build()

        XCTAssertEqual(config.attributes.count, 3)
        XCTAssertTrue(config.auditLogging)
    }

    func testABACErrorMessage() {
        let config = AuthzPolicyBuilder("restricted")
            .type(.abac)
            .attributes(["clearance >= 2"])
            .errorMessage("Insufficient clearance level")
            .build()

        XCTAssertEqual(config.errorMessage, "Insufficient clearance level")
    }

    func testABACToDictionary() {
        let config = AuthzPolicyBuilder("test")
            .type(.abac)
            .attributes(["test >= 1"])
            .build()

        let dict = config.toDictionary()

        XCTAssertEqual(dict["type"] as? String, "abac")
    }

    func testABACDefaultValues() {
        let config = AuthzPolicyBuilder("default")
            .type(.abac)
            .build()

        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 300)
        XCTAssertFalse(config.recursive)
    }
}

// MARK: - Authorization Policy Tests (19 tests)

final class AuthzPolicyTests: XCTestCase {
    func testRBACPolicy() {
        let config = AuthzPolicyBuilder("adminOnly")
            .type(.rbac)
            .rule("hasRole($context, 'admin')")
            .description("Access restricted to administrators")
            .auditLogging(true)
            .build()

        XCTAssertEqual(config.name, "adminOnly")
        XCTAssertEqual(config.type, .rbac)
        XCTAssertEqual(config.rule, "hasRole($context, 'admin')")
        XCTAssertTrue(config.auditLogging)
    }

    func testABACPolicyFull() {
        let config = AuthzPolicyBuilder("secretClearance")
            .type(.abac)
            .description("Requires top secret clearance")
            .attributes(["clearance_level >= 3", "background_check == true"])
            .build()

        XCTAssertEqual(config.name, "secretClearance")
        XCTAssertEqual(config.type, .abac)
        XCTAssertEqual(config.attributes.count, 2)
    }

    func testCustomPolicy() {
        let config = AuthzPolicyBuilder("customRule")
            .type(.custom)
            .rule("isOwner($context.userId, $resource.ownerId)")
            .description("Custom ownership rule")
            .build()

        XCTAssertEqual(config.type, .custom)
    }

    func testHybridPolicy() {
        let config = AuthzPolicyBuilder("auditAccess")
            .type(.hybrid)
            .description("Role and attribute-based access")
            .rule("hasRole($context, 'auditor')")
            .attributes(["audit_enabled == true"])
            .build()

        XCTAssertEqual(config.type, .hybrid)
        XCTAssertEqual(config.rule, "hasRole($context, 'auditor')")
    }

    func testMultiplePolicies() {
        let policy1 = AuthzPolicyBuilder("policy1")
            .type(.rbac)
            .build()

        let policy2 = AuthzPolicyBuilder("policy2")
            .type(.abac)
            .build()

        let policy3 = AuthzPolicyBuilder("policy3")
            .type(.custom)
            .build()

        XCTAssertEqual(policy1.name, "policy1")
        XCTAssertEqual(policy2.name, "policy2")
        XCTAssertEqual(policy3.name, "policy3")
    }

    func testPIIAccessPolicy() {
        let config = AuthzPolicyBuilder("piiAccess")
            .type(.rbac)
            .description("Access to Personally Identifiable Information")
            .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
            .build()

        XCTAssertEqual(config.name, "piiAccess")
    }

    func testAdminOnlyPolicy() {
        let config = AuthzPolicyBuilder("adminOnly")
            .type(.rbac)
            .description("Admin-only access")
            .rule("hasRole($context, 'admin')")
            .auditLogging(true)
            .build()

        XCTAssertTrue(config.auditLogging)
    }

    func testRecursivePolicy() {
        let config = AuthzPolicyBuilder("recursiveProtection")
            .type(.custom)
            .rule("canAccessNested($context)")
            .recursive(true)
            .description("Recursively applies to nested types")
            .build()

        XCTAssertTrue(config.recursive)
    }

    func testOperationSpecificPolicy() {
        let config = AuthzPolicyBuilder("readOnly")
            .type(.custom)
            .rule("hasRole($context, 'viewer')")
            .operations("read")
            .description("Policy applies only to read operations")
            .build()

        XCTAssertEqual(config.operations, "read")
    }

    func testCachedPolicy() {
        let config = AuthzPolicyBuilder("cachedAccess")
            .type(.custom)
            .rule("hasRole($context, 'viewer')")
            .cacheable(true)
            .cacheDurationSeconds(3600)
            .description("Access control with result caching")
            .build()

        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 3600)
    }

    func testAuditedPolicy() {
        let config = AuthzPolicyBuilder("auditedAccess")
            .type(.rbac)
            .rule("hasRole($context, 'auditor')")
            .auditLogging(true)
            .description("Access with comprehensive audit logging")
            .build()

        XCTAssertTrue(config.auditLogging)
    }

    func testPolicyWithErrorMessage() {
        let config = AuthzPolicyBuilder("restrictedAccess")
            .type(.rbac)
            .rule("hasRole($context, 'executive')")
            .errorMessage("Only executive level users can access this resource")
            .build()

        XCTAssertEqual(config.errorMessage, "Only executive level users can access this resource")
    }

    func testPolicyFluentChaining() {
        let config = AuthzPolicyBuilder("complexPolicy")
            .type(.hybrid)
            .description("Complex hybrid policy")
            .rule("hasRole($context, 'admin')")
            .attributes(["security_clearance >= 3"])
            .cacheable(true)
            .cacheDurationSeconds(1800)
            .recursive(false)
            .operations("create,update,delete")
            .auditLogging(true)
            .errorMessage("Insufficient privileges")
            .build()

        XCTAssertEqual(config.name, "complexPolicy")
        XCTAssertEqual(config.type, .hybrid)
        XCTAssertTrue(config.cacheable)
        XCTAssertTrue(config.auditLogging)
    }

    func testPolicyComposition() {
        let publicPolicy = AuthzPolicyBuilder("publicAccess")
            .type(.rbac)
            .rule("true")
            .build()

        let piiPolicy = AuthzPolicyBuilder("piiAccess")
            .type(.rbac)
            .rule("hasRole($context, 'data_manager')")
            .build()

        let adminPolicy = AuthzPolicyBuilder("adminAccess")
            .type(.rbac)
            .rule("hasRole($context, 'admin')")
            .build()

        XCTAssertEqual(publicPolicy.name, "publicAccess")
        XCTAssertEqual(piiPolicy.name, "piiAccess")
        XCTAssertEqual(adminPolicy.name, "adminAccess")
    }

    func testFinancialDataPolicy() {
        let config = AuthzPolicyBuilder("financialData")
            .type(.abac)
            .description("Access to financial records")
            .attributes(["clearance_level >= 2", "department == \"finance\""])
            .build()

        XCTAssertEqual(config.name, "financialData")
        XCTAssertEqual(config.attributes.count, 2)
    }

    func testSecurityClearancePolicy() {
        let config = AuthzPolicyBuilder("secretClearance")
            .type(.abac)
            .attributes(["clearance_level >= 3", "background_check == true"])
            .description("Requires top secret clearance")
            .build()

        XCTAssertEqual(config.attributes.count, 2)
    }

    func testDefaultPolicy() {
        let config = AuthzPolicyBuilder("default").build()

        XCTAssertEqual(config.name, "default")
        XCTAssertEqual(config.type, .custom)
        XCTAssertTrue(config.cacheable)
        XCTAssertEqual(config.cacheDurationSeconds, 300)
    }

    func testPolicyToDictionary() {
        let config = AuthzPolicyBuilder("test")
            .type(.rbac)
            .rule("test_rule")
            .build()

        let dict = config.toDictionary()

        XCTAssertEqual(dict["name"] as? String, "test")
        XCTAssertEqual(dict["type"] as? String, "rbac")
    }
}
