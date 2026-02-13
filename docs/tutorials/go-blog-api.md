<!-- Skip to main content -->
---

title: Build a Blog API with FraiseQL - Go Schema Authoring Tutorial
description: In this hands-on tutorial, you'll build a complete Blog API GraphQL schema using **Go struct tags and the builder pattern**. By the end, you'll understand:
keywords: ["project", "hands-on", "schema", "learning", "example", "step-by-step"]
tags: ["documentation", "reference"]
---

# Build a Blog API with FraiseQL - Go Schema Authoring Tutorial

**Status:** ✅ Production Ready
**Audience:** Developers, Architects
**Reading Time:** 30-45 minutes
**Last Updated:** 2026-02-05

## Overview

In this hands-on tutorial, you'll build a complete Blog API GraphQL schema using **Go struct tags and the builder pattern**. By the end, you'll understand:

- How to define types using Go structs with `FraiseQL` tags
- The builder pattern for declaring queries and mutations
- How struct tags map to GraphQL schema
- Schema compilation and deployment

**What we're building:** A blog API with users, posts, and comments - supporting queries for listing and fetching items, mutations for creating and updating content.

**Time estimate:** 30 minutes for basic setup, 45 minutes for complete implementation.

**Prerequisites:**

- Go 1.22+ installed
- Basic GraphQL knowledge (types, queries, mutations)
- PostgreSQL 14+ (for compilation and testing)
- FraiseQL CLI installed (`FraiseQL-cli`)
- Basic familiarity with Go struct tags

---

## Architecture Overview

FraiseQL's authoring workflow in Go:

```text
<!-- Code example in TEXT -->
┌─────────────────────────────────────────────────────┐
│ 1. Go Schema Definition                             │
│    - Struct tags with FraiseQL metadata             │
│    - Builder API for queries/mutations              │
│    - Type registration                              │
└────────────────┬────────────────────────────────────┘
                 │
                 ↓ go run cmd/export/main.go
┌─────────────────────────────────────────────────────┐
│ 2. Generated schema.json                            │
│    - JSON representation of your schema             │
│    - Type definitions, queries, mutations           │
│    - Validation metadata                            │
└────────────────┬────────────────────────────────────┘
                 │
                 ↓ FraiseQL-cli compile schema.json
┌─────────────────────────────────────────────────────┐
│ 3. schema.compiled.json                             │
│    - Optimized execution plan                       │
│    - SQL templates and execution instructions       │
│    - Configuration embedded in schema               │
└────────────────┬────────────────────────────────────┘
                 │
                 ↓ FraiseQL-server --schema schema.compiled.json
┌─────────────────────────────────────────────────────┐
│ 4. GraphQL Runtime (Rust)                           │
│    - Execute GraphQL queries                        │
│    - No Go dependencies at runtime                  │
│    - Pure Rust execution                            │
└─────────────────────────────────────────────────────┘
```text
<!-- Code example in TEXT -->

**Key Point:** Go is used for **authoring only**. The runtime is pure Rust with zero language bindings.

---

## Step 1: Project Setup

### Create a new Go project

```bash
<!-- Code example in BASH -->
mkdir FraiseQL-blog-api && cd FraiseQL-blog-api
go mod init FraiseQL-blog-api
go get github.com/FraiseQL/FraiseQL-go
```text
<!-- Code example in TEXT -->

### Directory structure

Create the following structure:

```text
<!-- Code example in TEXT -->
FraiseQL-blog-api/
├── go.mod
├── go.sum
├── cmd/
│   └── export/
│       └── main.go              # Schema export tool
├── schema/
│   └── types.go                 # Type definitions
├── queries/
│   └── queries.go               # Query definitions
├── mutations/
│   └── mutations.go             # Mutation definitions
├── schema.json                  # Generated (exported by export tool)
├── schema.compiled.json         # Generated (compiled by FraiseQL-cli)
└── Makefile                     # Build automation
```text
<!-- Code example in TEXT -->

### go.mod file

```go
<!-- Code example in Go -->
module FraiseQL-blog-api

go 1.22

require github.com/FraiseQL/FraiseQL-go v2.0.0-alpha.1
```text
<!-- Code example in TEXT -->

Run `go mod tidy` to download dependencies.

---

## Step 2: Define the Database Schema

Our blog API requires PostgreSQL tables and views. Create these before compiling the schema.

### PostgreSQL DDL

