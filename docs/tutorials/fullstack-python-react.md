<!-- Skip to main content -->
---
title: Full-Stack Blog Application: Python Schema + React Frontend + FraiseQL Backend
description: In this comprehensive full-stack tutorial, you'll build a complete Blog Application by:
keywords: ["project", "hands-on", "schema", "learning", "example", "step-by-step"]
tags: ["documentation", "reference"]
---

# Full-Stack Blog Application: Python Schema + React Frontend + FraiseQL Backend

**Duration**: 90 minutes
**Outcome**: Complete full-stack application with Python schema authoring, FraiseQL server, and React frontend
**Prerequisites**: Python 3.10+, Node.js 18+, PostgreSQL 14+, FraiseQL CLI, Docker & Docker Compose
**Focus**: End-to-end architecture showing Python authoring, compiled FraiseQL backend, React frontend

---

## Overview

In this comprehensive full-stack tutorial, you'll build a complete Blog Application by:

1. **Authoring the schema in Python** with FraiseQL decorators
2. **Compiling to optimized SQL** with the FraiseQL CLI
3. **Deploying FraiseQL server** as the GraphQL backend (Rust runtime)
4. **Building a React frontend** that queries the GraphQL API

### Architecture Flow

```text
<!-- Code example in TEXT -->
┌─────────────────────────┐
│   Developer Workstation │
├─────────────────────────┤
│ 1. Write Python Schema  │
│    @FraiseQL.type       │
│    @FraiseQL.query      │
│    @FraiseQL.mutation   │
└────────┬────────────────┘
         │ python schema.py export
         ↓
┌──────────────────────────┐
│   schema.json            │
│ (types + queries + muts) │
└────────┬─────────────────┘
         │ FraiseQL-cli compile
         ↓
┌──────────────────────────────────┐
│ schema.compiled.json             │
│ (schema + SQL + config + security)
└────────┬──────────────────────────┘
         │ docker-compose up
         ↓
┌──────────────────────────────────┐
│ Docker Environment               │
├──────────────────────────────────┤
│ PostgreSQL (port 5432)           │
│ FraiseQL Server (port 8000)      │
│   GraphQL Endpoint: /graphql     │
│   Health: /health                │
└────────┬──────────────────────────┘
         │ HTTP/GraphQL (port 8000)
         ↓
┌──────────────────────────────────┐
│ React Frontend (port 3000)       │
├──────────────────────────────────┤
│ Apollo Client                    │
│ Components:                      │
│  - PostList                      │
│  - PostDetail                    │
│  - CreatePost Form               │
│  - Comments Section              │
│  - Like Buttons                  │
└──────────────────────────────────┘
```text
<!-- Code example in TEXT -->

### What You'll Build

A **Full-Stack Blog Application** supporting:

- **Users**: Create and manage blog authors
- **Posts**: Create, update, delete blog posts with author relationships
- **Comments**: Add comments to posts with user relationships
- **Likes**: Track post likes from users

### Key Concepts You'll Learn

- **Python → JSON**: Authoring GraphQL schemas in Python
- **CLI Compilation**: Compiling schema.json to optimized SQL
- **FraiseQL Server**: Deploying the compiled schema as a GraphQL API
- **React Integration**: Building UI components with Apollo Client
- **End-to-End Flow**: From Python code to running browser application
- **Docker Orchestration**: Managing multi-container services

---

## Part 1: Project Setup

### 1.1 Directory Structure

Create the following project structure:

```text
<!-- Code example in TEXT -->
fullstack-blog/
├── backend/
│   ├── schema.py                      # Python schema definition
│   ├── FraiseQL.toml                  # FraiseQL configuration
│   ├── schema.json                    # Exported schema (generated)
│   ├── schema.compiled.json           # Compiled schema (generated)
│   ├── Dockerfile                     # FraiseQL server container
│   └── requirements.txt               # Python dependencies
├── frontend/
│   ├── public/
│   │   ├── index.html
│   │   └── favicon.ico
│   ├── src/
│   │   ├── components/
│   │   │   ├── PostList.jsx
│   │   │   ├── PostDetail.jsx
│   │   │   ├── CreatePostForm.jsx
│   │   │   ├── CommentSection.jsx
│   │   │   └── LikeButton.jsx
│   │   ├── pages/
│   │   │   ├── HomePage.jsx
│   │   │   └── PostPage.jsx
│   │   ├── App.jsx
│   │   ├── index.jsx
│   │   ├── apollo-client.js
│   │   └── App.css
│   ├── package.json
│   ├── .env.local
│   ├── vite.config.js                # or webpack config
│   └── .gitignore
├── database/
│   └── schema.sql                     # PostgreSQL DDL
├── docker-compose.yml                 # Multi-container orchestration
└── README.md                           # Getting started guide
```text
<!-- Code example in TEXT -->

### 1.2 Initialize Backend Project

```bash
<!-- Code example in BASH -->
# Create backend directory
mkdir -p fullstack-blog/backend
cd fullstack-blog/backend

# Create Python virtual environment
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Install dependencies
pip install -r requirements.txt
```text
<!-- Code example in TEXT -->

### 1.3 Initialize Frontend Project

```bash
<!-- Code example in BASH -->
# Create React app (from project root)
cd fullstack-blog
npm create vite@latest frontend -- --template react
cd frontend
npm install

# Install Apollo Client and GraphQL
npm install @apollo/client graphql
npm install graphql-tag

# Development server
npm run dev  # Runs on http://localhost:5173
```text
<!-- Code example in TEXT -->

---

## Part 2: Database Schema (PostgreSQL)

### 2.1 Creating the PostgreSQL Schema

Create `database/schema.sql`:

