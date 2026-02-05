# Building a Blog API with FraiseQL: Schema Authoring in Python

**Duration**: 30 minutes
**Outcome**: Complete GraphQL Blog API with schema authoring, compilation, and testing
**Prerequisites**: Python 3.10+, PostgreSQL (or SQLite for local testing), FraiseQL CLI
**Focus**: Schema definition in Python, NOT client implementation

---

## Overview

In this tutorial, you'll build a complete Blog API by:

1. **Defining the database schema** with SQL DDL (PostgreSQL)
2. **Creating Python type definitions** with FraiseQL decorators
3. **Authoring GraphQL queries and mutations** as Python functions
4. **Exporting** the schema to JSON
5. **Compiling** with the FraiseQL CLI
6. **Testing** your GraphQL API

### What You'll Build

A **Blog API** supporting:
- **Users**: Create and manage blog authors
- **Posts**: Create, update, delete blog posts with author relationships
- **Comments**: Add comments to posts with user relationships

### Key Concepts You'll Learn

- FraiseQL's Python decorators (`@fraiseql.type`, `@fraiseql.query`, `@fraiseql.mutation`)
- Python modern type hints (PEP 604: `X | None` instead of `Optional[X]`)
- Mapping Python types to SQL sources (views and functions)
- Query parameters and filtering
- Mutations for CREATE, UPDATE, DELETE operations
- Schema export and validation

---

## Part 1: Database Schema (SQL)

FraiseQL compiles GraphQL to SQL at build time. First, create the underlying database schema.

### Creating the PostgreSQL Database

```sql
-- Create users table
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    bio TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create posts table
CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    title VARCHAR(500) NOT NULL,
    content TEXT NOT NULL,
    author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    published BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create comments table
CREATE TABLE comments (
    id SERIAL PRIMARY KEY,
    post_id INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for common queries
CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_posts_published ON posts(published);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_user_id ON comments(user_id);
```

### Creating SQL Views for Queries

FraiseQL queries often read from **views** rather than raw tables. This provides a clean separation between your schema and implementation details.

```sql
-- View: All posts with author information
CREATE VIEW v_posts AS
SELECT
    p.id,
    p.title,
    p.content,
    p.author_id,
    p.published,
    p.created_at,
    p.updated_at,
    u.name AS author_name,
    u.email AS author_email
FROM posts p
JOIN users u ON p.author_id = u.id
ORDER BY p.created_at DESC;

-- View: Single post by ID with author
CREATE VIEW v_post_by_id AS
SELECT
    p.id,
    p.title,
    p.content,
    p.author_id,
    p.published,
    p.created_at,
    p.updated_at,
    u.name AS author_name,
    u.email AS author_email
FROM posts p
JOIN users u ON p.author_id = u.id;

-- View: Posts by a specific user
CREATE VIEW v_user_posts AS
SELECT
    p.id,
    p.title,
    p.content,
    p.author_id,
    p.published,
    p.created_at,
    p.updated_at,
    u.name AS author_name
FROM posts p
JOIN users u ON p.author_id = u.id;

-- View: Comments with author and post info
CREATE VIEW v_comments AS
SELECT
    c.id,
    c.post_id,
    c.user_id,
    c.content,
    c.created_at,
    c.updated_at,
    u.name AS author_name,
    p.title AS post_title
FROM comments c
JOIN users u ON c.user_id = u.id
JOIN posts p ON c.post_id = p.id;

-- View: All users
CREATE VIEW v_users AS
SELECT
    id,
    name,
    email,
    bio,
    created_at,
    updated_at
FROM users;
```

### Creating SQL Functions for Mutations

Mutations (CREATE, UPDATE, DELETE) are typically implemented as PostgreSQL **functions**. This keeps business logic in the database.

