package com.fraiseql.security

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class AuthorizationTest {

    @Test
    fun `should create authorization rule builder`() {
        val config = AuthorizeBuilder()
            .rule("isOwner(\$context.userId, \$field.ownerId)")
            .description("Ensures users can only access their own notes")
            .build()

        assertEquals("isOwner(\$context.userId, \$field.ownerId)", config.rule)
        assertEquals("Ensures users can only access their own notes", config.description)
    }

    @Test
    fun `should create authorization with policy reference`() {
        val config = AuthorizeBuilder()
            .policy("piiAccess")
            .description("References the piiAccess policy")
            .build()

        assertEquals("piiAccess", config.policy)
        assertTrue(config.cacheable)
    }

    @Test
    fun `should create authorization with error message`() {
        val config = AuthorizeBuilder()
            .rule("hasRole(\$context, 'admin')")
            .errorMessage("Only administrators can access this resource")
            .build()

        assertEquals("Only administrators can access this resource", config.errorMessage)
    }

    @Test
    fun `should create recursive authorization`() {
        val config = AuthorizeBuilder()
            .rule("canAccessNested(\$context)")
            .recursive(true)
            .description("Recursively applies to nested types")
            .build()

        assertTrue(config.recursive)
    }

    @Test
    fun `should create operation-specific authorization`() {
        val config = AuthorizeBuilder()
            .rule("isAdmin(\$context)")
            .operations("create,delete")
            .description("Only applies to create and delete operations")
            .build()

        assertEquals("create,delete", config.operations)
    }

    @Test
    fun `should create authorization with caching`() {
        val config = AuthorizeBuilder()
            .rule("checkAuthorization(\$context)")
            .cacheable(true)
            .cacheDurationSeconds(3600)
            .build()

        assertTrue(config.cacheable)
        assertEquals(3600, config.cacheDurationSeconds)
    }

    @Test
    fun `should create authorization without caching`() {
        val config = AuthorizeBuilder()
            .rule("checkSensitiveAuthorization(\$context)")
            .cacheable(false)
            .build()

        assertFalse(config.cacheable)
    }

    @Test
    fun `should create multiple authorization rules`() {
        val config1 = AuthorizeBuilder()
            .rule("isOwner(\$context.userId, \$field.ownerId)")
            .description("Ownership check")
            .build()

        val config2 = AuthorizeBuilder()
            .rule("hasScope(\$context, 'read:notes')")
            .description("Scope check")
            .build()

        assertTrue(config1.rule != config2.rule)
    }

    @Test
    fun `should support fluent chaining`() {
        val config = AuthorizeBuilder()
            .rule("isOwner(\$context.userId, \$field.ownerId)")
            .description("Ownership authorization")
            .errorMessage("You can only access your own notes")
            .recursive(false)
            .operations("read,update")
            .cacheable(true)
            .cacheDurationSeconds(600)
            .build()

        assertEquals("isOwner(\$context.userId, \$field.ownerId)", config.rule)
        assertEquals("Ownership authorization", config.description)
        assertEquals("You can only access your own notes", config.errorMessage)
        assertFalse(config.recursive)
        assertEquals("read,update", config.operations)
        assertTrue(config.cacheable)
        assertEquals(600, config.cacheDurationSeconds)
    }

    @Test
    fun `should support annotation syntax`() {
        @Authorize(
            rule = "isOwner(\$context.userId, \$field.ownerId)",
            description = "Ownership check"
        )
        class ProtectedNote

        val annotation = ProtectedNote::class.java.getAnnotation(Authorize::class.java)
        assertEquals("isOwner(\$context.userId, \$field.ownerId)", annotation?.rule)
    }

    @Test
    fun `should support annotation with all parameters`() {
        @Authorize(
            rule = "isOwner(\$context.userId, \$field.ownerId)",
            description = "Ownership check",
            errorMessage = "Access denied",
            recursive = true,
            operations = "read",
            cacheable = false,
            cacheDurationSeconds = 0
        )
        class FullyConfiguredNote

        val annotation = FullyConfiguredNote::class.java.getAnnotation(Authorize::class.java)
        assertTrue(annotation?.recursive ?: false)
    }
}