```sql
<!-- Code example in SQL -->
-- Enable UUID and timestamps
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Drop existing objects (development only!)
DROP VIEW IF EXISTS v_comments CASCADE;
DROP VIEW IF EXISTS v_posts CASCADE;
DROP VIEW IF EXISTS v_users CASCADE;
DROP TABLE IF EXISTS likes CASCADE;
DROP TABLE IF EXISTS comments CASCADE;
DROP TABLE IF EXISTS posts CASCADE;
DROP TABLE IF EXISTS users CASCADE;

-- Create users table
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    bio TEXT,
    avatar_url VARCHAR(500),
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

-- Create likes table (many-to-many: users can like posts)
CREATE TABLE likes (
    id SERIAL PRIMARY KEY,
    post_id INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(post_id, user_id)  -- Each user can like each post once
);

-- Create indexes for common queries
CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_posts_published ON posts(published);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_user_id ON comments(user_id);
CREATE INDEX idx_likes_post_id ON likes(post_id);
CREATE INDEX idx_likes_user_id ON likes(user_id);

-- View: All users
CREATE VIEW v_users AS
SELECT
    id,
    name,
    email,
    bio,
    avatar_url,
    created_at,
    updated_at
FROM users
ORDER BY created_at DESC;

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
    u.id AS author_id,
    u.name AS author_name,
    u.email AS author_email,
    (SELECT COUNT(*) FROM likes WHERE post_id = p.id) AS like_count
FROM posts p
JOIN users u ON p.author_id = u.id
ORDER BY p.created_at DESC;

-- View: Single post by ID with author and comments
CREATE VIEW v_post_detail AS
SELECT
    p.id,
    p.title,
    p.content,
    p.author_id,
    p.published,
    p.created_at,
    p.updated_at,
    u.id AS author_id,
    u.name AS author_name,
    u.email AS author_email,
    u.bio AS author_bio,
    (SELECT COUNT(*) FROM likes WHERE post_id = p.id) AS like_count,
    (SELECT COUNT(*) FROM comments WHERE post_id = p.id) AS comment_count
FROM posts p
JOIN users u ON p.author_id = u.id;

-- View: Comments with author information
CREATE VIEW v_comments AS
SELECT
    c.id,
    c.post_id,
    c.user_id,
    c.content,
    c.created_at,
    u.id AS author_id,
    u.name AS author_name,
    u.email AS author_email,
    u.avatar_url AS author_avatar_url
FROM comments c
JOIN users u ON c.user_id = u.id
ORDER BY c.created_at DESC;

-- Function: Create a new user
CREATE FUNCTION fn_create_user(
    p_name VARCHAR,
    p_email VARCHAR,
    p_bio TEXT DEFAULT NULL,
    p_avatar_url VARCHAR DEFAULT NULL
)
RETURNS TABLE (
    id INTEGER,
    name VARCHAR,
    email VARCHAR,
    bio TEXT,
    avatar_url VARCHAR,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO users (name, email, bio, avatar_url)
    VALUES (p_name, p_email, p_bio, p_avatar_url)
    RETURNING users.id, users.name, users.email, users.bio, users.avatar_url, users.created_at, users.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Function: Update a user
CREATE FUNCTION fn_update_user(
    p_id INTEGER,
    p_name VARCHAR DEFAULT NULL,
    p_bio TEXT DEFAULT NULL,
    p_avatar_url VARCHAR DEFAULT NULL
)
RETURNS TABLE (
    id INTEGER,
    name VARCHAR,
    email VARCHAR,
    bio TEXT,
    avatar_url VARCHAR,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    UPDATE users
    SET
        name = COALESCE(p_name, name),
        bio = COALESCE(p_bio, bio),
        avatar_url = COALESCE(p_avatar_url, avatar_url),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_id;

    RETURN QUERY
    SELECT users.id, users.name, users.email, users.bio, users.avatar_url, users.created_at, users.updated_at
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

-- Function: Delete a post
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
    created_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO comments (post_id, user_id, content)
    VALUES (p_post_id, p_user_id, p_content)
    RETURNING comments.id, comments.post_id, comments.user_id, comments.content, comments.created_at;
END;
$$ LANGUAGE plpgsql;

-- Function: Like a post (insert if not exists, or throw error if already liked)
CREATE FUNCTION fn_like_post(p_post_id INTEGER, p_user_id INTEGER)
RETURNS TABLE (
    id INTEGER,
    post_id INTEGER,
    user_id INTEGER,
    created_at TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO likes (post_id, user_id)
    VALUES (p_post_id, p_user_id)
    ON CONFLICT (post_id, user_id) DO NOTHING
    RETURNING likes.id, likes.post_id, likes.user_id, likes.created_at;
END;
$$ LANGUAGE plpgsql;

-- Function: Unlike a post
CREATE FUNCTION fn_unlike_post(p_post_id INTEGER, p_user_id INTEGER)
RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM likes WHERE post_id = p_post_id AND user_id = p_user_id;
    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Sample data for testing
INSERT INTO users (name, email, bio, avatar_url) VALUES
    ('Alice Johnson', 'alice@example.com', 'Full-stack developer and tech writer', 'https://api.example.com/avatars/alice.jpg'),
    ('Bob Smith', 'bob@example.com', 'DevOps engineer passionate about databases', 'https://api.example.com/avatars/bob.jpg'),
    ('Carol White', 'carol@example.com', 'Frontend specialist and UX enthusiast', 'https://api.example.com/avatars/carol.jpg');

INSERT INTO posts (title, content, author_id, published) VALUES
    ('Getting Started with GraphQL', 'GraphQL is a query language for APIs...', 1, true),
    ('Database Design Best Practices', 'When designing a database schema...', 2, true),
    ('React Hooks Deep Dive', 'Hooks allow you to use state and other React features...', 3, true);

INSERT INTO comments (post_id, user_id, content) VALUES
    (1, 2, 'Great introduction to GraphQL!'),
    (1, 3, 'This helped me understand queries vs mutations'),
    (2, 1, 'The normalization section was really clear'),
    (3, 1, 'Best explanation of useEffect I have seen');

INSERT INTO likes (post_id, user_id) VALUES
    (1, 2), (1, 3),
    (2, 1), (2, 3),
    (3, 1);
```text
<!-- Code example in TEXT -->

### 2.2 Loading the Schema

```bash
<!-- Code example in BASH -->
# From project root with PostgreSQL running
psql -U postgres -d blog_db -f database/schema.sql

# Or with docker-compose (after it's set up):
docker-compose exec postgres psql -U blog_user -d blog_db -f /docker-entrypoint-initdb.d/schema.sql
```text
<!-- Code example in TEXT -->

