# Building a Blog API with FraiseQL: TypeScript Schema Authoring Tutorial

**Audience:** TypeScript developers, schema designers, API builders
**Prerequisite:** Node.js 18+, TypeScript 5.0+, PostgreSQL 14+
**Reading Time:** 30-40 minutes
**Hands-On Time:** 45-60 minutes

---

## Overview

In this tutorial, we'll build a production-ready Blog API using FraiseQL's TypeScript schema authoring layer. You'll learn:

1. **Schema design principles** - How to structure GraphQL types for a blog
2. **TypeScript decorators** - Using `@Type`, `@Query`, `@Mutation` to define your API
3. **Type system** - Modeling relationships, optionals, arrays, and scalars
4. **Schema export** - Generating `schema.json` from TypeScript
5. **Compilation** - Converting to `schema.compiled.json` with the FraiseQL CLI
6. **Deployment** - Running a GraphQL server from your compiled schema
7. **Testing** - Executing queries against your API

**What we're building:** A blog platform with users, posts, and comments.

**Architecture:**

```text
TypeScript Schema        → schema.json        → schema.compiled.json → FraiseQL Server
(@Type, @Query, @Mutation)  (decorators)      (FraiseQL-cli)        (GraphQL API)
```text

---

## Part 1: Database Schema

Before writing GraphQL types, we need the underlying database schema. FraiseQL compiles to SQL, so understanding your data structure is essential.

### 1.1 PostgreSQL Schema (DDL)

Create the following tables in PostgreSQL:

```sql
-- Users table: Core user accounts
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Posts table: Blog posts created by users
CREATE TABLE posts (
    id BIGSERIAL PRIMARY KEY,
    title VARCHAR(500) NOT NULL,
    content TEXT NOT NULL,
    author_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    published_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Comments table: Comments on posts
CREATE TABLE comments (
    id BIGSERIAL PRIMARY KEY,
    text TEXT NOT NULL,
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for common queries
CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_posts_published ON posts(published_at DESC);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);
```text

### 1.2 Database Views (for GraphQL Queries)

FraiseQL queries typically execute against views, which provide data access boundaries and can include business logic:

```sql
-- View: Get all users with post count
CREATE VIEW v_user AS
SELECT
    u.id,
    u.email,
    u.name,
    u.bio,
    u.created_at,
    u.updated_at,
    COUNT(DISTINCT p.id) AS post_count
FROM users u
LEFT JOIN posts p ON u.id = p.author_id
GROUP BY u.id;

-- View: Get all published posts with author info
CREATE VIEW v_post AS
SELECT
    p.id,
    p.title,
    p.content,
    p.author_id,
    p.published_at,
    p.created_at,
    p.updated_at,
    u.email AS author_email,
    u.name AS author_name,
    COUNT(DISTINCT c.id) AS comment_count
FROM posts p
JOIN users u ON p.author_id = u.id
LEFT JOIN comments c ON p.id = c.post_id
WHERE p.published_at IS NOT NULL
GROUP BY p.id, u.id;

-- View: Get all comments with author info
CREATE VIEW v_comment AS
SELECT
    c.id,
    c.text,
    c.post_id,
    c.author_id,
    c.created_at,
    u.email AS author_email,
    u.name AS author_name
FROM comments c
JOIN users u ON c.author_id = u.id;
```text

### 1.3 Database Procedures (for Mutations)

FraiseQL mutations execute stored procedures that handle CREATE, UPDATE, DELETE operations:

```sql
-- Function: Create a new user
CREATE OR REPLACE FUNCTION fn_create_user(
    p_email VARCHAR,
    p_name VARCHAR,
    p_bio TEXT DEFAULT NULL
)
RETURNS SETOF users AS $$
INSERT INTO users (email, name, bio)
VALUES (p_email, p_name, p_bio)
ON CONFLICT (email) DO NOTHING
RETURNING *;
$$ LANGUAGE SQL;

-- Function: Create a new post
CREATE OR REPLACE FUNCTION fn_create_post(
    p_title VARCHAR,
    p_content TEXT,
    p_author_id BIGINT,
    p_published BOOLEAN DEFAULT FALSE
)
RETURNS SETOF posts AS $$
INSERT INTO posts (title, content, author_id, published_at)
VALUES (
    p_title,
    p_content,
    p_author_id,
    CASE WHEN p_published THEN CURRENT_TIMESTAMP ELSE NULL END
)
RETURNING *;
$$ LANGUAGE SQL;

-- Function: Create a new comment
CREATE OR REPLACE FUNCTION fn_create_comment(
    p_text TEXT,
    p_post_id BIGINT,
    p_author_id BIGINT
)
RETURNS SETOF comments AS $$
INSERT INTO comments (text, post_id, author_id)
VALUES (p_text, p_post_id, p_author_id)
RETURNING *;
$$ LANGUAGE SQL;

-- Function: Update a user
CREATE OR REPLACE FUNCTION fn_update_user(
    p_id BIGINT,
    p_name VARCHAR DEFAULT NULL,
    p_bio TEXT DEFAULT NULL
)
RETURNS SETOF users AS $$
UPDATE users
SET
    name = COALESCE(p_name, name),
    bio = COALESCE(p_bio, bio),
    updated_at = CURRENT_TIMESTAMP
WHERE id = p_id
RETURNING *;
$$ LANGUAGE SQL;
```text

---

## Part 2: TypeScript Project Setup

### 2.1 Project Structure

Create a new TypeScript project:

```bash
mkdir blog-api && cd blog-api
npm init -y
npm install --save-dev typescript ts-node @types/node
npm install FraiseQL

# Create src directory
mkdir src
```text

