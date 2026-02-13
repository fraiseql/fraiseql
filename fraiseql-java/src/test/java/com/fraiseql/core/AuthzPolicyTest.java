package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for authorization policies in FraiseQL.
 * Demonstrates policy definition and reuse via @AuthzPolicy decorator.
 */
@DisplayName("Authorization Policies")
public class AuthzPolicyTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // RBAC POLICIES
    // =========================================================================

    @Test
    @DisplayName("Define and reference RBAC policy")
    void testRBACPolicy() {
        FraiseQL.registerType(AdminOnlyPolicy.class);
        FraiseQL.registerType(AdminProtectedData.class);

        var policyInfo = registry.getType("AdminOnlyPolicy");
        var dataInfo = registry.getType("AdminProtectedData");

        assertTrue(policyInfo.isPresent());
        assertTrue(dataInfo.isPresent());
    }

    @Test
    @DisplayName("Multiple fields referencing same RBAC policy")
    void testMultipleFieldsWithRBACPolicy() {
        FraiseQL.registerType(PIIAccessPolicy.class);
        FraiseQL.registerType(Customer.class);

        var typeInfo = registry.getType("Customer");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("email"));
        assertTrue(fields.containsKey("phoneNumber"));
        assertTrue(fields.containsKey("socialSecurityNumber"));
    }

    // =========================================================================
    // ABAC POLICIES
    // =========================================================================

    @Test
    @DisplayName("Define and reference ABAC policy")
    void testABACPolicy() {
        FraiseQL.registerType(ClearancePolicy.class);
        FraiseQL.registerType(SecretData.class);

        var policyInfo = registry.getType("ClearancePolicy");
        var dataInfo = registry.getType("SecretData");

        assertTrue(policyInfo.isPresent());
        assertTrue(dataInfo.isPresent());
    }

    @Test
    @DisplayName("Attribute conditions in policy")
    void testAttributeConditionsInPolicy() {
        FraiseQL.registerType(FinancialAccessPolicy.class);
        FraiseQL.registerType(FinancialRecord.class);

        var typeInfo = registry.getType("FinancialRecord");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("transactionAmount"));
    }

    // =========================================================================
    // HYBRID POLICIES
    // =========================================================================

    @Test
    @DisplayName("Hybrid policy combining roles and attributes")
    void testHybridPolicy() {
        FraiseQL.registerType(AuditAccessPolicy.class);
        FraiseQL.registerType(AuditLog.class);

        var policyInfo = registry.getType("AuditAccessPolicy");
        var logInfo = registry.getType("AuditLog");

        assertTrue(policyInfo.isPresent());
        assertTrue(logInfo.isPresent());
    }

    // =========================================================================
    // HIERARCHICAL POLICIES
    // =========================================================================

    @Test
    @DisplayName("Policy with recursive application to nested types")
    void testRecursivePolicy() {
        FraiseQL.registerType(RecursiveAuthPolicy.class);
        FraiseQL.registerType(ParentData.class);

        var typeInfo = registry.getType("ParentData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("nestedData"));
    }

    // =========================================================================
    // OPERATION-SPECIFIC POLICIES
    // =========================================================================

    @Test
    @DisplayName("Policy applies to specific operations only")
    void testOperationSpecificPolicy() {
        FraiseQL.registerType(ReadOnlyPolicy.class);
        FraiseQL.registerType(ReadProtectedData.class);

        var typeInfo = registry.getType("ReadProtectedData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("sensitiveField"));
    }

    // =========================================================================
    // CACHED POLICIES
    // =========================================================================

    @Test
    @DisplayName("Policy with authorization decision caching")
    void testCachedPolicy() {
        FraiseQL.registerType(CachedAccessPolicy.class);
        FraiseQL.registerType(CachedProtectedData.class);

        var typeInfo = registry.getType("CachedProtectedData");
        assertTrue(typeInfo.isPresent());
    }

    // =========================================================================
    // AUDIT LOGGING POLICIES
    // =========================================================================

    @Test
    @DisplayName("Policy with audit logging enabled")
    void testAuditLoggingPolicy() {
        FraiseQL.registerType(AuditedAccessPolicy.class);
        FraiseQL.registerType(AuditedData.class);

        var typeInfo = registry.getType("AuditedData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("sensitiveField"));
    }

    // =========================================================================
    // POLICY MUTATIONS
    // =========================================================================

    @Test
    @DisplayName("Mutation protected by authorization policy")
    void testMutationWithPolicy() {
        FraiseQL.registerType(AdminOnlyPolicy.class);

        FraiseQL.mutation("deleteUser")
            .returnType("Boolean")
            .arg("userId", "String")
            .register();

        var mutation = registry.getMutation("deleteUser");
        assertTrue(mutation.isPresent());
    }

    // =========================================================================
    // TEST FIXTURES - POLICIES
    // =========================================================================

    @AuthzPolicy(
        name = "adminOnly",
        description = "Access restricted to administrators",
        type = AuthzPolicy.AuthzPolicyType.RBAC,
        rule = "hasRole($context, 'admin')",
        auditLogging = true
    )
    public static class AdminOnlyPolicy {}

    @GraphQLType
    public static class AdminProtectedData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(policy = "adminOnly")
        public String sensitiveData;
    }

    @AuthzPolicy(
        name = "piiAccess",
        description = "Access to Personally Identifiable Information",
        type = AuthzPolicy.AuthzPolicyType.RBAC,
        rule = "hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')"
    )
    public static class PIIAccessPolicy {}

    @GraphQLType
    public static class Customer {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(policy = "piiAccess")
        public String email;

        @GraphQLField
        @Authorize(policy = "piiAccess")
        public String phoneNumber;

        @GraphQLField
        @Authorize(policy = "piiAccess")
        public String socialSecurityNumber;
    }

    @AuthzPolicy(
        name = "secretClearance",
        description = "Requires top secret clearance",
        type = AuthzPolicy.AuthzPolicyType.ABAC,
        attributes = {"clearance_level >= 3", "background_check == true"}
    )
    public static class ClearancePolicy {}

    @GraphQLType
    public static class SecretData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(policy = "secretClearance")
        public String classification;

        @GraphQLField
        @Authorize(policy = "secretClearance")
        public String content;
    }

    @AuthzPolicy(
        name = "financialData",
        description = "Access to financial records",
        type = AuthzPolicy.AuthzPolicyType.ABAC,
        attributes = {"clearance_level >= 2", "department == 'finance'"}
    )
    public static class FinancialAccessPolicy {}

    @GraphQLType
    public static class FinancialRecord {
        @GraphQLField
        public String transactionId;

        @GraphQLField
        @Authorize(policy = "financialData")
        public float transactionAmount;
    }

    @AuthzPolicy(
        name = "auditAccess",
        description = "Access to audit trails with role and attribute checks",
        type = AuthzPolicy.AuthzPolicyType.HYBRID,
        rule = "hasRole($context, 'auditor')",
        attributes = {"audit_enabled == true"}
    )
    public static class AuditAccessPolicy {}

    @GraphQLType
    public static class AuditLog {
        @GraphQLField
        public String auditId;

        @GraphQLField
        @Authorize(policy = "auditAccess")
        public String auditTrail;
    }

    @AuthzPolicy(
        name = "recursiveProtection",
        description = "Recursively applies to nested types",
        type = AuthzPolicy.AuthzPolicyType.CUSTOM,
        rule = "canAccessNested($context)",
        recursive = true
    )
    public static class RecursiveAuthPolicy {}

    @GraphQLType
    public static class ParentData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(policy = "recursiveProtection")
        public NestedData nestedData;
    }

    @GraphQLType
    public static class NestedData {
        @GraphQLField
        public String value;
    }

    @AuthzPolicy(
        name = "readOnly",
        description = "Policy applies only to read operations",
        type = AuthzPolicy.AuthzPolicyType.CUSTOM,
        rule = "hasRole($context, 'viewer')",
        operations = "read"
    )
    public static class ReadOnlyPolicy {}

    @GraphQLType
    public static class ReadProtectedData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(policy = "readOnly")
        public String sensitiveField;
    }

    @AuthzPolicy(
        name = "cachedAccess",
        description = "Access control with result caching",
        type = AuthzPolicy.AuthzPolicyType.CUSTOM,
        rule = "hasRole($context, 'viewer')",
        cacheable = true,
        cacheDurationSeconds = 3600
    )
    public static class CachedAccessPolicy {}

    @GraphQLType
    public static class CachedProtectedData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(policy = "cachedAccess")
        public String cachedField;
    }

    @AuthzPolicy(
        name = "auditedAccess",
        description = "Access with comprehensive audit logging",
        type = AuthzPolicy.AuthzPolicyType.RBAC,
        rule = "hasRole($context, 'auditor')",
        auditLogging = true
    )
    public static class AuditedAccessPolicy {}

    @GraphQLType
    public static class AuditedData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(policy = "auditedAccess")
        public String sensitiveField;
    }
}