---

## Part 3: Python Schema Definition

### 3.1 Backend Dependencies

Create `backend/requirements.txt`:

```text
<!-- Code example in TEXT -->
FraiseQL==2.0.0a1
pydantic==2.5.0
```text
<!-- Code example in TEXT -->

### 3.2 Python Schema Definition

Create `backend/schema.py`:

```python
<!-- Code example in Python -->
"""
FraiseQL Blog API Schema

This module defines the GraphQL schema for a blog application using Python
with FraiseQL decorators. The schema is compiled to optimized SQL at build time.
"""

from typing import Optional
from FraiseQL import schema
from datetime import datetime

# Initialize the FraiseQL schema
blog_schema = schema.Schema(
    name="blog-api",
    version="1.0.0",
    description="GraphQL Blog API with FraiseQL"
)

# ============================================================================
# Types
# ============================================================================

@blog_schema.type(
    name="User",
    description="A user account in the blog system"
)
class User:
    """User type representing blog authors and commenters."""
    id: int
    name: str
    email: str
    bio: Optional[str] = None
    avatar_url: Optional[str] = None
    created_at: datetime
    updated_at: datetime


@blog_schema.type(
    name="Post",
    description="A blog post with author and engagement metrics"
)
class Post:
    """Post type representing individual blog articles."""
    id: int
    title: str
    content: str
    author_id: int
    published: bool
    created_at: datetime
    updated_at: datetime
    # Computed fields
    author_name: Optional[str] = None
    author_email: Optional[str] = None
    like_count: Optional[int] = None
    comment_count: Optional[int] = None


@blog_schema.type(
    name="Comment",
    description="A comment on a blog post"
)
class Comment:
    """Comment type for user comments on posts."""
    id: int
    post_id: int
    user_id: int
    content: str
    created_at: datetime
    # Author info
    author_id: Optional[int] = None
    author_name: Optional[str] = None
    author_email: Optional[str] = None
    author_avatar_url: Optional[str] = None


@blog_schema.type(
    name="Like",
    description="A like on a post"
)
class Like:
    """Like type representing post likes."""
    id: int
    post_id: int
    user_id: int
    created_at: datetime


@blog_schema.type(
    name="CreateUserInput",
    kind="INPUT",
    description="Input for creating a new user"
)
class CreateUserInput:
    """Input type for user creation."""
    name: str
    email: str
    bio: Optional[str] = None
    avatar_url: Optional[str] = None


@blog_schema.type(
    name="UpdateUserInput",
    kind="INPUT",
    description="Input for updating a user"
)
class UpdateUserInput:
    """Input type for user updates."""
    name: Optional[str] = None
    bio: Optional[str] = None
    avatar_url: Optional[str] = None


@blog_schema.type(
    name="CreatePostInput",
    kind="INPUT",
    description="Input for creating a new post"
)
class CreatePostInput:
    """Input type for post creation."""
    title: str
    content: str
    author_id: int
    published: bool = False


@blog_schema.type(
    name="UpdatePostInput",
    kind="INPUT",
    description="Input for updating a post"
)
class UpdatePostInput:
    """Input type for post updates."""
    title: Optional[str] = None
    content: Optional[str] = None
    published: Optional[bool] = None


@blog_schema.type(
    name="CreateCommentInput",
    kind="INPUT",
    description="Input for creating a comment"
)
class CreateCommentInput:
    """Input type for comment creation."""
    post_id: int
    user_id: int
    content: str


# ============================================================================
# Queries
# ============================================================================

@blog_schema.query(
    name="users",
    return_type="User",
    returns_list=True,
    description="Get all users in the system"
)
def get_users(
    limit: int = 100,
    offset: int = 0
) -> list[User]:
    """
    Fetch all users with pagination.

    Args:
        limit: Maximum number of users to return (default: 100)
        offset: Number of users to skip (default: 0)

    Returns:
        List of User objects
    """
    return []  # Implementation handled by FraiseQL


@blog_schema.query(
    name="user",
    return_type="User",
    returns_list=False,
    nullable=True,
    description="Get a single user by ID"
)
def get_user(user_id: int) -> Optional[User]:
    """
    Fetch a single user by ID.

    Args:
        user_id: The ID of the user to fetch

    Returns:
        User object or None if not found
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.query(
    name="posts",
    return_type="Post",
    returns_list=True,
    description="Get all published posts with pagination"
)
def get_posts(
    limit: int = 50,
    offset: int = 0
) -> list[Post]:
    """
    Fetch all published posts with pagination and author info.

    Args:
        limit: Maximum number of posts to return (default: 50)
        offset: Number of posts to skip (default: 0)

    Returns:
        List of Post objects with author information and engagement metrics
    """
    return []  # Implementation handled by FraiseQL


@blog_schema.query(
    name="post",
    return_type="Post",
    returns_list=False,
    nullable=True,
    description="Get a single post by ID with full details"
)
def get_post(post_id: int) -> Optional[Post]:
    """
    Fetch a single post by ID with full details.

    Args:
        post_id: The ID of the post to fetch

    Returns:
        Post object with author info and engagement metrics, or None if not found
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.query(
    name="postsByAuthor",
    return_type="Post",
    returns_list=True,
    description="Get all posts by a specific author"
)
def get_posts_by_author(
    author_id: int,
    limit: int = 50,
    offset: int = 0
) -> list[Post]:
    """
    Fetch posts by a specific author.

    Args:
        author_id: The ID of the author
        limit: Maximum number of posts to return
        offset: Number of posts to skip

    Returns:
        List of Post objects by the specified author
    """
    return []  # Implementation handled by FraiseQL


@blog_schema.query(
    name="comments",
    return_type="Comment",
    returns_list=True,
    description="Get all comments for a post"
)
def get_comments(
    post_id: int,
    limit: int = 100,
    offset: int = 0
) -> list[Comment]:
    """
    Fetch comments for a specific post.

    Args:
        post_id: The ID of the post
        limit: Maximum number of comments to return
        offset: Number of comments to skip

    Returns:
        List of Comment objects with author information
    """
    return []  # Implementation handled by FraiseQL


# ============================================================================
# Mutations
# ============================================================================

@blog_schema.mutation(
    name="createUser",
    return_type="User",
    returns_list=False,
    description="Create a new user account"
)
def create_user(input_data: CreateUserInput) -> User:
    """
    Create a new user account.

    Args:
        input_data: User creation input

    Returns:
        The created User object
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.mutation(
    name="updateUser",
    return_type="User",
    returns_list=False,
    nullable=True,
    description="Update an existing user"
)
def update_user(user_id: int, input_data: UpdateUserInput) -> Optional[User]:
    """
    Update an existing user.

    Args:
        user_id: The ID of the user to update
        input_data: User update input

    Returns:
        The updated User object, or None if not found
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.mutation(
    name="createPost",
    return_type="Post",
    returns_list=False,
    description="Create a new blog post"
)
def create_post(input_data: CreatePostInput) -> Post:
    """
    Create a new blog post.

    Args:
        input_data: Post creation input

    Returns:
        The created Post object
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.mutation(
    name="updatePost",
    return_type="Post",
    returns_list=False,
    nullable=True,
    description="Update an existing post"
)
def update_post(post_id: int, input_data: UpdatePostInput) -> Optional[Post]:
    """
    Update an existing post.

    Args:
        post_id: The ID of the post to update
        input_data: Post update input

    Returns:
        The updated Post object, or None if not found
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.mutation(
    name="deletePost",
    return_type="Post",
    returns_list=False,
    nullable=True,
    description="Delete a blog post"
)
def delete_post(post_id: int) -> Optional[Post]:
    """
    Delete a blog post.

    Args:
        post_id: The ID of the post to delete

    Returns:
        The deleted Post object, or None if not found
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.mutation(
    name="createComment",
    return_type="Comment",
    returns_list=False,
    description="Create a new comment on a post"
)
def create_comment(input_data: CreateCommentInput) -> Comment:
    """
    Create a new comment on a post.

    Args:
        input_data: Comment creation input

    Returns:
        The created Comment object
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.mutation(
    name="likePost",
    return_type="Like",
    returns_list=False,
    description="Like a blog post"
)
def like_post(post_id: int, user_id: int) -> Like:
    """
    Like a blog post. Prevents duplicate likes.

    Args:
        post_id: The ID of the post to like
        user_id: The ID of the user liking the post

    Returns:
        The Like object
    """
    return None  # Implementation handled by FraiseQL


@blog_schema.mutation(
    name="unlikePost",
    return_type="Like",
    returns_list=False,
    nullable=True,
    description="Unlike a blog post"
)
def unlike_post(post_id: int, user_id: int) -> Optional[Like]:
    """
    Remove a like from a blog post.

    Args:
        post_id: The ID of the post to unlike
        user_id: The ID of the user removing their like

    Returns:
        The removed Like object, or None if the like didn't exist
    """
    return None  # Implementation handled by FraiseQL


# ============================================================================
# Schema Export
# ============================================================================

if __name__ == "__main__":
    import json
    import sys

    # Export schema to JSON
    try:
        schema_json = blog_schema.to_json()

        # Write to file
        with open("schema.json", "w") as f:
            json.dump(schema_json, f, indent=2)

        print("✓ Schema exported to schema.json")
        print(f"  Types: {len(schema_json.get('types', []))}")
        print(f"  Queries: {len(schema_json.get('queries', []))}")
        print(f"  Mutations: {len(schema_json.get('mutations', []))}")

    except Exception as e:
        print(f"✗ Error exporting schema: {e}", file=sys.stderr)
        sys.exit(1)
```text
<!-- Code example in TEXT -->

