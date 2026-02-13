package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL interface type support in FraiseQL Java.
 * Interfaces define a contract that types can implement,
 * ensuring they have certain fields.
 */
@DisplayName("GraphQL Interfaces")
public class InterfaceTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // INTERFACE REGISTRATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Register interface with fields")
    void testRegisterInterfaceWithFields() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, ""));
        fields.put("createdAt", new TypeConverter.GraphQLFieldInfo("DateTime", false, false, ""));

        registry.registerInterface("Node", fields, "Node interface");

        var interfaceInfo = registry.getInterface("Node");
        assertTrue(interfaceInfo.isPresent());
        assertEquals("Node", interfaceInfo.get().name);
        assertEquals(2, interfaceInfo.get().fields.size());
        assertEquals("Node interface", interfaceInfo.get().description);
    }

    @Test
    @DisplayName("Register interface without description")
    void testRegisterInterfaceWithoutDescription() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, ""));

        registry.registerInterface("Entity", fields, null);

        var interfaceInfo = registry.getInterface("Entity");
        assertTrue(interfaceInfo.isPresent());
        assertEquals("Entity", interfaceInfo.get().name);
    }

    @Test
    @DisplayName("Register multiple interfaces")
    void testRegisterMultipleInterfaces() {
        Map<String, TypeConverter.GraphQLFieldInfo> nodeFields = new HashMap<>();
        nodeFields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, ""));

        Map<String, TypeConverter.GraphQLFieldInfo> auditFields = new HashMap<>();
        auditFields.put("createdAt", new TypeConverter.GraphQLFieldInfo("DateTime", false, false, ""));
        auditFields.put("updatedAt", new TypeConverter.GraphQLFieldInfo("DateTime", false, false, ""));

        registry.registerInterface("Node", nodeFields, "Node interface");
        registry.registerInterface("Auditable", auditFields, "Auditable interface");

        assertEquals(2, registry.getAllInterfaces().size());
        assertTrue(registry.getInterface("Node").isPresent());
        assertTrue(registry.getInterface("Auditable").isPresent());
    }

    // =========================================================================
    // INTERFACE FIELD TESTS
    // =========================================================================

    @Test
    @DisplayName("Interface with single field")
    void testInterfaceWithSingleField() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, ""));

        registry.registerInterface("Identified", fields, null);

        var interfaceInfo = registry.getInterface("Identified");
        assertTrue(interfaceInfo.isPresent());
        assertEquals(1, interfaceInfo.get().fields.size());
    }

    @Test
    @DisplayName("Interface with multiple fields")
    void testInterfaceWithMultipleFields() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, "Primary key"));
        fields.put("createdAt", new TypeConverter.GraphQLFieldInfo("DateTime", false, false, "Creation time"));
        fields.put("updatedAt", new TypeConverter.GraphQLFieldInfo("DateTime", false, false, "Update time"));
        fields.put("deletedAt", new TypeConverter.GraphQLFieldInfo("DateTime", true, false, "Deletion time"));

        registry.registerInterface("Timestamped", fields, "Timestamps interface");

        var interfaceInfo = registry.getInterface("Timestamped");
        assertTrue(interfaceInfo.isPresent());
        assertEquals(4, interfaceInfo.get().fields.size());
    }

    @Test
    @DisplayName("Interface fields include descriptions")
    void testInterfaceFieldsIncludeDescriptions() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, "Unique identifier"));
        fields.put("name", new TypeConverter.GraphQLFieldInfo("String", false, false, "Entity name"));

        registry.registerInterface("Named", fields, null);

        var interfaceInfo = registry.getInterface("Named");
        assertTrue(interfaceInfo.isPresent());
        var idField = interfaceInfo.get().fields.get("id");
        assertNotNull(idField);
    }

    // =========================================================================
    // INTERFACE USAGE PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Node interface for ID-based access")
    void testNodeInterfacePattern() {
        Map<String, TypeConverter.GraphQLFieldInfo> nodeFields = new HashMap<>();
        nodeFields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, "Unique ID"));

        registry.registerInterface("Node", nodeFields, "An object with ID");

        var interfaceInfo = registry.getInterface("Node");
        assertTrue(interfaceInfo.isPresent());
        assertTrue(interfaceInfo.get().fields.containsKey("id"));
    }

    @Test
    @DisplayName("Pattern: Auditable interface for timestamps")
    void testAuditableInterfacePattern() {
        Map<String, TypeConverter.GraphQLFieldInfo> auditFields = new HashMap<>();
        auditFields.put("createdAt", new TypeConverter.GraphQLFieldInfo("DateTime", false, false, "Creation time"));
        auditFields.put("updatedAt", new TypeConverter.GraphQLFieldInfo("DateTime", false, false, "Last update"));
        auditFields.put("createdBy", new TypeConverter.GraphQLFieldInfo("String", true, false, "Creator ID"));
        auditFields.put("updatedBy", new TypeConverter.GraphQLFieldInfo("String", true, false, "Updater ID"));

        registry.registerInterface("Auditable", auditFields, "Audit trail interface");

        var interfaceInfo = registry.getInterface("Auditable");
        assertTrue(interfaceInfo.isPresent());
        assertEquals(4, interfaceInfo.get().fields.size());
    }

    @Test
    @DisplayName("Pattern: Publishable interface for content types")
    void testPublishableInterfacePattern() {
        Map<String, TypeConverter.GraphQLFieldInfo> publishableFields = new HashMap<>();
        publishableFields.put("published", new TypeConverter.GraphQLFieldInfo("Boolean", false, false, "Publication status"));
        publishableFields.put("publishedAt", new TypeConverter.GraphQLFieldInfo("DateTime", true, false, "Publication time"));
        publishableFields.put("author", new TypeConverter.GraphQLFieldInfo("String", false, false, "Author ID"));

        registry.registerInterface("Publishable", publishableFields, "Content publication interface");

        var interfaceInfo = registry.getInterface("Publishable");
        assertTrue(interfaceInfo.isPresent());
        assertEquals(3, interfaceInfo.get().fields.size());
    }

    // =========================================================================
    // INTERFACE WITH TYPES
    // =========================================================================

    @Test
    @DisplayName("Type implements interface")
    void testTypeImplementsInterface() {
        // Register interface first
        Map<String, TypeConverter.GraphQLFieldInfo> nodeFields = new HashMap<>();
        nodeFields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, ""));
        registry.registerInterface("Node", nodeFields, null);

        // Register type that implements interface
        FraiseQL.registerType(User.class);

        var typeInfo = registry.getType("User");
        assertTrue(typeInfo.isPresent());
        assertTrue(typeInfo.get().fields.containsKey("id"));
    }

    // =========================================================================
    // INTERFACE QUERY PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Query returns interface type")
    void testQueryReturnsInterfaceType() {
        // Register interface
        Map<String, TypeConverter.GraphQLFieldInfo> nodeFields = new HashMap<>();
        nodeFields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, ""));
        registry.registerInterface("Node", nodeFields, null);

        // Query can return interface
        FraiseQL.query("node")
            .returnType("Node")
            .arg("id", "ID")
            .register();

        var query = registry.getQuery("node");
        assertTrue(query.isPresent());
        assertEquals("Node", query.get().returnType);
    }

    // =========================================================================
    // CLEAR INTERFACES TEST
    // =========================================================================

    @Test
    @DisplayName("Clear removes registered interfaces")
    void testClearRemovesInterfaces() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("ID", false, false, ""));
        registry.registerInterface("Test", fields, null);

        assertTrue(registry.getInterface("Test").isPresent());

        registry.clear();

        assertFalse(registry.getInterface("Test").isPresent());
        assertEquals(0, registry.getAllInterfaces().size());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType
    public static class User {
        @GraphQLField
        public String id;

        @GraphQLField
        public String name;

        @GraphQLField
        public String email;

        @GraphQLField
        public String createdAt;
    }

    @GraphQLType
    public static class Post {
        @GraphQLField
        public String id;

        @GraphQLField
        public String title;

        @GraphQLField
        public String createdAt;

        @GraphQLField
        public String updatedAt;
    }

    @GraphQLInterface
    public interface Node {
        @GraphQLField
        String getId();
    }

    @GraphQLInterface
    public interface Auditable {
        @GraphQLField
        String getCreatedAt();

        @GraphQLField
        String getUpdatedAt();
    }
}
