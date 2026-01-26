package com.fraiseql.security

import spock.lang.Specification

class AuthorizationSpec extends Specification {
    def "should create simple authorization rule"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("isOwner(\$context.userId, \$field.ownerId)")
            .description("Ownership check")
            .build()

        then:
        config.rule == "isOwner(\$context.userId, \$field.ownerId)"
        config.description == "Ownership check"
    }

    def "should create authorization with policy"() {
        when:
        def config = new AuthorizeBuilder()
            .policy("ownerOnly")
            .description("References named policy")
            .build()

        then:
        config.policy == "ownerOnly"
    }

    def "should support fluent chaining"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("hasPermission(\$context)")
            .description("Complex rule")
            .errorMessage("Access denied")
            .recursive(true)
            .operations("read")
            .build()

        then:
        config.rule == "hasPermission(\$context)"
        config.recursive == true
        config.operations == "read"
    }

    def "should set caching configuration"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("checkAccess(\$context)")
            .cacheable(true)
            .cacheDurationSeconds(600)
            .build()

        then:
        config.cacheable == true
        config.cacheDurationSeconds == 600
    }

    def "should set error message"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("adminOnly(\$context)")
            .errorMessage("Only administrators can access this")
            .build()

        then:
        config.errorMessage == "Only administrators can access this"
    }

    def "should set recursive application"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("checkNested(\$context)")
            .recursive(true)
            .description("Applied to nested types")
            .build()

        then:
        config.recursive == true
    }

    def "should set operation specific rule"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("canDelete(\$context)")
            .operations("delete")
            .description("Only applies to delete operations")
            .build()

        then:
        config.operations == "delete"
    }

    def "should convert to map"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("testRule")
            .description("Test")
            .build()
        def map = config.toMap()

        then:
        map.rule == "testRule"
        map.description == "Test"
    }

    def "should create multiple configurations"() {
        when:
        def config1 = new AuthorizeBuilder().rule("rule1").build()
        def config2 = new AuthorizeBuilder().rule("rule2").build()

        then:
        config1.rule != config2.rule
    }

    def "should return default cache settings"() {
        when:
        def config = new AuthorizeBuilder().rule("test").build()

        then:
        config.cacheable == true
        config.cacheDurationSeconds == 300
    }

    def "should set all options"() {
        when:
        def config = new AuthorizeBuilder()
            .rule("complex").policy("policy").description("Complex authorization")
            .errorMessage("Error").recursive(true).operations("create,read,update,delete")
            .cacheable(false).cacheDurationSeconds(1000).build()

        then:
        config.rule == "complex"
        config.cacheable == false
        config.cacheDurationSeconds == 1000
    }
}

class RoleBasedAccessControlSpec extends Specification {
    def "should create single role requirement"() {
        when:
        def config = new RoleRequiredBuilder().roles(["admin"]).build()

        then:
        config.roles.size() == 1
        config.roles[0] == "admin"
    }

    def "should create multiple role requirements"() {
        when:
        def config = new RoleRequiredBuilder().roles(["manager", "director"]).build()

        then:
        config.roles.size() == 2
        config.roles.containsAll(["manager", "director"])
    }