### 2.2 TypeScript Configuration

Create `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```text

**Key settings:**

- `experimentalDecorators: true` - Required for `@Type`, `@Query`, `@Mutation` decorators
- `emitDecoratorMetadata: true` - Preserves metadata for reflection
- `target: ES2020` - Modern JavaScript features
- `strict: true` - Full type checking (recommended)

### 2.3 Package Configuration

Create `package.json` scripts:

```json
{
  "name": "blog-api",
  "version": "1.0.0",
  "scripts": {
    "export": "ts-node src/schema.ts",
    "compile": "FraiseQL compile FraiseQL.toml",
    "build": "npm run export && npm run compile",
    "dev": "FraiseQL serve schema.compiled.json --port 4000",
    "test": "ts-node src/tests.ts"
  },
  "dependencies": {
    "FraiseQL": "^2.0.0"
  },
  "devDependencies": {
    "@types/node": "^20.0.0",
    "typescript": "^5.0.0",
    "ts-node": "^10.9.0"
  }
}
```text

---

## Part 3: FraiseQL Schema Definition

### 3.1 Creating the Schema File

Create `src/schema.ts`:

```typescript
/**
 * Blog API Schema - TypeScript Authoring
 *
 * This file defines the GraphQL schema using FraiseQL decorators.
 * It generates schema.json which is then compiled by FraiseQL-cli.
 */

import * as FraiseQL from "FraiseQL";

// ============================================================================
// TYPES: Define the shape of data returned by queries
// ============================================================================

/**
 * User type: Represents a user account
 *
 * Fields:
 * - id: Unique identifier (primary key from database)
 * - email: User's email address (unique)
 * - name: User's display name
 * - bio: Optional biography
 * - postCount: Computed count of user's posts
 * - createdAt: Account creation timestamp
 * - updatedAt: Last update timestamp
 */
@FraiseQL.Type({
  description: "A user account in the blog system"
})
class User {
  id: number;
  email: string;
  name: string;
  bio?: string;  // Optional field (nullable in GraphQL)
  postCount: number;
  createdAt: Date;
  updatedAt: Date;
}

/**
 * Post type: Represents a blog post
 *
 * Fields:
 * - id: Unique identifier
 * - title: Post title
 * - content: Post content (markdown)
 * - authorId: Foreign key to User
 * - publishedAt: Publication timestamp (null if draft)
 * - commentCount: Computed count of comments
 * - createdAt: Creation timestamp
 * - updatedAt: Last update timestamp
 */
@FraiseQL.Type({
  description: "A blog post"
})
class Post {
  id: number;
  title: string;
  content: string;
  authorId: number;
  publishedAt?: Date;  // Null for draft posts
  commentCount: number;
  createdAt: Date;
  updatedAt: Date;
}

/**
 * Comment type: Represents a comment on a post
 *
 * Fields:
 * - id: Unique identifier
 * - text: Comment content
 * - postId: Foreign key to Post
 * - authorId: Foreign key to User
 * - authorEmail: Author's email (denormalized from User)
 * - authorName: Author's name (denormalized from User)
 * - createdAt: Creation timestamp
 */
@FraiseQL.Type({
  description: "A comment on a blog post"
})
class Comment {
  id: number;
  text: string;
  postId: number;
  authorId: number;
  authorEmail: string;
  authorName: string;
  createdAt: Date;
}

// ============================================================================
// REGISTER TYPE FIELDS: Map TypeScript properties to GraphQL types
// ============================================================================

/**
 * Register User fields.
 *
 * Maps TypeScript property names to GraphQL types:
 * - Scalar types: String, Int, Float, Boolean, ID, DateTime, Decimal, JSON
 * - nullable: true means field can be null (marked with ? in TypeScript)
 * - Database column: Maps to database field name
 */
FraiseQL.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false, description: "Primary key" },
  { name: "email", type: "String", nullable: false, description: "User email" },
  { name: "name", type: "String", nullable: false, description: "User name" },
  { name: "bio", type: "String", nullable: true, description: "User biography" },
  { name: "postCount", type: "Int", nullable: false, description: "Number of posts by this user" },
  { name: "createdAt", type: "DateTime", nullable: false, description: "Account created" },
  { name: "updatedAt", type: "DateTime", nullable: false, description: "Account last updated" }
]);

/**
 * Register Post fields.
 *
 * Note: publishedAt is nullable (representing draft posts)
 */
FraiseQL.registerTypeFields("Post", [
  { name: "id", type: "ID", nullable: false, description: "Primary key" },
  { name: "title", type: "String", nullable: false, description: "Post title" },
  { name: "content", type: "String", nullable: false, description: "Post content" },
  { name: "authorId", type: "ID", nullable: false, description: "Author user ID" },
  { name: "publishedAt", type: "DateTime", nullable: true, description: "Publication date" },
  { name: "commentCount", type: "Int", nullable: false, description: "Number of comments" },
  { name: "createdAt", type: "DateTime", nullable: false, description: "Post created" },
  { name: "updatedAt", type: "DateTime", nullable: false, description: "Post last updated" }
]);

/**
 * Register Comment fields.
 */
FraiseQL.registerTypeFields("Comment", [
  { name: "id", type: "ID", nullable: false, description: "Primary key" },
  { name: "text", type: "String", nullable: false, description: "Comment text" },
  { name: "postId", type: "ID", nullable: false, description: "Post this comments on" },
  { name: "authorId", type: "ID", nullable: false, description: "Author user ID" },
  { name: "authorEmail", type: "String", nullable: false, description: "Author email" },
  { name: "authorName", type: "String", nullable: false, description: "Author name" },
  { name: "createdAt", type: "DateTime", nullable: false, description: "Comment created" }
]);

// ============================================================================
// QUERIES: Read operations
// ============================================================================

/**
 * Query: Get all users
 *
 * Executes against v_user view.
 * Supports optional filtering:
 * - limit: Max number of results (default 100)
 * - offset: Skip first N results (default 0)
 *
 * Returns: Array of User objects
 */
FraiseQL.registerQuery(
  "users",           // Query name
  "User",            // Return type
  true,              // Returns list (array)
  false,             // Not nullable (always returns array, may be empty)
  [
    {
      name: "limit",
      type: "Int",
      nullable: false,
      default: 100,
      description: "Maximum number of results"
    },
    {
      name: "offset",
      type: "Int",
      nullable: false,
      default: 0,
      description: "Skip first N results"
    }
  ],
  "Get all users with pagination",
  { sqlSource: "v_user" }
);

/**
 * Query: Get user by ID
 *
 * Executes against v_user view with ID filter.
 * Returns: Single User object or null
 */
FraiseQL.registerQuery(
  "userById",
  "User",
  false,             // Single result, not a list
  true,              // Can be null (user may not exist)
  [
    {
      name: "id",
      type: "ID",
      nullable: false,
      description: "User ID to fetch"
    }
  ],
  "Get a user by ID",
  { sqlSource: "v_user" }
);

/**
 * Query: Get user by email
 *
 * Executes against v_user view with email filter.
 * Returns: Single User object or null
 */
FraiseQL.registerQuery(
  "userByEmail",
  "User",
  false,
  true,
  [
    {
      name: "email",
      type: "String",
      nullable: false,
      description: "User email to fetch"
    }
  ],
  "Get a user by email address",
  { sqlSource: "v_user" }
);

/**
 * Query: Get all published posts
 *
 * Executes against v_post view (which filters for published_at IS NOT NULL).
 * Supports pagination and sorting.
 *
 * Returns: Array of Post objects
 */
FraiseQL.registerQuery(
  "posts",
  "Post",
  true,
  false,
  [
    {
      name: "limit",
      type: "Int",
      nullable: false,
      default: 50,
      description: "Maximum number of posts"
    },
    {
      name: "offset",
      type: "Int",
      nullable: false,
      default: 0,
      description: "Skip first N posts"
    }
  ],
  "Get all published posts with pagination",
  { sqlSource: "v_post" }
);

/**
 * Query: Get post by ID
 *
 * Returns: Single Post object or null
 */
FraiseQL.registerQuery(
  "postById",
  "Post",
  false,
  true,
  [
    {
      name: "id",
      type: "ID",
      nullable: false,
      description: "Post ID"
    }
  ],
  "Get a post by ID",
  { sqlSource: "v_post" }
);

/**
 * Query: Get posts by author
 *
 * Filters posts by author ID.
 * Returns: Array of Post objects from that author
 */
FraiseQL.registerQuery(
  "postsByAuthor",
  "Post",
  true,
  false,
  [
    {
      name: "authorId",
      type: "ID",
      nullable: false,
      description: "Author user ID"
    },
    {
      name: "limit",
      type: "Int",
      nullable: false,
      default: 50,
      description: "Maximum number of posts"
    }
  ],
  "Get all posts by a specific author",
  { sqlSource: "v_post" }
);

/**
 * Query: Get comments on a post
 *
 * Returns: Array of Comment objects for a specific post
 */
FraiseQL.registerQuery(
  "postComments",
  "Comment",
  true,
  false,
  [
    {
      name: "postId",
      type: "ID",
      nullable: false,
      description: "Post ID"
    }
  ],
  "Get all comments on a specific post",
  { sqlSource: "v_comment" }
);

// ============================================================================
// MUTATIONS: Write operations
// ============================================================================

/**
 * Mutation: Create user
 *
 * Calls fn_create_user stored procedure.
 * Returns: Newly created User object
 */
FraiseQL.registerMutation(
  "createUser",
  "User",
  false,             // Single result
  false,             // Not nullable
  [
    {
      name: "email",
      type: "String",
      nullable: false,
      description: "User email"
    },
    {
      name: "name",
      type: "String",
      nullable: false,
      description: "User name"
    },
    {
      name: "bio",
      type: "String",
      nullable: true,
      description: "User biography"
    }
  ],
  "Create a new user",
  { sqlSource: "fn_create_user", operation: "CREATE" }
);

/**
 * Mutation: Create post
 *
 * Calls fn_create_post stored procedure.
 * Returns: Newly created Post object
 */
FraiseQL.registerMutation(
  "createPost",
  "Post",
  false,
  false,
  [
    {
      name: "title",
      type: "String",
      nullable: false,
      description: "Post title"
    },
    {
      name: "content",
      type: "String",
      nullable: false,
      description: "Post content"
    },
    {
      name: "authorId",
      type: "ID",
      nullable: false,
      description: "Author user ID"
    },
    {
      name: "published",
      type: "Boolean",
      nullable: false,
      default: false,
      description: "Publish immediately?"
    }
  ],
  "Create a new blog post",
  { sqlSource: "fn_create_post", operation: "CREATE" }
);

/**
 * Mutation: Create comment
 *
 * Calls fn_create_comment stored procedure.
 * Returns: Newly created Comment object
 */
FraiseQL.registerMutation(
  "createComment",
  "Comment",
  false,
  false,
  [
    {
      name: "text",
      type: "String",
      nullable: false,
      description: "Comment text"
    },
    {
      name: "postId",
      type: "ID",
      nullable: false,
      description: "Post to comment on"
    },
    {
      name: "authorId",
      type: "ID",
      nullable: false,
      description: "Author user ID"
    }
  ],
  "Add a comment to a post",
  { sqlSource: "fn_create_comment", operation: "CREATE" }
);

/**
 * Mutation: Update user
 *
 * Calls fn_update_user stored procedure.
 * Returns: Updated User object
 */
FraiseQL.registerMutation(
  "updateUser",
  "User",
  false,
  true,              // Can be null if user doesn't exist
  [
    {
      name: "id",
      type: "ID",
      nullable: false,
      description: "User ID to update"
    },
    {
      name: "name",
      type: "String",
      nullable: true,
      description: "New name (optional)"
    },
    {
      name: "bio",
      type: "String",
      nullable: true,
      description: "New biography (optional)"
    }
  ],
  "Update user profile",
  { sqlSource: "fn_update_user", operation: "UPDATE" }
);

// ============================================================================
// EXPORT: Generate schema.json
// ============================================================================

/**
 * Export the schema to schema.json for compilation.
 *
 * This generates a JSON file containing all type definitions,
 * queries, and mutations. The FraiseQL CLI will compile this
 * into schema.compiled.json.
 */
async function exportSchema() {
  try {
    FraiseQL.exportTypes("schema.json", { pretty: true });
    console.log("✅ Schema exported successfully!");
    console.log("   Output: schema.json");
    console.log("   Types: 3 (User, Post, Comment)");
    console.log("   Queries: 6");
    console.log("   Mutations: 4");
    console.log("\n📝 Next steps:");
    console.log("   1. Create FraiseQL.toml with database configuration");
    console.log("   2. Run: npm run compile");
    console.log("   3. Run: npm run dev");
  } catch (error) {
    console.error("❌ Export failed:", error);
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  exportSchema();
}

export { User, Post, Comment };
```text

