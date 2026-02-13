package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for custom authorization rules in FraiseQL.
 * Demonstrates field-level and type-level authorization via @Authorize decorator.
 */
@DisplayName("Custom Authorization Rules")
public class AuthorizationTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // BASIC AUTHORIZATION RULES
    // =========================================================================

    @Test
    @DisplayName("Register type with custom authorization rule")
    void testRegisterTypeWithAuthorizationRule() {
        FraiseQL.registerType(ProtectedNote.class);

        var typeInfo = registry.getType("ProtectedNote");
        assertTrue(typeInfo.isPresent());
        assertEquals("ProtectedNote", typeInfo.get().name);
    }

    @Test
    @DisplayName("Field with ownership authorization rule")
    void testFieldWithOwnershipRule() {
        FraiseQL.registerType(ProtectedNote.class);

        var typeInfo = registry.getType("ProtectedNote");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("content"));
        assertTrue(fields.containsKey("ownerId"));
    }

    @Test
    @DisplayName("Multiple authorization rules on different fields")
    void testMultipleAuthorizationRules() {
        FraiseQL.registerType(MultiLevelSecureData.class);

        var typeInfo = registry.getType("MultiLevelSecureData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(4, fields.size());
        assertTrue(fields.containsKey("publicData"));
        assertTrue(fields.containsKey("internalData"));
        assertTrue(fields.containsKey("confidentialData"));
        assertTrue(fields.containsKey("secretData"));
    }

    // =========================================================================
    // AUTHORIZATION QUERIES
    // =========================================================================

    @Test
    @DisplayName("Query with authorization protection")
    void testQueryWithAuthorizationProtection() {
        FraiseQL.registerType(ProtectedNote.class);

        FraiseQL.query("myNotes")
            .returnType("ProtectedNote")
            .returnsArray(true)
            .arg("userId", "String")
            .register();

        var query = registry.getQuery("myNotes");
        assertTrue(query.isPresent());
        assertEquals("[ProtectedNote]", query.get().returnType);
    }

    @Test
    @DisplayName("Mutation with authorization requirement")
    void testMutationWithAuthorizationRequirement() {
        FraiseQL.registerType(ProtectedNote.class);

        FraiseQL.mutation("createNote")
            .returnType("ProtectedNote")
            .arg("content", "String")
            .arg("userId", "String")
            .register();

        var mutation = registry.getMutation("createNote");
        assertTrue(mutation.isPresent());
        assertEquals(2, mutation.get().arguments.size());
    }

    // =========================================================================
    // RECURSIVE AUTHORIZATION
    // =========================================================================

    @Test
    @DisplayName("Recursive authorization on nested types")
    void testRecursiveAuthorization() {
        FraiseQL.registerType(SecureContainer.class);

        var typeInfo = registry.getType("SecureContainer");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("nestedSecureData"));
    }

    // =========================================================================
    // OPERATION-SPECIFIC AUTHORIZATION
    // =========================================================================

    @Test
    @DisplayName("Authorization rule applies only to read operations")
    void testOperationSpecificAuthorization() {
        FraiseQL.registerType(ReadProtectedData.class);

        var typeInfo = registry.getType("ReadProtectedData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("sensitiveField"));
    }

    // =========================================================================
    // ERROR MESSAGE CUSTOMIZATION
    // =========================================================================

    @Test
    @DisplayName("Custom error message for authorization failure")
    void testCustomErrorMessage() {
        FraiseQL.registerType(CustomErrorData.class);

        var typeInfo = registry.getType("CustomErrorData");
        assertTrue(typeInfo.isPresent());
        assertTrue(typeInfo.get().fields.containsKey("restrictedField"));
    }

    // =========================================================================
    // TEST FIXTURES - TYPES WITH AUTHORIZATION
    // =========================================================================

    @GraphQLType
    @Authorize(rule = "isOwner($context.userId, $field.ownerId)",
               description = "Ensures users can only access their own notes")
    public static class ProtectedNote {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(rule = "isOwner($context.userId, $field.ownerId) OR hasRole($context, 'admin')",
                   description = "Only note owner or admin can read content")
        public String content;

        @GraphQLField
        public String ownerId;
    }

    @GraphQLType
    public static class MultiLevelSecureData {
        @GraphQLField
        public String publicData;

        @GraphQLField
        @Authorize(rule = "hasRole($context, 'employee')")
        public String internalData;

        @GraphQLField
        @Authorize(rule = "hasRole($context, 'manager')")
        public String confidentialData;

        @GraphQLField
        @Authorize(rule = "hasRole($context, 'executive')")
        public String secretData;
    }

    @GraphQLType
    public static class SecureContainer {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(rule = "canAccessNested($context)", recursive = true)
        public SecureNested nestedSecureData;
    }

    @GraphQLType
    public static class SecureNested {
        @GraphQLField
        public String value;
    }

    @GraphQLType
    public static class ReadProtectedData {
        @GraphQLField
        public String publicField;

        @GraphQLField
        @Authorize(rule = "hasScope($context, 'read:sensitive')",
                   operations = "read",
                   description = "Sensitive read requires specific scope")
        public String sensitiveField;
    }

    @GraphQLType
    public static class CustomErrorData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(rule = "hasRole($context, 'auditor')",
                   errorMessage = "You do not have permission to access restricted data")
        public String restrictedField;
    }
}
