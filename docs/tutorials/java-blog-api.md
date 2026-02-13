<!-- Skip to main content -->
---

title: Building a Blog API with FraiseQL: Java Schema Authoring Tutorial
description: In this tutorial, you'll build a complete GraphQL Blog API by authoring a schema in Java. You'll learn how to:
keywords: ["project", "hands-on", "schema", "learning", "example", "step-by-step"]
tags: ["documentation", "reference"]
---

# Building a Blog API with FraiseQL: Java Schema Authoring Tutorial

**Duration**: 30 minutes
**Outcome**: A fully functional GraphQL Blog API schema generated from Java
**Prerequisites**: Java 17+, Maven 3.8+ or Gradle 7.0+, PostgreSQL
**Focus**: Java annotations, builder pattern, and schema compilation

---

## Overview

In this tutorial, you'll build a complete GraphQL Blog API by authoring a schema in Java. You'll learn how to:

- Use `@GraphQLType` and `@GraphQLField` annotations to define types
- Use the builder pattern with `FraiseQL.query()` and `FraiseQL.mutation()` to define operations
- Export your schema to JSON using FraiseQL Java library
- Compile the schema with `FraiseQL-cli`
- Write integration tests to verify your schema
- Deploy to production with Spring Boot

The result is a type-safe, compiled GraphQL API with zero runtime schema validation overhead.

---

## Database Schema

First, let's set up the PostgreSQL database that our Blog API will query.

Create a file `schema.sql`:

```sql
<!-- Code example in SQL -->
-- Users table
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    bio TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Posts table
CREATE TABLE posts (
    id BIGSERIAL PRIMARY KEY,
    author_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,
    content TEXT NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'DRAFT',
    published_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Comments table
CREATE TABLE comments (
    id BIGSERIAL PRIMARY KEY,
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Tags table
CREATE TABLE tags (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(50) NOT NULL UNIQUE,
    slug VARCHAR(50) NOT NULL UNIQUE
);

-- Posts-Tags junction table (many-to-many)
CREATE TABLE post_tags (
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    tag_id BIGINT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);

-- Indexes for query performance
CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_posts_slug ON posts(slug);
CREATE INDEX idx_posts_status ON posts(status);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);
```text
<!-- Code example in TEXT -->

To apply this schema:

```bash
<!-- Code example in BASH -->
psql -U postgres -d blog_db -f schema.sql
```text
<!-- Code example in TEXT -->

---

## Project Setup

### Maven Configuration

Create a new Maven project:

```bash
<!-- Code example in BASH -->
mvn archetype:generate \
    -DgroupId=com.example \
    -DartifactId=blog-api \
    -DarchetypeArtifactId=maven-archetype-quickstart \
    -DinteractiveMode=false
cd blog-api
```text
<!-- Code example in TEXT -->

Replace your `pom.xml` with:

```xml
<!-- Code example in XML -->
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0
                             http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>com.example</groupId>
    <artifactId>blog-api</artifactId>
    <version>1.0.0</version>
    <packaging>jar</packaging>

    <name>Blog API</name>
    <description>GraphQL Blog API built with FraiseQL</description>

    <properties>
        <maven.compiler.source>17</maven.compiler.source>
        <maven.compiler.target>17</maven.compiler.target>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
        <FraiseQL.version>2.0.0</FraiseQL.version>
        <jackson.version>2.16.1</jackson.version>
        <junit.version>5.10.1</junit.version>
        <spring.boot.version>3.2.0</spring.boot.version>
    </properties>

    <dependencyManagement>
        <dependencies>
            <dependency>
                <groupId>org.springframework.boot</groupId>
                <artifactId>spring-boot-dependencies</artifactId>
                <version>${spring.boot.version}</version>
                <type>pom</type>
                <scope>import</scope>
            </dependency>
        </dependencies>
    </dependencyManagement>

    <dependencies>
        <!-- FraiseQL Java Authoring -->
        <dependency>
            <groupId>com.FraiseQL</groupId>
            <artifactId>FraiseQL-java</artifactId>
            <version>${FraiseQL.version}</version>
        </dependency>

        <!-- JSON Processing -->
        <dependency>
            <groupId>com.fasterxml.jackson.core</groupId>
            <artifactId>jackson-databind</artifactId>
            <version>${jackson.version}</version>
        </dependency>

        <!-- Spring Boot (optional, for deployment) -->
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-web</artifactId>
            <optional>true</optional>
        </dependency>

        <!-- Testing -->
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter-api</artifactId>
            <version>${junit.version}</version>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter-engine</artifactId>
            <version>${junit.version}</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <plugins>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-compiler-plugin</artifactId>
                <version>3.11.0</version>
                <configuration>
                    <source>17</source>
                    <target>17</target>
                </configuration>
            </plugin>

            <!-- Exec plugin for running schema export -->
            <plugin>
                <groupId>org.codehaus.mojo</groupId>
                <artifactId>exec-maven-plugin</artifactId>
                <version>3.1.0</version>
                <configuration>
                    <mainClass>com.example.schema.SchemaExporter</mainClass>
                </configuration>
            </plugin>

            <!-- Spring Boot Maven plugin (optional) -->
            <plugin>
                <groupId>org.springframework.boot</groupId>
                <artifactId>spring-boot-maven-plugin</artifactId>
                <version>${spring.boot.version}</version>
            </plugin>
        </plugins>
    </build>
</project>
```text
<!-- Code example in TEXT -->

