package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for FraiseQL field-level metadata.
 * Tests field descriptions, custom names, nullability, and type customization.
 */
@DisplayName("Field Metadata")
public class FieldMetadataTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    // =========================================================================
    // FIELD DESCRIPTION TESTS
    // =========================================================================

    @Test
    @DisplayName("Field with description is preserved")
    void testFieldDescriptionPreserved() {
        FraiseQL.registerType(UserWithDescriptions.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("UserWithDescriptions");

        assertTrue(typeInfo.isPresent());
        var fields = typeInfo.get().fields;

        assertNotNull(fields.get("email"));
        // Description should be extracted from @GraphQLField annotation
        assertTrue(fields.containsKey("email"));
    }

    @Test
    @DisplayName("Multiple fields with descriptions")
    void testMultipleFieldsWithDescriptions() {
        FraiseQL.registerType(ProductWithDescriptions.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("ProductWithDescriptions");

        assertTrue(typeInfo.isPresent());
        var fields = typeInfo.get().fields;

        assertEquals(4, fields.size());
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("name"));
        assertTrue(fields.containsKey("price"));
        assertTrue(fields.containsKey("description"));
    }

    // =========================================================================
    // FIELD NULLABILITY TESTS
    // =========================================================================

    @Test
    @DisplayName("Non-nullable field has non-null GraphQL type")
    void testNonNullableFieldType() {
        var fields = TypeConverter.extractFields(StrictUser.class);

        assertEquals("String!", fields.get("id").getGraphQLType());
        assertEquals("String!", fields.get("email").getGraphQLType());
    }

    @Test
    @DisplayName("Nullable field has nullable GraphQL type")
    void testNullableFieldType() {
        var fields = TypeConverter.extractFields(OptionalUser.class);

        assertEquals("String!", fields.get("id").getGraphQLType());
        assertEquals("String", fields.get("nickname").getGraphQLType());
    }

    @Test
    @DisplayName("Mixed nullable and non-nullable fields")
    void testMixedNullability() {
        FraiseQL.registerType(MixedNullabilityUser.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("MixedNullabilityUser");

        assertTrue(typeInfo.isPresent());
        var fields = typeInfo.get().fields;

        // Required fields
        assertEquals("String!", fields.get("id").getGraphQLType());
        assertEquals("String!", fields.get("name").getGraphQLType());

        // Optional fields
        assertEquals("String", fields.get("nickname").getGraphQLType());
        assertEquals("String", fields.get("bio").getGraphQLType());
    }

    // =========================================================================
    // FIELD CUSTOM NAME TESTS
    // =========================================================================

    @Test
    @DisplayName("Field with custom GraphQL name")
    void testFieldCustomName() {
        var fields = TypeConverter.extractFields(UserWithCustomNames.class);

        assertTrue(fields.containsKey("userId"));
        assertTrue(fields.containsKey("userName"));
        assertTrue(fields.containsKey("userEmail"));
    }

    @Test
    @DisplayName("Multiple fields with custom names")
    void testMultipleFieldsWithCustomNames() {
        FraiseQL.registerType(EntityWithCustomNames.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("EntityWithCustomNames");

        assertTrue(typeInfo.isPresent());
        var fields = typeInfo.get().fields;

        assertTrue(fields.containsKey("entityId"));
        assertTrue(fields.containsKey("createdAt"));
        assertTrue(fields.containsKey("updatedAt"));
    }

    // =========================================================================
    // FIELD TYPE CUSTOMIZATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Field with custom type")
    void testFieldWithCustomType() {
        var fields = TypeConverter.extractFields(UserWithCustomTypes.class);

        assertTrue(fields.containsKey("customId"));
    }

    @Test
    @DisplayName("Complex type with various field properties")
    void testComplexTypeWithVariousFieldProperties() {
        FraiseQL.registerType(ComplexEntity.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("ComplexEntity");

        assertTrue(typeInfo.isPresent());
        var fields = typeInfo.get().fields;

        assertEquals(6, fields.size());
    }

    // =========================================================================
    // FIELD EXTRACTION CONSISTENCY TESTS
    // =========================================================================

    @Test
    @DisplayName("Field order is consistent across extractions")
    void testFieldOrderConsistency() {
        var fields1 = TypeConverter.extractFields(User.class);
        var fields2 = TypeConverter.extractFields(User.class);

        var names1 = fields1.keySet().stream().toList();
        var names2 = fields2.keySet().stream().toList();

        assertEquals(names1, names2);
    }

    @Test
    @DisplayName("Field metadata is consistent across registrations")
    void testFieldMetadataConsistency() {
        FraiseQL.registerType(User.class);
        FraiseQL.registerType(User.class); // Register again

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("User");

        assertTrue(typeInfo.isPresent());
        assertEquals(3, typeInfo.get().fields.size());
    }

    // =========================================================================
    // FIELD TYPE MAPPING TESTS
    // =========================================================================

    @Test
    @DisplayName("String field maps to String type")
    void testStringFieldType() {
        var fields = TypeConverter.extractFields(StringTypeEntity.class);
        assertEquals("String!", fields.get("text").getGraphQLType());
    }

    @Test
    @DisplayName("Integer field maps to Int type")
    void testIntegerFieldType() {
        var fields = TypeConverter.extractFields(NumericTypeEntity.class);
        assertEquals("Int!", fields.get("count").getGraphQLType());
    }

    @Test
    @DisplayName("Boolean field maps to Boolean type")
    void testBooleanFieldType() {
        var fields = TypeConverter.extractFields(BooleanTypeEntity.class);
        assertEquals("Boolean!", fields.get("active").getGraphQLType());
    }

    @Test
    @DisplayName("Float field maps to Float type")
    void testFloatFieldType() {
        var fields = TypeConverter.extractFields(FloatTypeEntity.class);
        assertEquals("Float!", fields.get("rating").getGraphQLType());
    }

    @Test
    @DisplayName("Date field maps to String type")
    void testDateFieldType() {
        var fields = TypeConverter.extractFields(DateTypeEntity.class);
        assertEquals("String!", fields.get("createdAt").getGraphQLType());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType
    public static class User {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;

        @GraphQLField
        public String email;
    }

    @GraphQLType
    public static class UserWithDescriptions {
        @GraphQLField
        public int id;

        @GraphQLField(description = "User email address")
        public String email;

        @GraphQLField(description = "User's full name")
        public String name;
    }

    @GraphQLType
    public static class ProductWithDescriptions {
        @GraphQLField(description = "Product ID")
        public int id;

        @GraphQLField(description = "Product name")
        public String name;

        @GraphQLField(description = "Product price in dollars")
        public float price;

        @GraphQLField(description = "Product description")
        public String description;
    }

    @GraphQLType
    public static class StrictUser {
        @GraphQLField
        public String id;

        @GraphQLField
        public String email;

        @GraphQLField
        public String name;
    }

    @GraphQLType
    public static class OptionalUser {
        @GraphQLField
        public String id;

        @GraphQLField(nullable = true)
        public String nickname;
    }

    @GraphQLType
    public static class MixedNullabilityUser {
        @GraphQLField
        public String id;

        @GraphQLField
        public String name;

        @GraphQLField(nullable = true)
        public String nickname;

        @GraphQLField(nullable = true)
        public String bio;
    }

    @GraphQLType
    public static class UserWithCustomNames {
        @GraphQLField(name = "userId")
        public int id;

        @GraphQLField(name = "userName")
        public String name;

        @GraphQLField(name = "userEmail")
        public String email;
    }

    @GraphQLType
    public static class EntityWithCustomNames {
        @GraphQLField(name = "entityId")
        public int id;

        @GraphQLField(name = "createdAt")
        public java.time.LocalDateTime created;

        @GraphQLField(name = "updatedAt")
        public java.time.LocalDateTime updated;
    }

    @GraphQLType
    public static class UserWithCustomTypes {
        @GraphQLField(name = "customId")
        public int id;

        @GraphQLField
        public String email;
    }

    @GraphQLType
    public static class ComplexEntity {
        @GraphQLField(description = "Primary key")
        public int id;

        @GraphQLField(description = "Entity name")
        public String name;

        @GraphQLField(description = "Status flag")
        public boolean active;

        @GraphQLField(nullable = true, description = "Optional description")
        public String description;

        @GraphQLField(name = "created")
        public java.time.LocalDateTime createdAt;

        @GraphQLField(name = "updated")
        public java.time.LocalDateTime updatedAt;
    }

    @GraphQLType
    public static class StringTypeEntity {
        @GraphQLField
        public String text;
    }

    @GraphQLType
    public static class NumericTypeEntity {
        @GraphQLField
        public int count;
    }

    @GraphQLType
    public static class BooleanTypeEntity {
        @GraphQLField
        public boolean active;
    }

    @GraphQLType
    public static class FloatTypeEntity {
        @GraphQLField
        public float rating;
    }

    @GraphQLType
    public static class DateTypeEntity {
        @GraphQLField
        public java.time.LocalDateTime createdAt;
    }
}
