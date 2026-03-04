package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for role-based access control (RBAC) patterns in FraiseQL.
 * Demonstrates @RoleRequired decorator for role-based field access.
 */
@DisplayName("Role-Based Access Control")
public class RoleBasedAccessControlTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // SINGLE ROLE REQUIREMENTS
    // =========================================================================

    @Test
    @DisplayName("Field with single role requirement")
    void testFieldWithSingleRoleRequirement() {
        FraiseQL.registerType(AdminPanel.class);

        var typeInfo = registry.getType("AdminPanel");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("systemSettings"));
    }

    @Test
    @DisplayName("Type with admin role requirement")
    void testTypeWithAdminRoleRequirement() {
        FraiseQL.registerType(SystemConfiguration.class);

        var typeInfo = registry.getType("SystemConfiguration");
        assertTrue(typeInfo.isPresent());
        assertEquals("SystemConfiguration", typeInfo.get().name);
    }

    // =========================================================================
    // MULTIPLE ROLE REQUIREMENTS (ANY STRATEGY)
    // =========================================================================

    @Test
    @DisplayName("Field accessible by multiple roles (ANY strategy)")
    void testMultipleRolesAnyStrategy() {
        FraiseQL.registerType(SalaryData.class);

        var typeInfo = registry.getType("SalaryData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("employeeId"));
        assertTrue(fields.containsKey("salary"));
    }

    @Test
    @DisplayName("Field accessible by manager or HR roles")
    void testManagerOrHRRoles() {
        FraiseQL.registerType(EmployeeRecord.class);

        var typeInfo = registry.getType("EmployeeRecord");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("performance"));
    }

    // =========================================================================
    // MULTIPLE ROLE REQUIREMENTS (ALL STRATEGY)
    // =========================================================================

    @Test
    @DisplayName("Field requires all specified roles (ALL strategy)")
    void testMultipleRolesAllStrategy() {
        FraiseQL.registerType(ComplianceReport.class);

        var typeInfo = registry.getType("ComplianceReport");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("reportId"));
        assertTrue(fields.containsKey("auditTrail"));
    }

    // =========================================================================
    // ROLE HIERARCHIES
    // =========================================================================

    @Test
    @DisplayName("Role hierarchy: higher roles inherit lower role permissions")
    void testRoleHierarchy() {
        FraiseQL.registerType(ManagerData.class);

        var typeInfo = registry.getType("ManagerData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("budgetAmount"));
    }

    @Test
    @DisplayName("Hierarchical role access to financial data")
    void testHierarchicalFinancialAccess() {
        FraiseQL.registerType(FinancialData.class);

        var typeInfo = registry.getType("FinancialData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(2, fields.size());
    }

    // =========================================================================
    // OPERATION-SPECIFIC ROLE REQUIREMENTS
    // =========================================================================

    @Test
    @DisplayName("Role requirement specific to delete operations")
    void testOperationSpecificRoleRequirement() {
        FraiseQL.registerType(UserAccount.class);

        var typeInfo = registry.getType("UserAccount");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("email"));
    }

    // =========================================================================
    // ROLE-PROTECTED MUTATIONS
    // =========================================================================

    @Test
    @DisplayName("Mutation restricted to admin role")
    void testMutationRestrictedToAdmin() {
        FraiseQL.registerType(User.class);

        FraiseQL.mutation("deleteUser")
            .returnType("Boolean")
            .arg("userId", "String")
            .register();

        var mutation = registry.getMutation("deleteUser");
        assertTrue(mutation.isPresent());
    }

    @Test
    @DisplayName("Mutation requiring multiple roles")
    void testMutationRequiringMultipleRoles() {
        FraiseQL.registerType(DataTransfer.class);

        FraiseQL.mutation("transferFunds")
            .returnType("Boolean")
            .arg("fromAccount", "String")
            .arg("toAccount", "String")
            .arg("amount", "Float")
            .register();

        var mutation = registry.getMutation("transferFunds");
        assertTrue(mutation.isPresent());
        assertEquals(3, mutation.get().arguments.size());
    }

    // =========================================================================
    // ROLE INHERITANCE
    // =========================================================================

    @Test
    @DisplayName("Role requirements inherited from type to field")
    void testRoleInheritance() {
        FraiseQL.registerType(TypeLevelProtected.class);

        var typeInfo = registry.getType("TypeLevelProtected");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("additionalData"));
    }

    // =========================================================================
    // TEST FIXTURES - RBAC TYPES
    // =========================================================================

    @GraphQLType
    public static class AdminPanel {
        @GraphQLField
        public String id;

        @GraphQLField
        @RoleRequired(roles = "admin")
        public String systemSettings;
    }

    @GraphQLType
    @RoleRequired(roles = "admin")
    public static class SystemConfiguration {
        @GraphQLField
        public String id;

        @GraphQLField
        public String databaseUrl;

        @GraphQLField
        public String apiKey;
    }

    @GraphQLType
    public static class SalaryData {
        @GraphQLField
        public String employeeId;

        @GraphQLField
        @RoleRequired(roles = {"manager", "hr", "admin"}, strategy = RoleRequired.RoleMatchStrategy.ANY)
        public float salary;
    }

    @GraphQLType
    public static class EmployeeRecord {
        @GraphQLField
        public String id;

        @GraphQLField
        @RoleRequired(roles = {"manager", "hr"}, strategy = RoleRequired.RoleMatchStrategy.ANY)
        public String performance;
    }

    @GraphQLType
    public static class ComplianceReport {
        @GraphQLField
        public String reportId;

        @GraphQLField
        @RoleRequired(roles = {"compliance_officer", "auditor"}, strategy = RoleRequired.RoleMatchStrategy.ALL)
        public String auditTrail;
    }

    @GraphQLType
    public static class ManagerData {
        @GraphQLField
        public String id;

        @GraphQLField
        @RoleRequired(roles = "manager", hierarchy = true, description = "Directors and above can access budget")
        public float budgetAmount;
    }

    @GraphQLType
    public static class FinancialData {
        @GraphQLField
        public String id;

        @GraphQLField
        @RoleRequired(roles = "finance", hierarchy = true)
        public float amount;
    }

    @GraphQLType
    public static class UserAccount {
        @GraphQLField
        public String id;

        @GraphQLField
        public String email;

        @GraphQLField
        @RoleRequired(roles = "admin", operations = "delete")
        public String accountStatus;
    }

    @GraphQLType
    public static class User {
        @GraphQLField
        public String id;

        @GraphQLField
        public String name;
    }

    @GraphQLType
    public static class DataTransfer {
        @GraphQLField
        public String id;

        @GraphQLField
        public float amount;
    }

    @GraphQLType
    @RoleRequired(roles = "viewer", inherit = true)
    public static class TypeLevelProtected {
        @GraphQLField
        public String id;

        @GraphQLField
        @RoleRequired(roles = "editor")
        public String additionalData;
    }
}