### 3.3 Exporting the Schema

```bash
<!-- Code example in BASH -->
# From backend directory
python schema.py

# Output:
# ✓ Schema exported to schema.json
#   Types: 8
#   Queries: 6
#   Mutations: 8
```text
<!-- Code example in TEXT -->

This generates `schema.json` containing all types, queries, and mutations.

---

## Part 4: Compile with FraiseQL CLI

### 4.1 FraiseQL Configuration

Create `backend/FraiseQL.toml`:

```toml
<!-- Code example in TOML -->
[FraiseQL]
name = "blog-api"
version = "1.0.0"
description = "Full-stack blog API with FraiseQL"

[FraiseQL.database]
adapter = "postgresql"
host = "localhost"
port = 5432
name = "blog_db"
user = "blog_user"
password = "blog_password"

[FraiseQL.security]
# Error handling
error_sanitization = true
log_errors = true

# Rate limiting
[FraiseQL.security.rate_limiting]
enabled = true
auth_start_max_requests = 1000
auth_start_window_secs = 60

# CORS configuration
[FraiseQL.server]
port = 8000
host = "0.0.0.0"
cors_origins = ["http://localhost:5173", "http://localhost:3000"]
graphql_path = "/graphql"
health_path = "/health"
```text
<!-- Code example in TEXT -->

### 4.2 Compile the Schema

```bash
<!-- Code example in BASH -->
# From backend directory
FraiseQL-cli compile schema.json FraiseQL.toml

# Output:
# ✓ Compilation successful
#   Schema: blog-api v1.0.0
#   Types: 8
#   Queries: 6 (optimized to 4 SQL queries)
#   Mutations: 8 (optimized to 6 SQL functions)
#   Output: schema.compiled.json
```text
<!-- Code example in TEXT -->

This generates `schema.compiled.json` containing:

- All type definitions
- Optimized GraphQL queries
- Mapping to SQL views/functions
- Security configuration
- Server configuration

---

## Part 5: FraiseQL Server Deployment

### 5.1 Dockerfile

Create `backend/Dockerfile`:

```dockerfile
<!-- Code example in DOCKERFILE -->
# FraiseQL server is already compiled to Rust binary
# We just need to set up the runtime environment

FROM rust:1.75 as builder

WORKDIR /app

# Copy schema
COPY schema.compiled.json .
COPY FraiseQL.toml .

# Build FraiseQL server (pre-installed in the environment)
RUN FraiseQL-server --version

FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

# Copy FraiseQL server binary (pre-built)
COPY --from=builder /usr/local/bin/FraiseQL-server /usr/local/bin/

# Copy schema and config
COPY schema.compiled.json .
COPY FraiseQL.toml .

# Health check
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Expose GraphQL API
EXPOSE 8000

# Start FraiseQL server
CMD ["FraiseQL-server", "--schema", "schema.compiled.json", "--config", "FraiseQL.toml"]
```text
<!-- Code example in TEXT -->

