package com.fraiseql.core;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.Optional;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Phase 18 Cycle 9: Field-Level RBAC for Java SDK
 *
 * Tests that field scopes are properly extracted from @GraphQLField annotations,
 * stored in the schema registry, and exported to JSON for compiler consumption.
 *
 * RED Phase: 21 comprehensive test cases
 * - 15 happy path tests for scope extraction and export
 * - 6 validation tests for error handling
 */
@DisplayName("Phase 18 Cycle 9: Java SDK Field Scope Extraction & Export")
public class Phase18Cycle9ScopeExtractionTest {

    private SchemaRegistry registry;
    private ObjectMapper mapper;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
        mapper = new ObjectMapper();
    }

    // =========================================================================
    // HAPPY PATH: SINGLE SCOPE EXTRACTION (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Single scope is extracted from @GraphQLField annotation")
    void testSingleScopeExtraction() {
        // RED: This test fails because GraphQLFieldInfo doesn't store scope
        FraiseQL.registerType(UserWithSingleScope.class);

        var typeInfo = registry.getType("UserWithSingleScope");
        assertTrue(typeInfo.isPresent());

        var salaryField = typeInfo.get().fields.get("salary");
        assertNotNull(salaryField);
        assertEquals("read:user.salary", salaryField.getRequiresScope(),
            "Salary field should have single scope extracted");
    }

    @Test
    @DisplayName("Multiple different scopes on different fields are extracted correctly")
    void testMultipleDifferentScopesExtraction() {
        // RED: Tests extraction of different scopes on different fields
        FraiseQL.registerType(UserWithMultipleScopes.class);

        var typeInfo = registry.getType("UserWithMultipleScopes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals("read:user.email", fields.get("email").getRequiresScope());
        assertEquals("read:user.phone", fields.get("phone").getRequiresScope());
        assertEquals("read:user.ssn", fields.get("ssn").getRequiresScope());
    }

    @Test
    @DisplayName("Public field with no scope requirement is handled correctly")
    void testPublicFieldNoScopeExtraction() {
        // RED: Public fields should have null/empty scope
        FraiseQL.registerType(UserWithMixedFields.class);

        var typeInfo = registry.getType("UserWithMixedFields");
        assertTrue(typeInfo.isPresent());

        var idField = typeInfo.get().fields.get("id");
        assertNotNull(idField);
        assertNull(idField.getRequiresScope(),
            "Public id field should have no scope requirement");
    }

    // =========================================================================
    // HAPPY PATH: MULTIPLE SCOPES ON SINGLE FIELD (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Multiple scopes on single field are extracted as array")
    void testMultipleScopesOnSingleField() {
        // RED: Field with requiresScopes = {...} array
        FraiseQL.registerType(AdminWithMultipleScopesPerField.class);

        var typeInfo = registry.getType("AdminWithMultipleScopesPerField");
        assertTrue(typeInfo.isPresent());

        var adminNotesField = typeInfo.get().fields.get("adminNotes");
        assertNotNull(adminNotesField);

        var scopes = adminNotesField.getRequiresScopes();
        assertNotNull(scopes, "Field should have multiple scopes array");
        assertEquals(2, scopes.length, "adminNotes should require 2 scopes");
        assertArrayContains(scopes, "admin");
        assertArrayContains(scopes, "auditor");
    }

    @Test
    @DisplayName("Mixed: some fields with single scope, some with multiple")
    void testMixedSingleAndMultipleScopes() {
        // RED: Type with both single-scope and multi-scope fields
        FraiseQL.registerType(MixedScopeTypes.class);

        var typeInfo = registry.getType("MixedScopeTypes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;

        // Single scope field
        assertEquals("read:basic", fields.get("basicField").getRequiresScope());

        // Multiple scope field
        var advancedScopes = fields.get("advancedField").getRequiresScopes();
        assertNotNull(advancedScopes);
        assertEquals(2, advancedScopes.length);
    }

    @Test
    @DisplayName("Scope arrays are preserved in order")
    void testScopeArrayOrder() {
        // RED: Scopes array order must be preserved
        FraiseQL.registerType(OrderedScopes.class);

        var typeInfo = registry.getType("OrderedScopes");
        assertTrue(typeInfo.isPresent());

        var scopes = typeInfo.get().fields.get("restricted").getRequiresScopes();
        assertNotNull(scopes);
        assertEquals("first", scopes[0]);
        assertEquals("second", scopes[1]);
        assertEquals("third", scopes[2]);
    }

    // =========================================================================
    // HAPPY PATH: SCOPE PATTERNS (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Resource-based scope pattern (read:Resource.field)")
    void testResourceBasedScopePattern() {
        // RED: Resource pattern like read:User.email
        FraiseQL.registerType(ResourcePatternScopes.class);

        var typeInfo = registry.getType("ResourcePatternScopes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals("read:User.email", fields.get("email").getRequiresScope());
        assertEquals("read:User.phone", fields.get("phone").getRequiresScope());
    }

    @Test
    @DisplayName("Action-based scope pattern (action:*)")
    void testActionBasedScopePattern() {
        // RED: Action patterns like read:*, write:*, admin:*
        FraiseQL.registerType(ActionPatternScopes.class);

        var typeInfo = registry.getType("ActionPatternScopes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals("read:User.*", fields.get("readableField").getRequiresScope());
        assertEquals("write:User.*", fields.get("writableField").getRequiresScope());
    }

    @Test
    @DisplayName("Global wildcard scope (*)")
    void testGlobalWildcardScope() {
        // RED: Global wildcard matching all scopes
        FraiseQL.registerType(GlobalWildcardScope.class);

        var typeInfo = registry.getType("GlobalWildcardScope");
        assertTrue(typeInfo.isPresent());

        var adminField = typeInfo.get().fields.get("adminOverride");
        assertEquals("*", adminField.getRequiresScope(),
            "Admin override should use global wildcard");
    }

    // =========================================================================
    // HAPPY PATH: JSON EXPORT (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Scope is exported to JSON for single scope field")
    void testScopeExportToJsonSingleScope() {
        // RED: Scope must appear in JSON export
        FraiseQL.registerType(ExportTestSingleScope.class);

        var schema = SchemaFormatter.formatMinimalSchema(registry);
        assertNotNull(schema);

        var json = mapper.readTree(schema);
        var salaryField = json.get("types").get(0).get("fields").get("salary");

        assertNotNull(salaryField.get("requires_scope"),
            "JSON should contain requires_scope field");
        assertEquals("read:user.salary", salaryField.get("requires_scope").asText());
    }

    @Test
    @DisplayName("Scopes array is exported to JSON for multiple scopes field")
    void testScopeExportToJsonMultipleScopes() {
        // RED: requiresScopes array exported as requires_scopes
        FraiseQL.registerType(ExportTestMultipleScopes.class);

        var schema = SchemaFormatter.formatMinimalSchema(registry);
        assertNotNull(schema);

        var json = mapper.readTree(schema);
        var restrictedField = json.get("types").get(0).get("fields").get("restricted");

        var scopesNode = restrictedField.get("requires_scopes");
        assertNotNull(scopesNode, "JSON should contain requires_scopes array");
        assertTrue(scopesNode.isArray());
        assertEquals(2, scopesNode.size());
    }

    @Test
    @DisplayName("Public fields without scope are not exported with scope field")
    void testPublicFieldJsonExport() {
        // RED: Public fields should NOT have requires_scope in JSON
        FraiseQL.registerType(ExportTestPublicField.class);

        var schema = SchemaFormatter.formatMinimalSchema(registry);
        assertNotNull(schema);

        var json = mapper.readTree(schema);
        var idField = json.get("types").get(0).get("fields").get("id");

        assertNull(idField.get("requires_scope"),
            "Public field should not have requires_scope in JSON");
    }

    // =========================================================================
    // HAPPY PATH: SCOPE WITH OTHER METADATA (3 tests)
    // =========================================================================

    @Test
    @DisplayName("Scope is preserved alongside other field metadata")
    void testScopePreservedWithMetadata() {
        // RED: Scope doesn't interfere with description, nullable, etc.
        FraiseQL.registerType(ScopeWithMetadata.class);

        var typeInfo = registry.getType("ScopeWithMetadata");
        assertTrue(typeInfo.isPresent());

        var salaryField = typeInfo.get().fields.get("salary");
        assertEquals("read:user.salary", salaryField.getRequiresScope());
        assertEquals("User's annual salary", salaryField.description);
        assertFalse(salaryField.nullable);
    }

    @Test
    @DisplayName("Scope works alongside deprecated field marker")
    void testScopeWithDeprecation() {
        // RED: Scope and deprecated can coexist
        FraiseQL.registerType(ScopeWithDeprecation.class);

        var typeInfo = registry.getType("ScopeWithDeprecation");
        assertTrue(typeInfo.isPresent());

        var oldField = typeInfo.get().fields.get("oldSalary");
        assertEquals("read:user.salary", oldField.getRequiresScope());
        assertTrue(oldField.isDeprecated);
    }

    @Test
    @DisplayName("Multiple fields with scopes maintain separate metadata")
    void testMultipleScopedFieldsMetadataIndependence() {
        // RED: Each field's metadata is independent
        FraiseQL.registerType(MetadataIndependence.class);

        var typeInfo = registry.getType("MetadataIndependence");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;

        var field1 = fields.get("field1");
        var field2 = fields.get("field2");

        assertEquals("scope1", field1.getRequiresScope());
        assertEquals("Desc 1", field1.description);

        assertEquals("scope2", field2.getRequiresScope());
        assertEquals("Desc 2", field2.description);
    }

    // =========================================================================
    // VALIDATION: ERROR HANDLING (6 tests)
    // =========================================================================

    @Test
    @DisplayName("Invalid scope format is detected and logged")
    void testInvalidScopeFormatDetection() {
        // RED: Invalid scopes should be detected
        // Invalid: missing colon, wrong format
        assertThrows(RuntimeException.class, () -> {
            FraiseQL.registerType(InvalidScopeFormat.class);
        }, "Should reject invalid scope format");
    }

    @Test
    @DisplayName("Empty scope string is rejected")
    void testEmptyScopeRejection() {
        // RED: Empty string scope invalid
        assertThrows(RuntimeException.class, () -> {
            FraiseQL.registerType(EmptyScope.class);
        }, "Empty scope should be rejected");
    }

    @Test
    @DisplayName("Null scope is handled gracefully")
    void testNullScopeHandling() {
        // RED: Null scope treated as public field
        FraiseQL.registerType(NullScope.class);

        var typeInfo = registry.getType("NullScope");
        assertTrue(typeInfo.isPresent());

        var field = typeInfo.get().fields.get("publicField");
        assertNull(field.getRequiresScope(),
            "Null scope should remain null");
    }

    @Test
    @DisplayName("Empty requiresScopes array is rejected")
    void testEmptyScopesArrayRejection() {
        // RED: Empty array not allowed
        assertThrows(RuntimeException.class, () -> {
            FraiseQL.registerType(EmptyScopesArray.class);
        }, "Empty scopes array should be rejected");
    }

    @Test
    @DisplayName("Scope validation catches invalid action with hyphens")
    void testInvalidActionWithHyphensValidation() {
        // RED: Hyphens in action prefix are invalid
        assertThrows(RuntimeException.class, () -> {
            FraiseQL.registerType(InvalidActionWithHyphens.class);
        }, "Action with hyphens should be rejected");
    }

    @Test
    @DisplayName("Scope validation catches invalid resource with hyphens")
    void testInvalidResourceWithHyphensValidation() {
        // RED: Hyphens in resource name are invalid
        assertThrows(RuntimeException.class, () -> {
            FraiseQL.registerType(InvalidResourceWithHyphens.class);
        }, "Resource with hyphens should be rejected");
    }

    // =========================================================================
    // TEST FIXTURES: HAPPY PATH
    // =========================================================================

    @GraphQLType
    public static class UserWithSingleScope {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScope = "read:user.salary")
        public float salary;
    }

    @GraphQLType
    public static class UserWithMultipleScopes {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScope = "read:user.email")
        public String email;

        @GraphQLField(requiresScope = "read:user.phone")
        public String phone;

        @GraphQLField(requiresScope = "read:user.ssn")
        public String ssn;
    }

    @GraphQLType
    public static class UserWithMixedFields {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;

        @GraphQLField(requiresScope = "read:user.email")
        public String email;
    }

    @GraphQLType
    public static class AdminWithMultipleScopesPerField {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScopes = {"admin", "auditor"})
        public String adminNotes;
    }

    @GraphQLType
    public static class MixedScopeTypes {
        @GraphQLField(requiresScope = "read:basic")
        public String basicField;

        @GraphQLField(requiresScopes = {"read:advanced", "admin"})
        public String advancedField;
    }

    @GraphQLType
    public static class OrderedScopes {
        @GraphQLField(requiresScopes = {"first", "second", "third"})
        public String restricted;
    }

    @GraphQLType
    public static class ResourcePatternScopes {
        @GraphQLField(requiresScope = "read:User.email")
        public String email;

        @GraphQLField(requiresScope = "read:User.phone")
        public String phone;
    }

    @GraphQLType
    public static class ActionPatternScopes {
        @GraphQLField(requiresScope = "read:User.*")
        public String readableField;

        @GraphQLField(requiresScope = "write:User.*")
        public String writableField;
    }

    @GraphQLType
    public static class GlobalWildcardScope {
        @GraphQLField(requiresScope = "*")
        public String adminOverride;
    }

    @GraphQLType
    public static class ExportTestSingleScope {
        @GraphQLField(requiresScope = "read:user.salary")
        public float salary;
    }

    @GraphQLType
    public static class ExportTestMultipleScopes {
        @GraphQLField(requiresScopes = {"scope1", "scope2"})
        public String restricted;
    }

    @GraphQLType
    public static class ExportTestPublicField {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;
    }

    @GraphQLType
    public static class ScopeWithMetadata {
        @GraphQLField(
            requiresScope = "read:user.salary",
            description = "User's annual salary"
        )
        public float salary;
    }

    @GraphQLType
    public static class ScopeWithDeprecation {
        @GraphQLField(
            requiresScope = "read:user.salary",
            deprecated = "Use newSalary instead"
        )
        public float oldSalary;

        @GraphQLField(requiresScope = "read:user.salary")
        public float newSalary;
    }

    @GraphQLType
    public static class MetadataIndependence {
        @GraphQLField(
            requiresScope = "scope1",
            description = "Desc 1"
        )
        public String field1;

        @GraphQLField(
            requiresScope = "scope2",
            description = "Desc 2"
        )
        public String field2;
    }

    // =========================================================================
    // TEST FIXTURES: VALIDATION ERRORS
    // =========================================================================

    @GraphQLType
    public static class InvalidScopeFormat {
        @GraphQLField(requiresScope = "invalid_scope_no_colon")
        public String field;
    }

    @GraphQLType
    public static class EmptyScope {
        @GraphQLField(requiresScope = "")
        public String field;
    }

    @GraphQLType
    public static class NullScope {
        @GraphQLField
        public String publicField;
    }

    @GraphQLType
    public static class EmptyScopesArray {
        @GraphQLField(requiresScopes = {})
        public String field;
    }

    @GraphQLType
    public static class InvalidActionWithHyphens {
        @GraphQLField(requiresScope = "invalid-action:resource")
        public String field;
    }

    @GraphQLType
    public static class InvalidResourceWithHyphens {
        @GraphQLField(requiresScope = "read:invalid-resource-name")
        public String field;
    }

    // =========================================================================
    // TEST HELPERS
    // =========================================================================

    private void assertArrayContains(String[] array, String value) {
        for (String item : array) {
            if (item.equals(value)) {
                return;
            }
        }
        fail("Array does not contain: " + value);
    }
}
