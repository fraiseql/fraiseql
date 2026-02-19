package com.fraiseql

import com.fraiseql.schema.*
import kotlinx.serialization.json.*
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.DisplayName
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertContains
import kotlin.test.assertTrue
import kotlin.test.assertFalse

/**
 * Phase 18 Cycle 12: Field-Level RBAC for Kotlin SDK
 *
 * Tests that field scopes are properly extracted from @GraphQLField annotations,
 * stored in field registry, and exported to JSON for compiler consumption.
 *
 * RED Phase: 21 comprehensive test cases
 * - 15 happy path tests for scope extraction and export
 * - 6 validation tests for error handling
 */
@DisplayName("Phase 18 Cycle 12: Kotlin SDK Field Scope Extraction & Export")
class Phase18Cycle12ScopeExtractionTest {

    @BeforeEach
    fun setUp() {
        Schema.reset()
    }

    // =========================================================================
    // HAPPY PATH: SINGLE SCOPE EXTRACTION (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Single scope is extracted from @GraphQLField annotation")
    fun testSingleScopeExtraction() {
        // RED: This test fails because FieldDefinition doesn't store scope
        val fields = mapOf(
            "id" to mapOf("type" to "Int"),
            "salary" to mapOf(
                "type" to "Float",
                "scope" to "read:user.salary"
            )
        )

        Schema.registerType("UserWithScope", fields)

        val typeInfo = SchemaRegistry.getType("UserWithScope")
        assertNotNull(typeInfo)

        val salaryField = typeInfo.fields["salary"]
        assertNotNull(salaryField)
        assertEquals("read:user.salary", salaryField.scope,
            "Salary field should have single scope extracted")
    }

    @Test
    @DisplayName("Multiple different scopes on different fields are extracted correctly")
    fun testMultipleDifferentScopesExtraction() {
        // RED: Tests extraction of different scopes on different fields
        val fields = mapOf(
            "id" to mapOf("type" to "Int"),
            "email" to mapOf("type" to "String", "scope" to "read:user.email"),
            "phone" to mapOf("type" to "String", "scope" to "read:user.phone"),
            "ssn" to mapOf("type" to "String", "scope" to "read:user.ssn")
        )

        Schema.registerType("UserWithMultipleScopes", fields)

        val typeInfo = SchemaRegistry.getType("UserWithMultipleScopes")
        assertNotNull(typeInfo)

        assertEquals("read:user.email", typeInfo.fields["email"]?.scope)
        assertEquals("read:user.phone", typeInfo.fields["phone"]?.scope)
        assertEquals("read:user.ssn", typeInfo.fields["ssn"]?.scope)
    }

    @Test
    @DisplayName("Public field with no scope requirement is handled correctly")
    fun testPublicFieldNoScopeExtraction() {
        // RED: Public fields should have null/empty scope
        val fields = mapOf(
            "id" to mapOf("type" to "Int"),
            "name" to mapOf("type" to "String"),
            "email" to mapOf("type" to "String", "scope" to "read:user.email")
        )

        Schema.registerType("UserWithMixedFields", fields)

        val typeInfo = SchemaRegistry.getType("UserWithMixedFields")
        assertNotNull(typeInfo)

        val idField = typeInfo.fields["id"]
        assertNotNull(idField)
        assertNull(idField.scope, "Public id field should have no scope requirement")
    }

    // =========================================================================
    // HAPPY PATH: MULTIPLE SCOPES ON SINGLE FIELD (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Multiple scopes on single field are extracted as array")
    fun testMultipleScopesOnSingleField() {
        // RED: Field with scopes array
        val fields = mapOf(
            "id" to mapOf("type" to "Int"),
            "adminNotes" to mapOf(
                "type" to "String",
                "scopes" to listOf("admin", "auditor")
            )
        )

        Schema.registerType("AdminWithMultipleScopes", fields)

        val typeInfo = SchemaRegistry.getType("AdminWithMultipleScopes")
        assertNotNull(typeInfo)

        val adminNotesField = typeInfo.fields["adminNotes"]
        assertNotNull(adminNotesField)

        val scopes = adminNotesField.scopes
        assertNotNull(scopes, "Field should have multiple scopes array")
        assertEquals(2, scopes.size, "adminNotes should require 2 scopes")
        assertContains(scopes, "admin")
        assertContains(scopes, "auditor")
    }