    def "should use any role matching strategy"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["viewer", "editor"]).strategy(RoleMatchStrategy.ANY).build()

        then:
        config.strategy == RoleMatchStrategy.ANY
    }

    def "should use all role matching strategy"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["admin", "auditor"]).strategy(RoleMatchStrategy.ALL).build()

        then:
        config.strategy == RoleMatchStrategy.ALL
    }

    def "should use exactly role matching strategy"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["exact_role"]).strategy(RoleMatchStrategy.EXACTLY).build()

        then:
        config.strategy == RoleMatchStrategy.EXACTLY
    }

    def "should support role hierarchy"() {
        when:
        def config = new RoleRequiredBuilder().roles(["admin"]).hierarchy(true).build()

        then:
        config.hierarchy == true
    }

    def "should support role inheritance"() {
        when:
        def config = new RoleRequiredBuilder().roles(["editor"]).inherit(true).build()

        then:
        config.inherit == true
    }

    def "should set operation specific roles"() {
        when:
        def config = new RoleRequiredBuilder().roles(["editor"]).operations("create,update").build()

        then:
        config.operations == "create,update"
    }

    def "should set custom error message"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["admin"]).errorMessage("Administrator access required").build()

        then:
        config.errorMessage == "Administrator access required"
    }

    def "should configure caching"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["viewer"]).cacheable(true).cacheDurationSeconds(1800).build()

        then:
        config.cacheable == true
        config.cacheDurationSeconds == 1800
    }

    def "should create admin pattern"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["admin"]).strategy(RoleMatchStrategy.ANY).description("Admin access").build()

        then:
        config.roles.size() == 1
        config.roles[0] == "admin"
    }

    def "should create manager director pattern"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["manager", "director"]).strategy(RoleMatchStrategy.ANY).build()

        then:
        config.roles.size() == 2
        config.strategy == RoleMatchStrategy.ANY
    }

    def "should create data scientist pattern"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["data_scientist", "analyst"]).strategy(RoleMatchStrategy.ANY).build()

        then:
        config.roles.size() == 2
    }

    def "should convert to map"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["admin", "editor"]).strategy(RoleMatchStrategy.ANY).build()
        def map = config.toMap()

        then:
        map.strategy == "any"
    }

    def "should set description"() {
        when:
        def config = new RoleRequiredBuilder()
            .roles(["viewer"]).description("Read-only access").build()

        then:
        config.description == "Read-only access"
    }

    def "should return default values"() {
        when:
        def config = new RoleRequiredBuilder().roles(["user"]).build()

        then:
        config.hierarchy == false
        config.inherit == false
        config.cacheable == true
        config.cacheDurationSeconds == 300
    }
}

class AuthzPolicySpec extends Specification {
    def "should create RBAC policy"() {
        when:
        def config = new AuthzPolicyBuilder("adminOnly")
            .type(AuthzPolicyType.RBAC)
            .rule("hasRole(\$context, 'admin')")
            .description("Access restricted to administrators")
            .auditLogging(true)
            .build()

        then:
        config.name == "adminOnly"
        config.policyType == AuthzPolicyType.RBAC
        config.rule == "hasRole(\$context, 'admin')"
        config.auditLogging == true
    }

    def "should create ABAC policy"() {
        when:
        def config = new AuthzPolicyBuilder("secretClearance")
            .type(AuthzPolicyType.ABAC)
            .description("Requires top secret clearance")
            .attributes(["clearance_level >= 3", "background_check == true"])
            .build()

        then:
        config.name == "secretClearance"
        config.policyType == AuthzPolicyType.ABAC
        config.attributes.size() == 2
    }

    def "should create custom policy"() {
        when:
        def config = new AuthzPolicyBuilder("customRule")
            .type(AuthzPolicyType.CUSTOM)
            .rule("isOwner(\$context.userId, \$resource.ownerId)")
            .build()

        then:
        config.policyType == AuthzPolicyType.CUSTOM
    }

    def "should create hybrid policy"() {
        when:
        def config = new AuthzPolicyBuilder("auditAccess")
            .type(AuthzPolicyType.HYBRID)
            .rule("hasRole(\$context, 'auditor')")
            .attributes(["audit_enabled == true"])
            .build()

        then:
        config.policyType == AuthzPolicyType.HYBRID
        config.rule == "hasRole(\$context, 'auditor')"
    }

    def "should create multiple policies"() {
        when:
        def p1 = new AuthzPolicyBuilder("policy1").type(AuthzPolicyType.RBAC).build()
        def p2 = new AuthzPolicyBuilder("policy2").type(AuthzPolicyType.ABAC).build()
        def p3 = new AuthzPolicyBuilder("policy3").type(AuthzPolicyType.CUSTOM).build()

        then:
        p1.name == "policy1"
        p2.name == "policy2"
        p3.name == "policy3"
    }