### Gradle Configuration

If you prefer Gradle, create a `build.gradle`:

```gradle
<!-- Code example in GRADLE -->
plugins {
    id 'java'
    id 'org.springframework.boot' version '3.2.0' apply false
}

java {
    sourceCompatibility = '17'
    targetCompatibility = '17'
}

repositories {
    mavenCentral()
}

dependencies {
    // FraiseQL Java Authoring
    implementation 'com.FraiseQL:FraiseQL-java:2.0.0'

    // JSON Processing
    implementation 'com.fasterxml.jackson.core:jackson-databind:2.16.1'

    // Testing
    testImplementation 'org.junit.jupiter:junit-jupiter-api:5.10.1'
    testRuntimeOnly 'org.junit.jupiter:junit-jupiter-engine:5.10.1'
}

tasks.register('exportSchema', JavaExec) {
    classpath = sourceSets.main.runtimeClasspath
    mainClass = 'com.example.schema.SchemaExporter'
}
```text
<!-- Code example in TEXT -->

### IDE Setup

**IntelliJ IDEA:**

1. Open the project
2. File → Project Structure → Project
3. Set Project SDK to Java 17
4. Mark `src/main/java` as Sources
5. Mark `src/test/java` as Tests

**Eclipse:**

1. File → Import → Existing Maven Projects
2. Select the project directory
3. Eclipse automatically configures Java 17

---

## FraiseQL Schema Definition

Now let's define our Blog API schema using Java annotations and the builder pattern.

### Step 1: Define GraphQL Types

Create `src/main/java/com/example/schema/types/User.java`:

```java
<!-- Code example in Java -->
package com.example.schema.types;

import com.FraiseQL.core.GraphQLField;
import com.FraiseQL.core.GraphQLType;
import java.time.LocalDateTime;

/**
 * Represents a blog author or commenter.
 */
@GraphQLType(description = "A user who can write posts and comments")
public class User {

    @GraphQLField(description = "Unique user identifier")
    public Long id;

    @GraphQLField(description = "Unique username for login")
    public String username;

    @GraphQLField(description = "User's email address")
    public String email;

    @GraphQLField(nullable = true, description = "User's bio (optional)")
    public String bio;

    @GraphQLField(name = "created_at", description = "When the user account was created")
    public LocalDateTime createdAt;

    @GraphQLField(name = "updated_at", description = "When the user was last updated")
    public LocalDateTime updatedAt;
}
```text
<!-- Code example in TEXT -->

Create `src/main/java/com/example/schema/types/Post.java`:

```java
<!-- Code example in Java -->
package com.example.schema.types;

import com.FraiseQL.core.GraphQLField;
import com.FraiseQL.core.GraphQLType;
import java.time.LocalDateTime;
import java.util.List;

/**
 * Represents a blog post.
 */
@GraphQLType(description = "A published or draft blog post")
public class Post {

    @GraphQLField(description = "Unique post identifier")
    public Long id;

    @GraphQLField(description = "Post author")
    public User author;

    @GraphQLField(description = "Post title")
    public String title;

    @GraphQLField(description = "URL-friendly slug for the post")
    public String slug;

    @GraphQLField(description = "Post content in Markdown")
    public String content;

    @GraphQLField(description = "Current publication status: DRAFT, PUBLISHED, ARCHIVED")
    public String status;

    @GraphQLField(nullable = true, name = "published_at", description = "When the post was published")
    public LocalDateTime publishedAt;

    @GraphQLField(description = "Tags associated with this post")
    public List<Tag> tags;

    @GraphQLField(description = "Comments on this post")
    public List<Comment> comments;

    @GraphQLField(name = "created_at", description = "When the post was created")
    public LocalDateTime createdAt;

    @GraphQLField(name = "updated_at", description = "When the post was last updated")
    public LocalDateTime updatedAt;
}
```text
<!-- Code example in TEXT -->