    @Test
    @DisplayName("Mixed: some fields with single scope, some with multiple")
    fun testMixedSingleAndMultipleScopes() {
        // RED: Type with both single-scope and multi-scope fields
        val fields = mapOf(
            "basicField" to mapOf("type" to "String", "scope" to "read:basic"),
            "advancedField" to mapOf("type" to "String", "scopes" to listOf("read:advanced", "admin"))
        )

        Schema.registerType("MixedScopeTypes", fields)

        val typeInfo = SchemaRegistry.getType("MixedScopeTypes")
        assertNotNull(typeInfo)

        assertEquals("read:basic", typeInfo.fields["basicField"]?.scope)
        assertEquals(2, typeInfo.fields["advancedField"]?.scopes?.size)
    }

    @Test
    @DisplayName("Scope arrays are preserved in order")
    fun testScopeArrayOrder() {
        // RED: Scopes array order must be preserved
        val fields = mapOf(
            "restricted" to mapOf(
                "type" to "String",
                "scopes" to listOf("first", "second", "third")
            )
        )

        Schema.registerType("OrderedScopes", fields)

        val typeInfo = SchemaRegistry.getType("OrderedScopes")
        assertNotNull(typeInfo)

        val scopes = typeInfo.fields["restricted"]?.scopes
        assertNotNull(scopes)
        assertEquals(3, scopes.size)
        assertEquals("first", scopes[0])
        assertEquals("second", scopes[1])
        assertEquals("third", scopes[2])
    }

    // =========================================================================
    // HAPPY PATH: SCOPE PATTERNS (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Resource-based scope pattern (read:Resource.field)")
    fun testResourceBasedScopePattern() {
        // RED: Resource pattern like read:User.email
        val fields = mapOf(
            "email" to mapOf("type" to "String", "scope" to "read:User.email"),
            "phone" to mapOf("type" to "String", "scope" to "read:User.phone")
        )

        Schema.registerType("ResourcePatternScopes", fields)

        val typeInfo = SchemaRegistry.getType("ResourcePatternScopes")
        assertNotNull(typeInfo)

        assertEquals("read:User.email", typeInfo.fields["email"]?.scope)
    }

    @Test
    @DisplayName("Action-based scope pattern (action:*)")
    fun testActionBasedScopePattern() {
        // RED: Action patterns like read:*, write:*, admin:*
        val fields = mapOf(
            "readableField" to mapOf("type" to "String", "scope" to "read:User.*"),
            "writableField" to mapOf("type" to "String", "scope" to "write:User.*")
        )

        Schema.registerType("ActionPatternScopes", fields)

        val typeInfo = SchemaRegistry.getType("ActionPatternScopes")
        assertNotNull(typeInfo)

        assertEquals("read:User.*", typeInfo.fields["readableField"]?.scope)
        assertEquals("write:User.*", typeInfo.fields["writableField"]?.scope)
    }

    @Test
    @DisplayName("Global wildcard scope (*)")
    fun testGlobalWildcardScope() {
        // RED: Global wildcard matching all scopes
        val fields = mapOf(
            "adminOverride" to mapOf("type" to "String", "scope" to "*")
        )

        Schema.registerType("GlobalWildcardScope", fields)

        val typeInfo = SchemaRegistry.getType("GlobalWildcardScope")
        assertNotNull(typeInfo)

        assertEquals("*", typeInfo.fields["adminOverride"]?.scope,
            "Admin override should use global wildcard")
    }

