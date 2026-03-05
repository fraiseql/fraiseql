package com.fraiseql

import com.fraiseql.schema.Schema
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * SDK-3: Schema roundtrip golden test.
 *
 * Exercises the full decorator → JSON export pipeline: register a type
 * with fields (including a scoped field), export to JSON, and verify that
 * the output matches the expected schema.json structure exactly.
 *
 * This is the contract between the SDK and the fraiseql-cli compiler.
 * If the SDK produces malformed JSON the compiler rejects it — but
 * without this test that failure is silent during SDK development.
 */
class SchemaRoundtripTest {

    @BeforeEach
    fun setUp() {
        Schema.reset()
    }

    @AfterEach
    fun tearDown() {
        Schema.reset()
    }

    @Test
    fun `full decorator to export pipeline produces expected schema json structure`() {
        // Register a realistic type with a mix of field types, including a scoped field.
        Schema.registerType("Article", mapOf(
            "id"    to mapOf("type" to "ID",     "nullable" to false),
            "title" to mapOf("type" to "String", "nullable" to false),
            "body"  to mapOf("type" to "String", "nullable" to true),
            "email" to mapOf("type" to "String", "nullable" to false, "scope" to "read:Article.email"),
        ), "A published article")

        val jsonStr = Schema.exportTypes(pretty = true)
        val parsed = Json.parseToJsonElement(jsonStr).jsonObject

        // Must contain exactly the `types` key — no compiler-reserved keys
        assertTrue(parsed.containsKey("types"),    "output must have `types` key")
        assertFalse(parsed.containsKey("queries"),    "output must NOT have `queries`")
        assertFalse(parsed.containsKey("mutations"),  "output must NOT have `mutations`")
        assertFalse(parsed.containsKey("observers"),  "output must NOT have `observers`")
        assertFalse(parsed.containsKey("security"),   "output must NOT have `security`")
        assertFalse(parsed.containsKey("federation"), "output must NOT have `federation`")

        // Exactly one type was registered
        val types = parsed["types"]!!.jsonArray
        assertEquals(1, types.size, "exactly one type was registered")

        // Verify Article type structure
        val article = types[0].jsonObject
        assertEquals("Article",            article["name"]!!.jsonPrimitive.content, "type name must be Article")
        assertEquals("A published article", article["description"]!!.jsonPrimitive.content, "description must round-trip")

        // All four fields must be present
        val fields     = article["fields"]!!.jsonArray
        val fieldNames = fields.map { it.jsonObject["name"]!!.jsonPrimitive.content }
        assertTrue(fieldNames.contains("id"),    "field `id` must be present")
        assertTrue(fieldNames.contains("title"), "field `title` must be present")
        assertTrue(fieldNames.contains("body"),  "field `body` must be present")
        assertTrue(fieldNames.contains("email"), "field `email` must be present")

        // The scoped field must carry its scope annotation
        val emailField = fields.map { it.jsonObject }.firstOrNull {
            it["name"]!!.jsonPrimitive.content == "email"
        }
        assertNotNull(emailField, "email field must be present in output")
        assertEquals("read:Article.email", emailField!!["scope"]!!.jsonPrimitive.content,
            "scope annotation must round-trip")
    }

    @Test
    fun `multiple registered types all appear with correct names`() {
        Schema.registerType("User", mapOf(
            "id"   to mapOf("type" to "ID",     "nullable" to false),
            "name" to mapOf("type" to "String", "nullable" to false),
        ), "System user")

        Schema.registerType("Post", mapOf(
            "id"    to mapOf("type" to "ID",     "nullable" to false),
            "title" to mapOf("type" to "String", "nullable" to false),
        ), "Blog post")

        val parsed = Json.parseToJsonElement(Schema.exportTypes(pretty = true)).jsonObject
        val types  = parsed["types"]!!.jsonArray
        val names  = types.map { it.jsonObject["name"]!!.jsonPrimitive.content }

        assertEquals(2, types.size, "two types must be exported")
        assertTrue(names.contains("User"), "User must be present")
        assertTrue(names.contains("Post"), "Post must be present")
    }

    @Test
    fun `exported JSON satisfies the schema json structural contract`() {
        Schema.registerType("Order", mapOf(
            "id"     to mapOf("type" to "ID",     "nullable" to false),
            "amount" to mapOf("type" to "Float",  "nullable" to false),
            "status" to mapOf("type" to "String", "nullable" to true),
        ))

        val parsed = Json.parseToJsonElement(Schema.exportTypes(pretty = true)).jsonObject

        // Top-level shape: only `types`
        assertEquals(setOf("types"), parsed.keys,
            "schema.json for types-only export must contain exactly the `types` key")

        // Each type entry must have at minimum `name` and `fields`
        for (t in parsed["types"]!!.jsonArray) {
            val obj = t.jsonObject
            assertTrue(obj.containsKey("name"),   "every type entry must have `name`")
            assertTrue(obj.containsKey("fields"), "every type entry must have `fields`")
        }
    }
}