```sql
<!-- Code example in SQL -->
-- Create tables
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    bio TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    author_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    published BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE comments (
    id SERIAL PRIMARY KEY,
    post_id INT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    author_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create views for queries (read-only)
CREATE VIEW v_user AS
SELECT id, name, email, bio, created_at
FROM users;

CREATE VIEW v_post AS
SELECT id, author_id, title, content, published, created_at, updated_at
FROM posts;

CREATE VIEW v_comment AS
SELECT id, post_id, author_id, content, created_at
FROM comments;

-- Create stored functions for mutations
CREATE OR REPLACE FUNCTION fn_create_user(
    p_name VARCHAR,
    p_email VARCHAR,
    p_bio TEXT DEFAULT NULL
)
RETURNS TABLE (
    id INT, name VARCHAR, email VARCHAR, bio TEXT, created_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO users (name, email, bio)
    VALUES (p_name, p_email, p_bio)
    RETURNING users.id, users.name, users.email, users.bio, users.created_at;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION fn_create_post(
    p_author_id INT,
    p_title VARCHAR,
    p_content TEXT
)
RETURNS TABLE (
    id INT, author_id INT, title VARCHAR, content TEXT,
    published BOOLEAN, created_at TIMESTAMP, updated_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO posts (author_id, title, content)
    VALUES (p_author_id, p_title, p_content)
    RETURNING posts.id, posts.author_id, posts.title, posts.content,
              posts.published, posts.created_at, posts.updated_at;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION fn_publish_post(p_post_id INT)
RETURNS TABLE (
    id INT, author_id INT, title VARCHAR, content TEXT,
    published BOOLEAN, created_at TIMESTAMP, updated_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    UPDATE posts
    SET published = TRUE, updated_at = CURRENT_TIMESTAMP
    WHERE id = p_post_id
    RETURNING posts.id, posts.author_id, posts.title, posts.content,
              posts.published, posts.created_at, posts.updated_at;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION fn_create_comment(
    p_post_id INT,
    p_author_id INT,
    p_content TEXT
)
RETURNS TABLE (
    id INT, post_id INT, author_id INT, content TEXT, created_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO comments (post_id, author_id, content)
    VALUES (p_post_id, p_author_id, p_content)
    RETURNING comments.id, comments.post_id, comments.author_id,
              comments.content, comments.created_at;
END;
$$ LANGUAGE plpgsql;
```text
<!-- Code example in TEXT -->

Run this SQL against your PostgreSQL database to create the schema.

---

## Step 3: FraiseQL Schema Definition

### 3.1 Type Definitions with Struct Tags

Create `schema/types.go`:

```go
<!-- Code example in Go -->
package schema

// User represents a user in the system
type User struct {
 ID        int    `FraiseQL:"id,type=Int"`
 Name      string `FraiseQL:"name,type=String"`
 Email     string `FraiseQL:"email,type=String"`
 Bio       *string `FraiseQL:"bio,type=String"`
 CreatedAt string `FraiseQL:"createdAt,type=String"`
}

// Post represents a blog post
type Post struct {
 ID        int    `FraiseQL:"id,type=Int"`
 AuthorID  int    `FraiseQL:"authorId,type=Int"`
 Title     string `FraiseQL:"title,type=String"`
 Content   string `FraiseQL:"content,type=String"`
 Published bool   `FraiseQL:"published,type=Boolean"`
 CreatedAt string `FraiseQL:"createdAt,type=String"`
 UpdatedAt string `FraiseQL:"updatedAt,type=String"`
}

// Comment represents a comment on a post
type Comment struct {
 ID        int    `FraiseQL:"id,type=Int"`
 PostID    int    `FraiseQL:"postId,type=Int"`
 AuthorID  int    `FraiseQL:"authorId,type=Int"`
 Content   string `FraiseQL:"content,type=String"`
 CreatedAt string `FraiseQL:"createdAt,type=String"`
}
```text
<!-- Code example in TEXT -->

#### Understanding Struct Tags

The `FraiseQL` tag format is:

```go
<!-- Code example in Go -->
`FraiseQL:"<graphql_field_name>,type=<graphql_type>[,nullable=<true|false>]"`
```text
<!-- Code example in TEXT -->