```sql
-- Function: Create a new user
CREATE FUNCTION fn_create_user(
    p_name VARCHAR,
    p_email VARCHAR,
    p_bio TEXT DEFAULT NULL
)
RETURNS TABLE (
    id INTEGER,
    name VARCHAR,
    email VARCHAR,
    bio TEXT,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO users (name, email, bio)
    VALUES (p_name, p_email, p_bio)
    RETURNING users.id, users.name, users.email, users.bio, users.created_at, users.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Function: Update a user
CREATE FUNCTION fn_update_user(
    p_id INTEGER,
    p_name VARCHAR DEFAULT NULL,
    p_email VARCHAR DEFAULT NULL,
    p_bio TEXT DEFAULT NULL
)
RETURNS TABLE (
    id INTEGER,
    name VARCHAR,
    email VARCHAR,
    bio TEXT,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    UPDATE users
    SET
        name = COALESCE(p_name, name),
        email = COALESCE(p_email, email),
        bio = COALESCE(p_bio, bio),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_id;

    RETURN QUERY
    SELECT users.id, users.name, users.email, users.bio, users.created_at, users.updated_at
    FROM users
    WHERE users.id = p_id;
END;
$$ LANGUAGE plpgsql;

-- Function: Create a new post
CREATE FUNCTION fn_create_post(
    p_title VARCHAR,
    p_content TEXT,
    p_author_id INTEGER,
    p_published BOOLEAN DEFAULT FALSE
)
RETURNS TABLE (
    id INTEGER,
    title VARCHAR,
    content TEXT,
    author_id INTEGER,
    published BOOLEAN,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO posts (title, content, author_id, published)
    VALUES (p_title, p_content, p_author_id, p_published)
    RETURNING posts.id, posts.title, posts.content, posts.author_id, posts.published, posts.created_at, posts.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Function: Update a post
CREATE FUNCTION fn_update_post(
    p_id INTEGER,
    p_title VARCHAR DEFAULT NULL,
    p_content TEXT DEFAULT NULL,
    p_published BOOLEAN DEFAULT NULL
)
RETURNS TABLE (
    id INTEGER,
    title VARCHAR,
    content TEXT,
    author_id INTEGER,
    published BOOLEAN,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    UPDATE posts
    SET
        title = COALESCE(p_title, title),
        content = COALESCE(p_content, content),
        published = COALESCE(p_published, published),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_id;

    RETURN QUERY
    SELECT posts.id, posts.title, posts.content, posts.author_id, posts.published, posts.created_at, posts.updated_at
    FROM posts
    WHERE posts.id = p_id;
END;
$$ LANGUAGE plpgsql;

-- Function: Delete a post (soft delete via flag)
CREATE FUNCTION fn_delete_post(p_id INTEGER)
RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM posts WHERE id = p_id;
    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Function: Create a comment
CREATE FUNCTION fn_create_comment(
    p_post_id INTEGER,
    p_user_id INTEGER,
    p_content TEXT
)
RETURNS TABLE (
    id INTEGER,
    post_id INTEGER,
    user_id INTEGER,
    content TEXT,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO comments (post_id, user_id, content)
    VALUES (p_post_id, p_user_id, p_content)
    RETURNING comments.id, comments.post_id, comments.user_id, comments.content, comments.created_at, comments.updated_at;
END;
$$ LANGUAGE plpgsql;
```

---

## Part 2: FraiseQL Schema Definition (Python)

Now you'll translate the database schema into FraiseQL type definitions and operations using Python decorators.

### Project Setup

```bash
# Create a new directory for your project
mkdir blog-api
cd blog-api

# Create a Python virtual environment
python3.10 -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Install FraiseQL Python SDK
pip install fraiseql

# Verify installation
python -c "import fraiseql; print(fraiseql.__version__)"
```

### Step 1: Create `schema.py`

Create a file named `schema.py` in your project root:

