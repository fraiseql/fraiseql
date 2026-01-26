package com.fraiseql.security

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class AttributeBasedAccessControlTest {

    @Test
    fun `should create ABAC policy definition`() {
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
    fun `should create ABAC with variadic attributes`() {
        val config = AuthzPolicyBuilder("financialData")
            .attributes(
                "clearance_level >= 2",
                "department == \"finance\"",
                "mfa_enabled == true"
            )
            .build()

        assertEquals(3, config.attributes.size)
        assertTrue(config.attributes.contains("clearance_level >= 2"))
    }

    @Test
    fun `should create ABAC with array attributes`() {
        val config = AuthzPolicyBuilder("regionalData")
            .attributesArray(listOf("region == \"US\"", "gdpr_compliant == true"))
            .build()

        assertEquals(2, config.attributes.size)
    }

    @Test
    fun `should support clearance level pattern`() {
        val config = AuthzPolicyBuilder("classifiedDocument")
            .type(AuthzPolicyType.Abac)
            .description("Access based on clearance level")
            .attributes("clearance_level >= 2")
            .build()

        assertEquals(AuthzPolicyType.Abac, config.type)
        assertEquals(1, config.attributes.size)
    }

    @Test
    fun `should support department pattern`() {
        val config = AuthzPolicyBuilder("departmentData")
            .type(AuthzPolicyType.Abac)
            .attributes("department == \"HR\"")
            .description("HR department access only")
            .build()

        assertEquals("departmentData", config.name)
    }

    @Test
    fun `should support time-based pattern`() {
        val config = AuthzPolicyBuilder("timeRestrictedData")
            .type(AuthzPolicyType.Abac)
            .attributes(
                "current_time > \"09:00\"",
                "current_time < \"17:00\"",
                "day_of_week != \"Sunday\""
            )
            .description("Business hours access")
            .build()

        assertEquals(3, config.attributes.size)
    }

    @Test
    fun `should support geographic pattern`() {
        val config = AuthzPolicyBuilder("geographicRestriction")
            .type(AuthzPolicyType.Abac)
            .attributes("region in [\"US\", \"CA\", \"MX\"]")
            .description("North American access only")
            .build()

        assertEquals(1, config.attributes.size)
    }

    @Test
    fun `should support GDPR compliance pattern`() {
        val config = AuthzPolicyBuilder("personalData")
            .type(AuthzPolicyType.Abac)
            .attributes(
                "gdpr_compliant == true",
                "data_residency == \"EU\"",
                "consent_given == true"
            )
            .description("GDPR-compliant access")
            .build()

        assertEquals(3, config.attributes.size)
    }

    @Test
    fun `should support project-based pattern`() {
        val config = AuthzPolicyBuilder("projectData")
            .type(AuthzPolicyType.Abac)
            .attributes("user_project == resource_project")
            .description("Users can only access their own projects")
            .build()

        assertEquals(1, config.attributes.size)
    }

    @Test
    fun `should support data classification pattern`() {
        val config = AuthzPolicyBuilder("dataClassification")
            .type(AuthzPolicyType.Abac)
            .attributes(
                "user_classification >= resource_classification",
                "has_need_to_know == true"
            )
            .description("Classification-based access control")
            .build()

        assertEquals(2, config.attributes.size)
    }

    @Test
    fun `should support ABAC caching`() {
        val config = AuthzPolicyBuilder("cachedAbac")
            .type(AuthzPolicyType.Abac)
            .attributes("attribute1 == \"value\"")
            .cacheable(true)
            .cacheDurationSeconds(3600)
            .build()

        assertTrue(config.cacheable)
        assertEquals(3600, config.cacheDurationSeconds)
    }

    @Test
    fun `should support ABAC without cache`() {
        val config = AuthzPolicyBuilder("sensitiveAbac")
            .type(AuthzPolicyType.Abac)
            .attributes("sensitive_attribute == true")
            .cacheable(false)
            .build()

        assertFalse(config.cacheable)
    }

    @Test
    fun `should support ABAC audit logging`() {
        val config = AuthzPolicyBuilder("auditedAbac")
            .type(AuthzPolicyType.Abac)
            .attributes("access_control == true")
            .auditLogging(true)
            .build()

        assertTrue(config.auditLogging)
    }

    @Test
    fun `should support ABAC error message`() {
        val config = AuthzPolicyBuilder("restrictedAbac")
            .type(AuthzPolicyType.Abac)
            .attributes("clearance_level >= 3")
            .errorMessage("Your clearance level is insufficient for this resource")
            .build()

        assertEquals("Your clearance level is insufficient for this resource", config.errorMessage)
    }

    @Test
    fun `should support operation-specific ABAC`() {
        val config = AuthzPolicyBuilder("deleteRestricted")
            .type(AuthzPolicyType.Abac)
            .attributes("role == \"admin\"")
            .operations("delete,create")
            .build()

        assertEquals("delete,create", config.operations)
    }

    @Test
    fun `should support recursive ABAC`() {
        val config = AuthzPolicyBuilder("recursiveAbac")
            .type(AuthzPolicyType.Abac)
            .attributes("hierarchy_level >= 2")
            .recursive(true)
            .build()

        assertTrue(config.recursive)
    }

    @Test
    fun `should support fluent chaining`() {
        val config = AuthzPolicyBuilder("complexAbac")
            .type(AuthzPolicyType.Abac)
            .description("Complex ABAC policy")
            .attributes("clearance >= 2", "department == \"IT\"", "mfa == true")
            .cacheable(true)
            .cacheDurationSeconds(1800)
            .recursive(false)
            .operations("read,update")
            .auditLogging(true)
            .errorMessage("Access denied")
            .build()

        assertEquals("complexAbac", config.name)
        assertEquals(AuthzPolicyType.Abac, config.type)
        assertEquals(3, config.attributes.size)
        assertTrue(config.cacheable)
        assertTrue(config.auditLogging)
    }

    @Test
    fun `should support attributes with rule`() {
        val config = AuthzPolicyBuilder("hybridAbac")
            .type(AuthzPolicyType.Abac)
            .rule("hasAttribute(\$context, 'clearance_level', 3)")
            .attributes("clearance_level >= 3")
            .build()

        assertEquals("hasAttribute(\$context, 'clearance_level', 3)", config.rule)
    }

    @Test
    fun `should support annotation syntax`() {
        @AuthzPolicy(
            name = "abacExample",
            type = "abac",
            attributes = ["clearance >= 2", "department == \"Finance\""]
        )
        class AbacExample

        val annotation = AbacExample::class.java.getAnnotation(AuthzPolicy::class.java)
        assertEquals("abacExample", annotation?.name)
    }
}
