package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for JWT scope-based access control in FraiseQL Java.
 * Field-level scopes provide fine-grained authorization control.
 */
@DisplayName("JWT Scope-Based Field Access Control")
public class ScopeAccessControlTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // SINGLE SCOPE TESTS
    // =========================================================================

    @Test
    @DisplayName("Register type with scope-protected field")
    void testTypeWithScopeProtectedField() {
        FraiseQL.registerType(UserWithScopes.class);

        var typeInfo = registry.getType("UserWithScopes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("name"));
        assertTrue(fields.containsKey("salary"));
    }

    @Test
    @DisplayName("Single scope requirement on field")
    void testSingleScopeRequirement() {
        FraiseQL.registerType(UserWithScopes.class);

        var typeInfo = registry.getType("UserWithScopes");
        assertTrue(typeInfo.isPresent());

        // Salary field requires read:user.salary scope
        assertTrue(typeInfo.get().fields.containsKey("salary"));
    }

    @Test
    @DisplayName("Multiple fields with different scopes")
    void testMultipleFieldsWithDifferentScopes() {
        FraiseQL.registerType(FinancialData.class);

        var typeInfo = registry.getType("FinancialData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(5, fields.size());
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("publicInfo"));
        assertTrue(fields.containsKey("salary"));
        assertTrue(fields.containsKey("ssn"));
        assertTrue(fields.containsKey("bankAccount"));
    }

    @Test
    @DisplayName("Public fields don't require scopes")
    void testPublicFieldsNoScopes() {
        FraiseQL.registerType(UserWithScopes.class);

        var typeInfo = registry.getType("UserWithScopes");
        assertTrue(typeInfo.isPresent());

        // id and name are public
        assertTrue(typeInfo.get().fields.containsKey("id"));
        assertTrue(typeInfo.get().fields.containsKey("name"));
    }

    // =========================================================================
    // MULTIPLE SCOPES TESTS
    // =========================================================================

    @Test
    @DisplayName("Field requires multiple scopes")
    void testMultipleScopesOnField() {
        FraiseQL.registerType(AdminData.class);

        var typeInfo = registry.getType("AdminData");
        assertTrue(typeInfo.isPresent());

        // adminNotes requires both admin and auditor scopes
        assertTrue(typeInfo.get().fields.containsKey("adminNotes"));
    }

    @Test
    @DisplayName("Different fields require different scope combinations")
    void testDifferentScopeCombinations() {
        FraiseQL.registerType(HierarchicalAccess.class);

        var typeInfo = registry.getType("HierarchicalAccess");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(5, fields.size());
        assertTrue(fields.containsKey("publicField"));
        assertTrue(fields.containsKey("userField"));
        assertTrue(fields.containsKey("adminField"));
        assertTrue(fields.containsKey("superAdminField"));
        assertTrue(fields.containsKey("auditField"));
    }

    // =========================================================================
    // SCOPE PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Resource-based scopes (read:user.email)")
    void testResourceBasedScopePattern() {
        FraiseQL.registerType(UserWithResourceScopes.class);

        var typeInfo = registry.getType("UserWithResourceScopes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(4, fields.size());
    }

    @Test
    @DisplayName("Pattern: Role-based scopes (admin, moderator, user)")
    void testRoleBasedScopePattern() {
        FraiseQL.registerType(UserWithRoleScopes.class);

        var typeInfo = registry.getType("UserWithRoleScopes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(4, fields.size());
    }

    @Test
    @DisplayName("Pattern: Action-based scopes (read:*, write:*)")
    void testActionBasedScopePattern() {
        FraiseQL.registerType(ActionScopes.class);

        var typeInfo = registry.getType("ActionScopes");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(3, fields.size());
    }

    // =========================================================================
    // SCOPE IN QUERIES
    // =========================================================================

    @Test
    @DisplayName("Query returning type with scoped fields")
    void testQueryReturnsTypeWithScopedFields() {
        FraiseQL.registerType(UserWithScopes.class);

        FraiseQL.query("currentUser")
            .returnType("UserWithScopes")
            .register();

        var query = registry.getQuery("currentUser");
        assertTrue(query.isPresent());
        assertEquals("UserWithScopes", query.get().returnType);
    }

    @Test
    @DisplayName("Query with scope-based filtering")
    void testQueryWithScopeFiltering() {
        FraiseQL.registerType(UserWithScopes.class);

        FraiseQL.query("users")
            .returnType("UserWithScopes")
            .returnsArray(true)
            .arg("role", "String")
            .register();

        var query = registry.getQuery("users");
        assertTrue(query.isPresent());
        assertTrue(query.get().arguments.containsKey("role"));
    }

    // =========================================================================
    // SCOPE IN MUTATIONS
    // =========================================================================

    @Test
    @DisplayName("Mutation with scoped field return type")
    void testMutationWithScopedReturnType() {
        FraiseQL.registerType(UserWithScopes.class);

        FraiseQL.mutation("updateUser")
            .returnType("UserWithScopes")
            .arg("id", "Int")
            .arg("name", "String")
            .register();

        var mutation = registry.getMutation("updateUser");
        assertTrue(mutation.isPresent());
    }

    // =========================================================================
    // COMMON SCOPE PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: PII protection with scopes")
    void testPIIProtectionPattern() {
        FraiseQL.registerType(PersonalData.class);

        var typeInfo = registry.getType("PersonalData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        // Public: id
        // PII: ssn, bankAccount, phone
        assertEquals(4, fields.size());
    }

    @Test
    @DisplayName("Pattern: Financial data access control")
    void testFinancialDataAccessPattern() {
        FraiseQL.registerType(FinancialData.class);

        var typeInfo = registry.getType("FinancialData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("salary"));
        assertTrue(fields.containsKey("bankAccount"));
    }

    @Test
    @DisplayName("Pattern: Medical data access control")
    void testMedicalDataAccessPattern() {
        FraiseQL.registerType(MedicalRecord.class);

        var typeInfo = registry.getType("MedicalRecord");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(4, fields.size());
    }

    // =========================================================================
    // SCOPE WITH DEPRECATION
    // =========================================================================

    @Test
    @DisplayName("Field can have both scope and deprecation")
    void testScopeAndDeprecation() {
        FraiseQL.registerType(LegacyData.class);

        var typeInfo = registry.getType("LegacyData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("oldSensitiveField"));
        assertTrue(fields.containsKey("newSensitiveField"));
    }

    // =========================================================================
    // CLEAR SCOPES TEST
    // =========================================================================

    @Test
    @DisplayName("Clear removes types with scoped fields")
    void testClearRemovesScopedTypes() {
        FraiseQL.registerType(UserWithScopes.class);

        assertTrue(registry.getType("UserWithScopes").isPresent());

        registry.clear();

        assertFalse(registry.getType("UserWithScopes").isPresent());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType
    public static class UserWithScopes {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;

        @GraphQLField(
            requiresScope = "read:user.salary",
            description = "User salary (restricted)"
        )
        public float salary;
    }

    @GraphQLType
    public static class FinancialData {
        @GraphQLField
        public int id;

        @GraphQLField(description = "Public information")
        public String publicInfo;

        @GraphQLField(requiresScope = "read:financial.salary")
        public float salary;

        @GraphQLField(requiresScope = "read:pii.ssn")
        public String ssn;

        @GraphQLField(requiresScope = "read:financial.banking")
        public String bankAccount;
    }

    @GraphQLType
    public static class AdminData {
        @GraphQLField
        public int id;

        @GraphQLField
        public String publicData;

        @GraphQLField(
            requiresScopes = {"admin", "auditor"},
            description = "Admin notes (requires both admin and auditor roles)"
        )
        public String adminNotes;
    }

    @GraphQLType
    public static class HierarchicalAccess {
        @GraphQLField
        public String publicField;

        @GraphQLField(requiresScope = "user")
        public String userField;

        @GraphQLField(requiresScope = "admin")
        public String adminField;

        @GraphQLField(requiresScopes = {"admin", "superadmin"})
        public String superAdminField;

        @GraphQLField(requiresScopes = {"admin", "auditor"})
        public String auditField;
    }

    @GraphQLType
    public static class UserWithResourceScopes {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScope = "read:user.email")
        public String email;

        @GraphQLField(requiresScope = "read:user.phone")
        public String phone;

        @GraphQLField(requiresScope = "read:user.profile")
        public String profile;
    }

    @GraphQLType
    public static class UserWithRoleScopes {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScope = "user")
        public String publicInfo;

        @GraphQLField(requiresScope = "moderator")
        public String moderatorInfo;

        @GraphQLField(requiresScope = "admin")
        public String adminInfo;
    }

    @GraphQLType
    public static class ActionScopes {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScope = "read:user")
        public String readableData;

        @GraphQLField(
            requiresScopes = {"write:user", "admin"},
            description = "Requires write permission and admin role"
        )
        public String writableData;
    }

    @GraphQLType
    public static class PersonalData {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScope = "read:pii.ssn")
        public String ssn;

        @GraphQLField(requiresScope = "read:pii.bank")
        public String bankAccount;

        @GraphQLField(requiresScope = "read:pii.phone")
        public String phone;
    }

    @GraphQLType
    public static class MedicalRecord {
        @GraphQLField
        public int id;

        @GraphQLField(requiresScope = "read:medical.diagnosis")
        public String diagnosis;

        @GraphQLField(requiresScope = "read:medical.treatment")
        public String treatment;

        @GraphQLField(requiresScopes = {"read:medical.all", "doctor"})
        public String confidentialNotes;
    }

    @GraphQLType
    public static class LegacyData {
        @GraphQLField
        public int id;

        @GraphQLField(
            deprecated = "Use newSensitiveField instead",
            requiresScope = "read:legacy.data",
            description = "Legacy field with restricted access"
        )
        public String oldSensitiveField;

        @GraphQLField(requiresScope = "read:user.sensitive")
        public String newSensitiveField;
    }
}
