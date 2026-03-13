package com.fraiseql.core;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Phase 3 tests: JSON export and schema formatting
 */
public class Phase3Test {
    private static final ObjectMapper mapper = new ObjectMapper();

    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
    }

    /**
     * Test basic schema formatting to ObjectNode
     */
    @Test
    public void testFormatSchemaBasic() {
        FraiseQL.registerType(User.class);
        FraiseQL.query("users")
            .returnType(User.class)
            .returnsArray(true)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        assertNotNull(schema);
        assertTrue(schema.has("version"));
        assertTrue(schema.has("types"));
        assertTrue(schema.has("queries"));
        assertTrue(schema.has("mutations"));
        assertEquals("1.0", schema.get("version").asText());
    }

    /**
     * Test schema version is included
     */
    @Test
    public void testSchemaVersion() {
        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        assertEquals("1.0", schema.get("version").asText());
    }

    /**
     * Test type formatting with fields
     */
    @Test
    public void testFormatTypes() {
        FraiseQL.registerType(User.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        ObjectNode types = (ObjectNode) schema.get("types");
        assertNotNull(types);
        assertTrue(types.has("User"));

        ObjectNode userType = (ObjectNode) types.get("User");
        assertEquals("User", userType.get("name").asText());
        assertEquals("com.fraiseql.core.Phase3Test$User", userType.get("javaClass").asText());

        ObjectNode fields = (ObjectNode) userType.get("fields");
        assertTrue(fields.has("id"));
        assertTrue(fields.has("name"));

        ObjectNode idField = (ObjectNode) fields.get("id");
        assertEquals("Int!", idField.get("type").asText());
        assertEquals("Int", idField.get("baseType").asText());
        assertFalse(idField.get("nullable").asBoolean());
    }

    /**
     * Test query formatting
     */
    @Test
    public void testFormatQueries() {
        FraiseQL.registerType(User.class);
        FraiseQL.query("users")
            .returnType(User.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .arg("offset", "Int")
            .description("Get all users")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        ObjectNode queries = (ObjectNode) schema.get("queries");
        assertNotNull(queries);
        assertTrue(queries.has("users"));

        ObjectNode usersQuery = (ObjectNode) queries.get("users");
        assertEquals("users", usersQuery.get("name").asText());
        assertEquals("[User]", usersQuery.get("returnType").asText());
        assertEquals("Get all users", usersQuery.get("description").asText());

        ObjectNode args = (ObjectNode) usersQuery.get("arguments");
        assertTrue(args.has("limit"));
        assertTrue(args.has("offset"));
        assertEquals("Int", args.get("limit").asText());
    }

    /**
     * Test mutation formatting
     */
    @Test
    public void testFormatMutations() {
        FraiseQL.registerType(User.class);
        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .arg("email", "String")
            .description("Create a new user")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        ObjectNode mutations = (ObjectNode) schema.get("mutations");
        assertNotNull(mutations);
        assertTrue(mutations.has("createUser"));

        ObjectNode createMutation = (ObjectNode) mutations.get("createUser");
        assertEquals("createUser", createMutation.get("name").asText());
        assertEquals("User", createMutation.get("returnType").asText());
        assertEquals("Create a new user", createMutation.get("description").asText());

        ObjectNode args = (ObjectNode) createMutation.get("arguments");
        assertTrue(args.has("name"));
        assertTrue(args.has("email"));
        assertEquals("String", args.get("name").asText());
    }

    /**
     * Test complete schema formatting
     */
    @Test
    public void testFormatCompleteSchema() {
        // Register types
        FraiseQL.registerType(User.class);
        FraiseQL.registerType(Post.class);

        // Register queries
        FraiseQL.query("users")
            .returnType(User.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .register();

        FraiseQL.query("posts")
            .returnType(Post.class)
            .returnsArray(true)
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
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        // Verify structure
        assertEquals("1.0", schema.get("version").asText());
        assertEquals(2, schema.get("types").size());
        assertEquals(2, schema.get("queries").size());
        assertEquals(2, schema.get("mutations").size());
    }

    /**
     * Test schema export to JSON string
     */
    @Test
    public void testExportToJsonString() throws IOException {
        FraiseQL.registerType(User.class);
        FraiseQL.query("users")
            .returnType(User.class)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);
        String jsonString = SchemaFormatter.toJsonString(schema);

        assertNotNull(jsonString);
        assertTrue(jsonString.contains("\"version\""));
        assertTrue(jsonString.contains("\"types\""));
        assertTrue(jsonString.contains("\"User\""));
        assertTrue(jsonString.contains("\"queries\""));
    }

    /**
     * Test schema export to file
     */
    @Test
    public void testExportToFile(@TempDir Path tempDir) throws IOException {
        FraiseQL.registerType(User.class);
        FraiseQL.query("users")
            .returnType(User.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .description("Get all users")
            .register();

        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .register();

        String filePath = tempDir.resolve("schema.json").toString();
        FraiseQL.exportSchema(filePath);

        File file = new File(filePath);
        assertTrue(file.exists());
        assertTrue(file.length() > 0);

        // Verify content
        String content = Files.readString(file.toPath());
        assertTrue(content.contains("\"version\""));
        assertTrue(content.contains("\"User\""));
        assertTrue(content.contains("\"users\""));
        assertTrue(content.contains("\"createUser\""));
    }

    /**
     * Test exported JSON can be parsed back
     */
    @Test
    public void testExportRoundTrip(@TempDir Path tempDir) throws IOException {
        FraiseQL.registerType(User.class);
        FraiseQL.query("getUser")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        String filePath = tempDir.resolve("schema.json").toString();
        FraiseQL.exportSchema(filePath);

        // Parse the exported file
        ObjectNode parsed = (ObjectNode) mapper.readTree(new File(filePath));

        assertEquals("1.0", parsed.get("version").asText());
        assertTrue(parsed.has("types"));
        assertTrue(parsed.has("queries"));
        assertTrue(parsed.has("mutations"));

        ObjectNode types = (ObjectNode) parsed.get("types");
        assertTrue(types.has("User"));

        ObjectNode queries = (ObjectNode) parsed.get("queries");
        assertTrue(queries.has("getUser"));
    }

    /**
     * Test multiple types and operations export
     */
    @Test
    public void testComplexSchemaExport(@TempDir Path tempDir) throws IOException {
        // Register multiple types
        FraiseQL.registerTypes(User.class, Post.class, Comment.class);

        // Register multiple queries
        FraiseQL.query("users").returnType(User.class).returnsArray(true).arg("limit", "Int").register();
        FraiseQL.query("posts").returnType(Post.class).returnsArray(true).register();
        FraiseQL.query("comments").returnType(Comment.class).returnsArray(true).register();
        FraiseQL.query("user").returnType(User.class).arg("id", "Int").register();

        // Register multiple mutations
        FraiseQL.mutation("createUser").returnType(User.class).arg("name", "String").register();
        FraiseQL.mutation("createPost").returnType(Post.class).arg("userId", "Int").arg("title", "String").register();
        FraiseQL.mutation("createComment").returnType(Comment.class).arg("postId", "Int").arg("text", "String").register();

        String filePath = tempDir.resolve("schema.json").toString();
        FraiseQL.exportSchema(filePath);

        ObjectNode schema = (ObjectNode) mapper.readTree(new File(filePath));

        assertEquals(3, schema.get("types").size());
        assertEquals(4, schema.get("queries").size());
        assertEquals(3, schema.get("mutations").size());
    }

    /**
     * Test field description inclusion in export
     */
    @Test
    public void testFieldDescriptionExport(@TempDir Path tempDir) throws IOException {
        FraiseQL.registerType(UserWithDescription.class);
        FraiseQL.query("user").returnType(UserWithDescription.class).arg("id", "Int").register();

        String filePath = tempDir.resolve("schema.json").toString();
        FraiseQL.exportSchema(filePath);

        ObjectNode schema = (ObjectNode) mapper.readTree(new File(filePath));
        ObjectNode types = (ObjectNode) schema.get("types");
        ObjectNode userType = (ObjectNode) types.get("UserWithDescription");
        ObjectNode fields = (ObjectNode) userType.get("fields");
        ObjectNode nameField = (ObjectNode) fields.get("name");

        assertTrue(nameField.has("description"));
        assertEquals("The user's name", nameField.get("description").asText());
    }

    /**
     * Test field type information preservation
     */
    @Test
    public void testFieldTypeInfoExport(@TempDir Path tempDir) throws IOException {
        FraiseQL.registerType(User.class);

        String filePath = tempDir.resolve("schema.json").toString();
        FraiseQL.exportSchema(filePath);

        ObjectNode schema = (ObjectNode) mapper.readTree(new File(filePath));
        ObjectNode types = (ObjectNode) schema.get("types");
        ObjectNode userType = (ObjectNode) types.get("User");
        ObjectNode fields = (ObjectNode) userType.get("fields");

        // Check id field
        ObjectNode idField = (ObjectNode) fields.get("id");
        assertEquals("Int!", idField.get("type").asText());
        assertEquals("Int", idField.get("baseType").asText());
        assertFalse(idField.get("nullable").asBoolean());
        assertFalse(idField.get("isList").asBoolean());

        // Check name field
        ObjectNode nameField = (ObjectNode) fields.get("name");
        assertEquals("String!", nameField.get("type").asText());
        assertEquals("String", nameField.get("baseType").asText());
    }

    /**
     * Test empty schema export
     */
    @Test
    public void testEmptySchemaExport(@TempDir Path tempDir) throws IOException {
        String filePath = tempDir.resolve("empty_schema.json").toString();
        FraiseQL.exportSchema(filePath);

        ObjectNode schema = (ObjectNode) mapper.readTree(new File(filePath));
        assertEquals("1.0", schema.get("version").asText());
        assertEquals(0, schema.get("types").size());
        assertEquals(0, schema.get("queries").size());
        assertEquals(0, schema.get("mutations").size());
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

    @GraphQLType(description = "A comment on a post")
    public static class Comment {
        @GraphQLField
        public int id;

        @GraphQLField
        public int postId;

        @GraphQLField
        public String text;
    }

    @GraphQLType
    public static class UserWithDescription {
        @GraphQLField
        public int id;

        @GraphQLField(description = "The user's name")
        public String name;
    }
}
