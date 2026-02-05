package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Comprehensive tests for FraiseQL Java type system.
 * Tests type registration, field extraction, type conversion,
 * and complex type scenarios.
 */
@DisplayName("Type System")
public class TypeSystemTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    // =========================================================================
    // TYPE REGISTRATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Register simple type with fields")
    void testRegisterSimpleType() {
        FraiseQL.registerType(User.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("User");

        assertTrue(typeInfo.isPresent());
        assertEquals("User", typeInfo.get().name);
        assertEquals(3, typeInfo.get().fields.size());
        assertTrue(typeInfo.get().fields.containsKey("id"));
        assertTrue(typeInfo.get().fields.containsKey("email"));
        assertTrue(typeInfo.get().fields.containsKey("name"));
    }

    @Test
    @DisplayName("Register multiple types")
    void testRegisterMultipleTypes() {
        FraiseQL.registerTypes(User.class, Post.class, Comment.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        assertEquals(3, registry.getAllTypes().size());
        assertTrue(registry.getType("User").isPresent());
        assertTrue(registry.getType("Post").isPresent());
        assertTrue(registry.getType("Comment").isPresent());
    }

    @Test
    @DisplayName("Register type with custom name")
    void testRegisterTypeWithCustomName() {
        FraiseQL.registerType(UserAccount.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("Account");

        assertTrue(typeInfo.isPresent());
        assertEquals("Account", typeInfo.get().name);
    }

    @Test
    @DisplayName("Type includes field metadata")
    void testTypeIncludesFieldMetadata() {
        FraiseQL.registerType(UserWithMetadata.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("UserWithMetadata");

        assertTrue(typeInfo.isPresent());
        var fields = typeInfo.get().fields;

        // Check field with description
        assertTrue(fields.containsKey("name"));
        assertNotNull(fields.get("name"));
    }

    @Test
    @DisplayName("Type description is preserved")
    void testTypeDescriptionPreserved() {
        FraiseQL.registerType(Post.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("Post");

        assertTrue(typeInfo.isPresent());
        assertEquals("A blog post", typeInfo.get().description);
    }

    // =========================================================================
    // FIELD EXTRACTION TESTS
    // =========================================================================

    @Test
    @DisplayName("Extract fields from type")
    void testExtractFieldsFromType() {
        var fields = TypeConverter.extractFields(User.class);

        assertEquals(3, fields.size());
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("email"));
        assertTrue(fields.containsKey("name"));
    }

    @Test
    @DisplayName("Field types are correctly identified")
    void testFieldTypesCorrectlyIdentified() {
        var fields = TypeConverter.extractFields(User.class);

        assertEquals("Int!", fields.get("id").getGraphQLType());
        assertEquals("String!", fields.get("email").getGraphQLType());
        assertEquals("String!", fields.get("name").getGraphQLType());
    }

    @Test
    @DisplayName("Field nullability is respected")
    void testFieldNullabilityRespected() {
        var fields = TypeConverter.extractFields(UserWithNullable.class);

        assertEquals("String!", fields.get("id").getGraphQLType());
        assertEquals("String", fields.get("nickname").getGraphQLType());
    }

    @Test
    @DisplayName("Field custom names are extracted")
    void testFieldCustomNamesExtracted() {
        var fields = TypeConverter.extractFields(UserWithCustomFieldNames.class);

        assertTrue(fields.containsKey("userId"));
        assertTrue(fields.containsKey("userName"));
    }

    // =========================================================================
    // TYPE CONVERSION TESTS
    // =========================================================================

    @Test
    @DisplayName("Convert primitive int to GraphQL Int")
    void testConvertPrimitiveInt() {
        assertEquals("Int", TypeConverter.javaToGraphQL(int.class));
    }

    @Test
    @DisplayName("Convert Integer class to GraphQL Int")
    void testConvertIntegerClass() {
        assertEquals("Int", TypeConverter.javaToGraphQL(Integer.class));
    }

    @Test
    @DisplayName("Convert String to GraphQL String")
    void testConvertString() {
        assertEquals("String", TypeConverter.javaToGraphQL(String.class));
    }

    @Test
    @DisplayName("Convert boolean to GraphQL Boolean")
    void testConvertBoolean() {
        assertEquals("Boolean", TypeConverter.javaToGraphQL(boolean.class));
        assertEquals("Boolean", TypeConverter.javaToGraphQL(Boolean.class));
    }

    @Test
    @DisplayName("Convert float to GraphQL Float")
    void testConvertFloat() {
        assertEquals("Float", TypeConverter.javaToGraphQL(float.class));
        assertEquals("Float", TypeConverter.javaToGraphQL(Float.class));
    }

    @Test
    @DisplayName("Convert double to GraphQL Float")
    void testConvertDouble() {
        assertEquals("Float", TypeConverter.javaToGraphQL(double.class));
        assertEquals("Float", TypeConverter.javaToGraphQL(Double.class));
    }

    @Test
    @DisplayName("Convert long to GraphQL Int")
    void testConvertLong() {
        assertEquals("Int", TypeConverter.javaToGraphQL(long.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(Long.class));
    }

    @Test
    @DisplayName("Convert LocalDate to GraphQL String")
    void testConvertLocalDate() {
        assertEquals("String", TypeConverter.javaToGraphQL(java.time.LocalDate.class));
    }

    @Test
    @DisplayName("Convert LocalDateTime to GraphQL String")
    void testConvertLocalDateTime() {
        assertEquals("String", TypeConverter.javaToGraphQL(java.time.LocalDateTime.class));
    }

    @Test
    @DisplayName("Convert UUID to GraphQL String")
    void testConvertUUID() {
        assertEquals("String", TypeConverter.javaToGraphQL(java.util.UUID.class));
    }

    @Test
    @DisplayName("Convert BigDecimal to GraphQL Float")
    void testConvertBigDecimal() {
        assertEquals("Float", TypeConverter.javaToGraphQL(java.math.BigDecimal.class));
    }

    @Test
    @DisplayName("Convert custom type returns class simple name")
    void testConvertCustomType() {
        assertEquals("User", TypeConverter.javaToGraphQL(User.class));
    }

    // =========================================================================
    // COMPLEX TYPE SCENARIOS
    // =========================================================================

    @Test
    @DisplayName("Register type with many fields")
    void testRegisterTypeWithManyFields() {
        FraiseQL.registerType(ComplexType.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("ComplexType");

        assertTrue(typeInfo.isPresent());
        assertEquals(8, typeInfo.get().fields.size());
    }

    @Test
    @DisplayName("Multiple types with shared field names")
    void testMultipleTypesWithSharedFieldNames() {
        FraiseQL.registerTypes(User.class, Post.class, Comment.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();

        var userFields = registry.getType("User").get().fields;
        var postFields = registry.getType("Post").get().fields;

        // Both can have id field
        assertTrue(userFields.containsKey("id"));
        assertTrue(postFields.containsKey("id"));
    }

    @Test
    @DisplayName("Type field order is preserved")
    void testTypeFieldOrderPreserved() {
        var fields = TypeConverter.extractFields(User.class);

        // LinkedHashMap preserves insertion order
        var fieldNames = fields.keySet().stream().toList();
        assertEquals("id", fieldNames.get(0));
        assertEquals("email", fieldNames.get(1));
        assertEquals("name", fieldNames.get(2));
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType(description = "A user account")
    public static class User {
        @GraphQLField
        public int id;

        @GraphQLField
        public String email;

        @GraphQLField
        public String name;
    }

    @GraphQLType(description = "A blog post")
    public static class Post {
        @GraphQLField
        public int id;

        @GraphQLField
        public int userId;

        @GraphQLField
        public String title;

        @GraphQLField
        public String content;
    }

    @GraphQLType(description = "A comment on a post")
    public static class Comment {
        @GraphQLField
        public int id;

        @GraphQLField
        public int postId;

        @GraphQLField
        public int userId;

        @GraphQLField
        public String text;
    }

    @GraphQLType(name = "Account")
    public static class UserAccount {
        @GraphQLField
        public int id;

        @GraphQLField
        public String username;
    }

    @GraphQLType
    public static class UserWithMetadata {
        @GraphQLField
        public int id;

        @GraphQLField(description = "The user's full name")
        public String name;

        @GraphQLField(description = "Email address for notifications")
        public String email;
    }

    @GraphQLType
    public static class UserWithNullable {
        @GraphQLField
        public String id;

        @GraphQLField(nullable = true)
        public String nickname;
    }

    @GraphQLType
    public static class UserWithCustomFieldNames {
        @GraphQLField(name = "userId")
        public int id;

        @GraphQLField(name = "userName")
        public String name;
    }

    @GraphQLType
    public static class ComplexType {
        @GraphQLField
        public int id;

        @GraphQLField
        public String stringField;

        @GraphQLField
        public boolean booleanField;

        @GraphQLField
        public float floatField;

        @GraphQLField
        public long longField;

        @GraphQLField
        public java.time.LocalDate dateField;

        @GraphQLField
        public java.time.LocalDateTime dateTimeField;

        @GraphQLField(nullable = true)
        public String nullableField;
    }
}