```python
"""FraiseQL Blog API Schema Definition.

This module defines the GraphQL schema for a blog API using FraiseQL decorators.
It demonstrates:
- Type definitions with @fraiseql.type
- Query definitions with @fraiseql.query
- Mutation definitions with @fraiseql.mutation
- Relationship mapping between types

The schema is compiled to JSON for deployment.
"""

import fraiseql


# ============================================================================
# TYPE DEFINITIONS
# ============================================================================

@fraiseql.type
class User:
    """A blog author or commenter.

    Fields:
        id: Unique identifier
        name: User's full name
        email: User's email address
        bio: Optional biography
        created_at: Account creation timestamp
        updated_at: Last account update timestamp
    """

    id: int
    name: str
    email: str
    bio: str | None
    created_at: str
    updated_at: str


@fraiseql.type
class Post:
    """A blog post written by a user.

    Fields:
        id: Unique identifier
        title: Post title
        content: Post body content
        author_id: ID of the user who wrote this post
        published: Whether the post is published
        created_at: Post creation timestamp
        updated_at: Last post update timestamp

    The `author_id` field enables queries to fetch the author relationship
    when needed.
    """

    id: int
    title: str
    content: str
    author_id: int
    published: bool
    created_at: str
    updated_at: str


@fraiseql.type
class Comment:
    """A comment on a blog post.

    Fields:
        id: Unique identifier
        post_id: ID of the post this comment is on
        user_id: ID of the user who wrote this comment
        content: Comment text
        created_at: Comment creation timestamp
        updated_at: Last comment update timestamp
    """

    id: int
    post_id: int
    user_id: int
    content: str
    created_at: str
    updated_at: str


# ============================================================================
# QUERIES (Read Operations)
# ============================================================================

@fraiseql.query(
    sql_source="v_users",
    auto_params={
        "limit": True,        # Auto-generate limit parameter
        "offset": True,       # Auto-generate offset parameter
        "where": True,        # Auto-generate WHERE clause support
        "order_by": True,     # Auto-generate ORDER BY support
    }
)
def users(
    limit: int = 10,
    offset: int = 0,
) -> list[User]:
    """Get a list of all users with pagination.

    Args:
        limit: Maximum number of users to return (default: 10)
        offset: Number of users to skip for pagination (default: 0)

    Returns:
        List of User objects

    GraphQL Example:
        query {
            users(limit: 20, offset: 0) {
                id
                name
                email
                bio
                created_at
            }
        }
    """
    pass


@fraiseql.query(sql_source="v_users")
def user(id: int) -> User | None:
    """Get a single user by ID.

    Args:
        id: The user ID to fetch

    Returns:
        User object if found, None otherwise

    GraphQL Example:
        query {
            user(id: 1) {
                id
                name
                email
                bio
            }
        }
    """
    pass


@fraiseql.query(
    sql_source="v_posts",
    auto_params={
        "limit": True,
        "offset": True,
        "where": True,
        "order_by": True,
    }
)
def posts(
    limit: int = 10,
    offset: int = 0,
    published: bool = True,
    author_id: int | None = None,
) -> list[Post]:
    """Get a list of blog posts with optional filtering.

    Args:
        limit: Maximum number of posts to return (default: 10)
        offset: Pagination offset (default: 0)
        published: Filter by published status (default: True)
        author_id: Filter by author ID (optional)

    Returns:
        List of Post objects matching the filter criteria

    GraphQL Example:
        query {
            posts(limit: 20, published: true, author_id: 1) {
                id
                title
                content
                author_id
                created_at
            }
        }
    """
    pass


@fraiseql.query(sql_source="v_post_by_id")
def post(id: int) -> Post | None:
    """Get a single post by ID.

    Args:
        id: The post ID to fetch

    Returns:
        Post object if found, None otherwise

    GraphQL Example:
        query {
            post(id: 42) {
                id
                title
                content
                author_id
                published
                created_at
            }
        }
    """
    pass


@fraiseql.query(
    sql_source="v_user_posts",
    auto_params={
        "limit": True,
        "offset": True,
        "where": True,
        "order_by": True,
    }
)
def user_posts(
    user_id: int,
    limit: int = 10,
    offset: int = 0,
) -> list[Post]:
    """Get all posts written by a specific user.

    Args:
        user_id: The author's user ID
        limit: Maximum number of posts to return (default: 10)
        offset: Pagination offset (default: 0)

    Returns:
        List of Post objects written by the user

    GraphQL Example:
        query {
            user_posts(user_id: 1, limit: 50) {
                id
                title
                content
                published
                created_at
            }
        }
    """
    pass


@fraiseql.query(
    sql_source="v_comments",
    auto_params={
        "limit": True,
        "offset": True,
        "where": True,
        "order_by": True,
    }
)
def post_comments(
    post_id: int,
    limit: int = 20,
    offset: int = 0,
) -> list[Comment]:
    """Get all comments on a specific post.

    Args:
        post_id: The post ID to fetch comments for
        limit: Maximum number of comments to return (default: 20)
        offset: Pagination offset (default: 0)

    Returns:
        List of Comment objects on the post

    GraphQL Example:
        query {
            post_comments(post_id: 42, limit: 100) {
                id
                user_id
                content
                created_at
            }
        }
    """
    pass


# ============================================================================
# MUTATIONS (Write Operations)
# ============================================================================

@fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
def create_user(
    name: str,
    email: str,
    bio: str | None = None,
) -> User:
    """Create a new user.

    Args:
        name: User's full name
        email: User's email address (must be unique)
        bio: Optional user biography

    Returns:
        The newly created User object

    GraphQL Example:
        mutation {
            create_user(name: "Alice Smith", email: "alice@example.com") {
                id
                name
                email
                created_at
            }
        }
    """
    pass


@fraiseql.mutation(sql_source="fn_update_user", operation="UPDATE")
def update_user(
    id: int,
    name: str | None = None,
    email: str | None = None,
    bio: str | None = None,
) -> User:
    """Update an existing user's information.

    Args:
        id: User ID to update
        name: New name (optional, only updated if provided)
        email: New email (optional, only updated if provided)
        bio: New bio (optional, only updated if provided)

    Returns:
        The updated User object

    GraphQL Example:
        mutation {
            update_user(id: 1, bio: "Software engineer and blogger") {
                id
                name
                bio
                updated_at
            }
        }
    """
    pass


@fraiseql.mutation(sql_source="fn_create_post", operation="CREATE")
def create_post(
    title: str,
    content: str,
    author_id: int,
    published: bool = False,
) -> Post:
    """Create a new blog post.

    Args:
        title: Post title
        content: Post content
        author_id: ID of the user authoring the post
        published: Whether to publish immediately (default: False for drafts)

    Returns:
        The newly created Post object

    GraphQL Example:
        mutation {
            create_post(
                title: "Getting Started with GraphQL"
                content: "GraphQL is a query language..."
                author_id: 1
                published: true
            ) {
                id
                title
                author_id
                created_at
            }
        }
    """
    pass


@fraiseql.mutation(sql_source="fn_update_post", operation="UPDATE")
def update_post(
    id: int,
    title: str | None = None,
    content: str | None = None,
    published: bool | None = None,
) -> Post:
    """Update an existing blog post.

    Args:
        id: Post ID to update
        title: New title (optional)
        content: New content (optional)
        published: New publish status (optional)

    Returns:
        The updated Post object

    GraphQL Example:
        mutation {
            update_post(
                id: 42
                published: true
            ) {
                id
                title
                published
                updated_at
            }
        }
    """
    pass


@fraiseql.mutation(sql_source="fn_delete_post", operation="DELETE")
def delete_post(id: int) -> bool:
    """Delete a blog post.

    Args:
        id: Post ID to delete

    Returns:
        True if the post was deleted, False if not found

    GraphQL Example:
        mutation {
            delete_post(id: 42)
        }
    """
    pass


@fraiseql.mutation(sql_source="fn_create_comment", operation="CREATE")
def create_comment(
    post_id: int,
    user_id: int,
    content: str,
) -> Comment:
    """Create a new comment on a post.

    Args:
        post_id: ID of the post to comment on
        user_id: ID of the user writing the comment
        content: Comment text

    Returns:
        The newly created Comment object

    GraphQL Example:
        mutation {
            create_comment(
                post_id: 42
                user_id: 5
                content: "Great article! Very helpful."
            ) {
                id
                post_id
                user_id
                content
                created_at
            }
        }
    """
    pass


# ============================================================================
# SCHEMA EXPORT
# ============================================================================

if __name__ == "__main__":
    # Export the schema to JSON
    # This generates schema.json which will be compiled with fraiseql-cli
    fraiseql.export_schema("schema.json")

    print("\n✅ Schema exported successfully!")
    print("   Generated: schema.json")
    print("\n   Next steps:")
    print("   1. Review schema.json for correctness")
    print("   2. Compile: fraiseql-cli compile schema.json fraiseql.toml")
    print("   3. Start server: fraiseql-server --schema schema.compiled.json")
```