### 3.2 Understanding the Decorators

#### `@FraiseQL.Type()`

Marks a class as a GraphQL type:

```typescript
@FraiseQL.Type({
  description: "A user account"
})
class User {
  id: number;
  name: string;
}
```text

**What it does:**

- Registers the class name as a GraphQL type
- Metadata is collected at decoration time
- No runtime behavior

#### `registerTypeFields()`

Maps TypeScript properties to GraphQL fields:

```typescript
FraiseQL.registerTypeFields("User", [
  {
    name: "id",
    type: "ID",           // GraphQL scalar type
    nullable: false,      // Non-null field (!)
    description: "..."
  },
  {
    name: "email",
    type: "String",
    nullable: false
  }
]);
```text

**Scalar types:**

- `ID` - Unique identifier
- `String` - Text
- `Int` - 32-bit integer
- `Float` - Floating point number
- `Boolean` - True/false
- `DateTime` - ISO 8601 timestamp
- `Decimal` - Arbitrary precision number (for currency)
- `JSON` - Arbitrary JSON object

**Nullable rules:**

- `nullable: false` → `ID!` in GraphQL (required)
- `nullable: true` → `ID` in GraphQL (optional)
- Lists never represent individual array elements, only return types

#### `registerQuery()`

Defines a read operation:

```typescript
FraiseQL.registerQuery(
  "users",              // Query name in GraphQL
  "User",               // Return type (must be a registered @Type)
  true,                 // Returns list (array)
  false,                // Result not nullable
  [
    {
      name: "limit",
      type: "Int",
      nullable: false,
      default: 100        // Optional default value
    }
  ],
  "Get all users with pagination",
  { sqlSource: "v_user" }  // View or table name
);
```text

**Parameters:**

1. `name` - Query name in GraphQL schema
2. `returnType` - Type name (must exist)
3. `returnsList` - Does this query return an array?
4. `nullable` - Can the result be null?
5. `args` - Array of argument definitions
6. `description` - Documentation string
7. `config` - Additional configuration:
   - `sqlSource` - Table/view name to query
   - `whereColumn` - Column to filter by (for single-record lookups)

#### `registerMutation()`

Defines a write operation:

```typescript
FraiseQL.registerMutation(
  "createUser",
  "User",
  false,                // Single result (not list)
  false,                // Not nullable
  [
    { name: "email", type: "String", nullable: false },
    { name: "name", type: "String", nullable: false }
  ],
  "Create a new user",
  {
    sqlSource: "fn_create_user",    // Stored procedure name
    operation: "CREATE"              // CREATE, UPDATE, DELETE, or CUSTOM
  }
);
```text

### 3.3 TypeScript Type System

**Nullable vs Non-Nullable:**

```typescript
// TypeScript          → GraphQL
name: string          // String!      (required)
name?: string         // String       (optional)
name: string | null   // String       (optional)

// Arrays
posts: Post[]         // [Post!]!     (non-null array of non-null items)
posts?: Post[]        // [Post!]      (non-null array, but field optional)
```text

**Relationships:**

In FraiseQL, relationships are typically denormalized into views:

```typescript
// Instead of nested relationships...
@FraiseQL.Type()
class User {
  id: number;
  name: string;
  posts: Post[];        // ❌ Not supported at schema level
}

// ...use fields from the view:
@FraiseQL.Type()
class User {
  id: number;
  name: string;
  postCount: number;    // ✅ Computed in the view
}

// Then provide separate queries:
FraiseQL.registerQuery(
  "userById",
  "User",
  false,
  true,
  [{ name: "id", type: "ID", nullable: false }],
  "Get user by ID",
  { sqlSource: "v_user" }
);

FraiseQL.registerQuery(
  "postsByAuthor",
  "Post",
  true,
  false,
  [{ name: "authorId", type: "ID", nullable: false }],
  "Get posts by author",
  { sqlSource: "v_post" }
);
```text