    // =========================================================================
    // HAPPY PATH: JSON EXPORT (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Scope is exported to JSON for single scope field")
    fun testScopeExportToJsonSingleScope() {
        // RED: Scope must appear in JSON export
        val fields = mapOf(
            "salary" to mapOf("type" to "Float", "scope" to "read:user.salary")
        )

        Schema.registerType("ExportTestSingleScope", fields)

        val json = Schema.exportTypes()
        val schema = Json.parseToJsonElement(json).jsonObject

        val typesArray = schema["types"]?.jsonArray
        assertNotNull(typesArray)
        assertTrue(typesArray.size > 0)

        val typeObj = typesArray[0].jsonObject
        val fieldsArray = typeObj["fields"]?.jsonArray
        assertNotNull(fieldsArray)

        val salaryField = fieldsArray[0].jsonObject
        assertNotNull(salaryField["scope"],
            "JSON should contain scope field")
        assertEquals("read:user.salary", salaryField["scope"]?.jsonPrimitive?.content)
    }

    @Test
    @DisplayName("Scopes array is exported to JSON for multiple scopes field")
    fun testScopeExportToJsonMultipleScopes() {
        // RED: scopes array exported as scopes field
        val fields = mapOf(
            "restricted" to mapOf("type" to "String", "scopes" to listOf("scope1", "scope2"))
        )

        Schema.registerType("ExportTestMultipleScopes", fields)

        val json = Schema.exportTypes()
        val schema = Json.parseToJsonElement(json).jsonObject

        val typesArray = schema["types"]?.jsonArray
        assertNotNull(typesArray)

        val typeObj = typesArray[0].jsonObject
        val fieldsArray = typeObj["fields"]?.jsonArray
        assertNotNull(fieldsArray)

        val field = fieldsArray[0].jsonObject
        val scopesNode = field["scopes"]?.jsonArray
        assertNotNull(scopesNode, "JSON should contain scopes array")
        assertEquals(2, scopesNode.size)
    }

    @Test
    @DisplayName("Public fields without scope are not exported with scope field")
    fun testPublicFieldJsonExport() {
        // RED: Public fields should NOT have scope in JSON
        val fields = mapOf(
            "id" to mapOf("type" to "Int"),
            "name" to mapOf("type" to "String")
        )

        Schema.registerType("ExportTestPublicField", fields)

        val json = Schema.exportTypes()
        val schema = Json.parseToJsonElement(json).jsonObject

        val typesArray = schema["types"]?.jsonArray
        assertNotNull(typesArray)

        val typeObj = typesArray[0].jsonObject
        val fieldsArray = typeObj["fields"]?.jsonArray
        assertNotNull(fieldsArray)

        val idField = fieldsArray[0].jsonObject
        assertNull(idField["scope"],
            "Public field should not have scope in JSON")
        assertNull(idField["scopes"],
            "Public field should not have scopes in JSON")
    }

    // =========================================================================
    // HAPPY PATH: SCOPE WITH OTHER METADATA (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Scope is preserved alongside other field metadata")
    fun testScopePreservedWithMetadata() {
        // RED: Scope doesn't interfere with type, nullable, description
        val fields = mapOf(
            "salary" to mapOf(
                "type" to "Float",
                "scope" to "read:user.salary",
                "description" to "User's annual salary"
            )
        )

        Schema.registerType("ScopeWithMetadata", fields)

        val typeInfo = SchemaRegistry.getType("ScopeWithMetadata")
        assertNotNull(typeInfo)

        val salaryField = typeInfo.fields["salary"]
        assertNotNull(salaryField)
        assertEquals("Float", salaryField.type)
        assertEquals("read:user.salary", salaryField.scope)
        assertEquals("User's annual salary", salaryField.description)
    }

