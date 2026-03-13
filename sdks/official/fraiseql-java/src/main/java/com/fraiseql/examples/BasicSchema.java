package com.fraiseql.examples;

import com.fraiseql.core.*;

/**
 * BasicSchema - Complete example of FraiseQL Java authoring layer
 *
 * This example demonstrates:
 * 1. Defining GraphQL types with @GraphQLType and @GraphQLField
 * 2. Registering types with the schema
 * 3. Creating queries with the fluent API
 * 4. Creating mutations with the fluent API
 * 5. Exporting the schema to schema.json
 *
 * Output: schema.json - Ready for fraiseql-cli compile
 *
 * Usage:
 *   mvn exec:java -Dexec.mainClass="com.fraiseql.examples.BasicSchema"
 */
public class BasicSchema {

    public static void main(String[] args) {
        try {
            System.out.println("FraiseQL Java - BasicSchema Example");
            System.out.println("====================================\n");

            // Register types
            System.out.println("1. Registering GraphQL types...");
            FraiseQL.registerTypes(User.class, Post.class, Comment.class);
            System.out.println("   ✓ User, Post, Comment registered\n");

            // Register queries
            System.out.println("2. Registering GraphQL queries...");
            FraiseQL.query("users")
                .returnType(User.class)
                .returnsArray(true)
                .arg("limit", "Int")
                .arg("offset", "Int")
                .description("Get all users with pagination")
                .register();
            System.out.println("   ✓ users query registered");

            FraiseQL.query("user")
                .returnType(User.class)
                .arg("id", "Int")
                .description("Get a user by ID")
                .register();
            System.out.println("   ✓ user query registered");

            FraiseQL.query("posts")
                .returnType(Post.class)
                .returnsArray(true)
                .arg("userId", "Int")
                .description("Get posts for a specific user")
                .register();
            System.out.println("   ✓ posts query registered");

            FraiseQL.query("post")
                .returnType(Post.class)
                .arg("id", "Int")
                .description("Get a specific post")
                .register();
            System.out.println("   ✓ post query registered");

            FraiseQL.query("comments")
                .returnType(Comment.class)
                .returnsArray(true)
                .arg("postId", "Int")
                .description("Get comments for a post")
                .register();
            System.out.println("   ✓ comments query registered\n");

            // Register mutations
            System.out.println("3. Registering GraphQL mutations...");
            FraiseQL.mutation("createUser")
                .returnType(User.class)
                .arg("name", "String")
                .arg("email", "String")
                .description("Create a new user")
                .register();
            System.out.println("   ✓ createUser mutation registered");

            FraiseQL.mutation("updateUser")
                .returnType(User.class)
                .arg("id", "Int")
                .arg("name", "String")
                .arg("email", "String")
                .description("Update an existing user")
                .register();
            System.out.println("   ✓ updateUser mutation registered");

            FraiseQL.mutation("deleteUser")
                .returnType(User.class)
                .arg("id", "Int")
                .description("Delete a user")
                .register();
            System.out.println("   ✓ deleteUser mutation registered");

            FraiseQL.mutation("createPost")
                .returnType(Post.class)
                .arg("userId", "Int")
                .arg("title", "String")
                .arg("content", "String")
                .description("Create a new post")
                .register();
            System.out.println("   ✓ createPost mutation registered");

            FraiseQL.mutation("createComment")
                .returnType(Comment.class)
                .arg("postId", "Int")
                .arg("userId", "Int")
                .arg("text", "String")
                .description("Create a comment on a post")
                .register();
            System.out.println("   ✓ createComment mutation registered\n");

            // Export schema
            System.out.println("4. Exporting schema to schema.json...");
            String outputPath = "schema.json";
            FraiseQL.exportSchema(outputPath);
            System.out.println("   ✓ Schema exported to " + outputPath + "\n");

            // Print summary
            SchemaRegistry registry = FraiseQL.getRegistry();
            System.out.println("Schema Summary:");
            System.out.println("--------------");
            System.out.println("Types:     " + registry.getAllTypes().size());
            System.out.println("Queries:   " + registry.getAllQueries().size());
            System.out.println("Mutations: " + registry.getAllMutations().size());
            System.out.println("\nNext steps:");
            System.out.println("1. Run: fraiseql-cli compile schema.json -o schema.compiled.json");
            System.out.println("2. Run: fraiseql-server --schema schema.compiled.json --port 8000");
            System.out.println("3. Test: curl -X POST http://localhost:8000/graphql ...");

        } catch (Exception e) {
            System.err.println("Error: " + e.getMessage());
            e.printStackTrace();
            System.exit(1);
        }
    }

    /**
     * User type with id, name, email, and createdAt
     */
    @GraphQLType(description = "A user account")
    public static class User {
        @GraphQLField(description = "User ID")
        public int id;

        @GraphQLField(description = "User's name")
        public String name;

        @GraphQLField(description = "User's email address")
        public String email;

        @GraphQLField(name = "created_at", description = "When the user was created")
        public String createdAt;
    }

    /**
     * Post type with id, userId, title, content, and createdAt
     */
    @GraphQLType(description = "A blog post")
    public static class Post {
        @GraphQLField(description = "Post ID")
        public int id;

        @GraphQLField(description = "ID of the user who created this post")
        public int userId;

        @GraphQLField(description = "Post title")
        public String title;

        @GraphQLField(description = "Post content")
        public String content;

        @GraphQLField(name = "created_at", description = "When the post was created")
        public String createdAt;
    }

    /**
     * Comment type with id, postId, userId, text, and createdAt
     */
    @GraphQLType(description = "A comment on a post")
    public static class Comment {
        @GraphQLField(description = "Comment ID")
        public int id;

        @GraphQLField(description = "ID of the post this comment is on")
        public int postId;

        @GraphQLField(description = "ID of the user who created this comment")
        public int userId;

        @GraphQLField(description = "Comment text")
        public String text;

        @GraphQLField(name = "created_at", description = "When the comment was created")
        public String createdAt;
    }
}