### Understanding the Decorators

#### `@fraiseql.type`

Defines a GraphQL type that maps to your database schema:

```python
@fraiseql.type
class User:
    id: int              # GraphQL ID field (non-null by default)
    name: str            # GraphQL String field
    email: str | None    # GraphQL String field that can be null
```

**Key points:**
- Use Python 3.10+ type hints (`str | None` instead of `Optional[str]`)
- All fields must be typed
- Field names match your database columns
- `None` union means the field is nullable

#### `@fraiseql.query`

Defines a GraphQL query (read-only operation):

```python
@fraiseql.query(
    sql_source="v_posts",           # SQL view or table to query
    auto_params={
        "limit": True,              # Auto-generate limit/offset
        "offset": True,
        "where": True,              # Auto-generate WHERE filters
        "order_by": True,           # Auto-generate ORDER BY
    }
)
def posts(limit: int = 10) -> list[Post]:
    pass
```

**Parameters:**
- `sql_source`: The database view or table name (required)
- `auto_params`: Dictionary of auto-generated parameters
  - `"limit": True` - Automatically add `limit` parameter
  - `"offset": True` - Automatically add `offset` parameter
  - `"where": True` - Enable WHERE clause filtering
  - `"order_by": True` - Enable ORDER BY sorting

#### `@fraiseql.mutation`