Create `src/main/java/com/example/schema/types/Comment.java`:

```java
<!-- Code example in Java -->
package com.example.schema.types;

import com.FraiseQL.core.GraphQLField;
import com.FraiseQL.core.GraphQLType;
import java.time.LocalDateTime;

/**
 * Represents a comment on a blog post.
 */
@GraphQLType(description = "A comment on a blog post")
public class Comment {

    @GraphQLField(description = "Unique comment identifier")
    public Long id;

    @GraphQLField(description = "The post being commented on")
    public Post post;

    @GraphQLField(description = "Author of the comment")
    public User author;

    @GraphQLField(description = "Comment text content")
    public String content;

    @GraphQLField(name = "created_at", description = "When the comment was posted")
    public LocalDateTime createdAt;

    @GraphQLField(name = "updated_at", description = "When the comment was last updated")
    public LocalDateTime updatedAt;
}
```text
<!-- Code example in TEXT -->

Create `src/main/java/com/example/schema/types/Tag.java`:

```java
<!-- Code example in Java -->
package com.example.schema.types;

import com.FraiseQL.core.GraphQLField;
import com.FraiseQL.core.GraphQLType;

/**
 * Represents a tag for categorizing posts.
 */
@GraphQLType(description = "A category tag for organizing posts")
public class Tag {

    @GraphQLField(description = "Unique tag identifier")
    public Long id;

    @GraphQLField(description = "Tag name (e.g., 'Java', 'GraphQL')")
    public String name;

    @GraphQLField(description = "URL-friendly tag slug")
    public String slug;
}
```text
<!-- Code example in TEXT -->

### Step 2: Define GraphQL Queries

Create `src/main/java/com/example/schema/SchemaBuilder.java`:

```java
<!-- Code example in Java -->
package com.example.schema;

import com.FraiseQL.core.FraiseQL;
import com.example.schema.types.*;

/**
 * Defines all GraphQL queries and mutations for the Blog API.
 * This uses the FraiseQL builder pattern to programmatically construct the schema.
 */
public class SchemaBuilder {

    private SchemaBuilder() {
        // Utility class
    }

    /**
     * Register all GraphQL types in the schema.
     */
    public static void registerTypes() {
        FraiseQL.registerTypes(
            User.class,
            Post.class,
            Comment.class,
            Tag.class
        );
    }

    /**
     * Define all GraphQL queries.
     * Queries are read-only operations that fetch data.
     */
    public static void registerQueries() {
        // Query: Get a single user by ID
        FraiseQL.query("user")
            .description("Fetch a user by ID")
            .returnType(User.class)
            .arg("id", "ID!", null)
            .register();

        // Query: Get all users
        FraiseQL.query("users")
            .description("Fetch all users in the system")
            .returnType(User.class)
            .returnsArray(true)
            .arg("limit", "Int", 100)
            .arg("offset", "Int", 0)
            .register();

        // Query: Get a single post by slug
        FraiseQL.query("post")
            .description("Fetch a published post by its slug")
            .returnType(Post.class)
            .arg("slug", "String!", null)
            .register();

        // Query: Get all published posts
        FraiseQL.query("posts")
            .description("Fetch all published posts with optional filtering")
            .returnType(Post.class)
            .returnsArray(true)
            .arg("limit", "Int", 20)
            .arg("offset", "Int", 0)
            .arg("status", "String", "PUBLISHED")
            .arg("tag", "String", null)
            .register();

        // Query: Get posts by author
        FraiseQL.query("postsByAuthor")
            .description("Fetch all posts written by a specific user")
            .returnType(Post.class)
            .returnsArray(true)
            .arg("authorId", "ID!", null)
            .arg("limit", "Int", 20)
            .arg("offset", "Int", 0)
            .register();

        // Query: Get all tags
        FraiseQL.query("tags")
            .description("Fetch all available tags")
            .returnType(Tag.class)
            .returnsArray(true)
            .register();

        // Query: Search posts
        FraiseQL.query("searchPosts")
            .description("Full-text search across post titles and content")
            .returnType(Post.class)
            .returnsArray(true)
            .arg("query", "String!", null)
            .arg("limit", "Int", 20)
            .arg("offset", "Int", 0)
            .register();
    }

    /**
     * Define all GraphQL mutations.
     * Mutations are write operations that create, update, or delete data.
     */
    public static void registerMutations() {
        // Mutation: Create a new user
        FraiseQL.mutation("createUser")
            .description("Create a new blog user account")
            .returnType(User.class)
            .arg("username", "String!", null)
            .arg("email", "String!", null)
            .arg("bio", "String", null)
            .register();

        // Mutation: Update a user
        FraiseQL.mutation("updateUser")
            .description("Update an existing user's profile")
            .returnType(User.class)
            .arg("id", "ID!", null)
            .arg("username", "String", null)
            .arg("email", "String", null)
            .arg("bio", "String", null)
            .register();

        // Mutation: Create a new post
        FraiseQL.mutation("createPost")
            .description("Create a new blog post (initially as DRAFT)")
            .returnType(Post.class)
            .arg("authorId", "ID!", null)
            .arg("title", "String!", null)
            .arg("slug", "String!", null)
            .arg("content", "String!", null)
            .arg("tagIds", "[ID!]", null)
            .register();

        // Mutation: Update a post
        FraiseQL.mutation("updatePost")
            .description("Update an existing post")
            .returnType(Post.class)
            .arg("id", "ID!", null)
            .arg("title", "String", null)
            .arg("content", "String", null)
            .arg("status", "String", null)
            .register();

        // Mutation: Publish a post
        FraiseQL.mutation("publishPost")
            .description("Publish a post (change status to PUBLISHED)")
            .returnType(Post.class)
            .arg("id", "ID!", null)
            .register();

        // Mutation: Delete a post
        FraiseQL.mutation("deletePost")
            .description("Delete a post (soft delete)")
            .returnType("Boolean!")
            .arg("id", "ID!", null)
            .register();

        // Mutation: Add a comment
        FraiseQL.mutation("createComment")
            .description("Add a comment to a post")
            .returnType(Comment.class)
            .arg("postId", "ID!", null)
            .arg("authorId", "ID!", null)
            .arg("content", "String!", null)
            .register();

        // Mutation: Delete a comment
        FraiseQL.mutation("deleteComment")
            .description("Remove a comment")
            .returnType("Boolean!")
            .arg("id", "ID!", null)
            .register();

        // Mutation: Create a tag
        FraiseQL.mutation("createTag")
            .description("Create a new blog tag")
            .returnType(Tag.class)
            .arg("name", "String!", null)
            .arg("slug", "String!", null)
            .register();
    }

    /**
     * Build the complete schema.
     */
    public static void build() {
        registerTypes();
        registerQueries();
        registerMutations();
    }
}
```text
<!-- Code example in TEXT -->

