package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.Optional;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Phase 2 tests: Type system and schema registry
 */
public class Phase2Test {

    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
    }

    /**
     * Test TypeInfo generation with nullability
     */
    @Test
    public void testTypeInfoNullability() {
        TypeInfo nonNullable = new TypeInfo("String", false);
        assertEquals("String!", nonNullable.getGraphQLType());

        TypeInfo nullable = new TypeInfo("String", true);
        assertEquals("String", nullable.getGraphQLType());
    }

    /**
     * Test TypeInfo with list types
     */
    @Test
    public void testTypeInfoList() {
        TypeInfo list = new TypeInfo("User", false, true);
        assertEquals("[User]!", list.getGraphQLType());

        TypeInfo nullableList = new TypeInfo("User", true, true);
        assertEquals("[User]", nullableList.getGraphQLType());
    }

    /**
     * Test TypeConverter with basic Java types
     */
    @Test
    public void testTypeConverterBasicTypes() {
        assertEquals("Int", TypeConverter.javaToGraphQL(int.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(Integer.class));
        assertEquals("Float", TypeConverter.javaToGraphQL(float.class));
        assertEquals("Float", TypeConverter.javaToGraphQL(Float.class));
        assertEquals("Boolean", TypeConverter.javaToGraphQL(boolean.class));
        assertEquals("Boolean", TypeConverter.javaToGraphQL(Boolean.class));
        assertEquals("String", TypeConverter.javaToGraphQL(String.class));
    }

    /**
     * Test TypeConverter with numeric types
     */
    @Test
    public void testTypeConverterNumericTypes() {
        assertEquals("Int", TypeConverter.javaToGraphQL(long.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(Long.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(short.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(Short.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(byte.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(Byte.class));
    }

    /**
     * Test TypeConverter with temporal types
     */
    @Test
    public void testTypeConverterTemporalTypes() {
        assertEquals("String", TypeConverter.javaToGraphQL(java.time.LocalDate.class));
        assertEquals("String", TypeConverter.javaToGraphQL(java.time.LocalDateTime.class));
        assertEquals("String", TypeConverter.javaToGraphQL(java.util.Date.class));
        assertEquals("String", TypeConverter.javaToGraphQL(java.sql.Date.class));
        assertEquals("String", TypeConverter.javaToGraphQL(java.sql.Timestamp.class));
    }

    /**
     * Test TypeConverter with UUID
     */
    @Test
    public void testTypeConverterUUID() {
        assertEquals("String", TypeConverter.javaToGraphQL(java.util.UUID.class));
    }

    /**
     * Test TypeConverter with BigDecimal and BigInteger
     */
    @Test
    public void testTypeConverterBigNumbers() {
        assertEquals("Float", TypeConverter.javaToGraphQL(java.math.BigDecimal.class));
        assertEquals("Float", TypeConverter.javaToGraphQL(java.math.BigInteger.class));
    }

    /**
     * Test SchemaRegistry singleton
     */
    @Test
    public void testSchemaRegistrySingleton() {
        SchemaRegistry registry1 = SchemaRegistry.getInstance();
        SchemaRegistry registry2 = SchemaRegistry.getInstance();
        assertSame(registry1, registry2);
    }

    /**
     * Test registering a simple type
     */
    @Test
    public void testRegisterSimpleType() {
        FraiseQL.registerType(User.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.GraphQLTypeInfo> typeInfo = registry.getType("User");

        assertTrue(typeInfo.isPresent());
        assertEquals("User", typeInfo.get().name);
        assertEquals(2, typeInfo.get().fields.size());
    }

    /**
     * Test registering multiple types
     */
    @Test
    public void testRegisterMultipleTypes() {
        FraiseQL.registerTypes(User.class, Post.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        assertEquals(2, registry.getAllTypes().size());
        assertTrue(registry.getType("User").isPresent());
        assertTrue(registry.getType("Post").isPresent());
    }

    /**
     * Test field extraction from annotated type
     */
    @Test
    public void testFieldExtraction() {
        var fields = TypeConverter.extractFields(User.class);

        assertEquals(2, fields.size());
        assertTrue(fields.containsKey("id"));
        assertTrue(fields.containsKey("name"));

        assertEquals("Int!", fields.get("id").getGraphQLType());
        assertEquals("String!", fields.get("name").getGraphQLType());
    }

    /**
     * Test query registration
     */
    @Test
    public void testRegisterQuery() {
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .arg("limit", "Int")
            .description("Get all users")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.QueryInfo> queryInfo = registry.getQuery("users");

        assertTrue(queryInfo.isPresent());
        assertEquals("[User]", queryInfo.get().returnType);
        assertEquals(1, queryInfo.get().arguments.size());
        assertEquals("Get all users", queryInfo.get().description);
    }

    /**
     * Test mutation registration
     */
    @Test
    public void testRegisterMutation() {
        FraiseQL.mutation("createUser")
            .returnType("User")
            .arg("name", "String")
            .arg("email", "String")
            .description("Create a new user")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.MutationInfo> mutationInfo = registry.getMutation("createUser");

        assertTrue(mutationInfo.isPresent());
        assertEquals("User", mutationInfo.get().returnType);
        assertEquals(2, mutationInfo.get().arguments.size());
        assertEquals("Create a new user", mutationInfo.get().description);
    }

    /**
     * Test query builder with class return type
     */
    @Test
    public void testQueryBuilderWithClass() {
        FraiseQL.query("getUser")
            .returnType(User.class)
            .arg("id", "Int")
            .description("Get user by ID")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.QueryInfo> queryInfo = registry.getQuery("getUser");

        assertTrue(queryInfo.isPresent());
        assertEquals("User", queryInfo.get().returnType);
    }

    /**
     * Test mutation builder with class return type
     */
    @Test
    public void testMutationBuilderWithClass() {
        FraiseQL.mutation("updateUser")
            .returnType(User.class)
            .arg("id", "Int")
            .arg("name", "String")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.MutationInfo> mutationInfo = registry.getMutation("updateUser");

        assertTrue(mutationInfo.isPresent());
        assertEquals("User", mutationInfo.get().returnType);
    }

    /**
     * Test complete schema setup
     */
    @Test
    public void testCompleteSchema() {
        // Register types
        FraiseQL.registerType(User.class);
        FraiseQL.registerType(Post.class);

        // Register queries
        FraiseQL.query("users")
            .returnType(User.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .register();

        FraiseQL.query("getPosts")
            .returnType(Post.class)
            .returnsArray(true)
            .arg("userId", "Int")
            .register();

        // Register mutations
        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .register();

        FraiseQL.mutation("createPost")
            .returnType(Post.class)
            .arg("userId", "Int")
            .arg("title", "String")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();

        // Verify types
        assertEquals(2, registry.getAllTypes().size());

        // Verify queries
        assertEquals(2, registry.getAllQueries().size());
        assertTrue(registry.getQuery("users").isPresent());
        assertTrue(registry.getQuery("getPosts").isPresent());

        // Verify mutations
        assertEquals(2, registry.getAllMutations().size());
        assertTrue(registry.getMutation("createUser").isPresent());
        assertTrue(registry.getMutation("createPost").isPresent());
    }

    /**
     * Test clearing registry
     */
    @Test
    public void testClearRegistry() {
        FraiseQL.registerType(User.class);
        FraiseQL.query("users").returnType(User.class).register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        assertTrue(registry.getAllTypes().size() > 0);

        FraiseQL.clear();

        assertEquals(0, registry.getAllTypes().size());
        assertEquals(0, registry.getAllQueries().size());
        assertEquals(0, registry.getAllMutations().size());
    }

    /**
     * Test TypeInfo equality
     */
    @Test
    public void testTypeInfoEquality() {
        TypeInfo type1 = new TypeInfo("String", false);
        TypeInfo type2 = new TypeInfo("String", false);
        TypeInfo type3 = new TypeInfo("String", true);

        assertEquals(type1, type2);
        assertNotEquals(type1, type3);
    }

    /**
     * Test TypeInfo with description
     */
    @Test
    public void testTypeInfoWithDescription() {
        TypeInfo typeInfo = new TypeInfo("User", false, false, "A user in the system");
        assertEquals("A user in the system", typeInfo.description);
    }

    // Test fixture classes

    @GraphQLType(description = "A user account")
    public static class User {
        @GraphQLField
        public int id;

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
    }
}