---

## Part 4: Exporting the Schema

### 4.1 Running the Export

```bash
npm run export
```text

**Output:**

```text
✅ Schema exported successfully!
   Output: schema.json
   Types: 3 (User, Post, Comment)
   Queries: 6
   Mutations: 4
```text

This generates `schema.json`:

```json
{
  "version": "1.0",
  "types": [
    {
      "name": "User",
      "description": "A user account in the blog system",
      "kind": "OBJECT",
      "fields": [
        {
          "name": "id",
          "type": "ID",
          "nullable": false,
          "description": "Primary key"
        },
        {
          "name": "email",
          "type": "String",
          "nullable": false,
          "description": "User email"
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "description": "Get all users with pagination",
      "arguments": [
        {
          "name": "limit",
          "type": "Int",
          "nullable": false,
          "default": 100
        }
      ],
      "sql_source": "v_user"
    }
  ],
  "mutations": [
    {
      "name": "createUser",
      "return_type": "User",
      "returns_list": false,
      "nullable": false,
      "arguments": [
        {
          "name": "email",
          "type": "String",
          "nullable": false
        }
      ],
      "sql_source": "fn_create_user",
      "operation": "CREATE"
    }
  ]
}
```text

### 4.2 Understanding the Generated JSON

**Type Definition:**

- `name` - Type name
- `kind` - OBJECT, ENUM, INTERFACE, UNION, INPUT, SCALAR
- `fields` - Array of field definitions
- `description` - Type documentation

**Query Definition:**

- `name` - Query name
- `return_type` - Type returned
- `returns_list` - Is it an array?
- `arguments` - Input parameters
- `sql_source` - View to query

**Mutation Definition:**

- `name` - Mutation name
- `operation` - CREATE, UPDATE, DELETE, CUSTOM
- `sql_source` - Stored procedure to call

### 4.3 Troubleshooting Export Errors

**Error: "Type 'User' not found"**

- Ensure `registerTypeFields()` is called for every type before export
- Check spelling matches between `registerTypeFields("User")` and type usage

**Error: "Query 'users' references unknown return type 'User'"**

- Call `registerTypeFields()` for User before `registerQuery()` for users

**Error: "Field 'name' has type 'String' but no nullable option"**

- Always specify `nullable: true` or `nullable: false` for each field

**Solution: Check for TypeScript errors**

```bash
npx tsc --noEmit
```text

---

## Part 5: Creating the FraiseQL Configuration

### 5.1 FraiseQL.toml

Create `FraiseQL.toml` at the project root:

```toml
# Blog API Configuration

[FraiseQL]
name = "blog-api"
version = "1.0.0"
database_target = "postgresql"
description = "Blog API with users, posts, and comments"

# Database connection (can be overridden by environment variables)
[FraiseQL.database]
host = "localhost"
port = 5432
name = "blog_db"
username = "blog_user"
password = "blog_password"

# Security configuration (optional, for future auth/validation)
[FraiseQL.security]
enable_introspection = true
enable_mutations = true

# Development server configuration
[FraiseQL.server]
bind = "0.0.0.0"
port = 4000
cors_origins = ["*"]
```text

### 5.2 Environment Variable Overrides

For production, override sensitive values:

```bash
export FRAISEQL_DB_HOST=prod-db.example.com
export FRAISEQL_DB_NAME=blog_prod
export FRAISEQL_DB_USERNAME=blog_prod_user
export FRAISEQL_DB_PASSWORD=<secure-password>
```text

---

## Part 6: Compiling the Schema

### 6.1 Using FraiseQL-cli

Once you have `schema.json` and `FraiseQL.toml`, compile:

```bash
FraiseQL compile FraiseQL.toml --types schema.json
```text

**What it does:**

1. Validates all types exist and are properly defined
2. Validates all queries/mutations reference existing types
3. Validates all views/procedures exist in the database
4. Generates optimized SQL templates
5. Produces `schema.compiled.json`

**Output:**

```text
Compiling Blog API v1.0.0...
✅ Database connection successful
✅ Validated 3 types (User, Post, Comment)
✅ Validated 6 queries
✅ Validated 4 mutations
✅ Generated SQL templates
✅ Compiled schema saved: schema.compiled.json
```text

### 6.2 Understanding schema.compiled.json

The compiled schema includes:

```json
{
  "version": "1.0",
  "name": "blog-api",
  "types": [...],           // All type definitions
  "queries": [...],         // All query definitions with SQL
  "mutations": [...],       // All mutation definitions with SQL
  "sql_templates": {
    "users_list": "SELECT id, email, name, ... FROM v_user LIMIT $1 OFFSET $2",
    "user_by_id": "SELECT id, email, name, ... FROM v_user WHERE id = $1",
    "create_user": "SELECT * FROM fn_create_user($1, $2, $3)"
  },
  "database": {
    "target": "postgresql",
    "views": ["v_user", "v_post", "v_comment"],
    "procedures": ["fn_create_user", "fn_create_post", "fn_create_comment", "fn_update_user"]
  }
}
```text

### 6.3 Troubleshooting Compilation Errors

**Error: "View 'v_user' not found in database"**

- Ensure the PostgreSQL view exists
- Check database connection settings
- Verify you're connected to the correct database

**Error: "Column 'postCount' not found in view 'v_user'"**

- Check the view definition (created in Part 1)
- Verify view SELECT includes all required columns

**Error: "Procedure 'fn_create_user' expects 2 arguments but got 3"**