- **graphql_field_name**: Name in the GraphQL schema (usually camelCase, unlike Go's PascalCase)
- **type**: GraphQL type (Int, String, Boolean, Float, etc.)
- **nullable**: Optional. Set to true for nullable fields (or use pointer types like `*string`)

**Type Mapping Examples:**

| Go Type | GraphQL Type | Nullable |
|---------|-------------|----------|
| `int` | `Int!` | No |
| `*int` | `Int` | Yes |
| `string` | `String!` | No |
| `*string` | `String` | Yes |
| `bool` | `Boolean!` | No |
| `[]Post` | `[Post!]!` | No |

In our example:

- `ID int` → GraphQL `id: Int!`
- `Bio *string` → GraphQL `bio: String` (nullable)
- `CreatedAt string` → GraphQL `createdAt: String!`

### 3.2 Query Definitions

Create `queries/queries.go`:

```go
<!-- Code example in Go -->
package queries

import (
 "FraiseQL-blog-api/schema"
 "github.com/FraiseQL/FraiseQL-go/FraiseQL"
)

// InitQueries registers all query operations
func InitQueries() {
 // Query: Get all users with pagination
 FraiseQL.NewQuery("users").
  ReturnType(schema.User{}).
  ReturnsArray(true).
  Config(map[string]interface{}{
   "sql_source": "v_user",
   "auto_params": map[string]bool{
    "limit":    true,
    "offset":   true,
    "where":    true,
    "order_by": true,
   },
  }).
  Arg("limit", "Int", 10).
  Arg("offset", "Int", 0).
  Description("Get all users with pagination and optional filtering").
  Register()

 // Query: Get a single user by ID
 FraiseQL.NewQuery("user").
  ReturnType(schema.User{}).
  Config(map[string]interface{}{
   "sql_source": "v_user",
  }).
  Arg("id", "Int", nil).
  Description("Get a single user by their ID").
  Register()

 // Query: Get all posts with filtering
 FraiseQL.NewQuery("posts").
  ReturnType(schema.Post{}).
  ReturnsArray(true).
  Config(map[string]interface{}{
   "sql_source": "v_post",
   "auto_params": map[string]bool{
    "limit":    true,
    "offset":   true,
    "where":    true,
    "order_by": true,
   },
  }).
  Arg("authorId", "Int", nil, true).
  Arg("published", "Boolean", nil, true).
  Arg("limit", "Int", 20).
  Arg("offset", "Int", 0).
  Description("Get all posts with optional filtering by author or publication status").
  Register()

 // Query: Get a single post by ID
 FraiseQL.NewQuery("post").
  ReturnType(schema.Post{}).
  Config(map[string]interface{}{
   "sql_source": "v_post",
  }).
  Arg("id", "Int", nil).
  Description("Get a single post by its ID").
  Register()

 // Query: Get all comments on a post
 FraiseQL.NewQuery("comments").
  ReturnType(schema.Comment{}).
  ReturnsArray(true).
  Config(map[string]interface{}{
   "sql_source": "v_comment",
   "auto_params": map[string]bool{
    "limit":    true,
    "offset":   true,
    "where":    true,
    "order_by": true,
   },
  }).
  Arg("postId", "Int", nil).
  Arg("limit", "Int", 50).
  Arg("offset", "Int", 0).
  Description("Get all comments on a post with pagination").
  Register()

 // Query: Get a single comment by ID
 FraiseQL.NewQuery("comment").
  ReturnType(schema.Comment{}).
  Config(map[string]interface{}{
   "sql_source": "v_comment",
  }).
  Arg("id", "Int", nil).
  Description("Get a single comment by its ID").
  Register()
}
```text
<!-- Code example in TEXT -->

#### Understanding the Query Builder

The builder pattern used here follows this structure:

```go
<!-- Code example in Go -->
FraiseQL.NewQuery("operationName").
    ReturnType(TypeStruct{}).           // Required: type returned
    ReturnsArray(bool).                 // Optional: single or array
    Config(map[string]interface{}{      // Optional: SQL configuration
        "sql_source": "view_or_table",
        "auto_params": map[string]bool{...},
    }).
    Arg("argName", "GraphQLType", defaultValue, nullable...).
    Description("Human-readable description").
    Register()
```text
<!-- Code example in TEXT -->

**Key methods:**

- `ReturnType(T)` - The Go struct representing the return type
- `ReturnsArray(bool)` - Whether this query returns `[Type]` vs `Type`
- `Config(map)` - Configuration:
  - `sql_source`: View or table name in the database
  - `auto_params`: Automatically add WHERE/ORDER BY/LIMIT parameters
- `Arg(name, graphqlType, defaultValue, nullable...)` - Add query arguments
  - `name`: Argument name in GraphQL
  - `graphqlType`: GraphQL type (Int, String, Boolean, etc.)
  - `defaultValue`: Default value (nil for required)
  - `nullable...`: Optional bool to mark as nullable
- `Description(string)` - Documentation string
- `Register()` - Register this query

### 3.3 Mutation Definitions

Create `mutations/mutations.go`:

```go
<!-- Code example in Go -->
package mutations

import (
 "FraiseQL-blog-api/schema"
 "github.com/FraiseQL/FraiseQL-go/FraiseQL"
)

// InitMutations registers all mutation operations
func InitMutations() {
 // Mutation: Create a new user
 FraiseQL.NewMutation("createUser").
  ReturnType(schema.User{}).
  Config(map[string]interface{}{
   "sql_source": "fn_create_user",
   "operation":  "CREATE",
  }).
  Arg("name", "String", nil).
  Arg("email", "String", nil).
  Arg("bio", "String", nil, true).
  Description("Create a new user account").
  Register()

 // Mutation: Create a new blog post
 FraiseQL.NewMutation("createPost").
  ReturnType(schema.Post{}).
  Config(map[string]interface{}{
   "sql_source": "fn_create_post",
   "operation":  "CREATE",
  }).
  Arg("authorId", "Int", nil).
  Arg("title", "String", nil).
  Arg("content", "String", nil).
  Description("Create a new blog post").
  Register()

 // Mutation: Publish a post (set published=true)
 FraiseQL.NewMutation("publishPost").
  ReturnType(schema.Post{}).
  Config(map[string]interface{}{
   "sql_source": "fn_publish_post",
   "operation":  "UPDATE",
  }).
  Arg("id", "Int", nil).
  Description("Publish a blog post").
  Register()

 // Mutation: Create a comment on a post
 FraiseQL.NewMutation("createComment").
  ReturnType(schema.Comment{}).
  Config(map[string]interface{}{
   "sql_source": "fn_create_comment",
   "operation":  "CREATE",
  }).
  Arg("postId", "Int", nil).
  Arg("authorId", "Int", nil).
  Arg("content", "String", nil).
  Description("Create a comment on a blog post").
  Register()
}
```text
<!-- Code example in TEXT -->

#### Understanding Mutations

Mutations follow the same builder pattern as queries. The key differences:

- Use `NewMutation()` instead of `NewQuery()`
- Include `"operation": "CREATE|READ|UPDATE|DELETE"` in the config
- Typically don't use `auto_params` (mutations are explicit)
- Return single items (not arrays)

---

## Step 4: Export Schema to JSON

### Create the export tool

Create `cmd/export/main.go`:

```go
<!-- Code example in Go -->
package main

import (
 "log"

 "FraiseQL-blog-api/mutations"
 "FraiseQL-blog-api/queries"
 "FraiseQL-blog-api/schema"
 "github.com/FraiseQL/FraiseQL-go/FraiseQL"
)

func main() {
 // Initialize schema builders
 // These must be called to register queries/mutations
 queries.InitQueries()
 mutations.InitMutations()

 // Register all types
 if err := FraiseQL.RegisterTypes(
  schema.User{},
  schema.Post{},
  schema.Comment{},
 ); err != nil {
  log.Fatalf("Error registering types: %v", err)
 }

 // Export schema to JSON
 if err := FraiseQL.ExportSchema("schema.json"); err != nil {
  log.Fatalf("Error exporting schema: %v", err)
 }

 log.Println("✅ Schema exported to schema.json")
 log.Println("Run: FraiseQL-cli compile schema.json -o schema.compiled.json")
}
```text
<!-- Code example in TEXT -->

### Generate the schema

```bash
<!-- Code example in BASH -->
go run cmd/export/main.go
```text
<!-- Code example in TEXT -->

This produces `schema.json` containing:

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "User",
      "fields": [
        {"name": "id", "type": "Int!", "description": ""},
        {"name": "name", "type": "String!", "description": ""},
        {"name": "email", "type": "String!", "description": ""},
        {"name": "bio", "type": "String", "description": ""},
        {"name": "createdAt", "type": "String!", "description": ""}
      ]
    },
    {
      "name": "Post",
      "fields": [
        {"name": "id", "type": "Int!", "description": ""},
        {"name": "authorId", "type": "Int!", "description": ""},
        {"name": "title", "type": "String!", "description": ""},
        {"name": "content", "type": "String!", "description": ""},
        {"name": "published", "type": "Boolean!", "description": ""},
        {"name": "createdAt", "type": "String!", "description": ""},
        {"name": "updatedAt", "type": "String!", "description": ""}
      ]
    },
    {
      "name": "Comment",
      "fields": [
        {"name": "id", "type": "Int!", "description": ""},
        {"name": "postId", "type": "Int!", "description": ""},
        {"name": "authorId", "type": "Int!", "description": ""},
        {"name": "content", "type": "String!", "description": ""},
        {"name": "createdAt", "type": "String!", "description": ""}
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "User",
      "returnsArray": true,
      "args": [
        {"name": "limit", "type": "Int", "defaultValue": 10},
        {"name": "offset", "type": "Int", "defaultValue": 0}
      ],
      "description": "Get all users with pagination and optional filtering",
      "config": {
        "sql_source": "v_user",
        "auto_params": {"limit": true, "offset": true, "where": true, "order_by": true}
      }
    },
    {
      "name": "user",
      "returnType": "User",
      "returnsArray": false,
      "args": [
        {"name": "id", "type": "Int"}
      ],
      "description": "Get a single user by their ID",
      "config": {
        "sql_source": "v_user"
      }
    }
  ],
  "mutations": [
    {
      "name": "createUser",
      "returnType": "User",
      "returnsArray": false,
      "args": [
        {"name": "name", "type": "String"},
        {"name": "email", "type": "String"},
        {"name": "bio", "type": "String"}
      ],
      "description": "Create a new user account",
      "config": {
        "sql_source": "fn_create_user",
        "operation": "CREATE"
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Error Handling

If schema export fails, check:

1. **All types registered?** Each type in your queries/mutations must be in `RegisterTypes()`
2. **All builders registered?** Call `Register()` on each builder
3. **Valid struct tags?** Format: `FraiseQL:"fieldName,type=GraphQLType"`
4. **No circular dependencies?** Avoid self-referencing types without indirection

---

## Step 5: Compile the Schema

### Using FraiseQL-cli

The CLI validates your schema and generates an optimized compiled version:

```bash
<!-- Code example in BASH -->
FraiseQL-cli compile schema.json -o schema.compiled.json
```text
<!-- Code example in TEXT -->

This produces `schema.compiled.json` containing:

- Validated type definitions
- Generated SQL execution templates
- Operation metadata
- Configuration settings

### Validate before compiling

```bash
<!-- Code example in BASH -->
FraiseQL-cli validate schema.json
```text
<!-- Code example in TEXT -->

### Troubleshoot compilation errors

```bash
<!-- Code example in BASH -->
# Verbose compilation for detailed error messages
FraiseQL-cli compile schema.json -o schema.compiled.json --verbose

# Check specific operation
FraiseQL-cli describe schema.json --query users

# Validate SQL sources
FraiseQL-cli validate schema.json --check-sql
```text
<!-- Code example in TEXT -->

---

## Step 6: Testing Your Schema

### Unit Tests for Type Definitions

Create `schema/types_test.go`:

```go
<!-- Code example in Go -->
package schema

import (
 "testing"
)

func TestUserType(t *testing.T) {
 user := User{
  ID:        1,
  Name:      "Alice",
  Email:     "alice@example.com",
  Bio:       nil,
  CreatedAt: "2024-01-01T00:00:00Z",
 }

 if user.ID != 1 {
  t.Errorf("Expected ID 1, got %d", user.ID)
 }
 if user.Name != "Alice" {
  t.Errorf("Expected name 'Alice', got %s", user.Name)
 }
 if user.Bio != nil {
  t.Errorf("Expected nil bio, got %v", user.Bio)
 }
}

func TestPostType(t *testing.T) {
 post := Post{
  ID:        42,
  AuthorID:  1,
  Title:     "Hello World",
  Content:   "This is a blog post",
  Published: false,
  CreatedAt: "2024-01-01T00:00:00Z",
  UpdatedAt: "2024-01-01T00:00:00Z",
 }

 if post.Title != "Hello World" {
  t.Errorf("Expected title 'Hello World', got %s", post.Title)
 }
 if post.Published {
  t.Errorf("Expected published to be false")
 }
}

func TestCommentType(t *testing.T) {
 comment := Comment{
  ID:        1,
  PostID:    42,
  AuthorID:  1,
  Content:   "Great post!",
  CreatedAt: "2024-01-01T00:00:00Z",
 }

 if comment.PostID != 42 {
  t.Errorf("Expected post ID 42, got %d", comment.PostID)
 }
}
```text
<!-- Code example in TEXT -->

### Integration Tests for Schema Export

Create `cmd/export/main_test.go`:

```go
<!-- Code example in Go -->
package main

import (
 "encoding/json"
 "io/ioutil"
 "os"
 "testing"

 "FraiseQL-blog-api/mutations"
 "FraiseQL-blog-api/queries"
 "FraiseQL-blog-api/schema"
 "github.com/FraiseQL/FraiseQL-go/FraiseQL"
)

func TestSchemaExport(t *testing.T) {
 // Setup
 queries.InitQueries()
 mutations.InitMutations()

 if err := FraiseQL.RegisterTypes(
  schema.User{},
  schema.Post{},
  schema.Comment{},
 ); err != nil {
  t.Fatalf("Failed to register types: %v", err)
 }

 // Export to temporary file
 tmpfile, err := ioutil.TempFile("", "schema-*.json")
 if err != nil {
  t.Fatalf("Failed to create temp file: %v", err)
 }
 defer os.Remove(tmpfile.Name())

 if err := FraiseQL.ExportSchema(tmpfile.Name()); err != nil {
  t.Fatalf("Failed to export schema: %v", err)
 }

 // Parse and verify
 data, err := ioutil.ReadFile(tmpfile.Name())
 if err != nil {
  t.Fatalf("Failed to read exported schema: %v", err)
 }

 var schemaData map[string]interface{}
 if err := json.Unmarshal(data, &schemaData); err != nil {
  t.Fatalf("Failed to parse schema JSON: %v", err)
 }

 // Verify required fields
 if _, hasTypes := schemaData["types"]; !hasTypes {
  t.Error("Schema missing 'types' field")
 }
 if _, hasQueries := schemaData["queries"]; !hasQueries {
  t.Error("Schema missing 'queries' field")
 }
 if _, hasMutations := schemaData["mutations"]; !hasMutations {
  t.Error("Schema missing 'mutations' field")
 }

 // Verify type count
 types := schemaData["types"].([]interface{})
 if len(types) != 3 {
  t.Errorf("Expected 3 types, got %d", len(types))
 }

 // Verify query count
 queries := schemaData["queries"].([]interface{})
 if len(queries) < 6 {
  t.Errorf("Expected at least 6 queries, got %d", len(queries))
 }

 // Verify mutation count
 mutations := schemaData["mutations"].([]interface{})
 if len(mutations) < 4 {
  t.Errorf("Expected at least 4 mutations, got %d", len(mutations))
 }
}
```text
<!-- Code example in TEXT -->

### Run tests

```bash
<!-- Code example in BASH -->
go test ./...
```text
<!-- Code example in TEXT -->

---

## Step 7: Deployment

### Build the Rust Runtime

```bash
<!-- Code example in BASH -->
# Compile the schema
FraiseQL-cli compile schema.json -o schema.compiled.json

# Start FraiseQL server
FraiseQL-server --schema schema.compiled.json --port 8000
```text
<!-- Code example in TEXT -->

### Docker Deployment

Create `Dockerfile`:

```dockerfile
<!-- Code example in DOCKERFILE -->
# Build stage
FROM golang:1.22-alpine AS builder
WORKDIR /app
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -o export cmd/export/main.go
RUN ./export

# Compilation stage (requires FraiseQL-cli)
FROM FraiseQL/FraiseQL-cli:v2 AS compiler
COPY --from=builder /app/schema.json /app/schema.json
RUN FraiseQL-cli compile /app/schema.json -o /app/schema.compiled.json

# Runtime stage
FROM FraiseQL/FraiseQL-server:v2
COPY --from=compiler /app/schema.compiled.json /etc/FraiseQL/schema.compiled.json
EXPOSE 8000
CMD ["FraiseQL-server", "--schema", "/etc/FraiseQL/schema.compiled.json", "--port", "8000"]
```text
<!-- Code example in TEXT -->

### Docker Compose

Create `docker-compose.yml`:

```yaml
<!-- Code example in YAML -->
version: '3.8'

services:
  postgres:
    image: postgres:15-alpine
    environment:
      POSTGRES_DB: blog_api
      POSTGRES_PASSWORD: postgres
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./schema.sql:/docker-entrypoint-initdb.d/01-schema.sql
    ports:
      - "5432:5432"

  FraiseQL-server:
    build: .
    depends_on:
      - postgres
    environment:
      DATABASE_URL: postgresql://postgres:postgres@postgres:5432/blog_api
    ports:
      - "8000:8000"

volumes:
  postgres_data:
```text
<!-- Code example in TEXT -->

Deploy:

```bash
<!-- Code example in BASH -->
docker-compose up -d
```text
<!-- Code example in TEXT -->

### Health Checks

```bash
<!-- Code example in BASH -->
# GraphQL introspection query
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ __schema { queryType { name } } }"}'

# Simple query test
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ users(limit: 5) { id name email } }"}'

# Mutation test
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { createUser(name: \"Bob\", email: \"bob@example.com\") { id name email } }"
  }'
```text
<!-- Code example in TEXT -->

---

## Common Patterns

### Pattern 1: Pagination

```go
<!-- Code example in Go -->
FraiseQL.NewQuery("users").
    ReturnType(User{}).
    ReturnsArray(true).
    Arg("limit", "Int", 20).
    Arg("offset", "Int", 0).
    Config(map[string]interface{}{
        "sql_source": "v_user",
        "auto_params": map[string]bool{"limit": true, "offset": true},
    }).
    Register()

// Usage: query { users(limit: 10, offset: 20) { id name } }
```text
<!-- Code example in TEXT -->

### Pattern 2: Filtering

```go
<!-- Code example in Go -->
FraiseQL.NewQuery("posts").
    ReturnType(Post{}).
    ReturnsArray(true).
    Arg("authorId", "Int", nil, true).
    Arg("published", "Boolean", nil, true).
    Config(map[string]interface{}{
        "sql_source": "v_post",
        "auto_params": map[string]bool{
            "where": true,
            "limit": true,
            "offset": true,
        },
    }).
    Register()

// Usage: query { posts(authorId: 1, published: true, limit: 10) { id title } }
```text
<!-- Code example in TEXT -->

### Pattern 3: Sorting

```go
<!-- Code example in Go -->
FraiseQL.NewQuery("posts").
    ReturnType(Post{}).
    ReturnsArray(true).
    Config(map[string]interface{}{
        "sql_source": "v_post",
        "auto_params": map[string]bool{
            "order_by": true,
            "limit": true,
        },
    }).
    Arg("limit", "Int", 20).
    Register()

// Usage: query { posts(orderBy: "createdAt DESC", limit: 10) { id title createdAt } }
```text
<!-- Code example in TEXT -->

### Pattern 4: Relationships (Foreign Keys)

```go
<!-- Code example in Go -->
// User type has AuthorID field
type Post struct {
    ID        int    `FraiseQL:"id,type=Int"`
    AuthorID  int    `FraiseQL:"authorId,type=Int"`  // FK to User
    Title     string `FraiseQL:"title,type=String"`
}

// Query posts with author information
// Note: FraiseQL handles relationship resolution via SQL joins
FraiseQL.NewQuery("posts").
    ReturnType(Post{}).
    ReturnsArray(true).
    Config(map[string]interface{}{
        "sql_source": "v_post",
    }).
    Register()
```text
<!-- Code example in TEXT -->

### Pattern 5: Nullable Fields

```go
<!-- Code example in Go -->
type User struct {
    ID        int     `FraiseQL:"id,type=Int"`
    Name      string  `FraiseQL:"name,type=String"`
    Bio       *string `FraiseQL:"bio,type=String"`      // nullable (pointer)
    PhoneNum  *string `FraiseQL:"phoneNum,type=String"` // optional
}

type Post struct {
    ID        int    `FraiseQL:"id,type=Int"`
    Title     string `FraiseQL:"title,type=String"`
    UpdatedAt string `FraiseQL:"updatedAt,type=String"`
}
```text
<!-- Code example in TEXT -->

### Pattern 6: Optional Mutation Arguments

```go
<!-- Code example in Go -->
FraiseQL.NewMutation("updateUser").
    ReturnType(User{}).
    Config(map[string]interface{}{
        "sql_source": "fn_update_user",
        "operation": "UPDATE",
    }).
    Arg("id", "Int", nil).              // required
    Arg("name", "String", nil, true).   // nullable (optional)
    Arg("bio", "String", nil, true).    // nullable (optional)
    Register()

// Usage: mutation { updateUser(id: 1, name: "Alice Updated") { id name bio } }
```text
<!-- Code example in TEXT -->

### Pattern 7: Analytics with Aggregates

```go
<!-- Code example in Go -->
// Fact table for sales analytics
FraiseQL.NewFactTable("sales").
    TableName("tf_sales").
    Measure("revenue", "sum", "avg", "max").
    Measure("quantity", "sum", "count", "avg").
    Dimension("category", "data->>'category'", "text").
    Dimension("region", "data->>'region'", "text").
    Description("Sales transactions").
    Register()

// Aggregate query
FraiseQL.NewAggregateQueryConfig("salesByRegion").
    FactTableName("sales").
    AutoGroupBy(true).
    AutoAggregates(true).
    Description("Sales aggregated by region").
    Register()
```text
<!-- Code example in TEXT -->

---

## Next Steps

### Build a Gin HTTP Server

To serve the GraphQL API with middleware (auth, logging, etc):

```go
<!-- Code example in Go -->
// server/server.go
package server

import (
 "github.com/gin-gonic/gin"
)

func New() *gin.Engine {
 router := gin.Default()

 // GraphQL endpoint proxies to FraiseQL server
 router.POST("/graphql", func(c *gin.Context) {
  // Forward to FraiseQL-server on port 8000
  // Include auth headers, logging, rate limiting
 })

 return router
}
```text
<!-- Code example in TEXT -->

### Client Implementation

Generate a GraphQL client for type-safe queries:

```bash
<!-- Code example in BASH -->
# Using Gqlgen
go run github.com/99designs/gqlgen init
go run github.com/99designs/gqlgen generate
```text
<!-- Code example in TEXT -->

### Performance Tuning

```bash
<!-- Code example in BASH -->
# Enable query caching for frequent operations
FraiseQL-cli compile schema.json \
  --cache-strategy persistent \
  --cache-ttl 300

# Monitor query performance
FraiseQL-server --schema schema.compiled.json \
  --enable-metrics \
  --metrics-port 9090
```text
<!-- Code example in TEXT -->

---

## Troubleshooting

### Issue: "Struct tag format invalid"

**Problem:**

```text
<!-- Code example in TEXT -->
Error: invalid FraiseQL tag format: "idtype=Int"
```text
<!-- Code example in TEXT -->

**Solution:**
Struct tags must have exact format: `FraiseQL:"fieldName,type=GraphQLType"`

```go
<!-- Code example in Go -->
// Wrong
type User struct {
    ID int `FraiseQL:"idtype=Int"`
}

// Correct
type User struct {
    ID int `FraiseQL:"id,type=Int"`
}
```text
<!-- Code example in TEXT -->

### Issue: "Type not registered"

**Problem:**

```text
<!-- Code example in TEXT -->
Error: type User used in query but not registered
```text
<!-- Code example in TEXT -->

**Solution:**
All types must be registered in `RegisterTypes()`:

```go
<!-- Code example in Go -->
// Register BEFORE exporting
if err := FraiseQL.RegisterTypes(User{}, Post{}, Comment{}); err != nil {
    log.Fatal(err)
}

if err := FraiseQL.ExportSchema("schema.json"); err != nil {
    log.Fatal(err)
}
```text
<!-- Code example in TEXT -->

### Issue: "Query builder not registered"

**Problem:**

```text
<!-- Code example in TEXT -->
schema.json is empty or missing queries
```text
<!-- Code example in TEXT -->

**Solution:**
Ensure `init()` functions are called:

```go
<!-- Code example in Go -->
// queries/queries.go
func InitQueries() {  // Must be called explicitly
    FraiseQL.NewQuery("users").
        // ...
        .Register()
}

// cmd/export/main.go
func main() {
    queries.InitQueries()    // Call this!
    mutations.InitMutations()
    // ...
}
```text
<!-- Code example in TEXT -->

### Issue: "Type mismatch in mutation"

**Problem:**

```text
<!-- Code example in TEXT -->
Error: argument 'id' type Int does not match parameter type String
```text
<!-- Code example in TEXT -->

**Solution:**
GraphQL types must match database parameter types:

```go
<!-- Code example in Go -->
// fn_create_post(p_author_id INT, p_title VARCHAR)
FraiseQL.NewMutation("createPost").
    Arg("authorId", "Int", nil).      // Matches INT
    Arg("title", "String", nil).      // Matches VARCHAR
    Register()
```text
<!-- Code example in TEXT -->

### Issue: "Compilation fails with SQL source error"

**Problem:**

```text
<!-- Code example in TEXT -->
Error: SQL source 'v_user' not found in database
```text
<!-- Code example in TEXT -->

**Solution:**
Ensure PostgreSQL views/functions exist:

```bash
<!-- Code example in BASH -->
# Verify in PostgreSQL
psql -U postgres -d blog_api -c "\dv v_user"
psql -U postgres -d blog_api -c "\df fn_create_user"

# If missing, run DDL setup script
psql -U postgres -d blog_api -f schema.sql
```text
<!-- Code example in TEXT -->

---

## Complete Code Reference

### Full Directory Structure

```text
<!-- Code example in TEXT -->
FraiseQL-blog-api/
├── cmd/
│   └── export/
│       ├── main.go
│       └── main_test.go
├── schema/
│   ├── types.go
│   └── types_test.go
├── queries/
│   └── queries.go
├── mutations/
│   └── mutations.go
├── go.mod
├── go.sum
├── schema.sql              # PostgreSQL DDL
├── Makefile
├── Dockerfile
├── docker-compose.yml
└── README.md
```text
<!-- Code example in TEXT -->

### Makefile

```makefile
<!-- Code example in MAKEFILE -->
.PHONY: help test build export compile run clean docker-up docker-down

help:
 @echo "FraiseQL Blog API - Available targets:"
 @echo "  make test          - Run Go tests"
 @echo "  make build         - Build export binary"
 @echo "  make export        - Export schema.json"
 @echo "  make compile       - Compile schema with FraiseQL-cli"
 @echo "  make run           - Run FraiseQL server"
 @echo "  make docker-up     - Start Docker Compose stack"
 @echo "  make docker-down   - Stop Docker Compose stack"
 @echo "  make clean         - Remove generated files"

test:
 go test -v ./...

build:
 go build -o export cmd/export/main.go

export: build
 ./export

compile: export
 FraiseQL-cli compile schema.json -o schema.compiled.json

run: compile
 FraiseQL-server --schema schema.compiled.json --port 8000

docker-up:
 docker-compose up -d

docker-down:
 docker-compose down

clean:
 rm -f export schema.json schema.compiled.json
 go clean -testcache
```text
<!-- Code example in TEXT -->

### go.mod

```go
<!-- Code example in Go -->
module FraiseQL-blog-api

go 1.22

require github.com/FraiseQL/FraiseQL-go v2.0.0-alpha.1
```text
<!-- Code example in TEXT -->

---

## Summary

You've now built a complete GraphQL Blog API schema using FraiseQL's Go authoring layer. Key takeaways:

1. **Struct tags** define GraphQL schema declaratively
2. **Builder pattern** makes queries and mutations fluent and readable
3. **Type safety** is enforced at compile time, not runtime
4. **Go is for authoring only** - the runtime is pure Rust
5. **Schema export** generates JSON for compilation
6. **CLI compilation** optimizes your schema for performance

This approach combines Go's simplicity with GraphQL's power, enabling you to build type-safe, high-performance APIs.

---

## References

- **[FraiseQL Go Package](https://github.com/FraiseQL/FraiseQL-go)** - Complete API reference
- **[GraphQL Specification](https://spec.graphql.org/)** - GraphQL language spec
- **[Go Struct Tags](https://pkg.go.dev/reflect#StructTag)** - Go reflection documentation
- **[FraiseQL CLI Documentation](../reference/)** - FraiseQL-cli command reference
- **[PostgreSQL Documentation](https://www.postgresql.org/docs/)** - SQL reference

---

**Questions?** Check the [Troubleshooting section](#troubleshooting) above, or refer to the [FraiseQL documentation](../README.md).
