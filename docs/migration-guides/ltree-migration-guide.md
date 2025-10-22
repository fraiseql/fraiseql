# LTREE Migration Guide

## Converting from Traditional Hierarchical Patterns to PostgreSQL LTREE

This guide helps you migrate from traditional hierarchical data patterns (adjacency lists, nested sets, materialized paths) to PostgreSQL's native LTREE data type for improved performance and functionality.

## Why Migrate to LTREE?

### Benefits
- **10-100x faster** hierarchical queries with GiST indexes
- **23 specialized operators** for complex hierarchical operations
- **Automatic GraphQL integration** with FraiseQL
- **Native PostgreSQL support** - no custom functions needed
- **Pattern matching** with wildcards and advanced queries

### Performance Comparison

| Operation | Adjacency List | Nested Sets | Materialized Path | **LTREE** |
|-----------|----------------|-------------|-------------------|-----------|
| Find children | O(n) | O(log n) | O(log n) | **O(log n)** |
| Find ancestors | O(log n) | O(log n) | O(log n) | **O(log n)** |
| Tree restructuring | O(n) | O(n) | O(n) | **O(1)** |
| Pattern matching | N/A | Limited | Basic | **Advanced** |

## Migration Strategies

### 1. From Adjacency List Pattern

**Before:**
```sql
CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    name TEXT,
    parent_id INTEGER REFERENCES categories(id)
);
```

**After:**
```sql
CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    name TEXT,
    category_path LTREE NOT NULL
);

-- Create GiST index for optimal performance
CREATE INDEX idx_categories_path ON categories USING GIST (category_path);
```

**Migration Script:**
```sql
-- Add LTREE column
ALTER TABLE categories ADD COLUMN category_path LTREE;

-- Populate paths using recursive CTE
WITH RECURSIVE category_tree AS (
    -- Root categories (no parent)
    SELECT id, name, id::text AS path
    FROM categories
    WHERE parent_id IS NULL

    UNION ALL

    -- Child categories
    SELECT c.id, c.name, ct.path || '.' || c.id::text
    FROM categories c
    JOIN category_tree ct ON c.parent_id = ct.id
)
UPDATE categories
SET category_path = ct.path::ltree
FROM category_tree ct
WHERE categories.id = ct.id;

-- Add NOT NULL constraint after population
ALTER TABLE categories ALTER COLUMN category_path SET NOT NULL;
```

### 2. From Materialized Path Pattern

**Before:**
```sql
CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    name TEXT,
    path TEXT -- e.g., "1.2.3"
);
```

**After:**
```sql
-- Direct conversion if paths are numeric
UPDATE categories SET path = REPLACE(path, '.', '.') WHERE path IS NOT NULL;
ALTER TABLE categories ALTER COLUMN path TYPE LTREE USING path::ltree;

-- Rename column for clarity
ALTER TABLE categories RENAME COLUMN path TO category_path;
```

### 3. From Nested Sets Pattern

**Before:**
```sql
CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    name TEXT,
    lft INTEGER,
    rgt INTEGER
);
```

**After:**
```sql
ALTER TABLE categories ADD COLUMN category_path LTREE;

-- Complex migration using nested set traversal
-- (Implementation depends on your specific nested set structure)
```

## Query Migration Examples

### Finding Children

**Before (Adjacency List):**
```sql
SELECT * FROM categories
WHERE parent_id = (SELECT id FROM categories WHERE name = 'Electronics');
```

**After (LTREE):**
```sql
SELECT * FROM categories
WHERE category_path <@ (SELECT category_path FROM categories WHERE name = 'Electronics')::ltree
AND nlevel(category_path) = (SELECT nlevel(category_path) + 1 FROM categories WHERE name = 'Electronics');
```

### Finding All Descendants

**Before:**
```sql
WITH RECURSIVE descendants AS (
    SELECT * FROM categories WHERE parent_id = $root_id
    UNION ALL
    SELECT c.* FROM categories c
    JOIN descendants d ON c.parent_id = d.id
)
SELECT * FROM descendants;
```

**After:**
```sql
SELECT * FROM categories
WHERE category_path <@ (SELECT category_path FROM categories WHERE id = $root_id)::ltree;
```

### Finding Ancestors

**Before:**
```sql
WITH RECURSIVE ancestors AS (
    SELECT * FROM categories WHERE id = $child_id
    UNION ALL
    SELECT c.* FROM categories c
    JOIN ancestors a ON c.id = a.parent_id
)
SELECT * FROM ancestors WHERE id != $child_id;
```

**After:**
```sql
SELECT * FROM categories
WHERE category_path @> (SELECT category_path FROM categories WHERE id = $child_id)::ltree
AND id != $child_id;
```

### Pattern Matching

**Before:**
```sql
SELECT * FROM categories WHERE path LIKE 'electronics.%';
```