Defines a GraphQL mutation (write operation):

```python
@fraiseql.mutation(
    sql_source="fn_create_user",    # SQL function to call
    operation="CREATE"              # Operation type (CREATE, UPDATE, DELETE)
)
def create_user(name: str, email: str) -> User:
    pass
```

**Operation types:**
- `"CREATE"` - Insert new record
- `"UPDATE"` - Modify existing record
- `"DELETE"` - Remove record

---

## Part 3: Exporting the Schema

The Python schema is a blueprint. FraiseQL converts it to JSON for compilation.

### Export the Schema

```bash
python schema.py
```

**Output:**
```
✅ Schema exported successfully!
   Generated: schema.json

   Next steps:
   1. Review schema.json for correctness
   2. Compile: fraiseql-cli compile schema.json fraiseql.toml
   3. Start server: fraiseql-server --schema schema.compiled.json
```

### Examine `schema.json`

The exported file should look like:

```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "id",
          "type": "Int",
          "nonNull": true
        },
        {
          "name": "name",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "email",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "bio",
          "type": "String",
          "nonNull": false
        },
        {
          "name": "created_at",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "updated_at",
          "type": "String",
          "nonNull": true
        }
      ],
      "description": "A blog author or commenter."
    },
    {
      "name": "Post",
      "fields": [
        {
          "name": "id",
          "type": "Int",
          "nonNull": true
        },
        {
          "name": "title",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "content",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "author_id",
          "type": "Int",
          "nonNull": true
        },
        {
          "name": "published",
          "type": "Boolean",
          "nonNull": true
        },
        {
          "name": "created_at",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "updated_at",
          "type": "String",
          "nonNull": true
        }
      ],
      "description": "A blog post written by a user."
    },
    {
      "name": "Comment",
      "fields": [
        {
          "name": "id",
          "type": "Int",
          "nonNull": true
        },
        {
          "name": "post_id",
          "type": "Int",
          "nonNull": true
        },
        {
          "name": "user_id",
          "type": "Int",
          "nonNull": true
        },
        {
          "name": "content",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "created_at",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "updated_at",
          "type": "String",
          "nonNull": true
        }
      ],
      "description": "A comment on a blog post."
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "User",
      "isList": true,
      "description": "Get a list of all users with pagination.",
      "args": [
        {
          "name": "limit",
          "type": "Int",
          "nonNull": false,
          "defaultValue": 10
        },
        {
          "name": "offset",
          "type": "Int",
          "nonNull": false,
          "defaultValue": 0
        }
      ],
      "sqlSource": "v_users",
      "autoParams": {
        "limit": true,
        "offset": true,
        "where": true,
        "order_by": true
      }
    }
  ],
  "mutations": [
    {
      "name": "create_user",
      "returnType": "User",
      "description": "Create a new user.",
      "args": [
        {
          "name": "name",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "email",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "bio",
          "type": "String",
          "nonNull": false
        }
      ],
      "sqlSource": "fn_create_user",
      "operation": "CREATE"
    }
  ]
}
```

### Validate the Schema

Check for common issues:

```bash
# Verify JSON is valid
python -m json.tool schema.json > /dev/null && echo "✅ schema.json is valid JSON"

# Count definitions
python -c "import json; s = json.load(open('schema.json')); print(f'Types: {len(s.get(\"types\", []))}, Queries: {len(s.get(\"queries\", []))}, Mutations: {len(s.get(\"mutations\", []))}')"
```

**Expected output:**
```
✅ schema.json is valid JSON
Types: 3, Queries: 6, Mutations: 5
```

---

## Part 4: Compiling the Schema

The FraiseQL CLI compiles your schema to an optimized binary format with embedded SQL.

### Create `fraiseql.toml`

Create a configuration file for compilation:

```toml
# FraiseQL Configuration for Blog API

[fraiseql]
name = "blog-api"
version = "1.0.0"
description = "GraphQL Blog API built with FraiseQL"

# Database configuration
[fraiseql.database]
adapter = "postgres"  # postgresql, mysql, sqlite, sqlserver
pool_size = 10
timeout_secs = 30
max_retries = 3

# Security configuration
[fraiseql.security]
# Rate limiting on mutations
rate_limit_mutations = 100
rate_limit_window_secs = 60

# Query depth to prevent deeply nested queries
max_query_depth = 5

# Error handling
sanitize_errors = true  # Hide implementation details in errors
```

