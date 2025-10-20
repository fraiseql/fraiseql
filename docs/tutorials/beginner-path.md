# Beginner Learning Path

Complete pathway from zero to building production GraphQL APIs with FraiseQL.

**Time**: 2-3 hours
**Prerequisites**: Python 3.11+, PostgreSQL 14+, basic SQL knowledge

**📍 Navigation**: [← Quickstart](../quickstart.md) • [Core Concepts →](types-and-schema.md) • [Examples →](../../examples/)

## Learning Journey

### Phase 1: Quick Start (15 minutes)

1. **[5-Minute Quickstart](../quickstart.md)**
   - Build working API immediately
   - Understand basic pattern
   - Test in GraphQL Playground

2. **Verify Your Setup**
```bash
# Check installations
python --version  # 3.11+
psql --version    # PostgreSQL client

# Test quickstart
python app.py
# Open http://localhost:8000/graphql
```

**You should see**: GraphQL Playground with your API schema

---

### Phase 2: Core Concepts (30 minutes)

3. **[Database API](../core/database-api.md)** (Focus: select_from_json_view)
   - Repository pattern
   - QueryOptions for filtering
   - Pagination with PaginationInput
   - Ordering with OrderByInstructions

4. **[Types and Schema](../core/types-and-schema.md)** (Focus: @type decorator)
   - Python type hints → GraphQL types
   - Optional fields with `| None`
   - Lists with `list[Type]`

**Practice Exercise**:
```python
# Create a simple Note API
@fraiseql.type
class Note:
    id: UUID
    title: str
    content: str
    created_at: datetime

@query
async def notes(info) -> list[Note]:
    repo = info.context["repo"]
    results, _ = await repo.select_from_json_view(
        tenant_id=info.context["tenant_id"],
        view_name="v_note"
    )
    return [Note(**row) for row in results]
```

---

### Phase 3: N+1 Prevention (30 minutes)

5. **[Database Patterns](../advanced/database-patterns.md)** (Focus: JSONB Composition)
   - Composed views prevent N+1 queries
   - jsonb_build_object pattern
   - COALESCE for empty arrays

**Key Pattern**:
```sql
-- Instead of N queries, compose in view:
CREATE VIEW v_user_with_posts AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.pk_user,
        'name', u.name,
        'posts', COALESCE(
            (SELECT jsonb_agg(jsonb_build_object(
                'id', p.pk_post,
                'title', p.title
            ) ORDER BY p.created_at DESC)
            FROM tb_post p WHERE p.fk_author = u.id),
            '[]'::jsonb
        )
    ) AS data
FROM tb_user u;
```

**Practice**: Add comments to your Note API using composition

---

### Phase 4: Mutations (30 minutes)

6. **[Blog API Tutorial](./blog-api.md)** (Focus: Mutations section)
   - PostgreSQL functions for business logic
   - fn_ naming convention
   - Calling functions from Python

**Mutation Pattern**:
```sql
-- PostgreSQL function
CREATE FUNCTION fn_create_note(
    p_user_id UUID,
    p_title TEXT,
    p_content TEXT
) RETURNS UUID AS $$
DECLARE
    v_note_pk UUID;
BEGIN
    INSERT INTO tb_note (fk_user, title, content)
    SELECT id, p_title, p_content
    FROM tb_user WHERE pk_user = p_user_id
    RETURNING pk_note INTO v_note_pk;

    RETURN v_note_pk;
END;
$$ LANGUAGE plpgsql;
```

```python
# Python mutation
@mutation
async def create_note(info, title: str, content: str) -> Note:
    repo = info.context["repo"]
    user_id = info.context["user_id"]

    note_id = await repo.call_function(
        "fn_create_note",
        p_user_id=user_id,
        p_title=title,
        p_content=content
    )

    # Fetch and return created note
    results, _ = await repo.select_from_json_view(
        tenant_id=info.context["tenant_id"],
        view_name="v_note",
        options=QueryOptions(filters={"id": note_id})
    )

    return Note(**results[0])
```

---

### Phase 5: Complete Example (45 minutes)

7. **[Blog API Tutorial](./blog-api.md)** (Complete walkthrough)
   - Users, posts, comments
   - Threaded comments
   - Production patterns

**Build the full blog API** - This solidifies everything you've learned.

---

## Skills Checklist

After completing this path:

✅ Create PostgreSQL views with JSONB data column
✅ Define GraphQL types with Python type hints
✅ Write queries using repository pattern
✅ Prevent N+1 queries with view composition
✅ Implement mutations via PostgreSQL functions
✅ Use GraphQL Playground for testing
✅ Understand CQRS architecture
✅ Handle pagination and filtering

## Common Beginner Mistakes

### ❌ Mistake 1: No ID column in view
```sql
-- WRONG: Can't filter efficiently
CREATE VIEW v_user AS
SELECT jsonb_build_object(...) AS data
FROM tb_user;

-- CORRECT: Include ID for WHERE clauses
CREATE VIEW v_user AS
SELECT
    id,  -- ← Include this!
    jsonb_build_object(...) AS data
FROM tb_user;
```

### ❌ Mistake 2: Missing return type
```python
# WRONG: No type hint
@query
async def users(info):
    ...

# CORRECT: Always specify return type
@query
async def users(info) -> list[User]:
    ...
```

### ❌ Mistake 3: Not handling NULL
```python
# WRONG: Crashes on NULL
@fraiseql.type
class User:
    bio: str  # What if bio is NULL?

# CORRECT: Use | None for nullable fields
@fraiseql.type
class User:
    bio: str | None
```

### ❌ Mistake 4: Forgetting COALESCE in arrays
```sql
-- WRONG: Returns NULL instead of empty array
'posts', (SELECT jsonb_agg(...) FROM tb_post)

-- CORRECT: Use COALESCE
'posts', COALESCE(
    (SELECT jsonb_agg(...) FROM tb_post),
    '[]'::jsonb
)
```

## Quick Reference Card

### Essential Pattern
```python
# 1. Define type
@fraiseql.type
class Item:
    id: UUID
    name: str

# 2. Create view (in PostgreSQL)
CREATE VIEW v_item AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Item',
        'id', pk_item,
        'name', name
    ) AS data
FROM tb_item;

# 3. Query
@query
async def items(info) -> list[Item]:
    repo = info.context["repo"]
    results, _ = await repo.select_from_json_view(
        tenant_id=info.context["tenant_id"],
        view_name="v_item"
    )
    return [Item(**row) for row in results]
```

### Essential Commands
```bash
# Install
pip install fraiseql fastapi uvicorn

# Create database
createdb myapp

# Run app
python app.py
# Open http://localhost:8000/graphql

# Test SQL view
psql myapp -c "SELECT * FROM v_item LIMIT 1;"
```

## Next Steps

### Continue Learning

**Backend Focus**:
- [Database Patterns](../advanced/database-patterns.md) - tv_ pattern, entity change log
- [Performance](../performance/index.md) - Rust transformation, APQ caching
- [Multi-Tenancy](../advanced/multi-tenancy.md) - Tenant isolation

**Production Ready**:
- [Production Deployment](./production-deployment.md) - Docker, monitoring, security
- [Authentication](../advanced/authentication.md) - User auth patterns
- [Monitoring](../production/monitoring.md) - Observability

### Practice Projects

1. **Todo API** - Basic CRUD with users
2. **Recipe Manager** - Nested ingredients and steps
3. **Event Calendar** - Date filtering and recurring events
4. **Chat App** - Real-time messages with threads
5. **E-commerce** - Products, orders, inventory

## Troubleshooting

**"View not found" error**
- Check view name has `v_` prefix
- Verify view exists: `\dv v_*` in psql
- Ensure view has `data` column

**Type errors**
- Match Python types to PostgreSQL types
- Use `UUID` not `str` for UUIDs
- Add `| None` for nullable fields

**N+1 queries detected**
- Compose data in views, not in resolvers
- Use `jsonb_agg` for arrays
- Check [Database Patterns](../advanced/database-patterns.md)

## Tips for Success

💡 **Start simple** - Master basics before advanced patterns
💡 **Test SQL first** - Verify views in psql before using in Python
💡 **Read errors carefully** - FraiseQL provides detailed error messages
💡 **Use Playground** - Test queries interactively before writing code
💡 **Learn PostgreSQL** - FraiseQL power comes from PostgreSQL features

## Congratulations! 🎉

You've mastered FraiseQL fundamentals. You can now build type-safe, high-performance GraphQL APIs with PostgreSQL.

**Remember**: The better you know PostgreSQL, the more powerful your FraiseQL APIs become.

## See Also

- [Blog API Tutorial](./blog-api.md) - Complete working example
- [Database API](../core/database-api.md) - Repository reference
- [Database Patterns](../advanced/database-patterns.md) - Production patterns