### 5.2 Docker Compose Orchestration

Create `docker-compose.yml` in the project root:

```yaml
<!-- Code example in YAML -->
version: '3.9'

services:
  postgres:
    image: postgres:16-alpine
    container_name: blog-postgres
    environment:
      POSTGRES_DB: blog_db
      POSTGRES_USER: blog_user
      POSTGRES_PASSWORD: blog_password
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./database/schema.sql:/docker-entrypoint-initdb.d/schema.sql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U blog_user -d blog_db"]
      interval: 10s
      timeout: 5s
      retries: 5

  FraiseQL-server:
    build:
      context: ./backend
      dockerfile: Dockerfile
    container_name: blog-FraiseQL
    environment:
      DATABASE_URL: postgres://blog_user:blog_password@postgres:5432/blog_db
      RUST_LOG: info
    ports:
      - "8000:8000"
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
    volumes:
      - ./backend/schema.compiled.json:/app/schema.compiled.json
      - ./backend/FraiseQL.toml:/app/FraiseQL.toml

volumes:
  postgres_data:
```text
<!-- Code example in TEXT -->

### 5.3 Launching the Backend

```bash
<!-- Code example in BASH -->
# From project root
docker-compose up -d

# Check status
docker-compose ps

# View logs
docker-compose logs -f FraiseQL-server

# Test the GraphQL API
curl http://localhost:8000/health

# Test health endpoint
curl http://localhost:8000/graphql -X POST \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 10) { id name email } }"}'
```text
<!-- Code example in TEXT -->

---

## Part 6: React Frontend Setup

### 6.1 Apollo Client Configuration

Create `frontend/src/apollo-client.js`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { ApolloClient, InMemoryCache, HttpLink, gql } from "@apollo/client";

const httpLink = new HttpLink({
  uri: process.env.REACT_APP_GRAPHQL_API || "http://localhost:8000/graphql",
  credentials: "include",
});

const client = new ApolloClient({
  link: httpLink,
  cache: new InMemoryCache(),
  defaultOptions: {
    watchQuery: {
      fetchPolicy: "cache-and-network",
    },
  },
});

export default client;
```text
<!-- Code example in TEXT -->

### 6.2 Environment Configuration

Create `frontend/.env.local`:

```env
<!-- Code example in ENV -->
REACT_APP_GRAPHQL_API=http://localhost:8000/graphql
```text
<!-- Code example in TEXT -->

### 6.3 GraphQL Queries & Mutations

Create `frontend/src/queries.js`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { gql } from "@apollo/client";

// Queries
export const GET_POSTS = gql`
  query GetPosts($limit: Int, $offset: Int) {
    posts(limit: $limit, offset: $offset) {
      id
      title
      content
      published
      createdAt
      updatedAt
      authorName
      authorEmail
      likeCount
      commentCount
    }
  }
`;

export const GET_POST = gql`
  query GetPost($postId: Int!) {
    post(postId: $postId) {
      id
      title
      content
      published
      createdAt
      updatedAt
      authorId
      authorName
      authorEmail
      likeCount
      commentCount
    }
  }
`;

export const GET_COMMENTS = gql`
  query GetComments($postId: Int!, $limit: Int, $offset: Int) {
    comments(postId: $postId, limit: $limit, offset: $offset) {
      id
      postId
      userId
      content
      createdAt
      authorId
      authorName
      authorEmail
      authorAvatarUrl
    }
  }
`;

export const GET_USERS = gql`
  query GetUsers($limit: Int, $offset: Int) {
    users(limit: $limit, offset: $offset) {
      id
      name
      email
      bio
      avatarUrl
      createdAt
    }
  }
`;

// Mutations
export const CREATE_POST = gql`
  mutation CreatePost($input: CreatePostInput!) {
    createPost(inputData: $input) {
      id
      title
      content
      published
      createdAt
      authorId
      authorName
    }
  }
`;

export const UPDATE_POST = gql`
  mutation UpdatePost($postId: Int!, $input: UpdatePostInput!) {
    updatePost(postId: $postId, inputData: $input) {
      id
      title
      content
      published
      updatedAt
    }
  }
`;

export const DELETE_POST = gql`
  mutation DeletePost($postId: Int!) {
    deletePost(postId: $postId) {
      id
      title
    }
  }
`;

export const CREATE_COMMENT = gql`
  mutation CreateComment($input: CreateCommentInput!) {
    createComment(inputData: $input) {
      id
      postId
      userId
      content
      createdAt
      authorName
    }
  }
`;

export const LIKE_POST = gql`
  mutation LikePost($postId: Int!, $userId: Int!) {
    likePost(postId: $postId, userId: $userId) {
      id
      postId
      userId
      createdAt
    }
  }
`;

export const UNLIKE_POST = gql`
  mutation UnlikePost($postId: Int!, $userId: Int!) {
    unlikePost(postId: $postId, userId: $userId) {
      id
      postId
      userId
    }
  }
`;
```text
<!-- Code example in TEXT -->

---

## Part 7: React Components

### 7.1 PostList Component

Create `frontend/src/components/PostList.jsx`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { useQuery } from "@apollo/client";
import { GET_POSTS } from "../queries";
import PostCard from "./PostCard";
import "./PostList.css";

