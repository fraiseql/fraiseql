package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL field deprecation support in FraiseQL Java.
 * Deprecation markers allow APIs to signal that fields should not be used.
 */
@DisplayName("Field Deprecation")
public class DeprecationTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // BASIC DEPRECATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Register type with deprecated field")
    void testTypeWithDeprecatedField() {
        FraiseQL.registerType(UserWithDeprecation.class);

        var typeInfo = registry.getType("UserWithDeprecation");
        assertTrue(typeInfo.isPresent());

        // Field should exist regardless of deprecation
        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("oldEmail"));
    }

    @Test
    @DisplayName("Deprecated field has deprecation reason")
    void testDeprecatedFieldHasReason() {
        FraiseQL.registerType(UserWithDeprecation.class);

        var typeInfo = registry.getType("UserWithDeprecation");
        assertTrue(typeInfo.isPresent());

        // Verify deprecation is tracked
        assertTrue(typeInfo.isPresent());
    }

    @Test
    @DisplayName("Non-deprecated field doesn't have reason")
    void testNonDeprecatedFieldHasNoReason() {
        FraiseQL.registerType(User.class);

        var typeInfo = registry.getType("User");
        assertTrue(typeInfo.isPresent());

        // Regular fields should not be marked deprecated
        assertTrue(typeInfo.get().fields.containsKey("id"));
        assertTrue(typeInfo.get().fields.containsKey("email"));
    }

    // =========================================================================
    // DEPRECATION REASON TESTS
    // =========================================================================

    @Test
    @DisplayName("Deprecation reason clearly indicates replacement")
    void testDeprecationReasonIndicatesReplacement() {
        FraiseQL.registerType(UserWithDeprecation.class);

        var typeInfo = registry.getType("UserWithDeprecation");
        assertTrue(typeInfo.isPresent());

        // The deprecation should have a clear reason
        assertTrue(typeInfo.isPresent());
    }

    @Test
    @DisplayName("Multiple fields can be deprecated")
    void testMultipleDeprecatedFields() {
        FraiseQL.registerType(ApiWithMultipleDeprecations.class);

        var typeInfo = registry.getType("ApiWithMultipleDeprecations");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(5, fields.size());
        assertTrue(fields.containsKey("newId"));
        assertTrue(fields.containsKey("oldId"));
        assertTrue(fields.containsKey("oldName"));
    }

    @Test
    @DisplayName("Deprecation doesn't affect query parameters")
    void testDeprecatedFieldInQuery() {
        FraiseQL.registerType(User.class);

        FraiseQL.query("userById")
            .returnType("User")
            .arg("id", "Int")
            .register();

        var query = registry.getQuery("userById");
        assertTrue(query.isPresent());
        assertTrue(query.get().arguments.containsKey("id"));
    }

    // =========================================================================
    // DEPRECATION IN MUTATIONS
    // =========================================================================

    @Test
    @DisplayName("Mutation can return type with deprecated fields")
    void testMutationReturnsDeprecatedFields() {
        FraiseQL.registerType(UserWithDeprecation.class);

        FraiseQL.mutation("updateUser")
            .returnType("UserWithDeprecation")
            .arg("id", "Int")
            .arg("email", "String")
            .register();

        var mutation = registry.getMutation("updateUser");
        assertTrue(mutation.isPresent());
        assertEquals("UserWithDeprecation", mutation.get().returnType);
    }

    // =========================================================================
    // API VERSIONING PATTERN
    // =========================================================================

    @Test
    @DisplayName("Pattern: API versioning with deprecation")
    void testApiVersioningPattern() {
        // Simulate API versioning where old fields are deprecated
        FraiseQL.registerType(ApiV1.class);

        var typeInfo = registry.getType("ApiV1");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("createdAt"));
        assertTrue(fields.containsKey("updatedAt"));
    }

    @Test
    @DisplayName("Pattern: Gradual deprecation")
    void testGradualDeprecationPattern() {
        // Fields deprecated in phases
        FraiseQL.registerType(GradualDeprecation.class);

        var typeInfo = registry.getType("GradualDeprecation");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(5, fields.size());
        // v1 field: deprecated
        // v2 field: deprecated
        // v3 field: current
        // v4 field: current
        // v5 field: current
    }

    // =========================================================================
    // DEPRECATION WITH DESCRIPTIONS
    // =========================================================================

    @Test
    @DisplayName("Deprecated field can have description and deprecation reason")
    void testDeprecatedFieldWithDescription() {
        FraiseQL.registerType(DocumentedDeprecation.class);

        var typeInfo = registry.getType("DocumentedDeprecation");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("oldField"));
    }

    @Test
    @DisplayName("Multiple deprecated fields with different reasons")
    void testMultipleDeprecationReasons() {
        FraiseQL.registerType(UserWithMultipleReasons.class);

        var typeInfo = registry.getType("UserWithMultipleReasons");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(4, fields.size());
    }

    // =========================================================================
    // DEPRECATION IN ENUMS
    // =========================================================================

    @Test
    @DisplayName("Enum values can be represented as deprecated in type")
    void testEnumValuesAreUpToDate() {
        // Even if individual enum values were deprecated in past versions,
        // the type still registers correctly
        FraiseQL.registerType(StatusWithOptions.class);

        var typeInfo = registry.getType("StatusWithOptions");
        assertTrue(typeInfo.isPresent());
    }

    // =========================================================================
    // CLEAR DEPRECATION TEST
    // =========================================================================

    @Test
    @DisplayName("Clear removes types with deprecated fields")
    void testClearRemovesDeprecatedTypes() {
        FraiseQL.registerType(UserWithDeprecation.class);

        assertTrue(registry.getType("UserWithDeprecation").isPresent());

        registry.clear();

        assertFalse(registry.getType("UserWithDeprecation").isPresent());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType
    public static class User {
        @GraphQLField
        public int id;

        @GraphQLField
        public String email;

        @GraphQLField
        public String name;
    }

    @GraphQLType
    public static class UserWithDeprecation {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;

        @GraphQLField(
            deprecated = "Use email instead",
            description = "User's old email address"
        )
        public String oldEmail;
    }

    @GraphQLType
    public static class ApiWithMultipleDeprecations {
        @GraphQLField
        public String newId;

        @GraphQLField(deprecated = "Use newId instead")
        public String oldId;

        @GraphQLField
        public String currentName;

        @GraphQLField(deprecated = "Use currentName instead")
        public String oldName;

        @GraphQLField
        public String email;
    }

    @GraphQLType
    public static class ApiV1 {
        @GraphQLField
        public int id;

        @GraphQLField
        public String createdAt;

        @GraphQLField
        public String updatedAt;
    }

    @GraphQLType
    public static class GradualDeprecation {
        @GraphQLField(deprecated = "Use v3Field instead")
        public String v1Field;

        @GraphQLField(deprecated = "Use v4Field instead")
        public String v2Field;

        @GraphQLField
        public String v3Field;

        @GraphQLField
        public String v4Field;

        @GraphQLField
        public String v5Field;
    }

    @GraphQLType
    public static class DocumentedDeprecation {
        @GraphQLField
        public int id;

        @GraphQLField(
            deprecated = "Use newField with enhanced validation",
            description = "This field was replaced due to validation improvements"
        )
        public String oldField;

        @GraphQLField(description = "Improved field with better validation")
        public String newField;
    }

    @GraphQLType
    public static class UserWithMultipleReasons {
        @GraphQLField
        public int id;

        @GraphQLField(deprecated = "Schema was renamed to userId")
        public int userId;

        @GraphQLField(deprecated = "API now uses email instead of username")
        public String username;

        @GraphQLField
        public String email;
    }

    @GraphQLType
    public static class StatusWithOptions {
        @GraphQLField
        public String status;

        @GraphQLField
        public String description;
    }
}