    @Test
    @DisplayName("Scope works with nullable fields")
    fun testScopeWithNullableField() {
        // RED: Scope works on nullable fields
        val fields = mapOf(
            "optionalEmail" to mapOf(
                "type" to "String",
                "nullable" to true,
                "scope" to "read:user.email"
            )
        )

        Schema.registerType("ScopeWithNullable", fields)

        val typeInfo = SchemaRegistry.getType("ScopeWithNullable")
        assertNotNull(typeInfo)

        val emailField = typeInfo.fields["optionalEmail"]
        assertNotNull(emailField)
        assertTrue(emailField.nullable)
        assertEquals("read:user.email", emailField.scope)
    }

    @Test
    @DisplayName("Multiple fields with scopes maintain separate metadata")
    fun testMultipleScopedFieldsMetadataIndependence() {
        // RED: Each field's metadata is independent
        val fields = mapOf(
            "field1" to mapOf(
                "type" to "String",
                "scope" to "scope1",
                "description" to "Desc 1"
            ),
            "field2" to mapOf(
                "type" to "String",
                "scope" to "scope2",
                "description" to "Desc 2"
            )
        )

        Schema.registerType("MetadataIndependence", fields)

        val typeInfo = SchemaRegistry.getType("MetadataIndependence")
        assertNotNull(typeInfo)

        assertEquals("scope1", typeInfo.fields["field1"]?.scope)
        assertEquals("Desc 1", typeInfo.fields["field1"]?.description)
        assertEquals("scope2", typeInfo.fields["field2"]?.scope)
        assertEquals("Desc 2", typeInfo.fields["field2"]?.description)
    }

    // =========================================================================
    // VALIDATION: ERROR HANDLING (6 tests)
    // =========================================================================

    @Test
    @DisplayName("Invalid scope format is detected and raises error")
    fun testInvalidScopeFormatDetection() {
        // RED: Invalid scopes should be detected
        val fields = mapOf(
            "field" to mapOf(
                "type" to "String",
                "scope" to "invalid_scope_no_colon"
            )
        )

        assertThrows<RuntimeException> {
            Schema.registerType("InvalidScopeFormat", fields)
        }
    }

    @Test
    @DisplayName("Empty scope string is rejected")
    fun testEmptyScopeRejection() {
        // RED: Empty string scope invalid
        val fields = mapOf(
            "field" to mapOf("type" to "String", "scope" to "")
        )

        assertThrows<RuntimeException> {
            Schema.registerType("EmptyScope", fields)
        }
    }

    @Test
    @DisplayName("Empty scopes array is rejected")
    fun testEmptyScopesArrayRejection() {
        // RED: Empty array not allowed
        val fields = mapOf(
            "field" to mapOf("type" to "String", "scopes" to emptyList<String>())
        )

        assertThrows<RuntimeException> {
            Schema.registerType("EmptyScopesArray", fields)
        }
    }

    @Test
    @DisplayName("Scope validation catches invalid action prefix")
    fun testInvalidActionPrefixValidation() {
        // RED: Invalid action prefix format
        val fields = mapOf(
            "field" to mapOf("type" to "String", "scope" to "invalid-action:resource")
        )

        assertThrows<RuntimeException> {
            Schema.registerType("InvalidActionWithHyphens", fields)
        }
    }

    @Test
    @DisplayName("Scope validation catches invalid resource name")
    fun testInvalidResourceNameValidation() {
        // RED: Invalid resource name format
        val fields = mapOf(
            "field" to mapOf("type" to "String", "scope" to "read:invalid-resource-name")
        )

        assertThrows<RuntimeException> {
            Schema.registerType("InvalidResourceWithHyphens", fields)
        }
    }

    @Test
    @DisplayName("Conflicting both scope and scopes is rejected")
    fun testConflictingBothScopeAndScopes() {
        // RED: Can't have both scope and scopes on same field
        val fields = mapOf(
            "field" to mapOf(
                "type" to "String",
                "scope" to "read:user.email",
                "scopes" to listOf("admin", "auditor")
            )
        )

        assertThrows<RuntimeException> {
            Schema.registerType("ConflictingScopeAndScopes", fields)
        }
    }
}
