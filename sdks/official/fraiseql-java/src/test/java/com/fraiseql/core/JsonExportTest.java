package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
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
 * JSON export and schema formatting
 */
public class JsonExportTest {
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
        assertEquals("2.0.0", schema.get("version").asText());
    }

    /**
     * Test schema version is included
     */
    @Test
    public void testSchemaVersion() {
        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        assertEquals("2.0.0", schema.get("version").asText());
    }

    /**
     * Test type formatting with fields
     */
    @Test
    public void testFormatTypes() {
        FraiseQL.registerType(User.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        ObjectNode schema = SchemaFormatter.formatSchema(registry);

        assertTrue(schema.get("types").isArray());

        JsonNode userType = SchemaNodes.byName(schema, "types", "User");
        assertNotNull(userType);
        assertEquals("User", userType.get("name").asText());
        // `javaClass` is no longer emitted: the compiler denies unknown fields, and the
        // authoring language's class name has no meaning downstream of the export.
        assertFalse(userType.has("javaClass"));

        assertTrue(userType.get("fields").isArray());
        assertNotNull(SchemaNodes.field(userType, "id"));
        assertNotNull(SchemaNodes.field(userType, "name"));

        JsonNode idField = SchemaNodes.field(userType, "id");
        // Bare GraphQL type name; nullability travels in the sibling `nullable` key.
        assertEquals("Int", idField.get("type").asText());
        assertFalse(idField.has("baseType"));
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

        assertTrue(schema.get("queries").isArray());

        JsonNode usersQuery = SchemaNodes.byName(schema, "queries", "users");
        assertNotNull(usersQuery);
        assertEquals("users", usersQuery.get("name").asText());
        // The compiler reads `return_type` plus a separate `returns_list`; the camelCase
        // `returnType` carrying "[User]" was rejected with `missing field return_type`.
        assertEquals("User", usersQuery.get("return_type").asText());
        assertTrue(usersQuery.get("returns_list").asBoolean());
        assertEquals("Get all users", usersQuery.get("description").asText());

        assertNotNull(SchemaNodes.argument(usersQuery, "limit"));
        assertNotNull(SchemaNodes.argument(usersQuery, "offset"));
        // Arguments are objects now, not a name -> typeString map: the compiler needs a
        // per-argument `nullable`, which a bare type string cannot carry.
        assertEquals("Int", SchemaNodes.argument(usersQuery, "limit").get("type").asText());
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

        assertTrue(schema.get("mutations").isArray());

        JsonNode createMutation = SchemaNodes.byName(schema, "mutations", "createUser");
        assertNotNull(createMutation);
        assertEquals("createUser", createMutation.get("name").asText());
        assertEquals("User", createMutation.get("return_type").asText());
        assertFalse(createMutation.get("returns_list").asBoolean());
        assertEquals("Create a new user", createMutation.get("description").asText());

        JsonNode args = createMutation.get("arguments");
        assertNotNull(SchemaNodes.argument(createMutation, "name"));
        assertNotNull(SchemaNodes.argument(createMutation, "email"));
        assertEquals("String", SchemaNodes.argument(createMutation, "name").get("type").asText());
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
        assertEquals("2.0.0", schema.get("version").asText());
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

        assertEquals("2.0.0", parsed.get("version").asText());
        assertTrue(parsed.has("types"));
        assertTrue(parsed.has("queries"));
        assertTrue(parsed.has("mutations"));

        assertTrue(SchemaNodes.has(parsed, "types", "User"));
        assertTrue(SchemaNodes.has(parsed, "queries", "getUser"));
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
        JsonNode userType = SchemaNodes.byName(schema, "types", "UserWithDescription");
        JsonNode nameField = SchemaNodes.field(userType, "name");

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
        JsonNode userType = SchemaNodes.byName(schema, "types", "User");

        // The type is the bare GraphQL name; nullability is the sibling `nullable` key,
        // not a `!` suffix, and `baseType` is no longer emitted — the compiler denies
        // unknown fields, and it duplicated information `type` already carries.
        JsonNode idField = SchemaNodes.field(userType, "id");
        assertEquals("Int", idField.get("type").asText());
        assertFalse(idField.get("nullable").asBoolean());
        // `baseType` and `isList` are no longer emitted: the compiler denies unknown
        // fields, and both restate information `type` already carries.
        assertFalse(idField.has("baseType"));
        assertFalse(idField.has("isList"));

        // Check name field
        JsonNode nameField = SchemaNodes.field(userType, "name");
        assertEquals("String", nameField.get("type").asText());
        assertFalse(nameField.get("nullable").asBoolean());
    }

    /**
     * Test empty schema export
     */
    @Test
    public void testEmptySchemaExport(@TempDir Path tempDir) throws IOException {
        String filePath = tempDir.resolve("empty_schema.json").toString();
        FraiseQL.exportSchema(filePath);

        ObjectNode schema = (ObjectNode) mapper.readTree(new File(filePath));
        assertEquals("2.0.0", schema.get("version").asText());
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
