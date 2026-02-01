package com.fraiseql

import com.fraiseql.schema.Schema
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Test
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * Tests for minimal types.json export (TOML-based workflow)
 *
 * Validates that Schema.exportTypes() function generates minimal schema
 * with only types (no queries, mutations, observers, security, etc.)
 */
class ExportTypesTest {
    @BeforeEach
    fun setUp() {
        Schema.reset()
    }

    @AfterEach
    fun tearDown() {
        Schema.reset()
    }

    @Test
    fun testExportTypesMinimalSingleType() {
        // Register a single type
        Schema.registerType("User", mapOf(
            "id" to mapOf("type" to "ID", "nullable" to false),
            "name" to mapOf("type" to "String", "nullable" to false),
            "email" to mapOf("type" to "String", "nullable" to false),
        ), "User in the system")

        // Export minimal types
        val json = Schema.exportTypes(pretty = true)
        val parsed = Json.parseToJsonElement(json).jsonObject

        // Should have types section
        assertNotNull(parsed["types"])
        assertTrue(parsed["types"] is kotlinx.serialization.json.JsonArray)
        assertEquals(1, (parsed["types"] as kotlinx.serialization.json.JsonArray).size)

        // Should NOT have queries, mutations, observers
        assertFalse(parsed.containsKey("queries"))
        assertFalse(parsed.containsKey("mutations"))
        assertFalse(parsed.containsKey("observers"))
        assertFalse(parsed.containsKey("authz_policies"))

        // Verify User type
        val typesArray = parsed["types"] as kotlinx.serialization.json.JsonArray
        val userDef = typesArray[0].jsonObject
        assertEquals("User", userDef["name"]?.jsonPrimitive?.content)
        assertEquals("User in the system", userDef["description"]?.jsonPrimitive?.content)
    }

    @Test
    fun testExportTypesMultipleTypes() {
        // Register User type
        Schema.registerType("User", mapOf(
            "id" to mapOf("type" to "ID", "nullable" to false),
            "name" to mapOf("type" to "String", "nullable" to false),
        ))

        // Register Post type
        Schema.registerType("Post", mapOf(
            "id" to mapOf("type" to "ID", "nullable" to false),
            "title" to mapOf("type" to "String", "nullable" to false),
            "authorId" to mapOf("type" to "ID", "nullable" to false),
        ))

        // Export minimal
        val json = Schema.exportTypes(pretty = true)
        val parsed = Json.parseToJsonElement(json).jsonObject

        // Check types count
        val typesArray = parsed["types"] as kotlinx.serialization.json.JsonArray
        assertEquals(2, typesArray.size)

        // Verify both types present
        val typeNames = typesArray.map { it.jsonObject["name"]?.jsonPrimitive?.content }
        assertTrue(typeNames.contains("User"))
        assertTrue(typeNames.contains("Post"))
    }

    @Test
    fun testExportTypesNoQueries() {
        // Register type
        Schema.registerType("User", mapOf(
            "id" to mapOf("type" to "ID", "nullable" to false),
        ))

        // Export minimal
        val json = Schema.exportTypes(pretty = true)
        val parsed = Json.parseToJsonElement(json).jsonObject

        // Should have types
        assertNotNull(parsed["types"])

        // Should NOT have queries
        assertFalse(parsed.containsKey("queries"))
        assertFalse(parsed.containsKey("mutations"))
    }

    @Test
    fun testExportTypesCompactFormat() {
        // Register type
        Schema.registerType("User", mapOf(
            "id" to mapOf("type" to "ID", "nullable" to false),
        ))

        // Export compact (pretty=false)
        val compact = Schema.exportTypes(false)
        val pretty = Schema.exportTypes(true)

        // Both should be valid JSON
        assertNotNull(Json.parseToJsonElement(compact))
        assertNotNull(Json.parseToJsonElement(pretty))

        // Compact should be smaller or equal
        assertTrue(compact.length <= pretty.length)
    }

    @Test
    fun testExportTypesPrettyFormat() {
        // Register type
        Schema.registerType("User", mapOf(
            "id" to mapOf("type" to "ID", "nullable" to false),
        ))

        // Export pretty
        val json = Schema.exportTypes(true)

        // Should contain newlines (pretty format)
        assertTrue(json.contains("\n"))

        // Should be valid JSON
        assertNotNull(Json.parseToJsonElement(json))
    }

    @Test
    fun testExportTypesFile() {
        // Register type
        Schema.registerType("User", mapOf(
            "id" to mapOf("type" to "ID", "nullable" to false),
            "name" to mapOf("type" to "String", "nullable" to false),
        ))

        // Export to temporary file
        val tmpFile = "/tmp/fraiseql_types_test_kotlin.json"

        // Clean up if exists
        File(tmpFile).delete()

        // Export to file
        Schema.exportTypesFile(tmpFile)

        // Verify file exists
        assertTrue(File(tmpFile).exists())

        // Verify content
        val content = File(tmpFile).readText()
        val parsed = Json.parseToJsonElement(content).jsonObject

        assertNotNull(parsed["types"])
        val typesArray = parsed["types"] as kotlinx.serialization.json.JsonArray
        assertEquals(1, typesArray.size)

        // Cleanup
        File(tmpFile).delete()
    }

    @Test
    fun testExportTypesEmpty() {
        // Export with no types registered
        val json = Schema.exportTypes(true)
        val parsed = Json.parseToJsonElement(json).jsonObject

        // Should still have types key (as empty array)
        assertNotNull(parsed["types"])
        val typesArray = parsed["types"] as kotlinx.serialization.json.JsonArray
        assertEquals(0, typesArray.size)
    }
}