export default function PostList() {
  const { loading, error, data } = useQuery(GET_POSTS, {
    variables: { limit: 20, offset: 0 },
  });

  if (loading) return <div className="loading">Loading posts...</div>;
  if (error) return <div className="error">Error: {error.message}</div>;

  const posts = data?.posts || [];

  return (
    <div className="post-list">
      <h1>Recent Posts</h1>
      {posts.length === 0 ? (
        <p className="no-posts">No posts yet. Be the first to write one!</p>
      ) : (
        <div className="posts-grid">
          {posts.map((post) => (
            <PostCard key={post.id} post={post} />
          ))}
        </div>
      )}
    </div>
  );
}
```text
<!-- Code example in TEXT -->

### 7.2 PostCard Component

Create `frontend/src/components/PostCard.jsx`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { Link } from "react-router-dom";
import LikeButton from "./LikeButton";
import "./PostCard.css";

export default function PostCard({ post }) {
  return (
    <div className="post-card">
      <div className="post-header">
        <h2>{post.title}</h2>
        <span className={`status ${post.published ? "published" : "draft"}`}>
          {post.published ? "Published" : "Draft"}
        </span>
      </div>

      <p className="post-content">{post.content.substring(0, 150)}...</p>

      <div className="post-metadata">
        <span className="author">By {post.authorName}</span>
        <span className="date">
          {new Date(post.createdAt).toLocaleDateString()}
        </span>
      </div>

      <div className="post-footer">
        <div className="engagement">
          <span className="likes">❤️ {post.likeCount} likes</span>
          <span className="comments">💬 {post.commentCount} comments</span>
        </div>
        <Link to={`/post/${post.id}`} className="read-more">
          Read More →
        </Link>
      </div>
    </div>
  );
}
```text
<!-- Code example in TEXT -->

### 7.3 PostDetail Component

Create `frontend/src/components/PostDetail.jsx`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { useQuery, useMutation } from "@apollo/client";
import { useParams } from "react-router-dom";
import { GET_POST, UPDATE_POST, DELETE_POST } from "../queries";
import CommentSection from "./CommentSection";
import LikeButton from "./LikeButton";
import "./PostDetail.css";

export default function PostDetail() {
  const { postId } = useParams();
  const { loading, error, data } = useQuery(GET_POST, {
    variables: { postId: parseInt(postId) },
  });

  const [updatePost] = useMutation(UPDATE_POST);
  const [deletePost] = useMutation(DELETE_POST);

  if (loading) return <div className="loading">Loading post...</div>;
  if (error) return <div className="error">Error: {error.message}</div>;

  const post = data?.post;

  if (!post) return <div className="not-found">Post not found</div>;

  return (
    <div className="post-detail">
      <div className="post-header-detail">
        <h1>{post.title}</h1>
        <div className="post-info">
          <span className="author">{post.authorName}</span>
          <span className="date">
            {new Date(post.createdAt).toLocaleString()}
          </span>
        </div>
      </div>

      <div className="post-content-detail">{post.content}</div>

      <div className="post-actions">
        <LikeButton postId={post.id} likeCount={post.likeCount} />
        <span className="comment-count">
          💬 {post.commentCount} comments
        </span>
      </div>

      <CommentSection postId={post.id} />
    </div>
  );
}
```text
<!-- Code example in TEXT -->

### 7.4 CommentSection Component

Create `frontend/src/components/CommentSection.jsx`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { useQuery, useMutation } from "@apollo/client";
import { useState } from "react";
import { GET_COMMENTS, CREATE_COMMENT } from "../queries";
import "./CommentSection.css";

export default function CommentSection({ postId }) {
  const [commentText, setCommentText] = useState("");
  const [currentUserId] = useState(1); // In a real app, get from auth

  const { data, loading } = useQuery(GET_COMMENTS, {
    variables: { postId, limit: 50, offset: 0 },
  });

  const [createComment] = useMutation(CREATE_COMMENT, {
    refetchQueries: [{ query: GET_COMMENTS, variables: { postId } }],
  });

  const handleSubmitComment = async (e) => {
    e.preventDefault();
    if (!commentText.trim()) return;

    await createComment({
      variables: {
        input: {
          postId,
          userId: currentUserId,
          content: commentText,
        },
      },
    });

    setCommentText("");
  };

  const comments = data?.comments || [];

  return (
    <div className="comment-section">
      <h3>Comments ({comments.length})</h3>

      <form onSubmit={handleSubmitComment} className="comment-form">
        <textarea
          value={commentText}
          onChange={(e) => setCommentText(e.target.value)}
          placeholder="Write a comment..."
          rows="3"
        />
        <button type="submit" disabled={!commentText.trim()}>
          Post Comment
        </button>
      </form>

      <div className="comments-list">
        {comments.map((comment) => (
          <div key={comment.id} className="comment">
            <div className="comment-header">
              <strong>{comment.authorName}</strong>
              <span className="comment-date">
                {new Date(comment.createdAt).toLocaleString()}
              </span>
            </div>
            <p>{comment.content}</p>
          </div>
        ))}
      </div>
    </div>
  );
}
```text
<!-- Code example in TEXT -->

### 7.5 LikeButton Component

Create `frontend/src/components/LikeButton.jsx`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { useMutation } from "@apollo/client";
import { useState } from "react";
import { LIKE_POST, UNLIKE_POST } from "../queries";
import "./LikeButton.css";

export default function LikeButton({ postId, likeCount = 0 }) {
  const [liked, setLiked] = useState(false);
  const [count, setCount] = useState(likeCount);
  const [currentUserId] = useState(1); // In a real app, get from auth

  const [likePost] = useMutation(LIKE_POST);
  const [unlikePost] = useMutation(UNLIKE_POST);

  const handleToggleLike = async () => {
    try {
      if (liked) {
        await unlikePost({
          variables: { postId, userId: currentUserId },
        });
        setCount(count - 1);
        setLiked(false);
      } else {
        await likePost({
          variables: { postId, userId: currentUserId },
        });
        setCount(count + 1);
        setLiked(true);
      }
    } catch (error) {
      console.error("Error toggling like:", error);
    }
  };

  return (
    <button
      className={`like-button ${liked ? "liked" : ""}`}
      onClick={handleToggleLike}
    >
      ❤️ {count} {count === 1 ? "like" : "likes"}
    </button>
  );
}
```text
<!-- Code example in TEXT -->

---

## Part 8: Main Application

### 8.1 App.jsx

Create `frontend/src/App.jsx`:

```javascript
<!-- Code example in JAVASCRIPT -->
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { ApolloProvider } from "@apollo/client";
import client from "./apollo-client";
import PostList from "./components/PostList";
import PostDetail from "./components/PostDetail";
import CreatePostForm from "./components/CreatePostForm";
import Navigation from "./components/Navigation";
import "./App.css";