### Understanding the Builder Pattern

The FraiseQL builder pattern provides a fluent, readable API:

```java
<!-- Code example in Java -->
// Example: Building a query
FraiseQL.query("posts")           // Create query named "posts"
    .description("Get posts")     // Add description
    .returnType(Post.class)       // Set return type
    .returnsArray(true)           // Indicates return type is a list
    .arg("limit", "Int", 20)      // Add argument with default value
    .arg("offset", "Int", 0)      // Add another argument
    .register();                   // Register in schema

// This is equivalent to GraphQL:
// posts(limit: Int = 20, offset: Int = 0): [Post!]!
```text
<!-- Code example in TEXT -->

**Key patterns:**

- `.returnType(ClassName.class)` - Auto-converts Java type to GraphQL
- `.returnType("String!")` - Use raw GraphQL type names for scalars
- `.returnsArray(true)` - Wraps return type in list notation
- `.arg(name, type)` - Required argument (null default)
- `.arg(name, type, default)` - Optional argument with default

### Understanding Annotations

Annotations provide metadata at the field level:

```java
<!-- Code example in Java -->
@GraphQLType(description = "A blog post")
public class Post {

    @GraphQLField(description = "Post title")
    public String title;

    @GraphQLField(nullable = true, description = "Bio is optional")
    public String bio;

    @GraphQLField(name = "created_at", description = "Timestamp")
    public LocalDateTime createdAt;

    @GraphQLField(
        deprecated = "Use newEmail instead",
        description = "Deprecated email field"
    )
    public String oldEmail;
}
```text
<!-- Code example in TEXT -->

**Important patterns:**

- `nullable = true` → Field can be null in GraphQL (becomes `Type | null`)
- `nullable = false` (default) → Field cannot be null in GraphQL (becomes `Type!`)
- `name = "field_name"` → Customize GraphQL field name (use snake_case)
- `deprecated = "reason"` → Mark field as deprecated with migration advice

### Records vs Traditional Classes

In Java 17+, you can use records for immutable types:

