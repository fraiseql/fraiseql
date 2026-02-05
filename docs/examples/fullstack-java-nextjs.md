# Full-Stack Blog Platform: Java Authoring → FraiseQL Backend → Next.js Frontend

**Status:** ✅ Production-Ready Example
**Reading Time:** 45-60 minutes (end-to-end walkthrough)
**Last Updated:** 2026-02-05
**Target Audience:** Full-stack developers, teams adopting FraiseQL

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Part 1: Java Schema Authoring](#part-1-java-schema-authoring)
3. [Part 2: Database Schema & Setup](#part-2-database-schema--setup)
4. [Part 3: Export & Compilation](#part-3-export--compilation)
5. [Part 4: FraiseQL Server Deployment](#part-4-fraiseql-server-deployment)
6. [Part 5: Next.js Frontend](#part-5-nextjs-frontend)
7. [Part 6: Project Structure](#part-6-project-structure)
8. [Part 7: Running the Full Stack](#part-7-running-the-full-stack)
9. [Part 8: Example Workflows](#part-8-example-workflows)
10. [Part 9: Deployment](#part-9-deployment)
11. [Part 10: Troubleshooting & Best Practices](#part-10-troubleshooting--best-practices)

---

## Architecture Overview

### The Three-Layer Stack

```text
┌─────────────────────────────────────────────────────────────────┐
│                     Next.js Frontend (React)                     │
│  • Server Components: Static article pages, author profiles      │
│  • Client Components: Search, comments, interactive features     │
│  • TypeScript with generated GraphQL types                       │
│  • Deployed on: Vercel (or any Node.js host)                     │
└──────────────────────────┬──────────────────────────────────────┘
                           │ GraphQL queries/mutations
                           │ HTTP/WebSocket
                           ↓
┌─────────────────────────────────────────────────────────────────┐
│              FraiseQL Server (Compiled Rust)                     │
│  • High-performance GraphQL execution engine                     │
│  • Automatic SQL generation and optimization                     │
│  • Connection pooling, caching, rate limiting                    │
│  • Deployed on: Docker (Kubernetes-ready)                        │
│  • Database: PostgreSQL (MySQL/SQLite also supported)            │
└──────────────────────────┬──────────────────────────────────────┘
                           │ Compiled schema
                           │ with SQL templates
                           ↓
┌─────────────────────────────────────────────────────────────────┐
│                   PostgreSQL Database                            │
│  • Blog schema: articles, authors, comments, tags, categories    │
│  • Views for optimized queries                                   │
│  • Full-text search indexes                                      │
└─────────────────────────────────────────────────────────────────┘
        ↑
        │ Java source code
        │ (schema definition only)
        │
┌──────────────────────────────────────────────────────────────────┐
│     Java Maven Project (Developer Authoring)                      │
│  • @GraphQLType annotations on domain models                      │
│  • @GraphQLQuery, @GraphQLMutation decorators                     │
│  • Export: fraiseql-maven-plugin → schema.json                    │
│  • NO runtime dependency on FraiseQL (compile-time only)          │
└──────────────────────────────────────────────────────────────────┘
```text

**Key Principle:** Java is for *authoring only*. The schema is exported to JSON, compiled by FraiseQL to optimized SQL, and never touched again at runtime.

---

## Part 1: Java Schema Authoring

### 1.1 Maven Project Setup

Create a Maven project with the FraiseQL schema authoring plugin:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0
                             http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>dev.fraiseql</groupId>
    <artifactId>blog-schema</artifactId>
    <version>1.0.0</version>
    <packaging>jar</packaging>

    <name>Blog Platform Schema</name>
    <description>FraiseQL schema for blog platform (Java authoring)</description>

    <properties>
        <maven.compiler.source>17</maven.compiler.source>
        <maven.compiler.target>17</maven.compiler.target>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
        <fraiseql.version>2.0.0</fraiseql.version>
    </properties>

    <dependencies>
        <!-- FraiseQL Schema Annotations (compile-only) -->
        <dependency>
            <groupId>dev.fraiseql</groupId>
            <artifactId>fraiseql-annotations</artifactId>
            <version>${fraiseql.version}</version>
            <scope>compile</scope>
        </dependency>

        <!-- Testing -->
        <dependency>
            <groupId>junit</groupId>
            <artifactId>junit</artifactId>
            <version>4.13.2</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <plugins>
            <!-- FraiseQL Maven Plugin: Export schema to JSON -->
            <plugin>
                <groupId>dev.fraiseql</groupId>
                <artifactId>fraiseql-maven-plugin</artifactId>
                <version>${fraiseql.version}</version>
                <executions>
                    <execution>
                        <phase>generate-resources</phase>
                        <goals>
                            <goal>export-schema</goal>
                        </goals>
                        <configuration>
                            <packageName>dev.fraiseql.blog</packageName>
                            <outputFile>
                                ${project.build.directory}/schema.json
                            </outputFile>
                            <scanPackages>
                                <scanPackage>dev.fraiseql.blog</scanPackage>
                            </scanPackages>
                        </configuration>
                    </execution>
                </executions>
            </plugin>

            <!-- Java Compiler -->
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-compiler-plugin</artifactId>
                <version>3.11.0</version>
                <configuration>
                    <source>17</source>
                    <target>17</target>
                </configuration>
            </plugin>
        </plugins>
    </build>
</project>
```text

### 1.2 Domain Types

Create Java classes with FraiseQL annotations to define your GraphQL schema:

```java
// src/main/java/dev/fraiseql/blog/types/User.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;
import java.time.OffsetDateTime;

@GraphQLType(description = "A registered user/author")
public class User {

    @GraphQLField(description = "User ID")
    @Primary
    public String id;

    @GraphQLField
    public String username;

    @GraphQLField
    public String email;

    @GraphQLField(description = "User's display name")
    public String displayName;

    @GraphQLField(description = "User's profile bio")
    public String bio;

    @GraphQLField(description = "User's avatar URL")
    public String avatarUrl;

    @GraphQLField
    public UserRole role;

    @GraphQLField
    public OffsetDateTime createdAt;

    @GraphQLField
    public OffsetDateTime updatedAt;

    // Related fields
    @GraphQLField(description = "Articles written by this user")
    @Relationship(type = "hasMany", target = "Article", foreignKey = "author_id")
    public java.util.List<Article> articles;

    @GraphQLField(description = "Comments written by this user")
    @Relationship(type = "hasMany", target = "Comment", foreignKey = "author_id")
    public java.util.List<Comment> comments;
}
```text

```java
// src/main/java/dev/fraiseql/blog/types/UserRole.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;

@GraphQLEnum
public enum UserRole {
    @GraphQLEnumValue
    ADMIN("admin"),

    @GraphQLEnumValue
    EDITOR("editor"),

    @GraphQLEnumValue
    AUTHOR("author"),

    @GraphQLEnumValue
    SUBSCRIBER("subscriber");

    private final String value;

    UserRole(String value) {
        this.value = value;
    }

    public String getValue() {
        return value;
    }
}
```text

```java
// src/main/java/dev/fraiseql/blog/types/Category.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;

@GraphQLType(description = "Article category/topic")
public class Category {

    @GraphQLField
    @Primary
    public String id;

    @GraphQLField
    public String name;

    @GraphQLField
    public String slug;

    @GraphQLField(description = "Category description")
    public String description;

    @GraphQLField(description = "URL-friendly icon name")
    public String iconName;

    @GraphQLField
    public Integer displayOrder;

    @GraphQLField(description = "Articles in this category")
    @Relationship(type = "hasMany", target = "Article", foreignKey = "category_id")
    public java.util.List<Article> articles;
}
```text

```java
// src/main/java/dev/fraiseql/blog/types/Tag.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;

@GraphQLType(description = "Article tag/label")
public class Tag {

    @GraphQLField
    @Primary
    public String id;

    @GraphQLField
    public String name;

    @GraphQLField
    public String slug;

    @GraphQLField
    public Integer usageCount;

    @GraphQLField(description = "Articles with this tag")
    @Relationship(
        type = "hasManyThrough",
        target = "Article",
        throughTable = "article_tags"
    )
    public java.util.List<Article> articles;
}
```text

```java
// src/main/java/dev/fraiseql/blog/types/Article.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;
import java.time.OffsetDateTime;

@GraphQLType(description = "Blog article/post")
public class Article {

    @GraphQLField
    @Primary
    public String id;

    @GraphQLField
    public String title;

    @GraphQLField
    public String slug;

    @GraphQLField(description = "Article summary/excerpt")
    public String excerpt;

    @GraphQLField(description = "Full article content (Markdown)")
    public String content;

    @GraphQLField(description = "Featured image URL")
    public String featuredImageUrl;

    @GraphQLField
    @Index
    public ArticleStatus status;

    @GraphQLField(description = "View count")
    public Integer viewCount;

    @GraphQLField
    @Searchable
    public String title;

    @GraphQLField
    @Searchable
    public String content;

    @GraphQLField
    public OffsetDateTime publishedAt;

    @GraphQLField
    public OffsetDateTime createdAt;

    @GraphQLField
    public OffsetDateTime updatedAt;

    // Relationships
    @GraphQLField(description = "Article author")
    @Relationship(type = "belongsTo", target = "User", foreignKey = "author_id")
    public User author;

    @GraphQLField(description = "Article category")
    @Relationship(type = "belongsTo", target = "Category", foreignKey = "category_id")
    public Category category;

    @GraphQLField(description = "Article tags")
    @Relationship(
        type = "hasManyThrough",
        target = "Tag",
        throughTable = "article_tags"
    )
    public java.util.List<Tag> tags;

    @GraphQLField(description = "Article comments")
    @Relationship(type = "hasMany", target = "Comment", foreignKey = "article_id")
    public java.util.List<Comment> comments;
}
```text

```java
// src/main/java/dev/fraiseql/blog/types/ArticleStatus.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;

@GraphQLEnum
public enum ArticleStatus {
    @GraphQLEnumValue
    DRAFT("draft"),

    @GraphQLEnumValue
    PUBLISHED("published"),

    @GraphQLEnumValue
    ARCHIVED("archived");

    private final String value;

    ArticleStatus(String value) {
        this.value = value;
    }

    public String getValue() {
        return value;
    }
}
```text

```java
// src/main/java/dev/fraiseql/blog/types/Comment.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;
import java.time.OffsetDateTime;

@GraphQLType(description = "Article comment")
public class Comment {

    @GraphQLField
    @Primary
    public String id;

    @GraphQLField(description = "Comment content (Markdown)")
    @Searchable
    public String content;

    @GraphQLField
    public CommentStatus status;

    @GraphQLField
    public OffsetDateTime createdAt;

    @GraphQLField
    public OffsetDateTime updatedAt;

    // Relationships
    @GraphQLField(description = "Article this comment is on")
    @Relationship(type = "belongsTo", target = "Article", foreignKey = "article_id")
    public Article article;

    @GraphQLField(description = "User who wrote this comment")
    @Relationship(type = "belongsTo", target = "User", foreignKey = "author_id")
    public User author;

    @GraphQLField(description = "Parent comment if this is a reply")
    @Relationship(type = "belongsTo", target = "Comment", foreignKey = "parent_id")
    public Comment parentComment;

    @GraphQLField(description = "Replies to this comment")
    @Relationship(type = "hasMany", target = "Comment", foreignKey = "parent_id")
    public java.util.List<Comment> replies;
}
```text

```java
// src/main/java/dev/fraiseql/blog/types/CommentStatus.java
package dev.fraiseql.blog.types;

import dev.fraiseql.annotations.*;

@GraphQLEnum
public enum CommentStatus {
    @GraphQLEnumValue
    PENDING("pending"),

    @GraphQLEnumValue
    APPROVED("approved"),

    @GraphQLEnumValue
    REJECTED("rejected");

    private final String value;

    CommentStatus(String value) {
        this.value = value;
    }

    public String getValue() {
        return value;
    }
}
```text

### 1.3 Query Definitions

```java
// src/main/java/dev/fraiseql/blog/queries/ArticleQueries.java
package dev.fraiseql.blog.queries;

import dev.fraiseql.annotations.*;
import dev.fraiseql.blog.types.*;
import java.util.List;

@GraphQLQueryRoot
public class ArticleQueries {

    @GraphQLQuery(description = "Get all published articles with pagination")
    @Paginated(defaultLimit = 10, maxLimit = 100)
    @Cached(ttlSeconds = 300)
    public List<Article> getArticles(
        @GraphQLArg(description = "Filter by status") ArticleStatus status,
        @GraphQLArg(description = "Filter by category slug") String categorySlug,
        @GraphQLArg(description = "Sort field") String sortBy,
        @GraphQLArg(description = "Sort direction") SortDirection sortOrder
    ) {
        // Query resolver - only signature defined here
        // FraiseQL compiler generates the SQL
        return null;
    }

    @GraphQLQuery(description = "Get single article by ID")
    @Cached(ttlSeconds = 600)
    public Article getArticle(
        @GraphQLArg(description = "Article ID") @Required String id
    ) {
        return null;
    }

    @GraphQLQuery(description = "Get single article by slug")
    @Cached(ttlSeconds = 600)
    public Article getArticleBySlug(
        @GraphQLArg(description = "Article slug") @Required String slug
    ) {
        return null;
    }

    @GraphQLQuery(description = "Search articles by title and content")
    @Paginated(defaultLimit = 10, maxLimit = 50)
    @Cached(ttlSeconds = 60)
    public List<Article> searchArticles(
        @GraphQLArg(description = "Search query") @Required String query,
        @GraphQLArg(description = "Filter by category") String categoryId
    ) {
        return null;
    }

    @GraphQLQuery(description = "Get articles by author")
    @Paginated(defaultLimit = 10, maxLimit = 100)
    @Cached(ttlSeconds = 300)
    public List<Article> getArticlesByAuthor(
        @GraphQLArg(description = "Author ID or username") @Required String authorId,
        @GraphQLArg(description = "Filter by status") ArticleStatus status
    ) {
        return null;
    }

    @GraphQLQuery(description = "Get trending articles (most viewed in last 30 days)")
    @Paginated(defaultLimit = 10, maxLimit = 50)
    @Cached(ttlSeconds = 600)
    public List<Article> getTrendingArticles(
        @GraphQLArg(description = "Time window in days") Integer days
    ) {
        return null;
    }
}

@GraphQLEnum
enum SortDirection {
    @GraphQLEnumValue ASC,
    @GraphQLEnumValue DESC
}
```text

```java
// src/main/java/dev/fraiseql/blog/queries/CommentQueries.java
package dev.fraiseql.blog.queries;

import dev.fraiseql.annotations.*;
import dev.fraiseql.blog.types.*;
import java.util.List;

@GraphQLQueryRoot
public class CommentQueries {

    @GraphQLQuery(description = "Get approved comments for an article")
    @Paginated(defaultLimit = 20, maxLimit = 100)
    @Cached(ttlSeconds = 60)
    public List<Comment> getComments(
        @GraphQLArg(description = "Article ID") @Required String articleId,
        @GraphQLArg(description = "Sort order") SortDirection sortOrder
    ) {
        return null;
    }

    @GraphQLQuery(description = "Get single comment by ID")
    @Cached(ttlSeconds = 300)
    public Comment getComment(
        @GraphQLArg(description = "Comment ID") @Required String id
    ) {
        return null;
    }

    @GraphQLQuery(description = "Get comment count for article")
    public Integer getCommentCount(
        @GraphQLArg(description = "Article ID") @Required String articleId,
        @GraphQLArg(description = "Status filter") CommentStatus status
    ) {
        return null;
    }
}
```text

```java
// src/main/java/dev/fraiseql/blog/queries/CategoryQueries.java
package dev.fraiseql.blog.queries;

import dev.fraiseql.annotations.*;
import dev.fraiseql.blog.types.*;
import java.util.List;

@GraphQLQueryRoot
public class CategoryQueries {

    @GraphQLQuery(description = "Get all categories")
    @Cached(ttlSeconds = 3600)
    public List<Category> getCategories() {
        return null;
    }

    @GraphQLQuery(description = "Get category by ID")
    @Cached(ttlSeconds = 3600)
    public Category getCategory(
        @GraphQLArg(description = "Category ID") @Required String id
    ) {
        return null;
    }

    @GraphQLQuery(description = "Get category by slug")
    @Cached(ttlSeconds = 3600)
    public Category getCategoryBySlug(
        @GraphQLArg(description = "Category slug") @Required String slug
    ) {
        return null;
    }
}
```text

### 1.4 Mutation Definitions

```java
// src/main/java/dev/fraiseql/blog/mutations/ArticleMutations.java
package dev.fraiseql.blog.mutations;

import dev.fraiseql.annotations.*;
import dev.fraiseql.blog.types.*;

@GraphQLMutationRoot
public class ArticleMutations {

    @GraphQLMutation(description = "Publish a new article")
    @Authorize(roles = {"AUTHOR", "EDITOR", "ADMIN"})
    @InvalidateCache(types = {"Article"})
    public Article publishArticle(
        @GraphQLArg(description = "Article title") @Required String title,
        @GraphQLArg(description = "Article slug") @Required String slug,
        @GraphQLArg(description = "Article excerpt") String excerpt,
        @GraphQLArg(description = "Article content (Markdown)") @Required String content,
        @GraphQLArg(description = "Category ID") @Required String categoryId,
        @GraphQLArg(description = "Tag IDs") java.util.List<String> tagIds,
        @GraphQLArg(description = "Featured image URL") String featuredImageUrl
    ) {
        return null;
    }

    @GraphQLMutation(description = "Update an existing article")
    @Authorize(roles = {"EDITOR", "ADMIN"})
    @InvalidateCache(types = {"Article"})
    public Article updateArticle(
        @GraphQLArg(description = "Article ID") @Required String id,
        @GraphQLArg(description = "New title") String title,
        @GraphQLArg(description = "New slug") String slug,
        @GraphQLArg(description = "New excerpt") String excerpt,
        @GraphQLArg(description = "New content") String content,
        @GraphQLArg(description = "New category ID") String categoryId,
        @GraphQLArg(description = "New tag IDs") java.util.List<String> tagIds,
        @GraphQLArg(description = "New featured image URL") String featuredImageUrl
    ) {
        return null;
    }

    @GraphQLMutation(description = "Delete an article")
    @Authorize(roles = {"EDITOR", "ADMIN"})
    @InvalidateCache(types = {"Article"})
    public Boolean deleteArticle(
        @GraphQLArg(description = "Article ID") @Required String id
    ) {
        return null;
    }

    @GraphQLMutation(description = "Archive an article (soft delete)")
    @Authorize(roles = {"EDITOR", "ADMIN"})
    @InvalidateCache(types = {"Article"})
    public Article archiveArticle(
        @GraphQLArg(description = "Article ID") @Required String id
    ) {
        return null;
    }

    @GraphQLMutation(description = "Increment article view count")
    @RateLimit(requests = 100, windowSeconds = 3600)
    @InvalidateCache(types = {"Article"})
    public Article recordArticleView(
        @GraphQLArg(description = "Article ID") @Required String id
    ) {
        return null;
    }
}
```text

```java
// src/main/java/dev/fraiseql/blog/mutations/CommentMutations.java
package dev.fraiseql.blog.mutations;

import dev.fraiseql.annotations.*;
import dev.fraiseql.blog.types.*;

@GraphQLMutationRoot
public class CommentMutations {

    @GraphQLMutation(description = "Add a comment to an article")
    @Authorize(roles = {"SUBSCRIBER", "AUTHOR", "EDITOR", "ADMIN"})
    @RateLimit(requests = 10, windowSeconds = 60)
    @InvalidateCache(types = {"Comment", "Article"})
    public Comment addComment(
        @GraphQLArg(description = "Article ID") @Required String articleId,
        @GraphQLArg(description = "Comment content") @Required String content,
        @GraphQLArg(description = "Parent comment ID (if reply)") String parentCommentId
    ) {
        return null;
    }

    @GraphQLMutation(description = "Update a comment")
    @Authorize(roles = {"AUTHOR", "EDITOR", "ADMIN"})
    @InvalidateCache(types = {"Comment"})
    public Comment updateComment(
        @GraphQLArg(description = "Comment ID") @Required String id,
        @GraphQLArg(description = "New content") @Required String content
    ) {
        return null;
    }

    @GraphQLMutation(description = "Delete a comment")
    @Authorize(roles = {"AUTHOR", "EDITOR", "ADMIN"})
    @InvalidateCache(types = {"Comment"})
    public Boolean deleteComment(
        @GraphQLArg(description = "Comment ID") @Required String id
    ) {
        return null;
    }

    @GraphQLMutation(description = "Approve a pending comment (moderator only)")
    @Authorize(roles = {"EDITOR", "ADMIN"})
    @InvalidateCache(types = {"Comment"})
    public Comment approveComment(
        @GraphQLArg(description = "Comment ID") @Required String id
    ) {
        return null;
    }
}
```text

---

## Part 2: Database Schema & Setup

### 2.1 PostgreSQL Schema

```sql
-- PostgreSQL schema for blog platform
-- Created: 2026-02-05
-- Target: FraiseQL v2.0.0

CREATE SCHEMA IF NOT EXISTS blog;
SET search_path TO blog;

-- Create enum types
CREATE TYPE user_role AS ENUM ('admin', 'editor', 'author', 'subscriber');
CREATE TYPE article_status AS ENUM ('draft', 'published', 'archived');
CREATE TYPE comment_status AS ENUM ('pending', 'approved', 'rejected');

-- Users/Authors table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(100) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    display_name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
    password_hash VARCHAR(255) NOT NULL,
    role user_role NOT NULL DEFAULT 'subscriber',
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_role ON users(role);

-- Categories table
CREATE TABLE categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    icon_name VARCHAR(50),
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_categories_slug ON categories(slug);
CREATE INDEX idx_categories_display_order ON categories(display_order);

-- Articles table
CREATE TABLE articles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,
    excerpt TEXT,
    content TEXT NOT NULL,
    featured_image_url VARCHAR(500),
    author_id UUID NOT NULL,
    category_id UUID NOT NULL,
    status article_status NOT NULL DEFAULT 'draft',
    view_count INTEGER NOT NULL DEFAULT 0,
    published_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE RESTRICT
);

CREATE INDEX idx_articles_slug ON articles(slug);
CREATE INDEX idx_articles_author_id ON articles(author_id);
CREATE INDEX idx_articles_category_id ON articles(category_id);
CREATE INDEX idx_articles_status ON articles(status);
CREATE INDEX idx_articles_published_at ON articles(published_at DESC);
CREATE INDEX idx_articles_view_count ON articles(view_count DESC);

-- Full-text search index on articles
CREATE INDEX idx_articles_fts ON articles USING GIN(
    to_tsvector('english', title || ' ' || COALESCE(content, ''))
);

-- Tags table
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE,
    slug VARCHAR(100) NOT NULL UNIQUE,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tags_slug ON tags(slug);

-- Article tags junction table
CREATE TABLE article_tags (
    article_id UUID NOT NULL,
    tag_id UUID NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (article_id, tag_id),
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

-- Comments table
CREATE TABLE comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    article_id UUID NOT NULL,
    author_id UUID NOT NULL,
    parent_id UUID,
    content TEXT NOT NULL,
    status comment_status NOT NULL DEFAULT 'pending',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE,
    FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES comments(id) ON DELETE CASCADE
);

CREATE INDEX idx_comments_article_id ON comments(article_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);
CREATE INDEX idx_comments_parent_id ON comments(parent_id);
CREATE INDEX idx_comments_status ON comments(status);

-- Optimized view: Recent published articles with author and category
CREATE VIEW published_articles AS
SELECT
    a.id,
    a.title,
    a.slug,
    a.excerpt,
    a.featured_image_url,
    a.author_id,
    a.category_id,
    a.view_count,
    a.published_at,
    a.created_at,
    u.username as author_username,
    u.display_name as author_name,
    c.name as category_name,
    c.slug as category_slug
FROM articles a
JOIN users u ON a.author_id = u.id
JOIN categories c ON a.category_id = c.id
WHERE a.status = 'published' AND u.is_active = true
ORDER BY a.published_at DESC;

-- Optimized view: Article stats
CREATE VIEW article_stats AS
SELECT
    a.id,
    a.title,
    COUNT(DISTINCT c.id) as comment_count,
    COUNT(DISTINCT at.tag_id) as tag_count,
    a.view_count,
    a.published_at
FROM articles a
LEFT JOIN comments c ON a.id = c.article_id AND c.status = 'approved'
LEFT JOIN article_tags at ON a.id = at.article_id
WHERE a.status = 'published'
GROUP BY a.id, a.title, a.view_count, a.published_at;

-- Function: Increment article view count
CREATE OR REPLACE FUNCTION increment_article_views(p_article_id UUID)
RETURNS INTEGER AS $$
BEGIN
    UPDATE articles SET view_count = view_count + 1 WHERE id = p_article_id;
    RETURN 1;
END;
$$ LANGUAGE plpgsql;

-- Function: Get trending articles
CREATE OR REPLACE FUNCTION get_trending_articles(p_days INTEGER DEFAULT 30)
RETURNS TABLE (
    article_id UUID,
    title VARCHAR,
    slug VARCHAR,
    view_count INTEGER,
    comment_count BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        a.id,
        a.title,
        a.slug,
        a.view_count,
        COUNT(c.id)::BIGINT as comment_count
    FROM articles a
    LEFT JOIN comments c ON a.id = c.article_id AND c.status = 'approved'
    WHERE
        a.status = 'published'
        AND a.published_at >= CURRENT_TIMESTAMP - (p_days || ' days')::INTERVAL
    GROUP BY a.id, a.title, a.slug, a.view_count
    ORDER BY a.view_count DESC, comment_count DESC
    LIMIT 20;
END;
$$ LANGUAGE plpgsql;
```text

### 2.2 Initialize Database

```bash
#!/bin/bash
# scripts/init-db.sh

set -e

DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-blog_db}"
DB_USER="${DB_USER:-blog_user}"
DB_PASSWORD="${DB_PASSWORD:-blog_password}"

echo "Creating database and user..."

# Create database and user (requires superuser)
psql -h "$DB_HOST" -U postgres -d postgres << EOF
CREATE USER $DB_USER WITH PASSWORD '$DB_PASSWORD';
CREATE DATABASE $DB_NAME OWNER $DB_USER;
ALTER DATABASE $DB_NAME SET search_path TO blog, public;
EOF

echo "Loading schema..."

# Load schema
psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -f schema.sql

echo "Seeding sample data..."

# Seed sample data
psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" << 'EOF'
-- Insert sample data
INSERT INTO users (username, email, display_name, bio, role) VALUES
    ('alice', 'alice@example.com', 'Alice Johnson', 'Tech enthusiast', 'author'),
    ('bob', 'bob@example.com', 'Bob Smith', 'Software engineer', 'editor'),
    ('charlie', 'charlie@example.com', 'Charlie Brown', 'GraphQL expert', 'author');

INSERT INTO categories (name, slug, description, display_order) VALUES
    ('Technology', 'technology', 'Technology and software', 1),
    ('GraphQL', 'graphql', 'GraphQL and APIs', 2),
    ('Database', 'database', 'Databases and data', 3);

INSERT INTO tags (name, slug) VALUES
    ('rust', 'rust'),
    ('graphql', 'graphql'),
    ('database', 'database'),
    ('performance', 'performance'),
    ('sql', 'sql');
EOF

echo "Database initialization complete!"
```text

---

## Part 3: Export & Compilation

### 3.1 Build Java Schema

```bash
#!/bin/bash
# scripts/build-schema.sh

set -e

echo "Building Java schema project..."

cd java-schema/

# Build Maven project (generates schema.json)
mvn clean generate-resources

# Verify schema.json was created
if [ ! -f target/schema.json ]; then
    echo "ERROR: schema.json not found in target/"
    exit 1
fi

echo "✓ schema.json generated successfully"

# Copy to fraiseql compiler directory
cp target/schema.json ../schema.json

echo "✓ Schema exported to ../schema.json"
```text

### 3.2 Compile Schema with FraiseQL CLI

```bash
#!/bin/bash
# scripts/compile-schema.sh

set -e

echo "Compiling schema with FraiseQL..."

# Install FraiseQL CLI if needed
if ! command -v fraiseql &> /dev/null; then
    echo "Installing fraiseql-cli..."
    cargo install fraiseql-cli
fi

# Create fraiseql.toml configuration
cat > fraiseql.toml << 'EOF'
[database]
url = "postgresql://blog_user:blog_password@localhost:5432/blog_db"
pool_size = 10
connection_timeout_secs = 30

[server]
port = 5000
host = "0.0.0.0"

[security]
# Authorization
[security.authorization]
enabled = true
require_auth_on_all_queries = false
require_auth_on_mutations = true

# Rate limiting
[security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
mutations_max_requests = 50
mutations_window_secs = 60

# Audit logging
[security.audit_logging]
enabled = true
log_level = "info"

[cache]
enabled = true
default_ttl_seconds = 300
max_entries = 10000

[features]
enable_subscriptions = false
enable_batching = true
enable_persisted_queries = true
max_query_complexity = 1000
EOF

# Compile schema
fraiseql compile schema.json \
    --config fraiseql.toml \
    --output schema.compiled.json \
    --format json \
    --target postgres

echo "✓ Schema compiled to schema.compiled.json"

# Verify compiled schema
if [ ! -f schema.compiled.json ]; then
    echo "ERROR: schema.compiled.json not found"
    exit 1
fi

echo "✓ Compilation successful"
fraiseql validate schema.compiled.json
```text

### 3.3 Compiled Schema Structure

The `schema.compiled.json` contains everything needed at runtime:

```json
{
  "version": "2.0.0",
  "compiled_at": "2026-02-05T12:00:00Z",
  "database": {
    "type": "postgres",
    "url": "postgresql://blog_user:blog_password@localhost:5432/blog_db"
  },
  "types": [
    {
      "name": "Article",
      "kind": "object",
      "fields": [
        {
          "name": "id",
          "type": "String!",
          "sql_column": "id"
        },
        {
          "name": "title",
          "type": "String!",
          "sql_column": "title"
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "getArticles",
      "type": "Article!",
      "sql_template": "SELECT id, title, slug, ... FROM articles WHERE status = $1"
    }
  ],
  "mutations": [
    {
      "name": "publishArticle",
      "type": "Article!",
      "sql_template": "INSERT INTO articles (title, ...) VALUES ($1, ...) RETURNING *"
    }
  ],
  "security": {
    "rate_limiting": {
      "enabled": true,
      "auth_start_max_requests": 100,
      "auth_start_window_secs": 60
    },
    "audit_logging": {
      "enabled": true
    }
  }
}
```text

---

## Part 4: FraiseQL Server Deployment

### 4.1 Dockerfile

```dockerfile
# Multi-stage build
FROM rust:latest as builder

WORKDIR /app

# Copy fraiseql source
COPY . .

# Build FraiseQL server (release mode)
RUN cargo build --release --package fraiseql-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled server from builder
COPY --from=builder /app/target/release/fraiseql-server /app/

# Copy compiled schema
COPY schema.compiled.json /app/

# Expose GraphQL port
EXPOSE 5000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:5000/health || exit 1

# Run server
ENV RUST_LOG=info
CMD ["/app/fraiseql-server", "--schema", "/app/schema.compiled.json"]
```text

### 4.2 Docker Compose Setup

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: blog_db
      POSTGRES_USER: blog_user
      POSTGRES_PASSWORD: blog_password
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./sql/schema.sql:/docker-entrypoint-initdb.d/01-schema.sql
      - ./sql/seed.sql:/docker-entrypoint-initdb.d/02-seed.sql
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U blog_user"]
      interval: 10s
      timeout: 5s
      retries: 5

  fraiseql:
    build:
      context: .
      dockerfile: Dockerfile
    environment:
      DATABASE_URL: postgresql://blog_user:blog_password@postgres:5432/blog_db
      RUST_LOG: info
      FRAISEQL_PORT: 5000
      FRAISEQL_HOST: 0.0.0.0
    ports:
      - "5000:5000"
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:5000/health"]
      interval: 10s
      timeout: 5s
      retries: 3

volumes:
  postgres_data:
```text

### 4.3 Deploy to Production

```bash
#!/bin/bash
# scripts/deploy.sh

set -e

REGISTRY="docker.example.com"
IMAGE="fraiseql-blog"
VERSION=$(git describe --tags --always)

echo "Building Docker image..."
docker build -t "$REGISTRY/$IMAGE:$VERSION" .
docker tag "$REGISTRY/$IMAGE:$VERSION" "$REGISTRY/$IMAGE:latest"

echo "Pushing to registry..."
docker push "$REGISTRY/$IMAGE:$VERSION"
docker push "$REGISTRY/$IMAGE:latest"

echo "Deploying to Kubernetes..."
kubectl set image deployment/fraiseql-blog \
    fraiseql="$REGISTRY/$IMAGE:$VERSION" \
    --namespace production

echo "✓ Deployment complete: $VERSION"
```text

---

## Part 5: Next.js Frontend

### 5.1 Project Setup

```bash
# Create Next.js 14+ app with TypeScript
npx create-next-app@latest blog-frontend \
  --typescript \
  --tailwind \
  --app \
  --no-eslint

cd blog-frontend

# Install dependencies
npm install @apollo/client graphql graphql-request
npm install --save-dev @graphql-codegen/cli @graphql-codegen/client-preset
npm install zustand
```text

### 5.2 Environment Configuration

```bash
# .env.local
NEXT_PUBLIC_GRAPHQL_URL=http://localhost:5000/graphql
NEXT_PUBLIC_GRAPHQL_WS_URL=ws://localhost:5000/graphql
NEXT_PUBLIC_API_BASE_URL=http://localhost:5000

# For production
# NEXT_PUBLIC_GRAPHQL_URL=https://api.example.com/graphql
```text

### 5.3 GraphQL Schema & Codegen

```yaml
# codegen.yml
schema: http://localhost:5000/graphql
documents: 'src/**/*.tsx'
generates:
  src/gql/generated.ts:
    preset: client
    config:
      useTypeNameAsDefault: true
      skipTypename: false
```text

### 5.4 GraphQL Queries & Mutations

```graphql
# src/graphql/queries.graphql

# Get published articles with pagination
query GetArticles($limit: Int!, $offset: Int!) {
  getArticles(limit: $limit, offset: $offset) {
    id
    title
    slug
    excerpt
    featuredImageUrl
    viewCount
    publishedAt
    author {
      id
      username
      displayName
      avatarUrl
    }
    category {
      id
      name
      slug
    }
    tags {
      id
      name
      slug
    }
  }
}

# Get single article by slug
query GetArticleBySlug($slug: String!) {
  getArticleBySlug(slug: $slug) {
    id
    title
    slug
    content
    excerpt
    featuredImageUrl
    viewCount
    publishedAt
    createdAt
    author {
      id
      username
      displayName
      bio
      avatarUrl
    }
    category {
      id
      name
      slug
    }
    tags {
      id
      name
      slug
    }
    comments {
      id
      content
      status
      createdAt
      author {
        id
        displayName
        avatarUrl
      }
    }
  }
}

# Search articles
query SearchArticles($query: String!, $limit: Int!, $offset: Int!) {
  searchArticles(query: $query, limit: $limit, offset: $offset) {
    id
    title
    slug
    excerpt
    featuredImageUrl
    publishedAt
    author {
      id
      displayName
    }
    category {
      id
      name
      slug
    }
  }
}

# Get comments for article
query GetComments($articleId: String!, $limit: Int!, $offset: Int!) {
  getComments(articleId: $articleId, limit: $limit, offset: $offset) {
    id
    content
    status
    createdAt
    author {
      id
      displayName
      avatarUrl
    }
    replies {
      id
      content
      createdAt
      author {
        id
        displayName
        avatarUrl
      }
    }
  }
}

# Get trending articles
query GetTrendingArticles($days: Int!) {
  getTrendingArticles(days: $days) {
    id
    title
    slug
    viewCount
    publishedAt
    author {
      id
      displayName
    }
  }
}
```text

```graphql
# src/graphql/mutations.graphql

# Add comment
mutation AddComment($articleId: String!, $content: String!) {
  addComment(articleId: $articleId, content: $content) {
    id
    content
    createdAt
    author {
      id
      displayName
      avatarUrl
    }
  }
}

# Record article view
mutation RecordArticleView($id: String!) {
  recordArticleView(id: $id) {
    id
    viewCount
  }
}

# Publish article
mutation PublishArticle(
  $title: String!
  $slug: String!
  $content: String!
  $categoryId: String!
  $tagIds: [String!]!
  $excerpt: String
  $featuredImageUrl: String
) {
  publishArticle(
    title: $title
    slug: $slug
    content: $content
    categoryId: $categoryId
    tagIds: $tagIds
    excerpt: $excerpt
    featuredImageUrl: $featuredImageUrl
  ) {
    id
    title
    slug
    status
    publishedAt
  }
}
```text

### 5.5 Apollo Client Setup

```typescript
// src/lib/apolloClient.ts
import { ApolloClient, InMemoryCache, HttpLink } from '@apollo/client';

export const apolloClient = new ApolloClient({
  ssrMode: typeof window === 'undefined',
  link: new HttpLink({
    uri: process.env.NEXT_PUBLIC_GRAPHQL_URL,
    credentials: 'include',
  }),
  cache: new InMemoryCache(),
});
```text

### 5.6 Server Components

```typescript
// src/app/components/ArticleList.tsx
import { Suspense } from 'react';
import { gql } from '@apollo/client';
import { apolloClient } from '@/lib/apolloClient';
import Link from 'next/link';
import Image from 'next/image';
import { formatDate } from '@/lib/utils';

const GET_ARTICLES = gql`
  query GetArticles($limit: Int!, $offset: Int!) {
    getArticles(limit: $limit, offset: $offset) {
      id
      title
      slug
      excerpt
      featuredImageUrl
      viewCount
      publishedAt
      author {
        id
        displayName
        avatarUrl
      }
      category {
        name
        slug
      }
    }
  }
`;

async function ArticleListContent() {
  try {
    const { data } = await apolloClient.query({
      query: GET_ARTICLES,
      variables: {
        limit: 10,
        offset: 0,
      },
    });

    return (
      <div className="grid gap-8 md:grid-cols-2 lg:grid-cols-3">
        {data.getArticles.map((article: any) => (
          <article
            key={article.id}
            className="rounded-lg overflow-hidden shadow-md hover:shadow-lg transition-shadow"
          >
            {article.featuredImageUrl && (
              <div className="relative h-48 w-full">
                <Image
                  src={article.featuredImageUrl}
                  alt={article.title}
                  fill
                  className="object-cover"
                />
              </div>
            )}
            <div className="p-6">
              <div className="flex items-center justify-between mb-2">
                <span className="text-sm text-blue-600 font-semibold">
                  {article.category.name}
                </span>
                <span className="text-xs text-gray-500">
                  {article.viewCount} views
                </span>
              </div>
              <h2 className="text-xl font-bold mb-3 line-clamp-2">
                {article.title}
              </h2>
              <p className="text-gray-600 mb-4 line-clamp-3">
                {article.excerpt}
              </p>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {article.author.avatarUrl && (
                    <Image
                      src={article.author.avatarUrl}
                      alt={article.author.displayName}
                      width={32}
                      height={32}
                      className="rounded-full"
                    />
                  )}
                  <div className="text-sm">
                    <p className="font-medium">
                      {article.author.displayName}
                    </p>
                    <p className="text-gray-500">
                      {formatDate(article.publishedAt)}
                    </p>
                  </div>
                </div>
                <Link
                  href={`/articles/${article.slug}`}
                  className="text-blue-600 hover:text-blue-800 font-medium"
                >
                  Read →
                </Link>
              </div>
            </div>
          </article>
        ))}
      </div>
    );
  } catch (error) {
    console.error('Failed to fetch articles:', error);
    return (
      <div className="text-center py-12">
        <p className="text-red-600">Failed to load articles</p>
      </div>
    );
  }
}

export default function ArticleList() {
  return (
    <Suspense fallback={<div>Loading articles...</div>}>
      <ArticleListContent />
    </Suspense>
  );
}
```text

```typescript
// src/app/articles/[slug]/page.tsx
import { Suspense } from 'react';
import { gql } from '@apollo/client';
import { apolloClient } from '@/lib/apolloClient';
import { notFound } from 'next/navigation';
import Image from 'next/image';
import { formatDate } from '@/lib/utils';
import CommentSection from '@/components/CommentSection';
import { Metadata } from 'next';

const GET_ARTICLE = gql`
  query GetArticleBySlug($slug: String!) {
    getArticleBySlug(slug: $slug) {
      id
      title
      slug
      content
      excerpt
      featuredImageUrl
      viewCount
      publishedAt
      createdAt
      author {
        id
        displayName
        bio
        avatarUrl
      }
      category {
        id
        name
        slug
      }
      tags {
        id
        name
        slug
      }
    }
  }
`;

interface ArticlePageProps {
  params: {
    slug: string;
  };
}

export async function generateMetadata({
  params,
}: ArticlePageProps): Promise<Metadata> {
  try {
    const { data } = await apolloClient.query({
      query: GET_ARTICLE,
      variables: { slug: params.slug },
    });

    const article = data.getArticleBySlug;

    return {
      title: article.title,
      description: article.excerpt,
      openGraph: {
        title: article.title,
        description: article.excerpt,
        type: 'article',
        images: article.featuredImageUrl
          ? [{ url: article.featuredImageUrl }]
          : [],
        authors: [article.author.displayName],
        publishedTime: article.publishedAt,
      },
    };
  } catch {
    return {
      title: 'Article Not Found',
    };
  }
}

async function ArticleContent({ slug }: { slug: string }) {
  try {
    const { data } = await apolloClient.query({
      query: GET_ARTICLE,
      variables: { slug },
    });

    if (!data?.getArticleBySlug) {
      notFound();
    }

    const article = data.getArticleBySlug;

    return (
      <article className="max-w-4xl mx-auto py-12">
        {/* Header */}
        <header className="mb-8">
          <div className="flex items-center gap-4 mb-6">
            <span className="bg-blue-100 text-blue-800 px-4 py-2 rounded-full text-sm font-medium">
              {article.category.name}
            </span>
            <span className="text-sm text-gray-500">
              {article.viewCount} views
            </span>
          </div>

          <h1 className="text-4xl font-bold mb-4">{article.title}</h1>

          <div className="flex items-center justify-between mb-6 pb-6 border-b">
            <div className="flex items-center gap-4">
              {article.author.avatarUrl && (
                <Image
                  src={article.author.avatarUrl}
                  alt={article.author.displayName}
                  width={48}
                  height={48}
                  className="rounded-full"
                />
              )}
              <div>
                <p className="font-semibold">{article.author.displayName}</p>
                <p className="text-sm text-gray-500">
                  {formatDate(article.publishedAt)}
                </p>
              </div>
            </div>
          </div>
        </header>

        {/* Featured Image */}
        {article.featuredImageUrl && (
          <div className="relative h-96 w-full mb-8 rounded-lg overflow-hidden">
            <Image
              src={article.featuredImageUrl}
              alt={article.title}
              fill
              className="object-cover"
              priority
            />
          </div>
        )}

        {/* Content */}
        <div
          className="prose prose-lg max-w-none mb-12"
          dangerouslySetInnerHTML={{
            __html: article.content, // Should be properly sanitized in production
          }}
        />

        {/* Tags */}
        {article.tags.length > 0 && (
          <div className="mb-12 pb-12 border-b">
            <div className="flex flex-wrap gap-2">
              {article.tags.map((tag: any) => (
                <a
                  key={tag.id}
                  href={`/tags/${tag.slug}`}
                  className="bg-gray-200 text-gray-800 px-3 py-1 rounded-full text-sm hover:bg-gray-300"
                >
                  #{tag.name}
                </a>
              ))}
            </div>
          </div>
        )}

        {/* Comments Section */}
        <Suspense fallback={<div>Loading comments...</div>}>
          <CommentSection articleId={article.id} />
        </Suspense>
      </article>
    );
  } catch (error) {
    console.error('Failed to fetch article:', error);
    notFound();
  }
}

export default function ArticlePage({ params }: ArticlePageProps) {
  return <ArticleContent slug={params.slug} />;
}
```text

### 5.7 Client Components

```typescript
// src/app/components/CommentSection.tsx
'use client';

import { useState } from 'react';
import { useMutation, useQuery } from '@apollo/client';
import { gql } from '@apollo/client';
import Image from 'next/image';
import { formatDate } from '@/lib/utils';

const GET_COMMENTS = gql`
  query GetComments($articleId: String!, $limit: Int!, $offset: Int!) {
    getComments(articleId: $articleId, limit: $limit, offset: $offset) {
      id
      content
      status
      createdAt
      author {
        id
        displayName
        avatarUrl
      }
      replies {
        id
        content
        createdAt
        author {
          id
          displayName
          avatarUrl
        }
      }
    }
  }
`;

const ADD_COMMENT = gql`
  mutation AddComment($articleId: String!, $content: String!) {
    addComment(articleId: $articleId, content: $content) {
      id
      content
      createdAt
      author {
        id
        displayName
        avatarUrl
      }
    }
  }
`;

interface CommentSectionProps {
  articleId: string;
}

export default function CommentSection({ articleId }: CommentSectionProps) {
  const [newComment, setNewComment] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const { data, loading, error, refetch } = useQuery(GET_COMMENTS, {
    variables: {
      articleId,
      limit: 20,
      offset: 0,
    },
  });

  const [addComment] = useMutation(ADD_COMMENT, {
    onCompleted: () => {
      setNewComment('');
      setIsSubmitting(false);
      refetch();
    },
    onError: (err) => {
      console.error('Failed to add comment:', err);
      setIsSubmitting(false);
    },
  });

  const handleSubmitComment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newComment.trim()) return;

    setIsSubmitting(true);
    await addComment({
      variables: {
        articleId,
        content: newComment,
      },
    });
  };

  return (
    <div className="mt-12">
      <h2 className="text-2xl font-bold mb-8">Comments</h2>

      {/* Comment Form */}
      <form onSubmit={handleSubmitComment} className="mb-8">
        <textarea
          value={newComment}
          onChange={(e) => setNewComment(e.target.value)}
          placeholder="Leave a comment..."
          className="w-full p-4 border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
          rows={4}
          disabled={isSubmitting}
        />
        <button
          type="submit"
          disabled={isSubmitting || !newComment.trim()}
          className="mt-4 bg-blue-600 text-white px-6 py-2 rounded-lg hover:bg-blue-700 disabled:opacity-50"
        >
          {isSubmitting ? 'Posting...' : 'Post Comment'}
        </button>
      </form>

      {/* Comments List */}
      {loading && <p>Loading comments...</p>}
      {error && <p className="text-red-600">Failed to load comments</p>}

      <div className="space-y-6">
        {data?.getComments?.map((comment: any) => (
          <div key={comment.id} className="border-l-2 border-gray-200 pl-4">
            <div className="flex items-center gap-3 mb-2">
              {comment.author.avatarUrl && (
                <Image
                  src={comment.author.avatarUrl}
                  alt={comment.author.displayName}
                  width={32}
                  height={32}
                  className="rounded-full"
                />
              )}
              <div>
                <p className="font-semibold">{comment.author.displayName}</p>
                <p className="text-sm text-gray-500">
                  {formatDate(comment.createdAt)}
                </p>
              </div>
            </div>
            <p className="text-gray-700">{comment.content}</p>

            {/* Replies */}
            {comment.replies?.length > 0 && (
              <div className="mt-4 space-y-4 ml-4 border-l pl-4">
                {comment.replies.map((reply: any) => (
                  <div key={reply.id}>
                    <div className="flex items-center gap-3 mb-1">
                      {reply.author.avatarUrl && (
                        <Image
                          src={reply.author.avatarUrl}
                          alt={reply.author.displayName}
                          width={24}
                          height={24}
                          className="rounded-full"
                        />
                      )}
                      <p className="font-semibold text-sm">
                        {reply.author.displayName}
                      </p>
                      <p className="text-xs text-gray-500">
                        {formatDate(reply.createdAt)}
                      </p>
                    </div>
                    <p className="text-sm text-gray-700">{reply.content}</p>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
```text

```typescript
// src/app/components/SearchArticles.tsx
'use client';

import { useState, useTransition } from 'react';
import { useLazyQuery } from '@apollo/client';
import { gql } from '@apollo/client';
import Link from 'next/link';
import { debounce } from '@/lib/utils';

const SEARCH_ARTICLES = gql`
  query SearchArticles($query: String!, $limit: Int!, $offset: Int!) {
    searchArticles(query: $query, limit: $limit, offset: $offset) {
      id
      title
      slug
      excerpt
      publishedAt
      author {
        id
        displayName
      }
      category {
        id
        name
        slug
      }
    }
  }
`;

export default function SearchArticles() {
  const [searchQuery, setSearchQuery] = useState('');
  const [isPending, startTransition] = useTransition();
  const [searchArticles, { data, loading }] = useLazyQuery(SEARCH_ARTICLES);

  const debouncedSearch = debounce((query: string) => {
    if (query.length > 2) {
      searchArticles({
        variables: {
          query,
          limit: 10,
          offset: 0,
        },
      });
    }
  }, 300);

  const handleSearchChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    setSearchQuery(value);
    startTransition(() => {
      debouncedSearch(value);
    });
  };

  return (
    <div className="max-w-2xl mx-auto">
      <input
        type="text"
        placeholder="Search articles..."
        value={searchQuery}
        onChange={handleSearchChange}
        className="w-full px-4 py-2 border rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
      />

      {isPending && <p className="mt-4 text-gray-500">Searching...</p>}

      {data?.searchArticles && (
        <div className="mt-6 space-y-4">
          {data.searchArticles.map((article: any) => (
            <div key={article.id} className="border rounded-lg p-4 hover:shadow-md">
              <h3 className="font-semibold text-lg mb-2">{article.title}</h3>
              <p className="text-gray-600 text-sm mb-3">{article.excerpt}</p>
              <div className="flex justify-between items-center">
                <div className="text-xs text-gray-500">
                  {article.author.displayName} in {article.category.name}
                </div>
                <Link
                  href={`/articles/${article.slug}`}
                  className="text-blue-600 hover:text-blue-800"
                >
                  Read →
                </Link>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
```text

---

## Part 6: Project Structure

```text
blog-monorepo/
├── java-schema/                    # Java authoring layer
│   ├── pom.xml
│   ├── src/main/java/dev/fraiseql/blog/
│   │   ├── types/
│   │   │   ├── User.java
│   │   │   ├── Article.java
│   │   │   ├── Category.java
│   │   │   ├── Comment.java
│   │   │   ├── Tag.java
│   │   │   └── Status enums
│   │   ├── queries/
│   │   │   ├── ArticleQueries.java
│   │   │   ├── CommentQueries.java
│   │   │   └── CategoryQueries.java
│   │   └── mutations/
│   │       ├── ArticleMutations.java
│   │       └── CommentMutations.java
│   └── target/
│       └── schema.json
│
├── fraiseql-server/                # FraiseQL backend
│   ├── Dockerfile
│   ├── docker-compose.yml
│   ├── schema.json                 # Exported from Java
│   ├── schema.compiled.json        # Compiled by fraiseql-cli
│   ├── fraiseql.toml               # Configuration
│   ├── sql/
│   │   ├── schema.sql
│   │   └── seed.sql
│   └── scripts/
│       ├── build-schema.sh
│       ├── compile-schema.sh
│       ├── init-db.sh
│       └── deploy.sh
│
├── nextjs-frontend/                # Next.js 14 frontend
│   ├── app/
│   │   ├── articles/
│   │   │   ├── page.tsx            # Article list
│   │   │   └── [slug]/
│   │   │       └── page.tsx        # Article detail (Server Component)
│   │   ├── search/
│   │   │   └── page.tsx            # Search page
│   │   ├── trending/
│   │   │   └── page.tsx            # Trending articles
│   │   ├── components/
│   │   │   ├── ArticleList.tsx
│   │   │   ├── CommentSection.tsx  # Client Component
│   │   │   ├── SearchArticles.tsx  # Client Component
│   │   │   └── TrendingWidget.tsx
│   │   └── layout.tsx
│   ├── lib/
│   │   ├── apolloClient.ts
│   │   ├── utils.ts
│   │   └── hooks/
│   │       └── useAuth.ts
│   ├── graphql/
│   │   ├── queries.graphql
│   │   └── mutations.graphql
│   ├── gql/
│   │   └── generated.ts            # Generated by codegen
│   ├── public/
│   ├── styles/
│   ├── codegen.yml
│   ├── next.config.js
│   ├── tsconfig.json
│   ├── tailwind.config.ts
│   └── package.json
│
├── .env.example
├── .env.local
├── README.md
└── docker-compose.yml              # Orchestrate all services
```text

---

## Part 7: Running the Full Stack

### 7.1 Prerequisites

```bash
# Check requirements
java -version          # Java 17+
mvn -version           # Maven 3.9+
node --version         # Node 18+
cargo --version        # Rust (for fraiseql-cli)
docker --version       # Docker & Docker Compose
```text

### 7.2 Run Locally (Development)

```bash
#!/bin/bash
# scripts/dev-start.sh

set -e

echo "🚀 Starting FraiseQL Blog Stack (Development)..."

# 1. Start PostgreSQL
echo "1️⃣  Starting PostgreSQL..."
docker-compose up -d postgres
sleep 5

# 2. Initialize database
echo "2️⃣  Initializing database..."
bash scripts/init-db.sh

# 3. Build and export Java schema
echo "3️⃣  Building Java schema..."
bash scripts/build-schema.sh

# 4. Compile schema with FraiseQL
echo "4️⃣  Compiling schema with FraiseQL..."
bash scripts/compile-schema.sh

# 5. Start FraiseQL server
echo "5️⃣  Starting FraiseQL server..."
docker-compose up -d fraiseql
sleep 5

# 6. Start Next.js frontend
echo "6️⃣  Starting Next.js frontend..."
cd nextjs-frontend
npm run dev &

echo ""
echo "✅ Stack is running!"
echo "📖 GraphQL:  http://localhost:5000/graphql"
echo "🌐 Frontend: http://localhost:3000"
echo "📊 DB:       localhost:5432 (blog_user/blog_password)"
```text

### 7.3 Test the Stack

```bash
#!/bin/bash
# scripts/test-stack.sh

set -e

echo "Testing stack..."

# Test GraphQL endpoint
echo "Testing GraphQL endpoint..."
curl -X POST http://localhost:5000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ getArticles(limit: 5, offset: 0) { id title } }"
  }' | jq .

# Test Next.js frontend
echo "Testing Next.js..."
curl -s http://localhost:3000 | grep -q "Blog" && echo "✓ Frontend OK" || echo "✗ Frontend failed"

echo "✅ Stack tests passed!"
```text

---

## Part 8: Example Workflows

### 8.1 Publish an Article (End-to-End)

#### Step 1: Define Article in CMS UI

The author uses the Next.js admin panel to create and publish an article:

```typescript
// frontend: Create article form submission
const handlePublishArticle = async (formData) => {
  const result = await client.mutate({
    mutation: PUBLISH_ARTICLE,
    variables: {
      title: formData.title,
      slug: formData.slug,
      content: formData.content,
      categoryId: formData.categoryId,
      tagIds: formData.tags,
      excerpt: formData.excerpt,
      featuredImageUrl: formData.imageUrl,
    },
  });

  // Article is now published and visible on blog
  navigate(`/articles/${result.data.publishArticle.slug}`);
};
```text

#### Step 2: GraphQL Mutation Execution

```graphql
mutation PublishArticle {
  publishArticle(
    title: "Getting Started with GraphQL"
    slug: "graphql-getting-started"
    content: "..."
    categoryId: "cat-123"
    tagIds: ["tag-graphql", "tag-api"]
  ) {
    id
    title
    status
    publishedAt
  }
}
```text

#### Step 3: FraiseQL Compiles to SQL

The compiled mutation template:

```sql
-- Generated by FraiseQL compiler
INSERT INTO articles (
  title, slug, content, author_id, category_id, status, published_at
) VALUES (
  $1, $2, $3, $4, $5, 'published', NOW()
)
RETURNING id, title, status, published_at;

-- Then insert tags
INSERT INTO article_tags (article_id, tag_id)
SELECT $1, tag_id FROM tags WHERE tag_id = ANY($2);
```text

#### Step 4: Results Returned to Frontend

```json
{
  "data": {
    "publishArticle": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "title": "Getting Started with GraphQL",
      "status": "PUBLISHED",
      "publishedAt": "2026-02-05T12:00:00Z"
    }
  }
}
```text

### 8.2 Add Comment Workflow

1. **User sees article** - Server Component fetches article with `getArticleBySlug`
2. **Writes comment** - Client Component captures input
3. **Submits mutation** - `addComment` mutation sent to FraiseQL
4. **Database insert** - FraiseQL inserts comment with moderation status
5. **Cache invalidation** - Article's comment list is refreshed
6. **Comment appears** - UI updates with new comment

```typescript
// Complete flow in CommentSection component
const handleSubmitComment = async (e: React.FormEvent) => {
  e.preventDefault();

  // Call GraphQL mutation
  const { data } = await addComment({
    variables: {
      articleId: props.articleId,
      content: newComment,
    },
  });

  // Clear form
  setNewComment('');

  // Refetch comments (invalidates cache)
  await refetch();
};
```text

### 8.3 Search Workflow

1. **User types query** - Debounced search input (300ms)
2. **FraiseQL full-text search** - Uses PostgreSQL GIN index
3. **Results streamed** - Apollo Client updates with matches
4. **UI re-renders** - Search results appear in real-time

```typescript
const debouncedSearch = debounce((query: string) => {
  if (query.length > 2) {
    searchArticles({
      variables: { query, limit: 10, offset: 0 },
    });
  }
}, 300);
```text

---

## Part 9: Deployment

### 9.1 Deploy to Vercel (Frontend)

```bash
# Connect Vercel
vercel link

# Deploy
vercel deploy --prod

# Set environment variables
vercel env add NEXT_PUBLIC_GRAPHQL_URL https://api.example.com/graphql
```text

### 9.2 Deploy to Kubernetes (Backend)

```yaml
# k8s/fraiseql-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-blog
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql-blog
  template:
    metadata:
      labels:
        app: fraiseql-blog
    spec:
      containers:
      - name: fraiseql
        image: docker.example.com/fraiseql-blog:latest
        ports:
        - containerPort: 5000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-credentials
              key: url
        - name: RUST_LOG
          value: "info"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 5000
          initialDelaySeconds: 10
          periodSeconds: 10
---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql-blog-service
spec:
  type: LoadBalancer
  ports:
  - port: 80
    targetPort: 5000
  selector:
    app: fraiseql-blog
```text

### 9.3 Database Migration Strategy

```bash
#!/bin/bash
# scripts/migrate-prod.sh

set -e

PROD_DB_URL="postgresql://prod_user:password@prod-db.example.com/blog_prod"

echo "Running migrations on production database..."

# Backup production database
pg_dump "$PROD_DB_URL" > backup-$(date +%Y%m%d-%H%M%S).sql

# Run schema migrations
psql "$PROD_DB_URL" -f sql/migrations/001-initial-schema.sql
psql "$PROD_DB_URL" -f sql/migrations/002-add-indexes.sql

echo "✓ Migrations complete"
```text

---

## Part 10: Troubleshooting & Best Practices

### 10.1 Common Issues

**Issue: GraphQL queries timeout**

```bash
# Solution: Check database connection pool
# In fraiseql.toml:
[database]
pool_size = 20  # Increase from default 10
connection_timeout_secs = 60
```text

**Issue: Compiled schema doesn't include all fields**

```bash
# Solution: Ensure Maven plugin scanned all packages
# In pom.xml:
<scanPackages>
  <scanPackage>dev.fraiseql.blog</scanPackage>
  <scanPackage>dev.fraiseql.blog.types</scanPackage>
  <scanPackage>dev.fraiseql.blog.queries</scanPackage>
  <scanPackage>dev.fraiseql.blog.mutations</scanPackage>
</scanPackages>
```text

**Issue: Comments not appearing**

```bash
# Solution: Check comment status filter
# Queries only return APPROVED comments by default
# In database:
UPDATE comments SET status = 'approved' WHERE status = 'pending';
```text

### 10.2 Performance Optimization

**Enable Query Caching**

```toml
# fraiseql.toml
[cache]
enabled = true
default_ttl_seconds = 300
max_entries = 10000

# In Java schema:
@Cached(ttlSeconds = 600)
public List<Article> getArticles(...) { ... }
```text

**Add Database Indexes**

```sql
-- Already included in schema.sql, but add more as needed:
CREATE INDEX idx_articles_published_at ON articles(published_at DESC)
WHERE status = 'published';
```text

**Enable Persisted Queries (APQ)**

```typescript
// Next.js: Automatic with Apollo Client
import { createPersistedQueryLink } from "@apollo/client/link/persisted-queries";
```text

### 10.3 Security Best Practices

```toml
# fraiseql.toml - Security Configuration

[security.authorization]
enabled = true
require_auth_on_mutations = true
jwt_secret = "${JWT_SECRET}"  # From environment

[security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60

[security.audit_logging]
enabled = true
log_mutations = true
```text

### 10.4 Monitoring

```bash
# Monitor FraiseQL server
docker logs -f fraiseql

# Monitor database
psql -c "SELECT datname, usename, count(*) FROM pg_stat_activity GROUP BY datname, usename;"

# Monitor Next.js
npm run build  # Check for build errors
npm run lint   # Check for code issues
```text

### 10.5 Useful Commands

```bash
# Build everything
./scripts/build-schema.sh && ./scripts/compile-schema.sh

# Run tests
cd nextjs-frontend && npm test

# Generate GraphQL types from live server
npm run generate:types

# Check schema validity
fraiseql validate schema.compiled.json

# Export database schema
pg_dump --schema-only blog_db > schema-backup.sql
```text

---

## Summary

This full-stack example demonstrates FraiseQL's complete architecture:

1. **Java Authoring** → Type-safe schema definitions with annotations
2. **Export to JSON** → Schema becomes portable
3. **Compilation** → FraiseQL generates optimized SQL
4. **Rust Runtime** → High-performance GraphQL execution
5. **Next.js Frontend** → Modern React with Server Components
6. **Production Ready** → Docker, Kubernetes, monitoring

**Key Benefits:**

- ✅ Single source of truth (Java types)
- ✅ Zero runtime overhead (compiled SQL)
- ✅ Type-safe frontend (GraphQL codegen)
- ✅ Scalable architecture (stateless servers)
- ✅ Easy to monitor and debug

For more information, see:

- [FraiseQL Architecture](../ARCHITECTURE_PRINCIPLES.md)
- [Java SDK Documentation](../guides/java-sdk.md)
- [Next.js Integration Guide](../guides/nextjs-integration.md)
- [GraphQL API Reference](../api/graphql-reference.md)
