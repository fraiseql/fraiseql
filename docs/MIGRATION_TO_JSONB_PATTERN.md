# Migration Guide: FraiseQL v0.1.0a14 JSONB Data Column Pattern

## Overview

FraiseQL v0.1.0a14 introduces a **breaking change** that standardizes how data is stored and retrieved. All type instantiation now comes exclusively from a JSONB `data` column in your database views/tables.

## What Changed

### Before (v0.1.0a13 and earlier)

FraiseQL could instantiate types from:
- Individual columns at the row level
- Nested JSONB data
- Mixed patterns with backward compatibility

### Now (v0.1.0a14+)

FraiseQL **only** instantiates types from a `data` JSONB column:
- All object data must be in the `data` column
- Other columns are for filtering/access control only
- Single, consistent pattern across all types

## Migration Steps

### 1. Update Your Database Views

#### Old Pattern
```sql
-- Types instantiated from individual columns
CREATE VIEW user_view AS
SELECT
    id,
    email,
    name,
    created_at
FROM users;
```

#### New Pattern
```sql
-- All data in JSONB 'data' column
CREATE VIEW user_view AS
SELECT
    id,              -- For filtering
    tenant_id,       -- For access control
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'created_at', created_at
    ) as data        -- REQUIRED: All type data here
FROM users;
```

### 2. Update Complex Views with Nested Objects

#### Old Pattern
```sql
CREATE VIEW post_with_author AS
SELECT
    p.id,
    p.title,
    p.content,
    u.id as author_id,
    u.name as author_name,
    u.email as author_email
FROM posts p
JOIN users u ON p.author_id = u.id;
```

#### New Pattern
```sql
CREATE VIEW post_with_author AS
SELECT
    p.id,           -- For filtering
    p.author_id,    -- For joins
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', jsonb_build_object(
            'id', u.id,
            'name', u.name,
            'email', u.email
        )
    ) as data
FROM posts p
JOIN users u ON p.author_id = u.id;
```

### 3. Update Your Type Definitions

Your Python types remain the same! Just ensure they match the structure in your `data` column:

```python
@fraise_type
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime

@fraise_type
class Post:
    id: UUID
    title: str
    content: str
    author: User  # Nested object from JSONB
```

### 4. Update Repository Usage

No changes needed if using `find()` and `find_one()`:

```python
# This continues to work
user = await repo.find_one("user_view", id=user_id)
posts = await repo.find("post_with_author", author_id=author_id)
```

### 5. Development vs Production Modes

The dual-mode behavior remains:

```python
# Development mode - returns typed objects
repo = FraiseQLRepository(pool, {"mode": "development"})
user = await repo.find_one("user_view", id=user_id)
print(user.email)  # Typed access

# Production mode - returns raw dicts
repo = FraiseQLRepository(pool, {"mode": "production"})
user = await repo.find_one("user_view", id=user_id)
print(user["data"]["email"])  # Dict access
```

## Common Patterns

### Multi-Tenant Applications

```sql
CREATE VIEW tenant_users AS
SELECT
    id,
    tenant_id,      -- For access control
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'role', role
    ) as data
FROM users
WHERE tenant_id = current_setting('app.tenant_id')::uuid;
```

### Aggregated Data

```sql
CREATE VIEW user_stats AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'post_count', COUNT(p.id),
        'comment_count', COUNT(c.id),
        'last_active', MAX(GREATEST(p.created_at, c.created_at))
    ) as data
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
LEFT JOIN comments c ON c.author_id = u.id
GROUP BY u.id;
```

### Materialized Views

```sql
CREATE MATERIALIZED VIEW product_catalog AS
SELECT
    p.id,
    p.category_id,  -- For filtering
    jsonb_build_object(
        'id', p.id,
        'name', p.name,
        'price', p.price,
        'in_stock', p.quantity > 0,
        'category', (
            SELECT jsonb_build_object(
                'id', c.id,
                'name', c.name
            )
            FROM categories c
            WHERE c.id = p.category_id
        )
    ) as data
FROM products p;

CREATE INDEX idx_product_catalog_category
ON product_catalog(category_id);
```

## Benefits of This Pattern

1. **Consistency**: One pattern for all data access
2. **Performance**: Optimized JSONB operations in PostgreSQL
3. **Flexibility**: Easy to add fields without schema changes
4. **Security**: Clear separation between data and access control
5. **Caching**: JSONB data can be easily cached
6. **Denormalization**: Natural support for denormalized read models

## Troubleshooting

### Error: KeyError: 'data'

Your view doesn't have a `data` column. Update your view to include all type data in a JSONB `data` column.

### Nested Objects Not Working

Ensure nested objects are properly structured in your JSONB:

```sql
-- Correct
jsonb_build_object(
    'id', p.id,
    'author', jsonb_build_object(  -- Nested object
        'id', u.id,
        'name', u.name
    )
)

-- Incorrect (flat structure)
jsonb_build_object(
    'id', p.id,
    'author_id', u.id,  -- This won't instantiate as an object
    'author_name', u.name
)
```

### Performance Considerations

1. **Index JSONB columns** for frequently queried fields:
   ```sql
   CREATE INDEX idx_users_email ON users ((data->>'email'));
   ```

2. **Use filtering columns** to avoid JSONB operations in WHERE clauses:
   ```sql
   -- Good: Filter on column
   SELECT * FROM user_view WHERE tenant_id = $1;

   -- Avoid: Filter on JSONB
   SELECT * FROM user_view WHERE data->>'tenant_id' = $1;
   ```

## Need Help?

- Check the [examples](../examples/) directory for updated code
- Review the [test files](../tests/) for patterns
- Open an issue on [GitHub](https://github.com/fraiseql/fraiseql/issues)