    def "should create PII access policy"() {
        when:
        def config = new AuthzPolicyBuilder("piiAccess")
            .type(AuthzPolicyType.RBAC)
            .rule("hasRole(\$context, 'data_manager')")
            .build()

        then:
        config.name == "piiAccess"
    }

    def "should create admin only policy"() {
        when:
        def config = new AuthzPolicyBuilder("adminOnly")
            .type(AuthzPolicyType.RBAC)
            .auditLogging(true)
            .build()

        then:
        config.auditLogging == true
    }

    def "should create recursive policy"() {
        when:
        def config = new AuthzPolicyBuilder("recursiveProtection")
            .type(AuthzPolicyType.CUSTOM)
            .recursive(true)
            .build()

        then:
        config.recursive == true
    }

    def "should create operation specific policy"() {
        when:
        def config = new AuthzPolicyBuilder("readOnly")
            .type(AuthzPolicyType.CUSTOM)
            .operations("read")
            .build()

        then:
        config.operations == "read"
    }

    def "should create cached policy"() {
        when:
        def config = new AuthzPolicyBuilder("cachedAccess")
            .type(AuthzPolicyType.CUSTOM)
            .cacheable(true)
            .cacheDurationSeconds(3600)
            .build()

        then:
        config.cacheable == true
        config.cacheDurationSeconds == 3600
    }

    def "should create audited policy"() {
        when:
        def config = new AuthzPolicyBuilder("auditedAccess")
            .type(AuthzPolicyType.RBAC)
            .auditLogging(true)
            .build()

        then:
        config.auditLogging == true
    }

    def "should create policy with error message"() {
        when:
        def config = new AuthzPolicyBuilder("restrictedAccess")
            .type(AuthzPolicyType.RBAC)
            .errorMessage("Only executive level users can access this resource")
            .build()

        then:
        config.errorMessage == "Only executive level users can access this resource"
    }

    def "should support fluent chaining"() {
        when:
        def config = new AuthzPolicyBuilder("complexPolicy")
            .type(AuthzPolicyType.HYBRID)
            .rule("hasRole(\$context, 'admin')")
            .attributes(["security_clearance >= 3"])
            .cacheable(true).cacheDurationSeconds(1800)
            .recursive(false).operations("create,update,delete")
            .auditLogging(true).errorMessage("Insufficient privileges")
            .build()

        then:
        config.name == "complexPolicy"
        config.policyType == AuthzPolicyType.HYBRID
        config.cacheable == true
        config.auditLogging == true
    }

    def "should create policy composition"() {
        when:
        def p1 = new AuthzPolicyBuilder("publicAccess").type(AuthzPolicyType.RBAC).rule("true").build()
        def p2 = new AuthzPolicyBuilder("piiAccess").type(AuthzPolicyType.RBAC).build()
        def p3 = new AuthzPolicyBuilder("adminAccess").type(AuthzPolicyType.RBAC).build()

        then:
        p1.name == "publicAccess"
        p2.name == "piiAccess"
        p3.name == "adminAccess"
    }

    def "should create financial data policy"() {
        when:
        def config = new AuthzPolicyBuilder("financialData")
            .type(AuthzPolicyType.ABAC)
            .attributes(["clearance_level >= 2", "department == \"finance\""])
            .build()

        then:
        config.name == "financialData"
        config.attributes.size() == 2
    }

    def "should create security clearance policy"() {
        when:
        def config = new AuthzPolicyBuilder("secretClearance")
            .type(AuthzPolicyType.ABAC)
            .attributes(["clearance_level >= 3", "background_check == true"])
            .build()

        then:
        config.attributes.size() == 2
    }

    def "should create default configuration"() {
        when:
        def config = new AuthzPolicyBuilder("default").build()

        then:
        config.name == "default"
        config.policyType == AuthzPolicyType.CUSTOM
        config.cacheable == true
        config.cacheDurationSeconds == 300
    }

    def "should convert to map"() {
        when:
        def config = new AuthzPolicyBuilder("test").type(AuthzPolicyType.RBAC).rule("test_rule").build()
        def map = config.toMap()

        then:
        map.name == "test"
        map.type == "rbac"
    }
}
