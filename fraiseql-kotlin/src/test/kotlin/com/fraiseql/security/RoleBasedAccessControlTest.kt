package com.fraiseql.security

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RoleBasedAccessControlTest {

    @Test
    fun `should create single role requirement`() {
        val config = RoleRequiredBuilder()
            .roles("admin")
            .description("Admin role required")
            .build()

        assertEquals(1, config.roles.size)
        assertTrue(config.roles.contains("admin"))
    }

    @Test
    fun `should create multiple role requirements`() {
        val config = RoleRequiredBuilder()
            .roles("manager", "director")
            .description("Manager or director required")
            .build()

        assertEquals(2, config.roles.size)
        assertTrue(config.roles.contains("manager"))
        assertTrue(config.roles.contains("director"))
    }

    @Test
    fun `should create roles from array`() {
        val config = RoleRequiredBuilder()
            .rolesArray(listOf("viewer", "editor", "admin"))
            .description("Multiple roles via array")
            .build()

        assertEquals(3, config.roles.size)
    }

    @Test
    fun `should support ANY matching strategy`() {
        val config = RoleRequiredBuilder()
            .roles("manager", "director")
            .strategy(RoleMatchStrategy.Any)
            .description("User needs at least one role")
            .build()

        assertEquals(RoleMatchStrategy.Any, config.strategy)
    }

    @Test
    fun `should support ALL matching strategy`() {
        val config = RoleRequiredBuilder()
            .roles("admin", "auditor")
            .strategy(RoleMatchStrategy.All)
            .description("User needs all roles")
            .build()

        assertEquals(RoleMatchStrategy.All, config.strategy)
    }

    @Test
    fun `should support EXACTLY matching strategy`() {
        val config = RoleRequiredBuilder()
            .roles("admin")
            .strategy(RoleMatchStrategy.Exactly)
            .description("User must have exactly these roles")
            .build()

        assertEquals(RoleMatchStrategy.Exactly, config.strategy)
    }

    @Test
    fun `should support role hierarchy`() {
        val config = RoleRequiredBuilder()
            .roles("user")
            .hierarchy(true)
            .description("Role hierarchy enabled")
            .build()

        assertTrue(config.hierarchy)
    }

    @Test
    fun `should support role inheritance`() {
        val config = RoleRequiredBuilder()
            .roles("editor")
            .inherit(true)
            .description("Inherit role requirements")
            .build()

        assertTrue(config.inherit)
    }

    @Test
    fun `should support operation-specific rules`() {
        val config = RoleRequiredBuilder()
            .roles("admin")
            .operations("delete,create")
            .description("Admin for destructive operations")
            .build()

        assertEquals("delete,create", config.operations)
    }

    @Test
    fun `should support caching`() {
        val config = RoleRequiredBuilder()
            .roles("viewer")
            .cacheable(true)
            .cacheDurationSeconds(1800)
            .build()

        assertTrue(config.cacheable)
        assertEquals(1800, config.cacheDurationSeconds)
    }

    @Test
    fun `should support custom error message`() {
        val config = RoleRequiredBuilder()
            .roles("admin")
            .errorMessage("You must be an administrator to access this resource")
            .build()

        assertEquals("You must be an administrator to access this resource", config.errorMessage)
    }

    @Test
    fun `should support fluent chaining`() {
        val config = RoleRequiredBuilder()
            .roles("manager", "director")
            .strategy(RoleMatchStrategy.Any)
            .hierarchy(true)
            .description("Manager or director with hierarchy")
            .errorMessage("Insufficient role")
            .operations("read,update")
            .inherit(false)
            .cacheable(true)
            .cacheDurationSeconds(900)
            .build()

        assertEquals(2, config.roles.size)
        assertEquals(RoleMatchStrategy.Any, config.strategy)
        assertTrue(config.hierarchy)
        assertFalse(config.inherit)
        assertEquals(900, config.cacheDurationSeconds)
    }

    @Test
    fun `should support admin pattern`() {
        val config = RoleRequiredBuilder()
            .roles("admin")
            .strategy(RoleMatchStrategy.Exactly)
            .hierarchy(true)
            .description("Full admin access with hierarchy")
            .build()

        assertEquals(1, config.roles.size)
        assertTrue(config.hierarchy)
    }

    @Test
    fun `should support manager pattern`() {
        val config = RoleRequiredBuilder()
            .roles("manager", "director", "executive")
            .strategy(RoleMatchStrategy.Any)
            .description("Management tier access")
            .operations("read,create,update")
            .build()

        assertEquals(3, config.roles.size)
        assertEquals("read,create,update", config.operations)
    }

    @Test
    fun `should support data scientist pattern`() {
        val config = RoleRequiredBuilder()
            .roles("data_scientist", "analyst")
            .strategy(RoleMatchStrategy.Any)
            .description("Data access for scientists and analysts")
            .operations("read")
            .build()

        assertEquals(2, config.roles.size)
    }

    @Test
    fun `should support annotation syntax`() {
        @RoleRequired(
            roles = ["admin"],
            description = "Admin access required"
        )
        class AdminPanel

        val annotation = AdminPanel::class.java.getAnnotation(RoleRequired::class.java)
        assertEquals(1, annotation?.roles?.size)
    }

    @Test
    fun `should support annotation with strategy`() {
        @RoleRequired(
            roles = ["manager", "director"],
            strategy = "any",
            description = "Management access"
        )
        class SalaryData

        val annotation = SalaryData::class.java.getAnnotation(RoleRequired::class.java)
        assertEquals("any", annotation?.strategy)
    }

    @Test
    fun `should support annotation with all parameters`() {
        @RoleRequired(
            roles = ["admin", "auditor"],
            strategy = "all",
            hierarchy = true,
            description = "Full admin with auditor",
            errorMessage = "Insufficient privileges",
            operations = "delete,create",
            inherit = false,
            cacheable = true,
            cacheDurationSeconds = 1200
        )
        class ComplexRoleRequirement

        val annotation = ComplexRoleRequirement::class.java.getAnnotation(RoleRequired::class.java)
        assertTrue(annotation?.hierarchy ?: false)
    }

    @Test
    fun `should create multiple roles with different strategies`() {
        val any = RoleRequiredBuilder()
            .roles("editor", "contributor")
            .strategy(RoleMatchStrategy.Any)
            .build()

        val all = RoleRequiredBuilder()
            .roles("editor", "reviewer")
            .strategy(RoleMatchStrategy.All)
            .build()

        val exactly = RoleRequiredBuilder()
            .roles("admin")
            .strategy(RoleMatchStrategy.Exactly)
            .build()

        assertEquals(RoleMatchStrategy.Any, any.strategy)
        assertEquals(RoleMatchStrategy.All, all.strategy)
        assertEquals(RoleMatchStrategy.Exactly, exactly.strategy)
    }
}