```java
<!-- Code example in Java -->
// Using records (more concise)
@GraphQLType
public record User(
    @GraphQLField Long id,
    @GraphQLField String username,
    @GraphQLField String email,
    @GraphQLField(nullable = true) String bio
) {}

// Equivalent to traditional class
@GraphQLType
public class User {
    @GraphQLField public Long id;
    @GraphQLField public String username;
    @GraphQLField public String email;
    @GraphQLField(nullable = true) public String bio;
}
```text
<!-- Code example in TEXT -->

Records are recommended for immutability and less boilerplate.

### Null Safety Patterns

FraiseQL respects Java's null handling:

```java
<!-- Code example in Java -->
// Non-nullable field (default)
@GraphQLField
public String title;  // GraphQL: title: String!

// Nullable field
@GraphQLField(nullable = true)
public String bio;    // GraphQL: bio: String

// Use Optional<T> pattern for additional clarity
public Optional<String> bio() {
    return Optional.ofNullable(this.bio);
}
```text
<!-- Code example in TEXT -->

---

## Exporting Schema

Now we'll export the Java schema to `schema.json` that FraiseQL CLI can compile.

Create `src/main/java/com/example/schema/SchemaExporter.java`:

```java
<!-- Code example in Java -->
package com.example.schema;

import java.io.IOException;

/**
 * Exports the Blog API schema to schema.json.
 * Run: mvn exec:java
 * Or:  gradle exportSchema
 */
public class SchemaExporter {

    public static void main(String[] args) throws IOException {
        System.out.println("Building Blog API schema...");

        // Build the complete schema
        SchemaBuilder.build();

        // Export to schema.json
        String outputPath = args.length > 0 ? args[0] : "schema.json";
        System.out.println("Exporting schema to: " + outputPath);

        com.FraiseQL.core.FraiseQL.exportSchema(outputPath);

        System.out.println("✅ Schema exported successfully!");
        System.out.println("\nNext steps:");
        System.out.println("1. FraiseQL-cli compile schema.json");
        System.out.println("2. Deploy schema.compiled.json to your server");
    }
}
```text
<!-- Code example in TEXT -->

### Run Schema Export

**With Maven:**

```bash
<!-- Code example in BASH -->
mvn exec:java -Dexec.mainClass="com.example.schema.SchemaExporter"
```text
<!-- Code example in TEXT -->

**With Gradle:**

```bash
<!-- Code example in BASH -->
gradle exportSchema
```text
<!-- Code example in TEXT -->

This generates `schema.json`:

```json
<!-- Code example in JSON -->
{
  "types": {
    "User": {
      "name": "User",
      "description": "A user who can write posts and comments",
      "fields": {
        "id": {
          "name": "id",
          "type": "ID!",
          "description": "Unique user identifier"
        },
        "username": {
          "name": "username",
          "type": "String!",
          "description": "Unique username for login"
        },
        ...
      }
    },
    "Post": { ... },
    "Comment": { ... },
    "Tag": { ... }
  },
  "queries": [
    {
      "name": "user",
      "description": "Fetch a user by ID",
      "returnType": "User!",
      "args": {
        "id": "ID!"
      }
    },
    ...
  ],
  "mutations": [
    {
      "name": "createUser",
      "description": "Create a new blog user account",
      "returnType": "User!",
      "args": {
        "username": "String!",
        "email": "String!",
        "bio": "String"
      }
    },
    ...
  ]
}
```text
<!-- Code example in TEXT -->

---

## Compiling the Schema

The `schema.json` file is now ready to be compiled by FraiseQL CLI into an optimized runtime schema.

### Install FraiseQL CLI

```bash
<!-- Code example in BASH -->
# Using Rust/Cargo
cargo install FraiseQL-cli

# Or download binary from: https://github.com/FraiseQL/FraiseQL/releases
```text
<!-- Code example in TEXT -->

### Compile Schema

```bash
<!-- Code example in BASH -->
FraiseQL-cli compile schema.json
```text
<!-- Code example in TEXT -->

This generates `schema.compiled.json` containing:

- Optimized type definitions
- Pre-compiled SQL generation templates
- Security configuration
- Validation rules

### Maven Plugin Integration

Add to `pom.xml` to automatically compile during build:

```xml
<!-- Code example in XML -->
<plugin>
    <groupId>org.codehaus.mojo</groupId>
    <artifactId>exec-maven-plugin</artifactId>
    <version>3.1.0</version>
    <executions>
        <execution>
            <id>export-schema</id>
            <phase>generate-resources</phase>
            <goals>
                <goal>exec</goal>
            </goals>
            <configuration>
                <executable>sh</executable>
                <arguments>
                    <argument>-c</argument>
                    <argument>
                        mvn exec:java -Dexec.mainClass="com.example.schema.SchemaExporter" &&
                        FraiseQL-cli compile schema.json
                    </argument>
                </arguments>
            </configuration>
        </execution>
    </executions>
</plugin>
```text
<!-- Code example in TEXT -->