### Compile the Schema

```bash
fraiseql-cli compile schema.json fraiseql.toml
```

**Output:**
```
✅ Compilation successful!
   Generated: schema.compiled.json

   Statistics:
   - Types: 3
   - Queries: 6
   - Mutations: 5
   - SQL functions: 7
   - Database views: 5

   File size: 45 KB
```

### Examine the Compiled Schema

The compiled schema includes optimized SQL templates:

```bash
# View a snippet of the compiled schema
python -c "
import json
with open('schema.compiled.json') as f:
    schema = json.load(f)
    query = next((q for q in schema.get('queries', []) if q['name'] == 'posts'), None)
    if query:
        print(json.dumps(query, indent=2))
"
```

---

## Part 5: Testing Your Schema

### Preparing Test Data

First, populate your database with test data:

```sql
-- Insert test users
INSERT INTO users (name, email, bio) VALUES
('Alice Smith', 'alice@example.com', 'Full-stack developer'),
('Bob Johnson', 'bob@example.com', 'DevOps engineer'),
('Carol White', 'carol@example.com', 'Data scientist');

-- Insert test posts
INSERT INTO posts (title, content, author_id, published) VALUES
('Getting Started with GraphQL', 'GraphQL is a query language...', 1, true),
('PostgreSQL Performance Tips', 'Indexing is crucial for performance...', 1, true),
('Python Best Practices', 'Use type hints and modern syntax...', 2, true),
('Machine Learning Fundamentals', 'ML is transforming industries...', 3, false);

-- Insert test comments
INSERT INTO comments (post_id, user_id, content) VALUES
(1, 2, 'Great introduction to GraphQL!'),
(1, 3, 'This helped me understand the basics.'),
(2, 3, 'Performance tips are really useful.'),
(3, 1, 'Thanks for the Python advice.');
```

### Testing Queries

Use GraphQL clients like `graphql-cli` or write test scripts. Here are common GraphQL queries:

#### Query: Get All Users

```graphql
query {
    users(limit: 10) {
        id
        name
        email
        bio
        created_at
    }
}
```

**Expected response:**
```json
{
    "data": {
        "users": [
            {
                "id": 1,
                "name": "Alice Smith",
                "email": "alice@example.com",
                "bio": "Full-stack developer",
                "created_at": "2025-02-05T10:00:00Z"
            },
            {
                "id": 2,
                "name": "Bob Johnson",
                "email": "bob@example.com",
                "bio": "DevOps engineer",
                "created_at": "2025-02-05T10:05:00Z"
            }
        ]
    }
}
```

#### Query: Get a Single Post by ID

```graphql
query {
    post(id: 1) {
        id
        title
        content
        author_id
        published
        created_at
    }
}
```

**Expected response:**
```json
{
    "data": {
        "post": {
            "id": 1,
            "title": "Getting Started with GraphQL",
            "content": "GraphQL is a query language...",
            "author_id": 1,
            "published": true,
            "created_at": "2025-02-05T10:10:00Z"
        }
    }
}
```

#### Query: Get User's Posts with Filtering

```graphql
query {
    user_posts(user_id: 1, limit: 20, offset: 0) {
        id
        title
        published
        created_at
    }
}
```

#### Query: Get Comments on a Post

```graphql
query {
    post_comments(post_id: 1, limit: 50) {
        id
        user_id
        content
        created_at
    }
}
```

### Testing Mutations

#### Mutation: Create a New User

```graphql
mutation {
    create_user(
        name: "David Lee"
        email: "david@example.com"
        bio: "Cloud architect"
    ) {
        id
        name
        email
        created_at
    }
}
```

**Expected response:**
```json
{
    "data": {
        "create_user": {
            "id": 4,
            "name": "David Lee",
            "email": "david@example.com",
            "created_at": "2025-02-05T11:00:00Z"
        }
    }
}
```

#### Mutation: Create a New Post

```graphql
mutation {
    create_post(
        title: "Advanced GraphQL Patterns"
        content: "Building scalable GraphQL APIs..."
        author_id: 1
        published: true
    ) {
        id
        title
        author_id
        created_at
    }
}
```

#### Mutation: Update a Post

```graphql
mutation {
    update_post(
        id: 4
        published: true
    ) {
        id
        title
        published
        updated_at
    }
}
```

#### Mutation: Delete a Post