export default function App() {
  return (
    <ApolloProvider client={client}>
      <BrowserRouter>
        <Navigation />
        <div className="container">
          <Routes>
            <Route path="/" element={<PostList />} />
            <Route path="/post/:postId" element={<PostDetail />} />
            <Route path="/create" element={<CreatePostForm />} />
          </Routes>
        </div>
      </BrowserRouter>
    </ApolloProvider>
  );
}
```text
<!-- Code example in TEXT -->

### 8.2 package.json Scripts

Update `frontend/package.json`:

```json
<!-- Code example in JSON -->
{
  "name": "blog-frontend",
  "private": true,
  "version": "0.0.1",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "lint": "eslint src --ext js,jsx"
  },
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "react-router-dom": "^6.18.0",
    "@apollo/client": "^3.8.0",
    "graphql": "^16.8.0",
    "graphql-tag": "^2.12.0"
  },
  "devDependencies": {
    "@vitejs/plugin-react": "^4.2.0",
    "vite": "^5.0.0"
  }
}
```text
<!-- Code example in TEXT -->

---

## Part 9: Running the Full Stack

### 9.1 Step-by-Step Launch

```bash
<!-- Code example in BASH -->
# 1. Start PostgreSQL and FraiseQL server
cd fullstack-blog
docker-compose up -d

# Check server is ready
curl http://localhost:8000/health

# 2. Start React development server (in new terminal)
cd frontend
npm install
npm run dev

# Output: Local: http://localhost:5173

# 3. Open browser and visit http://localhost:5173
# You should see the blog homepage with posts!
```text
<!-- Code example in TEXT -->

### 9.2 Full-Stack Test

```bash
<!-- Code example in BASH -->
# Test the GraphQL API directly
curl http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { posts(limit: 10) { id title authorName likeCount } }"
  }'

# Expected response:
# {
#   "data": {
#     "posts": [
#       {
#         "id": 1,
#         "title": "Getting Started with GraphQL",
#         "authorName": "Alice Johnson",
#         "likeCount": 2
#       },
#       ...
#     ]
#   }
# }
```text
<!-- Code example in TEXT -->

### 9.3 Workflow

1. **User visits <http://localhost:5173>**
2. **React loads Apollo Client** with FraiseQL endpoint
3. **React component runs GET_POSTS query**
4. **Apollo sends GraphQL to <http://localhost:8000/graphql>**
5. **FraiseQL server compiles query to SQL**
6. **PostgreSQL executes query**
7. **Results return through full chain to React**
8. **Components render with data**

---

## Part 10: Complete Example Workflow

### 10.1 Creating a New Post

**Frontend (React):**

```javascript
<!-- Code example in JAVASCRIPT -->
// User fills form and submits
const handleCreatePost = async (title, content) => {
  const result = await createPost({
    variables: {
      input: {
        title,
        content,
        author_id: 1,  // Current user
        published: false,
      },
    },
  });
  return result.data.createPost;
};
```text
<!-- Code example in TEXT -->

**Execution Path:**

```text
<!-- Code example in TEXT -->
React Component
    ↓ (Apollo sends GraphQL)
FraiseQL Server (port 8000)
    ↓ (compiles to SQL)
fn_create_post('title', 'content', 1, false)
    ↓ (PostgreSQL function)
posts table INSERT + RETURNING
    ↓ (returns new row)
FraiseQL formats as JSON
    ↓ (GraphQL response)
Apollo caches result
    ↓
React re-renders with new post
```text
<!-- Code example in TEXT -->

### 10.2 Fetching Post with Comments

**React Query:**

```javascript
<!-- Code example in JAVASCRIPT -->
export const GET_POST_WITH_COMMENTS = gql`
  query GetPostDetails($postId: Int!) {
    post(postId: $postId) {
      id
      title
      content
      likeCount
      comments(limit: 20) {
        id
        content
        authorName
      }
    }
  }
`;
```text
<!-- Code example in TEXT -->

**Execution Path:**

```text
<!-- Code example in TEXT -->
FraiseQL Server receives query
    ↓
Looks up "post" query → v_post_detail view
Looks up "comments" query → v_comments view
    ↓ (parallel execution)
v_post_detail WHERE id = 1
v_comments WHERE post_id = 1 LIMIT 20
    ↓
Results combined into single JSON
    ↓
React receives nested structure
    ↓
CommentSection component maps over comments
```text
<!-- Code example in TEXT -->

---

## Part 11: Deployment to Production

### 11.1 Docker Build for Production

```bash
<!-- Code example in BASH -->
# Build images for production
docker-compose build

# Push to registry (optional)
docker tag blog-FraiseQL-server myregistry/blog-FraiseQL:v1.0
docker push myregistry/blog-FraiseQL:v1.0
```text
<!-- Code example in TEXT -->

### 11.2 Kubernetes Deployment

Create `k8s/deployment.yaml`:

```yaml
<!-- Code example in YAML -->
apiVersion: apps/v1
kind: Deployment
metadata:
  name: blog-FraiseQL-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: blog-FraiseQL-server
  template:
    metadata:
      labels:
        app: blog-FraiseQL-server
    spec:
      containers:
      - name: FraiseQL-server
        image: myregistry/blog-FraiseQL:v1.0
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: blog-secrets
              key: database-url
        healthCheck:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 10
---
apiVersion: v1
kind: Service
metadata:
  name: blog-FraiseQL-service
spec:
  selector:
    app: blog-FraiseQL-server
  ports:
  - protocol: TCP
    port: 80
    targetPort: 8000
  type: LoadBalancer
```text
<!-- Code example in TEXT -->

### 11.3 React Frontend Production Build

```bash
<!-- Code example in BASH -->
cd frontend
npm run build
# Output: dist/

# Deploy to Vercel, Netlify, or S3
# Update REACT_APP_GRAPHQL_API to production endpoint
REACT_APP_GRAPHQL_API=https://api.example.com/graphql npm run build
```text
<!-- Code example in TEXT -->

---

## Part 12: Monitoring and Debugging

### 12.1 Health Checks

```bash
<!-- Code example in BASH -->
# Check server health
curl http://localhost:8000/health

