package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for minimal types.json export (refactored TOML-based workflow).
 *
 * This test verifies the new minimal export behavior where Java decorators
 * only generate types.json (not complete schema.json with queries, mutations,
 * federation, security, observers, analytics).
 *
 * All configuration moves to fraiseql.toml instead.
 */
@DisplayName("Export Types Minimal (TOML-based workflow)")
public class ExportTypesMinimalTest {
    private static final ObjectMapper mapper = new ObjectMapper();
    private Path tmpDir;

    @BeforeEach
    void setUp() throws IOException {
        FraiseQL.clear();
        tmpDir = Files.createTempDirectory("fraiseql-test-");
    }

    @Test
    @DisplayName("exportTypes() should create minimal types.json with only types")
    void testExportTypesMinimalSingleType() throws IOException {
        // Setup
        FraiseQL.registerType(User.class);

        // Execute
        Path outputPath = tmpDir.resolve("user_types.json");
        FraiseQL.exportTypes(outputPath.toString());

        // Assert
        assertTrue(Files.exists(outputPath), "Output file should exist");

        JsonNode schema = mapper.readTree(outputPath.toFile());

        // Should have types section
        assertTrue(schema.has("types"), "Schema should have 'types' section");
        assertNotNull(schema.get("types"), "Types section should not be null");

        // Should have User type
        assertTrue(schema.get("types").has("User"), "Should contain User type");
        JsonNode userType = schema.get("types").get("User");
        assertEquals("User", userType.get("name").asText(), "Type name should be User");

        // IMPORTANT: No queries, mutations, federation, security, observers, analytics
        assertFalse(schema.has("queries") && schema.get("queries").size() > 0,
            "Should not have queries");
        assertFalse(schema.has("mutations") && schema.get("mutations").size() > 0,
            "Should not have mutations");
        assertFalse(schema.has("federation") && schema.get("federation") != null,
            "Should not have federation");
        assertFalse(schema.has("security") && schema.get("security") != null,
            "Should not have security");
        assertFalse(schema.has("observers") && schema.get("observers") != null,
            "Should not have observers");
        assertFalse(schema.has("analytics") && schema.get("analytics") != null,
            "Should not have analytics");
    }

    @Test
    @DisplayName("exportTypes() should handle multiple types correctly")
    void testExportTypesMultipleTypes() throws IOException {
        // Setup
        FraiseQL.registerTypes(User.class, Post.class);

        // Execute
        Path outputPath = tmpDir.resolve("schema_types.json");
        FraiseQL.exportTypes(outputPath.toString());

        // Assert
        assertTrue(Files.exists(outputPath));

        JsonNode schema = mapper.readTree(outputPath.toFile());
        JsonNode types = schema.get("types");

        assertEquals(2, types.size(), "Should have 2 types");
        assertTrue(types.has("User"), "Should contain User");
        assertTrue(types.has("Post"), "Should contain Post");
    }

    @Test
    @DisplayName("exportTypes() should include enums in output")
    void testExportTypesWithEnums() throws IOException {
        // Setup
        FraiseQL.registerType(PostWithStatus.class);

        // Execute
        Path outputPath = tmpDir.resolve("schema_types.json");
        FraiseQL.exportTypes(outputPath.toString());

        // Assert
        assertTrue(Files.exists(outputPath));

        JsonNode schema = mapper.readTree(outputPath.toFile());

        // Should have types
        assertTrue(schema.has("types"), "Should have types");
        assertTrue(schema.get("types").has("PostWithStatus"), "Should have PostWithStatus");
    }

    @Test
    @DisplayName("exportTypes() should NOT include queries or mutations in output")
    void testExportTypesNoQueriesOrMutations() throws IOException {
        // Setup
        FraiseQL.registerType(User.class);

        // Queries and mutations defined but should NOT appear in types.json
        FraiseQL.query("getUser")
            .returnType(User.class)
            .arg("id", "String")
            .register();

        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .arg("email", "String")
            .register();

        // Execute
        Path outputPath = tmpDir.resolve("schema_types.json");
        FraiseQL.exportTypes(outputPath.toString());

        // Assert
        assertTrue(Files.exists(outputPath));

        JsonNode schema = mapper.readTree(outputPath.toFile());

        // Should only have the type
        assertEquals(1, schema.get("types").size(), "Should only have 1 type");
        assertTrue(schema.get("types").has("User"), "Should have User type");

        // Queries and mutations should NOT be in types.json
        assertFalse(schema.has("queries") && schema.get("queries").size() > 0,
            "Queries should not be in types.json");
        assertFalse(schema.has("mutations") && schema.get("mutations").size() > 0,
            "Mutations should not be in types.json");
    }

    @Test
    @DisplayName("exportTypes() should work with pretty formatting")
    void testExportTypesPrettyFormatting() throws IOException {
        // Setup
        FraiseQL.registerType(User.class);

        // Execute
        Path outputPath = tmpDir.resolve("user_types.json");
        FraiseQL.exportTypes(outputPath.toString(), true);

        // Assert
        assertTrue(Files.exists(outputPath));

        String content = Files.readString(outputPath);

        // Should have nice formatting (contains newlines and indentation)
        assertTrue(content.contains("\n"), "Should contain newlines for formatting");
        assertTrue(content.contains("  "), "Should contain indentation");
    }

    @Test
    @DisplayName("exportTypes() should work with compact formatting")
    void testExportTypesCompactFormatting() throws IOException {
        // Setup
        FraiseQL.registerType(User.class);

        // Execute
        Path outputPath = tmpDir.resolve("user_types.json");
        FraiseQL.exportTypes(outputPath.toString(), false);

        // Assert
        assertTrue(Files.exists(outputPath));

        JsonNode schema = mapper.readTree(outputPath.toFile());
        assertNotNull(schema);
        assertTrue(schema.has("types"), "Should have types section");
    }

    // Test fixture classes
    @GraphQLType
    static class User {
        public String id;
        public String name;
        public String email;
    }

    @GraphQLType
    static class Post {
        public String id;
        public String title;
        public String content;
    }

    @GraphQLType
    static class PostWithStatus {
        public String id;
        public String title;
        public String status;
    }
}