**After:**
```sql
-- Simple wildcard matching
SELECT * FROM categories WHERE category_path ~ 'electronics.*';

-- Advanced pattern with depth constraints
SELECT * FROM categories
WHERE category_path ~ 'electronics.*'
AND nlevel(category_path) <= 4;
```

## FraiseQL Integration

### GraphQL Schema Migration

**Before:**
```graphql
type Category {
  id: ID!
  name: String!
  parentId: ID
  children: [Category!]!
}
```

**After:**
```graphql
type Category {
  id: ID!
  name: String!
  categoryPath: LTree!

  # Automatic LTREE filtering
  children(where: CategoryWhereInput): [Category!]!
}

type CategoryWhereInput {
  categoryPath: LTreeFilter
}

# LTREE-specific filter operations
input LTreeFilter {
  eq: LTree
  ancestorOf: LTree          # @>
  descendantOf: LTree        # <@
  matchesLquery: String      # ~
  nlevelEq: Int              # exact depth
  subpath: LTreeSubpathInput # extract portion
  # ... 18 more operators
}
```

### Query Examples

**Find all electronics:**
```graphql
query {
  categories(where: { categoryPath: { descendantOf: "electronics" } }) {
    id
    name
    categoryPath
  }
}
```

**Find 3-level deep categories:**
```graphql
query {
  categories(where: { categoryPath: { nlevelEq: 3 } }) {
    id
    name
  }
}
```

**Pattern matching:**
```graphql
query {
  categories(where: {
    categoryPath: { matchesLquery: "electronics.*.laptops" }
  }) {
    id
    name
  }
}
```

## Performance Optimization

### Essential Indexes

```sql
-- GiST index for hierarchical operations
CREATE INDEX idx_category_path ON categories USING GIST (category_path);

-- B-tree index for equality (automatically included in GiST)
-- Additional indexes for common query patterns
CREATE INDEX idx_category_depth ON categories (nlevel(category_path));
CREATE INDEX idx_category_parent ON categories (subpath(category_path, 0, -1));
```

### Query Optimization Tips

1. **Use GiST indexes** for all hierarchical queries
2. **Cast to LTREE explicitly** in WHERE clauses
3. **Consider nlevel()** for depth-based filtering
4. **Use subpath()** for parent path extraction
5. **Batch updates** during data restructuring

### Monitoring Performance

```sql
-- Check index usage
SELECT * FROM pg_stat_user_indexes WHERE relname = 'categories';

-- Analyze query plans
EXPLAIN ANALYZE SELECT * FROM categories WHERE category_path <@ 'electronics'::ltree;

-- Monitor LTREE-specific operations
SELECT * FROM pg_stat_user_functions WHERE funcname LIKE '%ltree%';
```

## Common Migration Challenges

### 1. Path Structure Changes

**Issue:** Existing paths may not follow LTREE naming conventions
**Solution:** Use text processing functions during migration

```sql
-- Clean and normalize paths
UPDATE categories
SET category_path = regexp_replace(lower(path), '[^a-z0-9.]', '_', 'g')::ltree;
```

### 2. Circular References

**Issue:** Adjacency list may have circular references
**Solution:** Detect and fix before migration

```sql
-- Detect circular references
WITH RECURSIVE cycle_detection AS (
    SELECT id, parent_id, ARRAY[id] AS path
    FROM categories
    WHERE parent_id IS NOT NULL

    UNION ALL

    SELECT c.id, c.parent_id, cd.path || c.id
    FROM categories c
    JOIN cycle_detection cd ON c.parent_id = cd.id
    WHERE NOT (c.id = ANY(cd.path))
)
SELECT * FROM cycle_detection WHERE id = ANY(path);
```

### 3. Performance Regression

**Issue:** Queries slower after migration
**Solution:** Verify GiST index creation and ANALYZE table

```sql
-- Ensure index exists
SELECT * FROM pg_indexes WHERE tablename = 'categories';

-- Update statistics
ANALYZE categories;

-- Test query performance
EXPLAIN ANALYZE SELECT * FROM categories WHERE category_path <@ 'root'::ltree;
```

## Rollback Strategy

Always backup before migration:

```sql
-- Create backup
CREATE TABLE categories_backup AS SELECT * FROM categories;

-- Rollback if needed
DROP TABLE categories;
ALTER TABLE categories_backup RENAME TO categories;
```

## Success Metrics

After migration, verify:

- ✅ Query performance improved by 10-100x
- ✅ All hierarchical operations work
- ✅ GraphQL integration functional
- ✅ Data integrity maintained
- ✅ Application functionality preserved

## Additional Resources

- [PostgreSQL LTREE Documentation](https://www.postgresql.org/docs/current/ltree.html)
- [FraiseQL Documentation](../README.md) - Comprehensive guides and references