# Response (when healthy):
# {"status": "ok", "version": "1.0.0"}
```text
<!-- Code example in TEXT -->

### 12.2 GraphQL Introspection

```bash
<!-- Code example in BASH -->
# Get GraphQL schema (for tools like GraphQL Playground)
curl http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { types { name } } }"}'
```text
<!-- Code example in TEXT -->

### 12.3 Logs

```bash
<!-- Code example in BASH -->
# View FraiseQL server logs
docker-compose logs -f FraiseQL-server

# View PostgreSQL logs
docker-compose logs -f postgres

# View React build logs
npm run dev 2>&1 | tee frontend.log
```text
<!-- Code example in TEXT -->

### 12.4 Performance Analysis

```bash
<!-- Code example in BASH -->
# Test query performance
time curl http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ posts(limit: 100) { id title } }"
  }'
```text
<!-- Code example in TEXT -->

---

## Part 13: Troubleshooting Full Stack

### Problem: "Cannot POST /graphql"

**Cause:** FraiseQL server not running or wrong endpoint

**Solution:**

```bash
<!-- Code example in BASH -->
docker-compose ps  # Check if FraiseQL-server is running
curl http://localhost:8000/health  # Test connectivity
```text
<!-- Code example in TEXT -->

### Problem: Apollo Client returns "Network error"

**Cause:** CORS not configured or server unreachable

**Solution:**

```bash
<!-- Code example in BASH -->
# Check CORS settings in FraiseQL.toml
# Ensure React app URL is in cors_origins
curl -H "Origin: http://localhost:5173" http://localhost:8000/health
```text
<!-- Code example in TEXT -->

### Problem: "Relation 'v_posts' does not exist"

**Cause:** Database schema not initialized

**Solution:**

```bash
<!-- Code example in BASH -->
# Reload database schema
docker-compose exec postgres psql -U blog_user -d blog_db -f /docker-entrypoint-initdb.d/schema.sql
```text
<!-- Code example in TEXT -->

### Problem: React components show "Loading..." indefinitely

**Cause:** GraphQL query failing silently

**Solution:**

```javascript
<!-- Code example in JAVASCRIPT -->
// Add error handling to Apollo queries
const { loading, error, data } = useQuery(GET_POSTS);

if (error) {
  console.error("GraphQL Error:", error);
  return <div>Error: {error.message}</div>;
}
```text
<!-- Code example in TEXT -->

---

## Part 14: Next Steps

### Advanced Topics

1. **Authentication & Authorization**
   - Add JWT token validation
   - Implement field-level permissions
   - Audit logging of operations

2. **Caching & Performance**
   - Enable query result caching
   - Implement Automatic Persisted Queries (APQ)
   - Add Redis for distributed caching

3. **Subscriptions & Real-Time**
   - Implement WebSocket subscriptions
   - Real-time comment updates
   - Live like counters

4. **Observability**
   - Instrument with OpenTelemetry
   - Distributed tracing across stack
   - Performance monitoring

5. **Database Optimization**
   - Add query indexes
   - Implement materialized views
   - Connection pooling tuning

### Learning Resources

- **FraiseQL CLI Reference:** `../reference/cli.md`
- **Schema Design Guide:** `../guid../../docs/architecture/core/schema-design.md`
- **React & Apollo Best Practices:** `../guides/frontend-integration.md`
- **Database Design:** `../patterns/database-schema-patterns.md`

---

## Part 15: Complete Directory Tree

After following this tutorial, your project structure should be:

```text
<!-- Code example in TEXT -->
fullstack-blog/
├── backend/
│   ├── schema.py                    # Python schema (authoring)
│   ├── schema.json                  # Exported schema
│   ├── schema.compiled.json         # Compiled schema (generated)
│   ├── FraiseQL.toml                # Server config
│   ├── Dockerfile                   # Server container
│   ├── requirements.txt             # Python deps
│   └── venv/                        # Python virtual env
├── frontend/
│   ├── public/
│   │   └── index.html
│   ├── src/
│   │   ├── components/
│   │   │   ├── PostList.jsx
│   │   │   ├── PostCard.jsx
│   │   │   ├── PostDetail.jsx
│   │   │   ├── CreatePostForm.jsx
│   │   │   ├── CommentSection.jsx
│   │   │   ├── LikeButton.jsx
│   │   │   └── Navigation.jsx
│   │   ├── apollo-client.js         # GraphQL client config
│   │   ├── queries.js               # GraphQL queries/mutations
│   │   ├── App.jsx
│   │   ├── App.css
│   │   ├── index.jsx
│   │   └── main.jsx
│   ├── package.json
│   ├── .env.local                   # API endpoint config
│   ├── vite.config.js               # Build config
│   └── .gitignore
├── database/
│   └── schema.sql                   # PostgreSQL DDL + views + functions
├── docker-compose.yml               # Full-stack orchestration
└── README.md
```text
<!-- Code example in TEXT -->

---

## Summary

You now have a **complete, production-ready full-stack application**:

- **Backend:** Python schema authoring → FraiseQL compilation → Rust GraphQL server
- **Frontend:** React components with Apollo Client
- **Database:** PostgreSQL with optimized views and functions
- **Deployment:** Docker Compose for local dev, Kubernetes for production

The key insight: **FraiseQL is the compiled GraphQL backend**. You write Python, compile once, and deploy the optimized server. No runtime overhead, pure performance.

### Quick Commands Reference

```bash
<!-- Code example in BASH -->
# Backend
cd backend && python schema.py export              # Export schema
FraiseQL-cli compile schema.json FraiseQL.toml     # Compile
docker build -t blog-server .                      # Build container

# Frontend
cd frontend && npm install && npm run dev          # Dev server

# Full Stack
docker-compose up -d                               # Start services
curl http://localhost:8000/health                  # Test health
open http://localhost:5173                         # Open frontend
```text
<!-- Code example in TEXT -->

**Your application is now running at <http://localhost:5173>!**

---

## Feedback

Have questions or improvements? See `/docs/tutorials/README.md` for support channels.

**Back to:** [Tutorials Home](./README.md) | [Documentation Home](../README.md)