- Check the function signature in the database
- Update `registerMutation()` to match parameter count

**Solution: Debug with psql**

```bash
# Connect to database
psql -h localhost -U blog_user -d blog_db

# Check view exists
\dv v_user;

# Check view structure
SELECT * FROM v_user LIMIT 1;

# Check function signature
\df fn_create_user;
```text

---

## Part 7: Running the GraphQL Server

### 7.1 Starting the Server

```bash
npm run dev
```text

**Output:**

```text
Starting FraiseQL server...
✅ Loaded schema.compiled.json
✅ Connected to PostgreSQL (localhost:5432)
✅ Server running at http://localhost:4000/graphql
```text

### 7.2 Testing with GraphQL IDE

Visit <http://localhost:4000/graphql> in your browser to open the GraphQL IDE (Apollo Sandbox or similar).

**Example Query:**

```graphql
query GetAllUsers {
  users(limit: 10) {
    id
    email
    name
    postCount
    createdAt
  }
}
```text

**Expected Response:**

```json
{
  "data": {
    "users": [
      {
        "id": "1",
        "email": "alice@example.com",
        "name": "Alice",
        "postCount": 5,
        "createdAt": "2024-01-15T10:30:00Z"
      },
      {
        "id": "2",
        "email": "bob@example.com",
        "name": "Bob",
        "postCount": 3,
        "createdAt": "2024-01-16T14:20:00Z"
      }
    ]
  }
}
```text

### 7.3 Example Mutations

**Create User:**

```graphql
mutation {
  createUser(
    email: "charlie@example.com"
    name: "Charlie"
    bio: "Software engineer"
  ) {
    id
    email
    name
  }
}
```text

**Create Post:**

```graphql
mutation {
  createPost(
    title: "My First Post"
    content: "# Hello World\n\nThis is my first blog post!"
    authorId: "1"
    published: true
  ) {
    id
    title
    createdAt
  }
}
```text

**Create Comment:**

```graphql
mutation {
  createComment(
    text: "Great post!"
    postId: "1"
    authorId: "2"
  ) {
    id
    text
    createdAt
  }
}
```text

---

## Part 8: Testing Your Schema

### 8.1 Using curl

```bash
# Query all users
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users(limit: 10) { id email name } }"
  }'

# Create a user
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { createUser(email: \"test@example.com\" name: \"Test\") { id } }"
  }'
```text

### 8.2 Using Postman

1. Create new POST request to <http://localhost:4000/graphql>
2. Set header: `Content-Type: application/json`
3. Body (raw):

```json
{
  "query": "{ users(limit: 10) { id email name } }"
}
```text

### 8.3 TypeScript Integration Tests

Create `src/tests.ts`:

```typescript
/**
 * Integration tests for Blog API
 */

import fetch from "node-fetch";

const BASE_URL = "http://localhost:4000/graphql";

interface GraphQLResponse {
  data?: Record<string, unknown>;
  errors?: Array<{ message: string }>;
}

async function query(gql: string): Promise<GraphQLResponse> {
  const response = await fetch(BASE_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ query: gql })
  });
  return response.json() as Promise<GraphQLResponse>;
}

async function runTests() {
  console.log("Running Blog API tests...\n");

  // Test 1: Get all users
  console.log("Test 1: Get all users");
  const usersResult = await query(`{
    users(limit: 10) {
      id
      email
      name
    }
  }`);

  if (usersResult.errors) {
    console.log("❌ Failed:", usersResult.errors[0].message);
  } else {
    console.log("✅ Passed:", usersResult.data?.users ? "Got users" : "Empty list");
  }

  // Test 2: Create user
  console.log("\nTest 2: Create user");
  const createResult = await query(`
    mutation {
      createUser(
        email: "test${Date.now()}@example.com"
        name: "Test User"
      ) {
        id
        email
      }
    }
  `);

  if (createResult.errors) {
    console.log("❌ Failed:", createResult.errors[0].message);
  } else {
    console.log("✅ Passed: User created");
  }

  console.log("\nTests complete!");
}

if (require.main === module) {
  runTests().catch(console.error);
}
```text

---

## Part 9: Common Patterns

### 9.1 Pagination

**Query Definition:**

```typescript
FraiseQL.registerQuery(
  "posts",
  "Post",
  true,
  false,
  [
    { name: "limit", type: "Int", nullable: false, default: 50 },
    { name: "offset", type: "Int", nullable: false, default: 0 }
  ],
  "Get posts with pagination",
  { sqlSource: "v_post" }
);
```text

**Usage:**

```graphql
query {
  posts(limit: 20, offset: 0) {
    id
    title
    createdAt
  }
}
```text

### 9.2 Filtering

Create separate queries for common filters:

```typescript
// Query: Posts by author
FraiseQL.registerQuery(
  "postsByAuthor",
  "Post",
  true,
  false,
  [
    { name: "authorId", type: "ID", nullable: false },
    { name: "limit", type: "Int", nullable: false, default: 50 }
  ],
  "Get posts by a specific author",
  { sqlSource: "v_post" }
);

// Query: Comments on a post
FraiseQL.registerQuery(
  "postComments",
  "Comment",
  true,
  false,
  [{ name: "postId", type: "ID", nullable: false }],
  "Get comments on a post",
  { sqlSource: "v_comment" }
);
```text

### 9.3 Sorting

Include sort parameter as enum:

```typescript
// In FraiseQL.toml config (future feature):
// [queries.posts]
// sortBy = ["created_at", "title"]
// sortOrder = ["ASC", "DESC"]
```text

### 9.4 Optional Fields

Use `nullable: true` for optional parameters:

```typescript
FraiseQL.registerMutation(
  "updateUser",
  "User",
  false,
  true,
  [
    { name: "id", type: "ID", nullable: false },
    { name: "name", type: "String", nullable: true },    // Optional
    { name: "bio", type: "String", nullable: true }      // Optional
  ],
  "Update user profile",
  { sqlSource: "fn_update_user" }
);
```text

### 9.5 Computed Fields

Add fields that are computed in the view:

```sql
-- v_user includes computed fields
CREATE VIEW v_user AS
SELECT
    u.id,
    u.email,
    u.name,
    COUNT(DISTINCT p.id) AS post_count,      -- Computed
    COUNT(DISTINCT c.id) AS comment_count,   -- Computed
    u.created_at
FROM users u
LEFT JOIN posts p ON u.id = p.author_id
LEFT JOIN comments c ON u.id = c.author_id
GROUP BY u.id;
```text

Then register these fields:

```typescript
FraiseQL.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  { name: "postCount", type: "Int", nullable: false },       // Computed in view
  { name: "commentCount", type: "Int", nullable: false }     // Computed in view
]);
```text

---

## Part 10: Deployment

### 10.1 Docker Deployment

Create `Dockerfile`:

```dockerfile
FROM node:18-alpine

WORKDIR /app

# Copy schema files
COPY schema.compiled.json .
COPY FraiseQL.toml .

# Install FraiseQL server (pre-compiled binary)
RUN apk add --no-cache curl && \
    curl -fsSL https://releases.FraiseQL.io/FraiseQL-server-latest-linux-x64.tar.gz | \
    tar -xz -C /usr/local/bin

EXPOSE 4000

# Start server
CMD ["FraiseQL-server", \
     "--config", "FraiseQL.toml", \
     "--schema", "schema.compiled.json", \
     "--port", "4000"]
```text

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:15-alpine
    environment:
      POSTGRES_DB: blog_db
      POSTGRES_USER: blog_user
      POSTGRES_PASSWORD: blog_password
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./schema.sql:/docker-entrypoint-initdb.d/schema.sql

  FraiseQL:
    build: .
    environment:
      FRAISEQL_DB_HOST: postgres
      FRAISEQL_DB_NAME: blog_db
      FRAISEQL_DB_USERNAME: blog_user
      FRAISEQL_DB_PASSWORD: blog_password
    ports:
      - "4000:4000"
    depends_on:
      - postgres

volumes:
  postgres_data:
```text

**Deploy:**

```bash
# Build and start
docker-compose up -d

# Check logs
docker-compose logs -f FraiseQL

# Test
curl http://localhost:4000/graphql -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 1) { id } }"}'
```text

### 10.2 Health Checks

FraiseQL provides health check endpoints:

```bash
# Health check
curl http://localhost:4000/health

# Response
{
  "status": "ok",
  "version": "2.0.0",
  "database": "connected",
  "uptime_seconds": 125
}
```text

### 10.3 Environment Configuration

**Production setup:**

```bash
# .env.production
FRAISEQL_DB_HOST=prod-db.internal
FRAISEQL_DB_NAME=blog_prod
FRAISEQL_DB_USERNAME=blog_prod_user
FRAISEQL_DB_PASSWORD=<from-vault>
FRAISEQL_ENABLE_INTROSPECTION=false
FRAISEQL_LOG_LEVEL=info
```text

---

## Part 11: Troubleshooting

### Common Issues

**Issue: "Cannot find module 'FraiseQL'"**

Solution:

```bash
npm install FraiseQL --save
```text

**Issue: "Experimental decorators must be set to true"**

Solution: Verify `tsconfig.json` has:

```json
{
  "compilerOptions": {
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true
  }
}
```text

**Issue: "Query executes but returns empty results"**

Check:

1. View exists in database: `\dv v_post`
2. View has data: `SELECT COUNT(*) FROM v_post`
3. Column names match schema definition
4. Indexes are present for performance

**Issue: "Mutation returns error: function doesn't exist"**

Check:

1. Function exists: `\df fn_create_user`
2. Function signature matches mutation args
3. Function returns the correct type

**Issue: "GraphQL IDE shows introspection error"**

Check:

1. Server is running: `curl http://localhost:4000/health`
2. `enable_introspection = true` in FraiseQL.toml
3. Browser console for CORS errors

### Debug Mode

Enable verbose logging:

```bash
# Set environment variable
export RUST_LOG=debug

# Then start server
npm run dev

# Check logs
docker-compose logs -f FraiseQL
```text

---

## Part 12: Complete Working Example

### schema.sql (All DDL)