```graphql
mutation {
    delete_post(id: 4)
}
```

#### Mutation: Create a Comment

```graphql
mutation {
    create_comment(
        post_id: 1
        user_id: 2
        content: "Excellent tutorial!"
    ) {
        id
        post_id
        user_id
        content
        created_at
    }
}
```

### Testing Error Cases

#### Missing Required Field

```graphql
mutation {
    create_user(name: "Eve", email: "eve@example.com") {
        id
        name
    }
}
# This will fail: bio is optional but field may not exist
```

#### Invalid Type

```graphql
query {
    post(id: "not-a-number") {
        id
        title
    }
}
```

**Expected error:**
```json
{
    "errors": [
        {
            "message": "Invalid argument: id must be an integer"
        }
    ]
}
```

---

## Part 6: Deployment Overview

Once your schema is tested, you're ready to deploy.

### Starting the Server

```bash
# Start FraiseQL server with compiled schema
fraiseql-server \
    --schema schema.compiled.json \
    --database-url postgresql://user:password@localhost/blog_db \
    --port 8000

# Server starts on http://localhost:8000
# GraphQL endpoint: http://localhost:8000/graphql
```

### Health Check

```bash
curl http://localhost:8000/health
```

**Response:**
```json
{
    "status": "ok",
    "version": "2.0.0",
    "uptime_secs": 42
}
```

### GraphQL Introspection

The server exposes GraphQL introspection by default:

```bash
curl -X POST http://localhost:8000/graphql \
    -H "Content-Type: application/json" \
    -d '{"query": "{__schema { types { name } }}"}'
```

This returns all available types in your schema.

---

## Part 7: Common Patterns

### Pattern 1: Adding Pagination to Queries

The `auto_params` feature makes pagination automatic:

```python
@fraiseql.query(
    sql_source="v_posts",
    auto_params={"limit": True, "offset": True}
)
def posts(limit: int = 10, offset: int = 0) -> list[Post]:
    """Paginated posts query."""
    pass
```

GraphQL usage:
```graphql
query {
    posts(limit: 20, offset: 40) {  # Fetch 20 posts, skip first 40
        id
        title
    }
}
```

### Pattern 2: Adding Filtering

Use `auto_params={"where": True}` to enable filters:

```python
@fraiseql.query(
    sql_source="v_posts",
    auto_params={"where": True}
)
def posts(published: bool | None = None) -> list[Post]:
    """Filtered posts query."""
    pass
```

GraphQL usage:
```graphql
query {
    posts(where: {published: {_eq: true}, author_id: {_eq: 1}}) {
        id
        title
        published
    }
}
```

### Pattern 3: Adding Sorting

Use `auto_params={"order_by": True}`:

```python
@fraiseql.query(
    sql_source="v_posts",
    auto_params={"order_by": True}
)
def posts() -> list[Post]:
    """Sortable posts query."""
    pass
```

GraphQL usage:
```graphql
query {
    posts(order_by: {field: "created_at", direction: DESC}) {
        id
        title
        created_at
    }
}
```

### Pattern 4: Author Relationships

Include related data by joining in your SQL view:

```sql
-- View includes author info
CREATE VIEW v_posts_with_author AS
SELECT
    p.id,
    p.title,
    p.content,
    p.author_id,
    u.name AS author_name,
    u.email AS author_email
FROM posts p
JOIN users u ON p.author_id = u.id;
```

Python schema:
```python
@fraiseql.type
class PostWithAuthor:
    id: int
    title: str
    content: str
    author_id: int
    author_name: str
    author_email: str

@fraiseql.query(sql_source="v_posts_with_author")
def posts() -> list[PostWithAuthor]:
    """Posts with inline author info."""
    pass
```

### Pattern 5: Filtering Comments by Post

```python
@fraiseql.query(
    sql_source="v_comments",
    auto_params={"where": True, "limit": True}
)
def post_comments(
    post_id: int,
    limit: int = 20,
) -> list[Comment]:
    """Get comments filtered by post."""
    pass
```

GraphQL usage:
```graphql
query {
    post_comments(
        post_id: 1
        limit: 50
        where: {user_id: {_eq: 2}}
    ) {
        id
        content
        user_id
        created_at
    }
}
```

---

## Part 8: Common Mistakes to Avoid

### Mistake 1: Using Old Python Type Hints

❌ **Wrong:**
```python
from typing import Optional
def user(id: int) -> Optional[User]:
    pass
```