---

## Testing Your Schema

### JUnit 5 Integration Tests

Create `src/test/java/com/example/schema/SchemaExportTest.java`:

```java
<!-- Code example in Java -->
package com.example.schema;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.FraiseQL.core.FraiseQL;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

@DisplayName("Blog API Schema Export Tests")
public class SchemaExportTest {

    private static final ObjectMapper mapper = new ObjectMapper();
    private Path tmpDir;

    @BeforeEach
    void setUp() throws IOException {
        FraiseQL.clear();
        tmpDir = Files.createTempDirectory("blog-api-test-");
    }

    @Test
    @DisplayName("Schema should export successfully")
    void testSchemaExport() throws IOException {
        // Build schema
        SchemaBuilder.build();

        // Export
        Path outputPath = tmpDir.resolve("schema.json");
        FraiseQL.exportSchema(outputPath.toString());

        // Assert file exists
        assertTrue(Files.exists(outputPath), "Schema file should exist");

        // Parse and validate
        JsonNode schema = mapper.readTree(outputPath.toFile());
        assertNotNull(schema, "Schema should not be null");
    }

    @Test
    @DisplayName("Schema should contain all required types")
    void testSchemaTypes() throws IOException {
        SchemaBuilder.build();
        Path outputPath = tmpDir.resolve("schema.json");
        FraiseQL.exportSchema(outputPath.toString());

        JsonNode schema = mapper.readTree(outputPath.toFile());
        JsonNode types = schema.get("types");

        // Assert all types are present
        assertTrue(types.has("User"), "Should have User type");
        assertTrue(types.has("Post"), "Should have Post type");
        assertTrue(types.has("Comment"), "Should have Comment type");
        assertTrue(types.has("Tag"), "Should have Tag type");
    }

    @Test
    @DisplayName("User type should have all required fields")
    void testUserTypeFields() throws IOException {
        SchemaBuilder.build();
        Path outputPath = tmpDir.resolve("schema.json");
        FraiseQL.exportSchema(outputPath.toString());

        JsonNode schema = mapper.readTree(outputPath.toFile());
        JsonNode userFields = schema.get("types").get("User").get("fields");

        assertTrue(userFields.has("id"), "User should have id field");
        assertTrue(userFields.has("username"), "User should have username field");
        assertTrue(userFields.has("email"), "User should have email field");
        assertTrue(userFields.has("bio"), "User should have bio field");
        assertTrue(userFields.has("createdAt"), "User should have createdAt field");
    }

    @Test
    @DisplayName("Post type should reference User type")
    void testPostTypeRelationships() throws IOException {
        SchemaBuilder.build();
        Path outputPath = tmpDir.resolve("schema.json");
        FraiseQL.exportSchema(outputPath.toString());

        JsonNode schema = mapper.readTree(outputPath.toFile());
        JsonNode postFields = schema.get("types").get("Post").get("fields");

        assertTrue(postFields.has("author"), "Post should have author field");
        assertEquals("User!", postFields.get("author").get("type").asText(),
            "Post.author should reference User type");
    }

    @Test
    @DisplayName("Schema should have all required queries")
    void testSchemaQueries() throws IOException {
        SchemaBuilder.build();
        Path outputPath = tmpDir.resolve("schema.json");
        FraiseQL.exportSchema(outputPath.toString());

        JsonNode schema = mapper.readTree(outputPath.toFile());
        JsonNode queries = schema.get("queries");

        assertTrue(hasQuery(queries, "user"), "Should have user query");
        assertTrue(hasQuery(queries, "users"), "Should have users query");
        assertTrue(hasQuery(queries, "post"), "Should have post query");
        assertTrue(hasQuery(queries, "posts"), "Should have posts query");
        assertTrue(hasQuery(queries, "searchPosts"), "Should have searchPosts query");
    }

    @Test
    @DisplayName("Schema should have all required mutations")
    void testSchemaMutations() throws IOException {
        SchemaBuilder.build();
        Path outputPath = tmpDir.resolve("schema.json");
        FraiseQL.exportSchema(outputPath.toString());

        JsonNode schema = mapper.readTree(outputPath.toFile());
        JsonNode mutations = schema.get("mutations");

        assertTrue(hasMutation(mutations, "createUser"), "Should have createUser mutation");
        assertTrue(hasMutation(mutations, "createPost"), "Should have createPost mutation");
        assertTrue(hasMutation(mutations, "publishPost"), "Should have publishPost mutation");
        assertTrue(hasMutation(mutations, "deletePost"), "Should have deletePost mutation");
        assertTrue(hasMutation(mutations, "createComment"), "Should have createComment mutation");
    }

    private boolean hasQuery(JsonNode queries, String name) {
        for (JsonNode query : queries) {
            if (query.get("name").asText().equals(name)) {
                return true;
            }
        }
        return false;
    }

    private boolean hasMutation(JsonNode mutations, String name) {
        for (JsonNode mutation : mutations) {
            if (mutation.get("name").asText().equals(name)) {
                return true;
            }
        }
        return false;
    }
}
```text
<!-- Code example in TEXT -->

