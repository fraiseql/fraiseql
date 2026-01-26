package com.fraiseql.security

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class AuthzPolicyTest {

    @Test
    fun `should create RBAC policy`() {
        val config = AuthzPolicyBuilder("adminOnly")
            .type(AuthzPolicyType.Rbac)
            .rule("hasRole(\$context, 'admin')")
            .description("Access restricted to administrators")
            .auditLogging(true)
            .build()

        assertEquals("adminOnly", config.name)
        assertEquals(AuthzPolicyType.Rbac, config.type)
        assertEquals("hasRole(\$context, 'admin')", config.rule)
        assertTrue(config.auditLogging)
    }

    @Test
    fun `should create ABAC policy`() {
        val config = AuthzPolicyBuilder("secretClearance")
            .type(AuthzPolicyType.Abac)
            .description("Requires top secret clearance")
            .attributes("clearance_level >= 3", "background_check == true")
            .build()

        assertEquals("secretClearance", config.name)
        assertEquals(AuthzPolicyType.Abac, config.type)
        assertEquals(2, config.attributes.size)
    }

    @Test
    fun `should create CUSTOM policy`() {
        val config = AuthzPolicyBuilder("customRule")
            .type(AuthzPolicyType.Custom)
            .rule("isOwner(\$context.userId, \$resource.ownerId)")
            .description("Custom ownership rule")
            .build()

        assertEquals(AuthzPolicyType.Custom, config.type)
    }

    @Test
    fun `should create HYBRID policy`() {
        val config = AuthzPolicyBuilder("auditAccess")
            .type(AuthzPolicyType.Hybrid)
            .description("Role and attribute-based access")
            .rule("hasRole(\$context, 'auditor')")
            .attributes("audit_enabled == true")
            .build()

        assertEquals(AuthzPolicyType.Hybrid, config.type)
        assertEquals("hasRole(\$context, 'auditor')", config.rule)
    }

    @Test
    fun `should create multiple policies`() {
        val policy1 = AuthzPolicyBuilder("policy1")
            .type(AuthzPolicyType.Rbac)
            .build()

        val policy2 = AuthzPolicyBuilder("policy2")
            .type(AuthzPolicyType.Abac)
            .build()

        val policy3 = AuthzPolicyBuilder("policy3")
            .type(AuthzPolicyType.Custom)
            .build()

        assertEquals("policy1", policy1.name)
        assertEquals("policy2", policy2.name)
        assertEquals("policy3", policy3.name)
    }

    @Test
    fun `should create PII access policy`() {
        val config = AuthzPolicyBuilder("piiAccess")
            .type(AuthzPolicyType.Rbac)
            .description("Access to Personally Identifiable Information")
            .rule("hasRole(\$context, 'data_manager') OR hasScope(\$context, 'read:pii')")
            .build()

        assertEquals("piiAccess", config.name)
    }

    @Test
    fun `should create admin only policy`() {
        val config = AuthzPolicyBuilder("adminOnly")
            .type(AuthzPolicyType.Rbac)
            .description("Admin-only access")
            .rule("hasRole(\$context, 'admin')")
            .auditLogging(true)
            .build()

        assertTrue(config.auditLogging)
    }

    @Test
    fun `should create recursive policy`() {
        val config = AuthzPolicyBuilder("recursiveProtection")
            .type(AuthzPolicyType.Custom)
            .rule("canAccessNested(\$context)")
            .recursive(true)
            .description("Recursively applies to nested types")
            .build()

        assertTrue(config.recursive)
    }

    @Test
    fun `should create operation-specific policy`() {
        val config = AuthzPolicyBuilder("readOnly")
            .type(AuthzPolicyType.Custom)
            .rule("hasRole(\$context, 'viewer')")
            .operations("read")
            .description("Policy applies only to read operations")
            .build()

        assertEquals("read", config.operations)
    }

    @Test
    fun `should create cached policy`() {
        val config = AuthzPolicyBuilder("cachedAccess")
            .type(AuthzPolicyType.Custom)
            .rule("hasRole(\$context, 'viewer')")
            .cacheable(true)
            .cacheDurationSeconds(3600)
            .description("Access control with result caching")
            .build()

        assertTrue(config.cacheable)
        assertEquals(3600, config.cacheDurationSeconds)
    }

    @Test
    fun `should create audited policy`() {
        val config = AuthzPolicyBuilder("auditedAccess")
            .type(AuthzPolicyType.Rbac)
            .rule("hasRole(\$context, 'auditor')")
            .auditLogging(true)
            .description("Access with comprehensive audit logging")
            .build()

        assertTrue(config.auditLogging)
    }

    @Test
    fun `should create policy with error message`() {
        val config = AuthzPolicyBuilder("restrictedAccess")
            .type(AuthzPolicyType.Rbac)
            .rule("hasRole(\$context, 'executive')")
            .errorMessage("Only executive level users can access this resource")
            .build()

        assertEquals("Only executive level users can access this resource", config.errorMessage)
    }

    @Test
    fun `should support fluent chaining`() {
        val config = AuthzPolicyBuilder("complexPolicy")
            .type(AuthzPolicyType.Hybrid)
            .description("Complex hybrid policy")
            .rule("hasRole(\$context, 'admin')")
            .attributes("security_clearance >= 3")
            .cacheable(true)
            .cacheDurationSeconds(1800)
            .recursive(false)
            .operations("create,update,delete")
            .auditLogging(true)
            .errorMessage("Insufficient privileges")
            .build()

        assertEquals("complexPolicy", config.name)
        assertEquals(AuthzPolicyType.Hybrid, config.type)
        assertTrue(config.cacheable)
        assertTrue(config.auditLogging)
    }

    @Test
    fun `should create policy composition`() {
        val publicPolicy = AuthzPolicyBuilder("publicAccess")
            .type(AuthzPolicyType.Rbac)
            .rule("true")
            .build()

        val piiPolicy = AuthzPolicyBuilder("piiAccess")
            .type(AuthzPolicyType.Rbac)
            .rule("hasRole(\$context, 'data_manager')")
            .build()

        val adminPolicy = AuthzPolicyBuilder("adminAccess")
            .type(AuthzPolicyType.Rbac)
            .rule("hasRole(\$context, 'admin')")
            .build()

        assertEquals("publicAccess", publicPolicy.name)
        assertEquals("piiAccess", piiPolicy.name)
        assertEquals("adminAccess", adminPolicy.name)
    }

    @Test
    fun `should create financial data policy`() {
        val config = AuthzPolicyBuilder("financialData")
            .type(AuthzPolicyType.Abac)
            .description("Access to financial records")
            .attributes("clearance_level >= 2", "department == \"finance\"")
            .build()

        assertEquals("financialData", config.name)
        assertEquals(2, config.attributes.size)
    }

    @Test
    fun `should create security clearance policy`() {
        val config = AuthzPolicyBuilder("secretClearance")
            .type(AuthzPolicyType.Abac)
            .attributes("clearance_level >= 3", "background_check == true")
            .description("Requires top secret clearance")
            .build()

        assertEquals(2, config.attributes.size)
    }

    @Test
    fun `should support annotation basic syntax`() {
        @AuthzPolicy(
            name = "adminOnly",
            rule = "hasRole(\$context, 'admin')"
        )
        class AdminPolicy

        val annotation = AdminPolicy::class.java.getAnnotation(AuthzPolicy::class.java)
        assertEquals("adminOnly", annotation?.name)
    }

    @Test
    fun `should support annotation with all parameters`() {
        @AuthzPolicy(
            name = "complexPolicy",
            type = "hybrid",
            description = "Complex policy",
            rule = "hasRole(\$context, 'admin')",
            attributes = ["clearance >= 3"],
            errorMessage = "Access denied",
            recursive = true,
            operations = "delete,create",
            auditLogging = true,
            cacheable = true,
            cacheDurationSeconds = 1800
        )
        class ComplexPolicy

        val annotation = ComplexPolicy::class.java.getAnnotation(AuthzPolicy::class.java)
        assertEquals("complexPolicy", annotation?.name)
        assertEquals("hybrid", annotation?.type)
    }

    @Test
    fun `should support all policy types`() {
        val rbac = AuthzPolicyBuilder("rbac")
            .type(AuthzPolicyType.Rbac)
            .build()

        val abac = AuthzPolicyBuilder("abac")
            .type(AuthzPolicyType.Abac)
            .build()

        val custom = AuthzPolicyBuilder("custom")
            .type(AuthzPolicyType.Custom)
            .build()

        val hybrid = AuthzPolicyBuilder("hybrid")
            .type(AuthzPolicyType.Hybrid)
            .build()

        assertEquals(AuthzPolicyType.Rbac, rbac.type)
        assertEquals(AuthzPolicyType.Abac, abac.type)
        assertEquals(AuthzPolicyType.Custom, custom.type)
        assertEquals(AuthzPolicyType.Hybrid, hybrid.type)
    }

    @Test
    fun `should create default configuration`() {
        val config = AuthzPolicyBuilder("default").build()

        assertEquals("default", config.name)
        assertEquals(AuthzPolicyType.Custom, config.type)
        assertTrue(config.cacheable)
        assertEquals(300, config.cacheDurationSeconds)
    }
}
