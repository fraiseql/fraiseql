package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.LinkedHashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL enum type support in FraiseQL Java.
 * Enums are scalar types that have a fixed set of allowed values.
 */
@DisplayName("GraphQL Enums")
public class EnumTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // ENUM REGISTRATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Register enum with builder pattern")
    void testRegisterEnumWithBuilder() {
        Map<String, String> values = new LinkedHashMap<>();
        values.put("PENDING", "pending");
        values.put("SHIPPED", "shipped");
        values.put("DELIVERED", "delivered");

        registry.registerEnum("OrderStatus", values, "Order status enum");

        var enumInfo = registry.getEnum("OrderStatus");
        assertTrue(enumInfo.isPresent());
        assertEquals("OrderStatus", enumInfo.get().name);
        assertEquals(3, enumInfo.get().values.size());
        assertEquals("Order status enum", enumInfo.get().description);
    }

    @Test
    @DisplayName("Register enum without description")
    void testRegisterEnumWithoutDescription() {
        Map<String, String> values = new LinkedHashMap<>();
        values.put("ACTIVE", "active");
        values.put("INACTIVE", "inactive");

        registry.registerEnum("UserStatus", values, null);

        var enumInfo = registry.getEnum("UserStatus");
        assertTrue(enumInfo.isPresent());
        assertEquals("UserStatus", enumInfo.get().name);
        assertEquals(2, enumInfo.get().values.size());
    }

    @Test
    @DisplayName("Enum values are preserved in order")
    void testEnumValuesPreservedInOrder() {
        Map<String, String> values = new LinkedHashMap<>();
        values.put("FIRST", "first");
        values.put("SECOND", "second");
        values.put("THIRD", "third");

        registry.registerEnum("Priority", values, null);

        var enumInfo = registry.getEnum("Priority");
        assertTrue(enumInfo.isPresent());

        var valueNames = enumInfo.get().values.keySet().stream().toList();
        assertEquals("FIRST", valueNames.get(0));
        assertEquals("SECOND", valueNames.get(1));
        assertEquals("THIRD", valueNames.get(2));
    }

    @Test
    @DisplayName("Register multiple enums")
    void testRegisterMultipleEnums() {
        Map<String, String> statusValues = new LinkedHashMap<>();
        statusValues.put("ACTIVE", "active");
        statusValues.put("INACTIVE", "inactive");

        Map<String, String> roleValues = new LinkedHashMap<>();
        roleValues.put("ADMIN", "admin");
        roleValues.put("USER", "user");
        roleValues.put("GUEST", "guest");

        registry.registerEnum("UserStatus", statusValues, "User status");
        registry.registerEnum("UserRole", roleValues, "User role");

        assertEquals(2, registry.getAllEnums().size());
        assertTrue(registry.getEnum("UserStatus").isPresent());
        assertTrue(registry.getEnum("UserRole").isPresent());
    }

    // =========================================================================
    // ENUM VALUE TESTS
    // =========================================================================

    @Test
    @DisplayName("Enum with single value")
    void testEnumWithSingleValue() {
        Map<String, String> values = new LinkedHashMap<>();
        values.put("ONLY", "only");

        registry.registerEnum("SingleEnum", values, null);

        var enumInfo = registry.getEnum("SingleEnum");
        assertTrue(enumInfo.isPresent());
        assertEquals(1, enumInfo.get().values.size());
        assertTrue(enumInfo.get().values.containsKey("ONLY"));
        assertEquals("only", enumInfo.get().values.get("ONLY"));
    }

    @Test
    @DisplayName("Enum with many values")
    void testEnumWithManyValues() {
        Map<String, String> values = new LinkedHashMap<>();
        for (int i = 1; i <= 10; i++) {
            values.put("VALUE_" + i, "value_" + i);
        }

        registry.registerEnum("LargeEnum", values, null);

        var enumInfo = registry.getEnum("LargeEnum");
        assertTrue(enumInfo.isPresent());
        assertEquals(10, enumInfo.get().values.size());
    }

    @Test
    @DisplayName("Enum values can have different formats")
    void testEnumValueFormats() {
        Map<String, String> values = new LinkedHashMap<>();
        values.put("SNAKE_CASE", "snake_case");
        values.put("UPPER_CASE", "UPPER_CASE");
        values.put("lowercase", "lowercase");
        values.put("CamelCase", "CamelCase");

        registry.registerEnum("FormatEnum", values, null);

        var enumInfo = registry.getEnum("FormatEnum");
        assertTrue(enumInfo.isPresent());
        assertEquals(4, enumInfo.get().values.size());
    }

    // =========================================================================
    // ENUM USAGE IN TYPES
    // =========================================================================

    @Test
    @DisplayName("Enum can be used in type field")
    void testEnumInTypeField() {
        // Register enum first
        Map<String, String> statusValues = new LinkedHashMap<>();
        statusValues.put("PENDING", "pending");
        statusValues.put("COMPLETE", "complete");
        registry.registerEnum("TaskStatus", statusValues, null);

        // Register type that uses enum
        FraiseQL.registerType(Task.class);

        var typeInfo = registry.getType("Task");
        assertTrue(typeInfo.isPresent());
        assertTrue(typeInfo.get().fields.containsKey("status"));
    }

    // =========================================================================
    // ENUM IN MUTATION ARGUMENTS
    // =========================================================================

    @Test
    @DisplayName("Enum can be used as mutation argument")
    void testEnumInMutationArgument() {
        // Register enum
        Map<String, String> statusValues = new LinkedHashMap<>();
        statusValues.put("ACTIVE", "active");
        statusValues.put("INACTIVE", "inactive");
        registry.registerEnum("UserStatus", statusValues, null);

        // Use in mutation
        FraiseQL.mutation("updateUserStatus")
            .returnType("User")
            .arg("userId", "Int")
            .arg("status", "UserStatus")
            .register();

        var mutation = registry.getMutation("updateUserStatus");
        assertTrue(mutation.isPresent());
        assertEquals(2, mutation.get().arguments.size());
        assertTrue(mutation.get().arguments.containsKey("status"));
    }

    // =========================================================================
    // CLEAR ENUMS TEST
    // =========================================================================

    @Test
    @DisplayName("Clear removes registered enums")
    void testClearRemovesEnums() {
        Map<String, String> values = new LinkedHashMap<>();
        values.put("VALUE1", "value1");
        registry.registerEnum("TestEnum", values, null);

        assertTrue(registry.getEnum("TestEnum").isPresent());

        registry.clear();

        assertFalse(registry.getEnum("TestEnum").isPresent());
        assertEquals(0, registry.getAllEnums().size());
    }

    // =========================================================================
    // COMMON ENUM PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: HTTP method enum")
    void testHttpMethodEnumPattern() {
        Map<String, String> methods = new LinkedHashMap<>();
        methods.put("GET", "GET");
        methods.put("POST", "POST");
        methods.put("PUT", "PUT");
        methods.put("DELETE", "DELETE");
        methods.put("PATCH", "PATCH");

        registry.registerEnum("HttpMethod", methods, "HTTP request methods");

        var enumInfo = registry.getEnum("HttpMethod");
        assertTrue(enumInfo.isPresent());
        assertEquals(5, enumInfo.get().values.size());
    }

    @Test
    @DisplayName("Pattern: Status transition enums")
    void testStatusTransitionEnumPattern() {
        Map<String, String> orderStatus = new LinkedHashMap<>();
        orderStatus.put("NEW", "new");
        orderStatus.put("CONFIRMED", "confirmed");
        orderStatus.put("PROCESSING", "processing");
        orderStatus.put("SHIPPED", "shipped");
        orderStatus.put("DELIVERED", "delivered");
        orderStatus.put("CANCELLED", "cancelled");

        registry.registerEnum("OrderStatus", orderStatus, "Order lifecycle states");

        var enumInfo = registry.getEnum("OrderStatus");
        assertTrue(enumInfo.isPresent());
        assertEquals(6, enumInfo.get().values.size());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType
    public static class Task {
        @GraphQLField
        public int id;

        @GraphQLField
        public String title;

        @GraphQLField
        public String status;
    }

    @GraphQLType
    public static class User {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;

        @GraphQLField
        public String status;
    }
}