Run tests:

```bash
<!-- Code example in BASH -->
# Maven
mvn test

# Gradle
gradle test
```text
<!-- Code example in TEXT -->

---

## Common Patterns

### Pagination

```java
<!-- Code example in Java -->
FraiseQL.query("posts")
    .returnType(Post.class)
    .returnsArray(true)
    .arg("limit", "Int", 20)      // Default page size
    .arg("offset", "Int", 0)      // Default offset
    .register();
```text
<!-- Code example in TEXT -->

In GraphQL queries:

```graphql
<!-- Code example in GraphQL -->
query {
  posts(limit: 10, offset: 20) {
    id
    title
  }
}
```text
<!-- Code example in TEXT -->

### Filtering

```java
<!-- Code example in Java -->
FraiseQL.query("posts")
    .returnType(Post.class)
    .returnsArray(true)
    .arg("status", "String", "PUBLISHED")  // Filter by status
    .arg("tag", "String", null)             // Filter by tag
    .arg("authorId", "ID", null)            // Filter by author
    .register();
```text
<!-- Code example in TEXT -->

### Sorting

```java
<!-- Code example in Java -->
FraiseQL.query("posts")
    .returnType(Post.class)
    .returnsArray(true)
    .arg("sortBy", "String", "created_at")  // Sort field
    .arg("sortOrder", "String", "DESC")     // ASC or DESC
    .register();
```text
<!-- Code example in TEXT -->

### Relationships

Use Java types directly to establish relationships:

```java
<!-- Code example in Java -->
@GraphQLType
public class Post {
    @GraphQLField
    public User author;              // Single reference

    @GraphQLField
    public List<Comment> comments;   // Multiple references

    @GraphQLField
    public List<Tag> tags;           // Many-to-many
}
```text
<!-- Code example in TEXT -->

### Java Stream API Integration

For filtering collections in resolvers:

```java
<!-- Code example in Java -->
List<Post> publishedPosts = posts.stream()
    .filter(p -> "PUBLISHED".equals(p.status))
    .sorted((a, b) -> b.createdAt.compareTo(a.createdAt))
    .limit(20)
    .collect(Collectors.toList());
```text
<!-- Code example in TEXT -->

---

## Deployment

### Spring Boot Integration

Create `src/main/java/com/example/BlogApiApplication.java`:

```java
<!-- Code example in Java -->
package com.example;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.context.annotation.Bean;
import org.springframework.web.bind.annotation.*;

@SpringBootApplication
public class BlogApiApplication {

    public static void main(String[] args) {
        SpringApplication.run(BlogApiApplication.class, args);
    }

    @RestController
    @RequestMapping("/graphql")
    public static class GraphQLController {

        @PostMapping
        public Object query(@RequestBody String query) {
            // Load compiled schema and execute query
            // Implementation depends on FraiseQL-server integration
            return new Object();
        }
    }
}
```text
<!-- Code example in TEXT -->

### Docker Deployment

Create `Dockerfile`:

```dockerfile
<!-- Code example in DOCKERFILE -->
FROM maven:3.8.1-openjdk-17 as builder
WORKDIR /app
COPY . .
RUN mvn clean package -DskipTests

FROM openjdk:17-slim
WORKDIR /app
COPY --from=builder /app/target/blog-api-*.jar app.jar
COPY schema.compiled.json schema.compiled.json
EXPOSE 8080
CMD ["java", "-jar", "app.jar"]
```text
<!-- Code example in TEXT -->

Build and run:

```bash
<!-- Code example in BASH -->
docker build -t blog-api:1.0 .
docker run -p 8080:8080 blog-api:1.0
```text
<!-- Code example in TEXT -->

### JAR Deployment

Build:

```bash
<!-- Code example in BASH -->
mvn clean package
```text
<!-- Code example in TEXT -->

Run:

```bash
<!-- Code example in BASH -->
java -jar target/blog-api-1.0.0.jar
```text
<!-- Code example in TEXT -->

---

## Next Steps

### 1. Spring Boot Integration

Learn how to integrate FraiseQL compiled schema with Spring Boot resolvers:

```java
<!-- Code example in Java -->
@Component
public class PostResolver {
    @Autowired
    private PostService service;

    public Post post(String slug) {
        return service.findBySlug(slug);
    }

    public List<Post> posts(int limit, int offset, String status) {
        return service.findPublished(limit, offset);
    }
}
```text
<!-- Code example in TEXT -->

### 2. Building REST Endpoints

Expose GraphQL as REST endpoints for client applications.

### 3. Client Integration

Generate TypeScript/JavaScript client code from compiled schema for type-safe frontend development.

### 4. Real Database Integration

Replace mock data with actual PostgreSQL queries using JDBC or JPA.

---

## Troubleshooting

### Annotation Processing Errors

**Error**: "Cannot resolve symbol @GraphQLField"

**Solution**: Ensure `FraiseQL-java` dependency is in `pom.xml`:

```xml
<!-- Code example in XML -->
<dependency>
    <groupId>com.FraiseQL</groupId>
    <artifactId>FraiseQL-java</artifactId>
    <version>2.0.0</version>
</dependency>
```text
<!-- Code example in TEXT -->

### Type Mismatch Issues

**Error**: "Type mismatch: Java type not recognized"

**Solution**: Use supported Java types:

- Primitives: `int`, `long`, `float`, `double`, `boolean`
- Objects: `Integer`, `Long`, `Float`, `Double`, `Boolean`, `String`
- Collections: `List<T>`, `Set<T>`, `Map<String, T>`
- Date/Time: `LocalDateTime`, `LocalDate`, `Instant`
- Enums: Any `public enum`

### Maven Build Issues

**Error**: "FraiseQL-cli not found during build"

**Solution**: Install FraiseQL-cli separately:

```bash
<!-- Code example in BASH -->
cargo install FraiseQL-cli
```text
<!-- Code example in TEXT -->

Or ensure it's in your system PATH.

### Schema Export Failures

**Error**: "java.io.IOException: Cannot write to file"

**Solution**: Ensure write permissions in output directory:

```bash
<!-- Code example in BASH -->
chmod 755 /path/to/schema/output
```text
<!-- Code example in TEXT -->

---

## Complete Code Summary

### Project Structure

```text
<!-- Code example in TEXT -->
blog-api/
├── pom.xml
├── src/
│   ├── main/java/com/example/
│   │   └── schema/
│   │       ├── types/
│   │       │   ├── User.java
│   │       │   ├── Post.java
│   │       │   ├── Comment.java
│   │       │   └── Tag.java
│   │       ├── SchemaBuilder.java
│   │       └── SchemaExporter.java
│   └── test/java/com/example/
│       └── schema/
│           └── SchemaExportTest.java
├── schema.sql
├── schema.json (generated)
├── schema.compiled.json (generated)
└── Dockerfile
```text
<!-- Code example in TEXT -->

### Key Commands

```bash
<!-- Code example in BASH -->
# Generate schema
mvn exec:java -Dexec.mainClass="com.example.schema.SchemaExporter"

# Compile schema
FraiseQL-cli compile schema.json

# Run tests
mvn test

# Build JAR
mvn clean package

# Deploy with Docker
docker build -t blog-api:1.0 .
docker run -p 8080:8080 blog-api:1.0
```text
<!-- Code example in TEXT -->

### Key Takeaways

1. **Annotations define types**: `@GraphQLType` and `@GraphQLField` metadata
2. **Builder pattern defines operations**: `FraiseQL.query()`, `FraiseQL.mutation()`
3. **Export generates JSON**: `FraiseQL.exportSchema()` creates `schema.json`
4. **CLI compiles optimizations**: `FraiseQL-cli compile` produces `schema.compiled.json`
5. **Type-safe deployment**: Compiled schema provides runtime safety with zero validation overhead

---

## Further Reading

- [FraiseQL Architecture](../../docs/architecture/README.md)
- [Java Authoring API Reference](../integrations/sdk/java-reference.md)
- [GraphQL Specification](https://spec.graphql.org/)
- [Spring Boot Documentation](https://spring.io/projects/spring-boot)

**Questions?** See [troubleshooting.md](../troubleshooting.md) or open an issue on [GitHub](https://github.com/FraiseQL/FraiseQL).
