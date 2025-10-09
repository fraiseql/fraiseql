---
← [Home](../index.md) | [Learning Paths](index.md) | [Next: Backend Developer](backend-developer.md) →
---

# Learning Path: Beginner

> **For:** Developers new to GraphQL or FraiseQL
> **Time to complete:** 2-3 hours
> **Goal:** Build your first working GraphQL API with FraiseQL

Welcome to FraiseQL! This learning path will take you from zero to building your first GraphQL API. No prior GraphQL experience required - just basic SQL and Python knowledge.

## Prerequisites

Before starting, ensure you have:

- Python 3.13 or higher installed
- PostgreSQL installed and running
- Basic understanding of SQL queries
- Familiarity with Python functions and decorators

## Learning Journey

### 📚 Phase 1: Foundation (30 minutes)

Start here to understand what FraiseQL is and why it's different:

1. **[Introduction](../index.md)** *(5 min)*

   - What is FraiseQL?
   - Key benefits and use cases
   - Architecture overview

2. **[Core Concepts](../core-concepts/index.md)** *(10 min)*

   - Database-first philosophy
   - CQRS pattern basics
   - Type safety principles

3. **[Architecture Overview](../core-concepts/architecture.md)** *(15 min)*

   - How FraiseQL works
   - Request flow
   - Database views concept

### 🚀 Phase 2: Hands-On Basics (45 minutes)

Get your hands dirty with actual code:

4. **[5-Minute Quickstart](../getting-started/quickstart.md)** *(5 min)*

   - Copy-paste example
   - See FraiseQL in action
   - Understand the basic pattern

5. **[Installation Guide](../getting-started/installation.md)** *(10 min)*

   - Detailed setup instructions
   - Environment configuration
   - Troubleshooting tips

6. **[GraphQL Playground](../getting-started/graphql-playground.md)** *(10 min)*

   - Interactive testing
   - Writing queries
   - Understanding responses

7. **[Your First API](../getting-started/first-api.md)** *(20 min)*

   - Build a real user management API
   - Add authentication
   - Handle errors properly

### 🔧 Phase 3: Core Skills (45 minutes)

Deepen your understanding of key concepts:

8. **[Type System](../core-concepts/type-system.md)** *(15 min)*

   - GraphQL types in Python
   - Built-in scalar types
   - Custom type definitions

9. **[Database Views](../core-concepts/database-views.md)** *(15 min)*

   - View patterns
   - JSONB optimization
   - Query performance

10. **[Query Translation](../core-concepts/query-translation.md)** *(15 min)*

    - GraphQL to SQL conversion
    - N+1 query prevention
    - Performance optimization

### 🎯 Phase 4: Complete Example (30 minutes)

Put it all together with a real application:

11. **[Blog API Tutorial](../tutorials/blog-api.md)** *(30 min)*

    - Complete production example
    - Posts, comments, users
    - Best practices demonstrated

## Skills You'll Have

After completing this path, you'll be able to:

✅ Create PostgreSQL views for FraiseQL
✅ Define GraphQL types with Python
✅ Write queries and mutations
✅ Handle errors properly
✅ Use the GraphQL playground
✅ Understand CQRS architecture
✅ Optimize query performance

## Quick Reference

### Essential Commands

```bash
# Install FraiseQL
pip install fraiseql

# Create a database
createdb my_app

# Run your API
uvicorn app:app --reload

# Open GraphQL Playground
# Navigate to: http://localhost:8000/graphql
```

### Basic Pattern

```python
import fraiseql
from fraiseql import ID, FraiseQL

# 1. Define your type
@fraiseql.type
class Item:
    id: ID
    name: str
    description: str

# 2. Create a view in PostgreSQL
"""
CREATE VIEW v_item AS
SELECT jsonb_build_object(
    'id', id,
    'name', name,
    'description', description
) AS data
FROM items;
"""

# 3. Define a query
@fraiseql.query
async def items(info) -> list[Item]:
    repo = info.context["repo"]
    return await repo.find("v_item")

# 4. Initialize app
app = FraiseQL(database_url="postgresql://localhost/my_app")
```

## Common Beginner Mistakes

### ❌ Mistake 1: Forgetting the ID column
```sql
-- Wrong: No ID column for filtering
CREATE VIEW v_user AS
SELECT jsonb_build_object(...) AS data
FROM users;

-- Correct: Include ID for efficient filtering
CREATE VIEW v_user AS
SELECT
    id,  -- Include this!
    jsonb_build_object(...) AS data
FROM users;
```

### ❌ Mistake 2: Missing type hints
```python
# Wrong: No return type
@fraiseql.query
async def users(info):
    ...

# Correct: Always specify return type
@fraiseql.query
async def users(info) -> list[User]:
    ...
```

### ❌ Mistake 3: Not handling NULL values
```python
# Wrong: Will fail on NULL values
@fraiseql.type
class User:
    email: str  # What if email is NULL?

# Correct: Handle optional fields
@fraiseql.type
class User:
    email: str | None  # Can be NULL
```

## Next Steps

### Continue Learning

- **[Backend Developer Path](backend-developer.md)** - PostgreSQL-focused approach
- **[Frontend Developer Path](frontend-developer.md)** - Consuming GraphQL APIs
- **[Migration Path](migrating.md)** - Coming from other frameworks

### Explore Advanced Topics

- **[Authentication](../advanced/authentication.md)** - User authentication patterns
- **[Performance](../advanced/performance.md)** - Optimization techniques
- **[Security](../advanced/security.md)** - Production best practices

### Get Help

- **[Error Reference](../errors/error-types.md)** - Common errors explained
- **[Troubleshooting](../errors/troubleshooting.md)** - Solutions to common issues
- **[API Reference](../api-reference/index.md)** - Complete API documentation

## Practice Projects

Once you've completed the learning path, try these practice projects:

1. **Todo API** - Simple CRUD operations
2. **Recipe Manager** - Nested relationships
3. **Event Calendar** - Date handling and filtering
4. **Chat Application** - Real-time features
5. **E-commerce API** - Complex business logic

## Tips for Success

💡 **Start simple** - Don't try to build everything at once
💡 **Use the playground** - Test queries before writing code
💡 **Read error messages** - FraiseQL has helpful error messages
💡 **Check your SQL** - Test views in psql first
💡 **Ask for help** - The community is friendly and helpful

## Congratulations! 🎉

By completing this learning path, you've mastered the fundamentals of FraiseQL. You can now build efficient, type-safe GraphQL APIs with PostgreSQL.

Remember: FraiseQL's power comes from leveraging PostgreSQL's features. The more you know about PostgreSQL, the more powerful your APIs will be.

Happy coding!
