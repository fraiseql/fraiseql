package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Phase 5 tests: Advanced features including argument defaults, validation, and nullable types
 */
public class Phase5AdvancedTest {

    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
    }

    /**
     * Test ArgumentBuilder with defaults
     */
    @Test
    public void testArgumentBuilderBasic() {
        ArgumentBuilder builder = new ArgumentBuilder()
            .add("limit", "Int")
            .add("offset", "Int");

        Map<String, String> args = builder.build();
        assertEquals(2, args.size());
        assertEquals("Int", args.get("limit"));
        assertEquals("Int", args.get("offset"));
    }

    /**
     * Test ArgumentBuilder with default values
     */
    @Test
    public void testArgumentBuilderWithDefaults() {
        ArgumentBuilder builder = new ArgumentBuilder()
            .add("limit", "Int", 10)
            .add("offset", "Int", 0)
            .add("filter", "String", null);

        assertTrue(builder.hasDefault("limit"));
        assertTrue(builder.hasDefault("offset"));
        assertFalse(builder.hasDefault("filter"));
        assertEquals(10, builder.getDefault("limit"));
        assertEquals(0, builder.getDefault("offset"));
    }

    /**
     * Test ArgumentBuilder with descriptions
     */
    @Test
    public void testArgumentBuilderWithDescriptions() {
        ArgumentBuilder builder = new ArgumentBuilder()
            .add("limit", "Int", 10, "Maximum items to return")
            .add("offset", "Int", 0, "Pagination offset")
            .add("sort", "String", null, "Sort order");

        Map<String, ArgumentBuilder.ArgumentInfo> detailed = builder.buildDetailed();
        assertEquals(3, detailed.size());
        assertEquals("Maximum items to return", detailed.get("limit").description);
    }

    /**
     * Test ArgumentBuilder with defaults filtering
     */
    @Test
    public void testArgumentBuilderGetDefaults() {
        ArgumentBuilder builder = new ArgumentBuilder()
            .add("id", "Int")
            .add("limit", "Int", 10)
            .add("offset", "Int", 0)
            .add("filter", "String");

        var withDefaults = builder.getArgumentsWithDefaults();
        assertEquals(2, withDefaults.size());
        assertTrue(withDefaults.stream().anyMatch(a -> a.name.equals("limit")));
        assertTrue(withDefaults.stream().anyMatch(a -> a.name.equals("offset")));
    }

    /**
     * Test schema validation: valid schema
     */
    @Test
    public void testValidateValidSchema() {
        FraiseQL.registerType(User.class);
        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        assertTrue(result.valid);
        assertTrue(result.errors.isEmpty());
    }

    /**
     * Test schema validation: undefined return type
     */
    @Test
    public void testValidateUndefinedType() {
        FraiseQL.query("user")
            .returnType("UndefinedType")
            .arg("id", "Int")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        assertFalse(result.valid);
        assertTrue(result.errors.stream()
            .anyMatch(e -> e.contains("UndefinedType")));
    }

    /**
     * Test schema validation: empty schema warning
     */
    @Test
    public void testValidateEmptySchema() {
        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        assertTrue(result.valid);
        assertTrue(result.warnings.stream()
            .anyMatch(w -> w.contains("No types")));
    }

    /**
     * Test schema validation: type with no fields warning
     */
    @Test
    public void testValidateTypeWithNoFields() {
        @GraphQLType
        class EmptyType {
        }

        FraiseQL.registerType(EmptyType.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        assertTrue(result.valid);
        assertTrue(result.warnings.stream()
            .anyMatch(w -> w.contains("has no fields")));
    }

    /**
     * Test schema validation: query with no arguments warning
     */
    @Test
    public void testValidateQueryNoArgs() {
        FraiseQL.registerType(User.class);
        FraiseQL.query("allUsers")
            .returnType(User.class)
            .returnsArray(true)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        assertTrue(result.valid);
        assertTrue(result.warnings.stream()
            .anyMatch(w -> w.contains("has no arguments")));
    }

    /**
     * Test schema validator: statistics
     */
    @Test
    public void testValidatorStatistics() {
        FraiseQL.registerTypes(User.class, Post.class);
        FraiseQL.query("users").returnType(User.class).register();
        FraiseQL.query("posts").returnType(Post.class).register();
        FraiseQL.mutation("createUser").returnType(User.class).arg("name", "String").register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        String stats = SchemaValidator.getStatistics(registry);

        assertTrue(stats.contains("2 types"));
        assertTrue(stats.contains("2 queries"));
        assertTrue(stats.contains("1 mutation"));
    }

    /**
     * Test nullable field representation
     */
    @Test
    public void testNullableFieldType() {
        var fields = TypeConverter.extractFields(UserWithOptional.class);

        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("email"));

        var idField = fields.get("id");
        assertFalse(idField.nullable);
        assertEquals("Int!", idField.getGraphQLType());

        var emailField = fields.get("email");
        assertTrue(emailField.nullable);
        assertEquals("String", emailField.getGraphQLType());
    }

    /**
     * Test field with list type
     */
    @Test
    public void testListFieldType() {
        var fields = TypeConverter.extractFields(TypeWithList.class);

        var tagsField = fields.get("tags");
        assertTrue(tagsField.isList);
        assertEquals("[String]!", tagsField.getGraphQLType());
    }

    /**
     * Test schema with nullable and list combinations
     */
    @Test
    public void testComplexTypeSchema() {
        FraiseQL.registerType(ComplexType.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("ComplexType").get();

        assertEquals(4, typeInfo.fields.size());

        // Required scalar
        var idField = typeInfo.fields.get("id");
        assertEquals("Int!", idField.getGraphQLType());

        // Nullable scalar
        var nameField = typeInfo.fields.get("name");
        assertEquals("String", nameField.getGraphQLType());

        // List of non-null
        var tagsField = typeInfo.fields.get("tags");
        assertEquals("[String]!", tagsField.getGraphQLType());

        // Nullable list
        var optionalTagsField = typeInfo.fields.get("optionalTags");
        assertEquals("[String]", optionalTagsField.getGraphQLType());
    }

    /**
     * Test ArgumentBuilder with optional method
     */
    @Test
    public void testArgumentOptional() {
        ArgumentBuilder builder = new ArgumentBuilder()
            .add("required", "Int")
            .add("withDefault", "Int", 10)
            .add("optional", "String");

        var detailed = builder.buildDetailed();

        // Argument with default is optional
        assertTrue(detailed.get("withDefault").isOptional());

        // Argument without default is not optional
        assertFalse(detailed.get("required").isOptional());
    }

    /**
     * Test validation result toString
     */
    @Test
    public void testValidationResultString() {
        FraiseQL.registerType(User.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        String str = result.toString();
        assertTrue(str.contains("valid=true"));
        assertTrue(str.contains("ValidationResult"));
    }

    /**
     * Test schema with all type features
     */
    @Test
    public void testComprehensiveSchema() {
        // Register types
        FraiseQL.registerTypes(User.class, Post.class, ComplexType.class);

        // Create query with ArgumentBuilder
        ArgumentBuilder userQueryArgs = new ArgumentBuilder()
            .add("id", "Int", null, "User ID")
            .add("limit", "Int", 10, "Results limit");

        // Register queries
        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        FraiseQL.query("posts")
            .returnType(Post.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .description("Get posts with pagination")
            .register();

        // Register mutations
        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .arg("email", "String")
            .register();

        // Validate
        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        assertTrue(result.valid);
        assertEquals(3, registry.getAllTypes().size());
        assertEquals(2, registry.getAllQueries().size());
        assertEquals(1, registry.getAllMutations().size());
    }

    // Test fixture types

    @GraphQLType
    public static class User {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;
    }

    @GraphQLType
    public static class Post {
        @GraphQLField
        public int id;

        @GraphQLField
        public String title;
    }

    @GraphQLType
    public static class UserWithOptional {
        @GraphQLField
        public int id;

        @GraphQLField(nullable = true)
        public String email;
    }

    @GraphQLType
    public static class TypeWithList {
        @GraphQLField
        public int id;

        @GraphQLField
        public String[] tags;
    }

    @GraphQLType
    public static class ComplexType {
        @GraphQLField
        public int id;

        @GraphQLField(nullable = true)
        public String name;

        @GraphQLField
        public String[] tags;

        @GraphQLField(nullable = true)
        public String[] optionalTags;
    }
}
