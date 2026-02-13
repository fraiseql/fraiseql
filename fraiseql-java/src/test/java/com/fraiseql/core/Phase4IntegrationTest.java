package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.File;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Phase 4 integration tests: Complete workflows and real-world scenarios
 */
public class Phase4IntegrationTest {

    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
    }

    /**
     * Test complete blog schema workflow
     */
    @Test
    public void testBlogSchemaWorkflow(@TempDir Path tempDir) throws Exception {
        // Define types
        FraiseQL.registerTypes(User.class, Post.class);

        // Define queries
        FraiseQL.query("users")
            .returnType(User.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .description("Get all users")
            .register();

        FraiseQL.query("posts")
            .returnType(Post.class)
            .returnsArray(true)
            .description("Get all posts")
            .register();

        // Define mutations
        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .description("Create user")
            .register();

        FraiseQL.mutation("createPost")
            .returnType(Post.class)
            .arg("userId", "Int")
            .arg("title", "String")
            .description("Create post")
            .register();

        // Export and verify
        String filePath = tempDir.resolve("blog.json").toString();
        FraiseQL.exportSchema(filePath);

        String content = Files.readString(Path.of(filePath));
        assertTrue(content.contains("\"User\""));
        assertTrue(content.contains("\"Post\""));
        assertTrue(content.contains("\"users\""));
        assertTrue(content.contains("\"createUser\""));
    }

    /**
     * Test API server schema with authentication
     */
    @Test
    public void testApiServerSchema(@TempDir Path tempDir) throws Exception {
        // User, Auth, and Admin types
        FraiseQL.registerTypes(ApiUser.class, AuthToken.class, ApiResponse.class);

        // Queries
        FraiseQL.query("me")
            .returnType(ApiUser.class)
            .description("Get current authenticated user")
            .register();

        FraiseQL.query("users")
            .returnType(ApiUser.class)
            .returnsArray(true)
            .arg("role", "String")
            .description("Get users by role")
            .register();

        FraiseQL.query("health")
            .returnType(ApiResponse.class)
            .description("Health check")
            .register();

        // Mutations
        FraiseQL.mutation("login")
            .returnType(AuthToken.class)
            .arg("email", "String")
            .arg("password", "String")
            .description("Authenticate user")
            .register();

        FraiseQL.mutation("logout")
            .returnType(ApiResponse.class)
            .description("Log out current user")
            .register();

        FraiseQL.mutation("updateProfile")
            .returnType(ApiUser.class)
            .arg("name", "String")
            .arg("email", "String")
            .description("Update user profile")
            .register();

        // Export
        String filePath = tempDir.resolve("api.json").toString();
        FraiseQL.exportSchema(filePath);

        File file = new File(filePath);
        assertTrue(file.exists());

        String content = Files.readString(Path.of(filePath));
        assertEquals(1, countOccurrences(content, "\"ApiUser\""));
        assertEquals(1, countOccurrences(content, "\"login\""));
    }

    /**
     * Test social media schema
     */
    @Test
    public void testSocialMediaSchema(@TempDir Path tempDir) throws Exception {
        // Types
        FraiseQL.registerTypes(
            SocialUser.class,
            Post.class,
            Comment.class,
            Like.class,
            Notification.class
        );

        // Queries
        FraiseQL.query("user")
            .returnType(SocialUser.class)
            .arg("id", "Int")
            .register();

        FraiseQL.query("feed")
            .returnType(Post.class)
            .returnsArray(true)
            .arg("userId", "Int")
            .arg("limit", "Int")
            .register();

        FraiseQL.query("notifications")
            .returnType(Notification.class)
            .returnsArray(true)
            .arg("unreadOnly", "Boolean")
            .register();

        // Mutations
        FraiseQL.mutation("createPost")
            .returnType(Post.class)
            .arg("content", "String")
            .register();

        FraiseQL.mutation("likePost")
            .returnType(Like.class)
            .arg("postId", "Int")
            .register();

        FraiseQL.mutation("comment")
            .returnType(Comment.class)
            .arg("postId", "Int")
            .arg("text", "String")
            .register();

        FraiseQL.mutation("follow")
            .returnType(SocialUser.class)
            .arg("userId", "Int")
            .register();

        // Verify
        String filePath = tempDir.resolve("social.json").toString();
        FraiseQL.exportSchema(filePath);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        assertEquals(5, registry.getAllTypes().size());
        assertEquals(3, registry.getAllQueries().size());
        assertEquals(4, registry.getAllMutations().size());
    }

    /**
     * Test analytics schema with aggregation
     */
    @Test
    public void testAnalyticsSchema(@TempDir Path tempDir) throws Exception {
        // Types
        FraiseQL.registerTypes(AnalyticsEvent.class, UserMetrics.class, ReportData.class);

        // Queries with metrics
        FraiseQL.query("events")
            .returnType(AnalyticsEvent.class)
            .returnsArray(true)
            .arg("eventType", "String")
            .arg("startDate", "String")
            .arg("endDate", "String")
            .description("Get analytics events")
            .register();

        FraiseQL.query("userMetrics")
            .returnType(UserMetrics.class)
            .arg("userId", "Int")
            .description("Get user metrics")
            .register();

        FraiseQL.query("report")
            .returnType(ReportData.class)
            .arg("reportType", "String")
            .arg("dateRange", "String")
            .description("Generate report")
            .register();

        // Mutations for tracking
        FraiseQL.mutation("trackEvent")
            .returnType(AnalyticsEvent.class)
            .arg("eventType", "String")
            .arg("userId", "Int")
            .arg("properties", "String")
            .description("Track user event")
            .register();

        FraiseQL.mutation("updateMetrics")
            .returnType(UserMetrics.class)
            .arg("userId", "Int")
            .description("Recalculate metrics")
            .register();

        // Export
        String filePath = tempDir.resolve("analytics.json").toString();
        FraiseQL.exportSchema(filePath);

        assertTrue(Files.exists(Path.of(filePath)));
        String content = Files.readString(Path.of(filePath));
        assertTrue(content.contains("\"trackEvent\""));
    }

    /**
     * Test schema validation: all types registered, all operations valid
     */
    @Test
    public void testSchemaValidation(@TempDir Path tempDir) throws Exception {
        // Register types
        FraiseQL.registerTypes(Entity1.class, Entity2.class, Entity3.class);

        // Create valid operations
        for (int i = 0; i < 10; i++) {
            FraiseQL.query("query" + i)
                .returnType(Entity1.class)
                .arg("arg" + i, "Int")
                .register();
        }

        for (int i = 0; i < 5; i++) {
            FraiseQL.mutation("mutation" + i)
                .returnType(Entity2.class)
                .arg("param" + i, "String")
                .register();
        }

        // Export and verify complete schema
        String filePath = tempDir.resolve("validation.json").toString();
        FraiseQL.exportSchema(filePath);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        assertEquals(3, registry.getAllTypes().size());
        assertEquals(10, registry.getAllQueries().size());
        assertEquals(5, registry.getAllMutations().size());

        // Verify all queries are in exported file
        String content = Files.readString(Path.of(filePath));
        for (int i = 0; i < 10; i++) {
            assertTrue(content.contains("\"query" + i + "\""));
        }
    }

    /**
     * Test large schema with many types and operations
     */
    @Test
    public void testLargeSchema(@TempDir Path tempDir) throws Exception {
        // Register many types
        Class<?>[] types = new Class[20];
        for (int i = 0; i < 20; i++) {
            types[i] = Entity1.class;  // Reuse for simplicity
        }
        FraiseQL.registerType(Entity1.class);
        FraiseQL.registerType(Entity2.class);
        FraiseQL.registerType(Entity3.class);

        // Create many operations
        for (int i = 0; i < 50; i++) {
            FraiseQL.query("q" + i)
                .returnType(Entity1.class)
                .arg("a", "Int")
                .register();
        }

        for (int i = 0; i < 30; i++) {
            FraiseQL.mutation("m" + i)
                .returnType(Entity2.class)
                .arg("b", "String")
                .register();
        }

        // Export large schema
        String filePath = tempDir.resolve("large.json").toString();
        FraiseQL.exportSchema(filePath);

        File file = new File(filePath);
        assertTrue(file.exists());
        assertTrue(file.length() > 10000);  // Large file

        String content = Files.readString(Path.of(filePath));
        assertEquals(1, countOccurrences(content, "\"version\""));
    }

    /**
     * Test schema with all GraphQL types represented
     */
    @Test
    public void testAllTypesSchema(@TempDir Path tempDir) throws Exception {
        // Type with all supported Java types
        FraiseQL.registerType(CompleteType.class);

        // Queries using all types
        FraiseQL.query("getByInt")
            .returnType(CompleteType.class)
            .arg("id", "Int")
            .register();

        FraiseQL.query("getByString")
            .returnType(CompleteType.class)
            .arg("name", "String")
            .register();

        FraiseQL.query("getByFloat")
            .returnType(CompleteType.class)
            .arg("price", "Float")
            .register();

        FraiseQL.query("getByBoolean")
            .returnType(CompleteType.class)
            .arg("active", "Boolean")
            .register();

        // Export
        String filePath = tempDir.resolve("complete.json").toString();
        FraiseQL.exportSchema(filePath);

        String content = Files.readString(Path.of(filePath));
        assertTrue(content.contains("\"Int\""));
        assertTrue(content.contains("\"String\""));
        assertTrue(content.contains("\"Float\""));
        assertTrue(content.contains("\"Boolean\""));
    }

    // Helper method
    private int countOccurrences(String text, String pattern) {
        int count = 0;
        int index = 0;
        while ((index = text.indexOf(pattern, index)) != -1) {
            count++;
            index += pattern.length();
        }
        return count;
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
        public int userId;
        @GraphQLField
        public String title;
    }

    @GraphQLType
    public static class Comment {
        @GraphQLField
        public int id;
        @GraphQLField
        public int postId;
        @GraphQLField
        public String text;
    }

    @GraphQLType
    public static class Like {
        @GraphQLField
        public int id;
        @GraphQLField
        public int postId;
    }

    @GraphQLType
    public static class ApiUser {
        @GraphQLField
        public int id;
        @GraphQLField
        public String email;
        @GraphQLField
        public String role;
    }

    @GraphQLType
    public static class AuthToken {
        @GraphQLField
        public String token;
        @GraphQLField
        public int expiresIn;
    }

    @GraphQLType
    public static class ApiResponse {
        @GraphQLField
        public String message;
        @GraphQLField
        public int code;
    }

    @GraphQLType
    public static class SocialUser {
        @GraphQLField
        public int id;
        @GraphQLField
        public String username;
        @GraphQLField
        public int followers;
    }

    @GraphQLType
    public static class Notification {
        @GraphQLField
        public int id;
        @GraphQLField
        public String type;
        @GraphQLField
        public boolean read;
    }

    @GraphQLType
    public static class AnalyticsEvent {
        @GraphQLField
        public int id;
        @GraphQLField
        public String eventType;
        @GraphQLField
        public int userId;
    }

    @GraphQLType
    public static class UserMetrics {
        @GraphQLField
        public int userId;
        @GraphQLField
        public int events;
        @GraphQLField
        public long sessionTime;
    }

    @GraphQLType
    public static class ReportData {
        @GraphQLField
        public String reportType;
        @GraphQLField
        public String period;
        @GraphQLField
        public String data;
    }

    @GraphQLType
    public static class Entity1 {
        @GraphQLField
        public int id;
    }

    @GraphQLType
    public static class Entity2 {
        @GraphQLField
        public int id;
    }

    @GraphQLType
    public static class Entity3 {
        @GraphQLField
        public int id;
    }

    @GraphQLType
    public static class CompleteType {
        @GraphQLField
        public int intField;
        @GraphQLField
        public String stringField;
        @GraphQLField
        public float floatField;
        @GraphQLField
        public boolean booleanField;
    }
}