✅ **Correct:**
```python
def user(id: int) -> User | None:
    pass
```

### Mistake 2: Forgetting `sql_source`

❌ **Wrong:**
```python
@fraiseql.query()
def users() -> list[User]:
    pass
```

✅ **Correct:**
```python
@fraiseql.query(sql_source="v_users")
def users() -> list[User]:
    pass
```

### Mistake 3: Inconsistent Field Names

If your database column is `created_at`, your Python field must also be `created_at`:

❌ **Wrong:**
```python
@fraiseql.type
class Post:
    createdAt: str  # Doesn't match database
```

✅ **Correct:**
```python
@fraiseql.type
class Post:
    created_at: str  # Matches database
```

### Mistake 4: Missing Null Annotations

If a database field can be NULL, mark it as nullable:

❌ **Wrong:**
```python
@fraiseql.type
class User:
    bio: str  # But bio can be NULL in database
```

✅ **Correct:**
```python
@fraiseql.type
class User:
    bio: str | None  # Nullable
```

### Mistake 5: Using Non-Existent SQL Functions

Ensure all `sql_source` values exist in your database:

```bash
# Check if function exists
psql -c "SELECT proname FROM pg_proc WHERE proname = 'fn_create_post';"
```

---

## Part 9: Next Steps

### Learning Path

1. **Now**: You've built a complete schema with types, queries, and mutations
2. **Next**: Add authentication and authorization ([see authorization guide](../guides/authorization-quick-start.md))
3. **Then**: Add real-time subscriptions ([see subscriptions guide](../guides/observability.md))
4. **Finally**: Deploy to production ([see deployment guide](../deployment/guide.md))

### Advanced Topics

- **Aggregation Queries**: Use `@fraiseql.fact_table` for analytics workloads
- **Federation**: Split schema across multiple services
- **Observers**: Add audit logging and monitoring
- **Custom Resolvers**: Implement complex business logic

---

## Part 10: Troubleshooting

### Schema Export Errors

#### Error: `ImportError: No module named 'fraiseql'`

**Solution:**
```bash
pip install fraiseql
```

#### Error: `Module has no attribute 'type'`

**Solution:** Check that you're using the correct import:
```python
import fraiseql
# Then use @fraiseql.type, not @fraiseql_type
```

### Compilation Errors

#### Error: `View 'v_posts' not found`

**Solution:** Verify the view exists in your database:
```sql
SELECT * FROM information_schema.views WHERE table_name = 'v_posts';
```

#### Error: `Type mismatch: expected String, got Integer`

**Solution:** Check field types match between Python and database:
```python
# If database column is VARCHAR, use str in Python
@fraiseql.type
class Post:
    title: str  # Correct
```

### Deployment Errors

#### Error: `Connection refused`

**Solution:** Verify database is running:
```bash
psql postgresql://user:password@localhost/blog_db -c "SELECT 1"
```

---

## Part 11: Complete Copy-Paste Ready Code

### Complete `schema.py`

See Part 2 above for the full schema file.

### Complete SQL Schema

See Part 1 above for the complete database setup.

### Complete `fraiseql.toml`

```toml
[fraiseql]
name = "blog-api"
version = "1.0.0"
description = "GraphQL Blog API"

[fraiseql.database]
adapter = "postgres"
pool_size = 10
timeout_secs = 30

[fraiseql.security]
rate_limit_mutations = 100
rate_limit_window_secs = 60
max_query_depth = 5
sanitize_errors = true
```

---

## Summary

You've learned how to:

✅ Design a relational database schema with SQL
✅ Create Python type definitions with `@fraiseql.type`
✅ Author GraphQL queries with `@fraiseql.query`
✅ Author GraphQL mutations with `@fraiseql.mutation`
✅ Export schemas to JSON
✅ Compile with the FraiseQL CLI
✅ Test with GraphQL queries
✅ Deploy the API

This workflow puts **schema authoring** front and center—defining your data model in Python decorators, exporting to JSON, and compiling to optimized SQL execution.

---

## Additional Resources

- [Python SDK Reference](../reference/python-sdk.md)
- [TOML Configuration Reference](../TOML_REFERENCE.md)
- [GraphQL Schema Design Best Practices](../guides/schema-design-best-practices.md)
- [Common Patterns](../guides/PATTERNS.md)
- [Troubleshooting](../TROUBLESHOOTING.md)

---

**Questions?** See [FAQ](../FAQ.md) or open an issue on [GitHub](https://github.com/fraiseql/fraiseql-v2).