```sql
-- PostgreSQL schema for blog API

CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE posts (
    id BIGSERIAL PRIMARY KEY,
    title VARCHAR(500) NOT NULL,
    content TEXT NOT NULL,
    author_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    published_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE comments (
    id BIGSERIAL PRIMARY KEY,
    text TEXT NOT NULL,
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_posts_published ON posts(published_at DESC);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);

-- Views

CREATE VIEW v_user AS
SELECT
    u.id,
    u.email,
    u.name,
    u.bio,
    u.created_at,
    u.updated_at,
    COUNT(DISTINCT p.id) AS post_count
FROM users u
LEFT JOIN posts p ON u.id = p.author_id
GROUP BY u.id;

CREATE VIEW v_post AS
SELECT
    p.id,
    p.title,
    p.content,
    p.author_id,
    p.published_at,
    p.created_at,
    p.updated_at,
    u.email AS author_email,
    u.name AS author_name,
    COUNT(DISTINCT c.id) AS comment_count
FROM posts p
JOIN users u ON p.author_id = u.id
LEFT JOIN comments c ON p.id = c.post_id
WHERE p.published_at IS NOT NULL
GROUP BY p.id, u.id;

CREATE VIEW v_comment AS
SELECT
    c.id,
    c.text,
    c.post_id,
    c.author_id,
    c.created_at,
    u.email AS author_email,
    u.name AS author_name
FROM comments c
JOIN users u ON c.author_id = u.id;

-- Functions

CREATE OR REPLACE FUNCTION fn_create_user(
    p_email VARCHAR,
    p_name VARCHAR,
    p_bio TEXT DEFAULT NULL
)
RETURNS SETOF users AS $$
INSERT INTO users (email, name, bio)
VALUES (p_email, p_name, p_bio)
ON CONFLICT (email) DO NOTHING
RETURNING *;
$$ LANGUAGE SQL;

CREATE OR REPLACE FUNCTION fn_create_post(
    p_title VARCHAR,
    p_content TEXT,
    p_author_id BIGINT,
    p_published BOOLEAN DEFAULT FALSE
)
RETURNS SETOF posts AS $$
INSERT INTO posts (title, content, author_id, published_at)
VALUES (
    p_title,
    p_content,
    p_author_id,
    CASE WHEN p_published THEN CURRENT_TIMESTAMP ELSE NULL END
)
RETURNING *;
$$ LANGUAGE SQL;

CREATE OR REPLACE FUNCTION fn_create_comment(
    p_text TEXT,
    p_post_id BIGINT,
    p_author_id BIGINT
)
RETURNS SETOF comments AS $$
INSERT INTO comments (text, post_id, author_id)
VALUES (p_text, p_post_id, p_author_id)
RETURNING *;
$$ LANGUAGE SQL;

CREATE OR REPLACE FUNCTION fn_update_user(
    p_id BIGINT,
    p_name VARCHAR DEFAULT NULL,
    p_bio TEXT DEFAULT NULL
)
RETURNS SETOF users AS $$
UPDATE users
SET
    name = COALESCE(p_name, name),
    bio = COALESCE(p_bio, bio),
    updated_at = CURRENT_TIMESTAMP
WHERE id = p_id
RETURNING *;
$$ LANGUAGE SQL;
```text

### package.json

```json
{
  "name": "blog-api",
  "version": "1.0.0",
  "description": "Blog API using FraiseQL and TypeScript",
  "main": "dist/schema.js",
  "scripts": {
    "export": "ts-node src/schema.ts",
    "compile": "FraiseQL compile FraiseQL.toml --types schema.json",
    "build": "npm run export && npm run compile",
    "dev": "FraiseQL serve schema.compiled.json --port 4000",
    "test": "ts-node src/tests.ts",
    "clean": "rm -f schema.json schema.compiled.json && rm -rf dist"
  },
  "dependencies": {
    "FraiseQL": "^2.0.0"
  },
  "devDependencies": {
    "@types/node": "^20.0.0",
    "typescript": "^5.0.0",
    "ts-node": "^10.9.0",
    "node-fetch": "^2.6.0"
  }
}
```text

### FraiseQL.toml

```toml
[FraiseQL]
name = "blog-api"
version = "1.0.0"
database_target = "postgresql"
description = "Blog API with users, posts, and comments"

[FraiseQL.database]
host = "localhost"
port = 5432
name = "blog_db"
username = "blog_user"
password = "blog_password"

[FraiseQL.server]
bind = "0.0.0.0"
port = 4000
cors_origins = ["*"]

[FraiseQL.security]
enable_introspection = true
enable_mutations = true
```text

---

## Part 13: Next Steps

### Learning More

1. **Authentication:** See [Authentication Integration Guide](../integrations/authentication/API-REFERENCE.md)
2. **Advanced Queries:** See [Pagination & Keyset Cursors](../specs/pagination-keyset.md)
3. **Performance:** See [Performance Optimization Guide](../performance/projection-optimization.md)
4. **Federation:** See [Apollo Federation Integration](../integrations/federation/README.md)

### Building the React Client

```typescript
// Example: React hook to fetch posts
import { useQuery, gql } from "@apollo/client";

const GET_POSTS = gql`
  query GetPosts($limit: Int!, $offset: Int!) {
    posts(limit: $limit, offset: $offset) {
      id
      title
      content
      authorName
      createdAt
    }
  }
`;

export function PostList() {
  const { loading, data, error } = useQuery(GET_POSTS, {
    variables: { limit: 20, offset: 0 }
  });

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return (
    <div>
      {data.posts.map((post) => (
        <article key={post.id}>
          <h2>{post.title}</h2>
          <p>{post.content}</p>
          <small>By {post.authorName}</small>
        </article>
      ))}
    </div>
  );
}
```text

---

## Summary

You've learned:

1. ✅ Database schema design for GraphQL
2. ✅ TypeScript decorators for schema authoring
3. ✅ Type system modeling (scalars, nullability, relationships)
4. ✅ Query and mutation definition
5. ✅ Schema export to JSON
6. ✅ Compilation with FraiseQL CLI
7. ✅ Running a GraphQL server
8. ✅ Testing queries and mutations
9. ✅ Deployment patterns
10. ✅ Troubleshooting common issues

**Key Takeaway:** TypeScript decorators make schema authoring ergonomic and type-safe. The FraiseQL compiler handles the hard work of optimization and SQL generation.

---

**Questions?** See [FAQ](../FAQ.md) or [Troubleshooting Guide](../TROUBLESHOOTING.md).

**Ready to deploy?** See [Deployment Security Guide](../DEPLOYMENT_SECURITY.md).
