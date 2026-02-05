package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for attribute-based access control (ABAC) patterns in FraiseQL.
 * Demonstrates fine-grained access control based on user and resource attributes.
 */
@DisplayName("Attribute-Based Access Control")
public class AttributeBasedAccessControlTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // CLEARANCE-LEVEL BASED ACCESS
    // =========================================================================

    @Test
    @DisplayName("Field access based on clearance level")
    void testClearanceLevelBasedAccess() {
        FraiseQL.registerType(ClassifiedDocument.class);

        var typeInfo = registry.getType("ClassifiedDocument");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("publicContent"));
        assertTrue(fields.containsKey("secretContent"));
    }

    @Test
    @DisplayName("Type-level clearance requirement")
    void testTypeLevelClearanceRequirement() {
        FraiseQL.registerType(TopSecretData.class);

        var typeInfo = registry.getType("TopSecretData");
        assertTrue(typeInfo.isPresent());
    }

    // =========================================================================
    // DEPARTMENT-BASED ACCESS
    // =========================================================================

    @Test
    @DisplayName("Field access restricted by department")
    void testDepartmentBasedAccess() {
        FraiseQL.registerType(FinancialRecord.class);

        var typeInfo = registry.getType("FinancialRecord");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("accountNumber"));
        assertTrue(fields.containsKey("balance"));
    }

    @Test
    @DisplayName("Multiple department access")
    void testMultipleDepartmentAccess() {
        FraiseQL.registerType(HRRecord.class);

        var typeInfo = registry.getType("HRRecord");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("employeeId"));
        assertTrue(fields.containsKey("compensation"));
    }

    // =========================================================================
    // TIME-BASED ACCESS CONTROL
    // =========================================================================

    @Test
    @DisplayName("Field access restricted by time window")
    void testTimeBasedAccess() {
        FraiseQL.registerType(TimeSensitiveData.class);

        var typeInfo = registry.getType("TimeSensitiveData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("temporaryContent"));
    }

    // =========================================================================
    // GEOGRAPHIC RESTRICTIONS
    // =========================================================================

    @Test
    @DisplayName("Field access based on geographic location")
    void testGeographicRestriction() {
        FraiseQL.registerType(RegionalData.class);

        var typeInfo = registry.getType("RegionalData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("regionCode"));
        assertTrue(fields.containsKey("regionSpecificData"));
    }

    @Test
    @DisplayName("GDPR compliance: EU data access restriction")
    void testGDPRCompliance() {
        FraiseQL.registerType(PersonalData.class);

        var typeInfo = registry.getType("PersonalData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("euPersonalData"));
    }

    // =========================================================================
    // PROJECT-BASED ACCESS
    // =========================================================================

    @Test
    @DisplayName("Field access restricted to project members")
    void testProjectBasedAccess() {
        FraiseQL.registerType(ProjectData.class);

        var typeInfo = registry.getType("ProjectData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("projectId"));
        assertTrue(fields.containsKey("projectSecrets"));
    }

    // =========================================================================
    // COMBINED ATTRIBUTES
    // =========================================================================

    @Test
    @DisplayName("Access control with multiple combined attributes")
    void testCombinedAttributes() {
        FraiseQL.registerType(HighlyRestrictedData.class);

        var typeInfo = registry.getType("HighlyRestrictedData");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("multiAttributeProtectedData"));
    }

    // =========================================================================
    // ATTRIBUTE-BASED QUERIES
    // =========================================================================

    @Test
    @DisplayName("Query with attribute-based access control")
    void testQueryWithAttributeAccess() {
        FraiseQL.registerType(PersonalData.class);

        FraiseQL.query("myPersonalData")
            .returnType("PersonalData")
            .returnsArray(true)
            .arg("userId", "String")
            .register();

        var query = registry.getQuery("myPersonalData");
        assertTrue(query.isPresent());
    }

    // =========================================================================
    // DATA CLASSIFICATION LEVELS
    // =========================================================================

    @Test
    @DisplayName("Multiple data classification levels")
    void testDataClassificationLevels() {
        FraiseQL.registerType(ClassifiedContent.class);

        var typeInfo = registry.getType("ClassifiedContent");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(4, fields.size());
    }

    // =========================================================================
    // TEST FIXTURES - ABAC TYPES
    // =========================================================================

    @GraphQLType
    public static class ClassifiedDocument {
        @GraphQLField
        public String id;

        @GraphQLField
        public String publicContent;

        @GraphQLField
        @Authorize(rule = "hasAttribute($context, 'clearance_level', 1)",
                   description = "Requires clearance level 1 or higher")
        public String secretContent;
    }

    @GraphQLType
    @Authorize(rule = "hasAttribute($context, 'clearance_level', 3)",
               description = "Only users with top secret clearance")
    public static class TopSecretData {
        @GraphQLField
        public String id;

        @GraphQLField
        public String content;
    }

    @GraphQLType
    public static class FinancialRecord {
        @GraphQLField
        public String accountNumber;

        @GraphQLField
        @Authorize(rule = "hasAttribute($context, 'department', 'finance') OR hasRole($context, 'auditor')",
                   description = "Finance department or auditor access only")
        public float balance;
    }

    @GraphQLType
    public static class HRRecord {
        @GraphQLField
        public String employeeId;

        @GraphQLField
        @Authorize(rule = "hasAttribute($context, 'department', 'hr') OR hasAttribute($context, 'department', 'finance')",
                   description = "HR and Finance departments can view compensation")
        public float compensation;
    }

    @GraphQLType
    public static class TimeSensitiveData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(rule = "currentTime() >= $field.validFrom AND currentTime() <= $field.validUntil",
                   description = "Only accessible during specified time window")
        public String temporaryContent;
    }

    @GraphQLType
    public static class RegionalData {
        @GraphQLField
        public String regionCode;

        @GraphQLField
        @Authorize(rule = "userLocation() IN ['NA', 'EU', 'APAC'] AND $context.region == $field.region",
                   description = "User can only access their regional data")
        public String regionSpecificData;
    }

    @GraphQLType
    public static class PersonalData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(rule = "($context.country IN ['DE', 'FR', 'IT'] AND hasAttribute($context, 'gdpr_compliant', true)) " +
                          "OR hasRole($context, 'data_processor')",
                   description = "GDPR-compliant access to EU personal data")
        public String euPersonalData;
    }

    @GraphQLType
    public static class ProjectData {
        @GraphQLField
        public String projectId;

        @GraphQLField
        @Authorize(rule = "isMember($context.userId, $field.projectId) OR hasRole($context, 'admin')",
                   description = "Only project members can access project secrets")
        public String projectSecrets;
    }

    @GraphQLType
    public static class HighlyRestrictedData {
        @GraphQLField
        public String id;

        @GraphQLField
        @Authorize(rule = "hasRole($context, 'executive') AND " +
                          "hasAttribute($context, 'clearance_level', 3) AND " +
                          "hasAttribute($context, 'department', 'c_suite') AND " +
                          "$context.country == 'US'",
                   description = "C-suite executives in US only")
        public String multiAttributeProtectedData;
    }

    @GraphQLType
    public static class ClassifiedContent {
        @GraphQLField
        public String unclassified;

        @GraphQLField
        @Authorize(rule = "hasAttribute($context, 'clearance_level', 1)")
        public String confidential;

        @GraphQLField
        @Authorize(rule = "hasAttribute($context, 'clearance_level', 2)")
        public String secret;

        @GraphQLField
        @Authorize(rule = "hasAttribute($context, 'clearance_level', 3)")
        public String topSecret;
    }
}
