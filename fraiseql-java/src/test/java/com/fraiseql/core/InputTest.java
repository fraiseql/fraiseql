package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL input type support in FraiseQL Java.
 * Input types are used as arguments to queries and mutations.
 */
@DisplayName("GraphQL Input Types")
public class InputTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // INPUT TYPE REGISTRATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Register input type with fields")
    void testRegisterInputTypeWithFields() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("email", new TypeConverter.GraphQLFieldInfo("String", false, false, "User email"));
        fields.put("name", new TypeConverter.GraphQLFieldInfo("String", false, false, "User name"));
        fields.put("phone", new TypeConverter.GraphQLFieldInfo("String", true, false, "User phone"));

        registry.registerInputType("CreateUserInput", fields, "Input for creating user");

        var inputInfo = registry.getInputType("CreateUserInput");
        assertTrue(inputInfo.isPresent());
        assertEquals("CreateUserInput", inputInfo.get().name);
        assertEquals(3, inputInfo.get().fields.size());
        assertEquals("Input for creating user", inputInfo.get().description);
    }

    @Test
    @DisplayName("Register input type without description")
    void testRegisterInputTypeWithoutDescription() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("query", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));
        fields.put("limit", new TypeConverter.GraphQLFieldInfo("Int", true, false, ""));

        registry.registerInputType("SearchInput", fields, null);

        var inputInfo = registry.getInputType("SearchInput");
        assertTrue(inputInfo.isPresent());
        assertEquals("SearchInput", inputInfo.get().name);
        assertEquals(2, inputInfo.get().fields.size());
    }

    @Test
    @DisplayName("Register multiple input types")
    void testRegisterMultipleInputTypes() {
        Map<String, TypeConverter.GraphQLFieldInfo> userFields = new HashMap<>();
        userFields.put("name", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));

        Map<String, TypeConverter.GraphQLFieldInfo> postFields = new HashMap<>();
        postFields.put("title", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));

        registry.registerInputType("CreateUserInput", userFields, null);
        registry.registerInputType("CreatePostInput", postFields, null);

        assertEquals(2, registry.getAllInputTypes().size());
        assertTrue(registry.getInputType("CreateUserInput").isPresent());
        assertTrue(registry.getInputType("CreatePostInput").isPresent());
    }

    // =========================================================================
    // INPUT FIELD TESTS
    // =========================================================================

    @Test
    @DisplayName("Input type with single field")
    void testInputTypeWithSingleField() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("Int", false, false, ""));

        registry.registerInputType("IdInput", fields, null);

        var inputInfo = registry.getInputType("IdInput");
        assertTrue(inputInfo.isPresent());
        assertEquals(1, inputInfo.get().fields.size());
    }

    @Test
    @DisplayName("Input type with many fields")
    void testInputTypeWithManyFields() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("field1", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));
        fields.put("field2", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));
        fields.put("field3", new TypeConverter.GraphQLFieldInfo("Int", false, false, ""));
        fields.put("field4", new TypeConverter.GraphQLFieldInfo("Int", true, false, ""));
        fields.put("field5", new TypeConverter.GraphQLFieldInfo("Boolean", true, false, ""));

        registry.registerInputType("ComplexInput", fields, null);

        var inputInfo = registry.getInputType("ComplexInput");
        assertTrue(inputInfo.isPresent());
        assertEquals(5, inputInfo.get().fields.size());
    }

    @Test
    @DisplayName("Input type fields include descriptions")
    void testInputTypeFieldsIncludeDescriptions() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("email", new TypeConverter.GraphQLFieldInfo("String", false, false, "Email address"));
        fields.put("password", new TypeConverter.GraphQLFieldInfo("String", false, false, "Password"));
        fields.put("rememberMe", new TypeConverter.GraphQLFieldInfo("Boolean", true, false, "Remember login"));

        registry.registerInputType("LoginInput", fields, null);

        var inputInfo = registry.getInputType("LoginInput");
        assertTrue(inputInfo.isPresent());
        assertTrue(inputInfo.get().fields.containsKey("email"));
    }

    // =========================================================================
    // INPUT TYPE NULLABILITY TESTS
    // =========================================================================

    @Test
    @DisplayName("Input with required and optional fields")
    void testInputWithRequiredAndOptionalFields() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("email", new TypeConverter.GraphQLFieldInfo("String", false, false, "Required email"));
        fields.put("phone", new TypeConverter.GraphQLFieldInfo("String", true, false, "Optional phone"));
        fields.put("address", new TypeConverter.GraphQLFieldInfo("String", true, false, "Optional address"));

        registry.registerInputType("ContactInput", fields, null);

        var inputInfo = registry.getInputType("ContactInput");
        assertTrue(inputInfo.isPresent());
        assertEquals(3, inputInfo.get().fields.size());
    }

    // =========================================================================
    // INPUT TYPE USAGE PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Create mutation input")
    void testCreateMutationInputPattern() {
        Map<String, TypeConverter.GraphQLFieldInfo> createFields = new HashMap<>();
        createFields.put("email", new TypeConverter.GraphQLFieldInfo("String", false, false, "Email"));
        createFields.put("name", new TypeConverter.GraphQLFieldInfo("String", false, false, "Name"));
        createFields.put("phone", new TypeConverter.GraphQLFieldInfo("String", true, false, "Phone"));

        registry.registerInputType("CreateUserInput", createFields, "User creation input");

        // Use in mutation
        FraiseQL.mutation("createUser")
            .returnType("User")
            .arg("input", "CreateUserInput")
            .register();

        var mutation = registry.getMutation("createUser");
        assertTrue(mutation.isPresent());
        assertTrue(mutation.get().arguments.containsKey("input"));
    }

    @Test
    @DisplayName("Pattern: Update mutation input")
    void testUpdateMutationInputPattern() {
        Map<String, TypeConverter.GraphQLFieldInfo> updateFields = new HashMap<>();
        updateFields.put("id", new TypeConverter.GraphQLFieldInfo("Int", false, false, "User ID"));
        updateFields.put("email", new TypeConverter.GraphQLFieldInfo("String", true, false, "New email"));
        updateFields.put("name", new TypeConverter.GraphQLFieldInfo("String", true, false, "New name"));

        registry.registerInputType("UpdateUserInput", updateFields, "User update input");

        FraiseQL.mutation("updateUser")
            .returnType("User")
            .arg("input", "UpdateUserInput")
            .register();

        var mutation = registry.getMutation("updateUser");
        assertTrue(mutation.isPresent());
    }

    @Test
    @DisplayName("Pattern: Filter input for queries")
    void testFilterInputPattern() {
        Map<String, TypeConverter.GraphQLFieldInfo> filterFields = new HashMap<>();
        filterFields.put("query", new TypeConverter.GraphQLFieldInfo("String", true, false, "Search query"));
        filterFields.put("status", new TypeConverter.GraphQLFieldInfo("String", true, false, "Status filter"));
        filterFields.put("limit", new TypeConverter.GraphQLFieldInfo("Int", true, false, "Result limit"));
        filterFields.put("offset", new TypeConverter.GraphQLFieldInfo("Int", true, false, "Result offset"));

        registry.registerInputType("UserFilter", filterFields, "User search filter");

        FraiseQL.query("searchUsers")
            .returnType("User")
            .returnsArray(true)
            .arg("filter", "UserFilter")
            .register();

        var query = registry.getQuery("searchUsers");
        assertTrue(query.isPresent());
        assertTrue(query.get().arguments.containsKey("filter"));
    }

    // =========================================================================
    // NESTED INPUT TYPES
    // =========================================================================

    @Test
    @DisplayName("Input can reference other input types")
    void testInputCanReferenceOtherInputTypes() {
        // Address input
        Map<String, TypeConverter.GraphQLFieldInfo> addressFields = new HashMap<>();
        addressFields.put("street", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));
        addressFields.put("city", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));

        // User input with nested address
        Map<String, TypeConverter.GraphQLFieldInfo> userFields = new HashMap<>();
        userFields.put("name", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));
        userFields.put("address", new TypeConverter.GraphQLFieldInfo("AddressInput", false, false, ""));

        registry.registerInputType("AddressInput", addressFields, null);
        registry.registerInputType("CreateUserInput", userFields, null);

        assertTrue(registry.getInputType("AddressInput").isPresent());
        assertTrue(registry.getInputType("CreateUserInput").isPresent());
    }

    // =========================================================================
    // INPUT TYPE WITH MUTATIONS
    // =========================================================================

    @Test
    @DisplayName("Multiple mutations using same input type")
    void testMultipleMutationsUsingSameInputType() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("id", new TypeConverter.GraphQLFieldInfo("Int", false, false, ""));
        fields.put("status", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));

        registry.registerInputType("StatusInput", fields, null);

        FraiseQL.mutation("updateUserStatus")
            .returnType("User")
            .arg("input", "StatusInput")
            .register();

        FraiseQL.mutation("updatePostStatus")
            .returnType("Post")
            .arg("input", "StatusInput")
            .register();

        assertEquals(2, registry.getAllMutations().size());
        assertTrue(registry.getMutation("updateUserStatus").isPresent());
        assertTrue(registry.getMutation("updatePostStatus").isPresent());
    }

    // =========================================================================
    // CLEAR INPUT TYPES TEST
    // =========================================================================

    @Test
    @DisplayName("Clear removes registered input types")
    void testClearRemovesInputTypes() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = new HashMap<>();
        fields.put("test", new TypeConverter.GraphQLFieldInfo("String", false, false, ""));

        registry.registerInputType("TestInput", fields, null);

        assertTrue(registry.getInputType("TestInput").isPresent());

        registry.clear();

        assertFalse(registry.getInputType("TestInput").isPresent());
        assertEquals(0, registry.getAllInputTypes().size());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLInput
    public static class CreateUserInput {
        @GraphQLField
        public String email;

        @GraphQLField
        public String name;

        @GraphQLField(nullable = true)
        public String phone;
    }

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
    public static class Post {
        @GraphQLField
        public int id;

        @GraphQLField
        public String title;
    }
}
