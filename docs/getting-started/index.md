# Getting Started

Welcome to FraiseQL! This guide will help you get up and running with your first GraphQL API.

## What You'll Learn

By the end of this section, you'll understand:

- How to install and configure FraiseQL
- The core concepts behind FraiseQL's approach
- How to build your first GraphQL API
- How to use the GraphQL playground for testing
- Best practices for structuring your application

## Prerequisites

Before you begin, you should have:

- **Python 3.10 or higher** - FraiseQL uses modern Python type hints
- **PostgreSQL 13 or higher** - For JSONB and advanced SQL features
- **Basic SQL knowledge** - You'll be writing views and functions
- **Familiarity with GraphQL concepts** - Helpful but not required

## The FraiseQL Approach

FraiseQL takes a unique approach to building GraphQL APIs:

1. **Database-First**: Your PostgreSQL views define your API's data structure
2. **Type-Safe**: Python type hints generate your GraphQL schema
3. **Performance-Focused**: Let PostgreSQL optimize your queries
4. **CQRS Pattern**: Separate read (views) and write (functions) operations

## Quick Overview

Here's the typical workflow for building a FraiseQL API:

### 1. Design Your Database Schema

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);
```

### 2. Create Views for Queries

```sql
CREATE VIEW v_user AS
SELECT 
    id,  -- Separate column for filtering
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM users;
```

### 3. Define Your GraphQL Types

```python
from fraiseql import FraiseQL, ID
from datetime import datetime
from dataclasses import dataclass

@fraiseql.type
class User:
    id: ID
    name: str
    email: str
    created_at: datetime
```

### 4. Implement Queries

```python
@app.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    return await repo.find("v_user")
```

### 5. Add Mutations

```sql
-- PostgreSQL function for business logic
CREATE FUNCTION fn_create_user(
    p_name TEXT,
    p_email TEXT
) RETURNS UUID AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO users (name, email)
    VALUES (p_name, p_email)
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$ LANGUAGE plpgsql;
```

```python
@app.mutation
async def create_user(info, name: str, email: str) -> User:
    repo = info.context["repo"]
    user_id = await repo.call_function(
        "fn_create_user",
        p_name=name,
        p_email=email
    )
    result = await repo.find_one("v_user", where={"id": user_id})
    return User(**result)
```

## Your Learning Path

<div class="grid cards" markdown>

-   :material-numeric-1-circle:{ .lg .middle } **Installation**

    ---

    Set up your development environment

    [:octicons-arrow-right-24: Install FraiseQL](installation.md)

-   :material-numeric-2-circle:{ .lg .middle } **5-Minute Quickstart**

    ---

    Build your first API from scratch

    [:octicons-arrow-right-24: Quick Start](quickstart.md)

-   :material-numeric-3-circle:{ .lg .middle } **GraphQL Playground**

    ---

    Test and explore your API interactively

    [:octicons-arrow-right-24: Use Playground](graphql-playground.md)

-   :material-numeric-4-circle:{ .lg .middle } **Your First Real API**

    ---

    Build a complete, production-ready API

    [:octicons-arrow-right-24: Build API](first-api.md)

</div>

## Key Concepts to Remember

!!! info "View Naming Conventions"
    - `v_` - Regular views (computed on demand)
    - `tv_` - Table views (materialized for performance)
    - `fn_` - PostgreSQL functions for mutations

!!! tip "Performance Tips"
    - Include commonly filtered columns separately in views
    - Use JSONB aggregation for nested data
    - Let PostgreSQL handle joins and optimization

!!! warning "Common Mistakes"
    - Forgetting the `data` column with JSONB in views
    - Missing type hints (they define your schema!)
    - Not handling `None` values with `| None` syntax

## Getting Help

- **Examples**: Check the `/examples` directory in the repository
- **API Reference**: See the [API documentation](../api-reference/index.md)
- **Community**: Open an issue on [GitHub](https://github.com/fraiseql/fraiseql)

## Ready?

Let's start by [installing FraiseQL](installation.md) →