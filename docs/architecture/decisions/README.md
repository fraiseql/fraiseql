# Architecture Decisions

Understanding why FraiseQL works the way it does helps you leverage its full power. Let's explore the key architectural decisions that make FraiseQL unique.

## Why PostgreSQL First?

**Decision**: FraiseQL only supports PostgreSQL, not multiple databases.

**Reasoning**:
- PostgreSQL's advanced features (JSONB, views, functions, CTEs) enable incredible performance optimizations
- Single database focus means we can leverage PostgreSQL-specific features fully
- 90% of applications only use one database anyway
- PostgreSQL has become the de facto standard for modern applications

**Trade-offs**:
- ✅ **Pro**: 10-100x better performance through PostgreSQL-specific optimizations
- ✅ **Pro**: Simpler codebase, fewer bugs, faster development
- ✅ **Pro**: Can use advanced features like JSONB, arrays, full-text search
- ❌ **Con**: Can't switch to MySQL/MongoDB/etc. later
- ❌ **Con**: Teams without PostgreSQL experience need to learn it

**Real-world impact**:
Your API responses complete in 1-10ms instead of 50-500ms. Complex queries that would require multiple roundtrips in other databases complete in a single, optimized query.

## Why Views Instead of ORMs?

**Decision**: Use PostgreSQL views (`v_`) as the primary abstraction layer, not ORM models.

**Reasoning**:
- Views are declarative - you describe what you want, PostgreSQL figures out how to get it
- Database optimizer has full visibility into your query patterns
- Views can be indexed, materialized, and optimized at the database level
- No impedance mismatch between objects and relations

**Trade-offs**:
- ✅ **Pro**: Predictable, optimizable performance
- ✅ **Pro**: Full power of SQL available (window functions, CTEs, etc.)
- ✅ **Pro**: Changes to views don't require application restarts
- ❌ **Con**: Developers need basic SQL knowledge
- ❌ **Con**: Less familiar to ORM-heavy teams

**Real-world impact**:
```sql
-- This view automatically optimizes nested data fetching
CREATE VIEW v_user_with_posts AS
SELECT jsonb_build_object(
    'id', u.id,
    'name', u.name,
    'posts', (
        SELECT jsonb_agg(p.*)
        FROM posts p
        WHERE p.author_id = u.id
        ORDER BY p.created_at DESC
    )
) as data
FROM users u;

-- No N+1 queries, no DataLoader configuration needed
```

## Why JSONB Columns?

**Decision**: Every view returns a single `data` column containing JSONB.

**Reasoning**:
- JSONB perfectly matches GraphQL's nested structure
- PostgreSQL can index and query inside JSONB efficiently
- Allows schema evolution without migrations
- Eliminates object-relational mapping entirely
- **Views can compose other views' pre-built JSONB structures**

**Trade-offs**:
- ✅ **Pro**: Direct GraphQL to JSON mapping
- ✅ **Pro**: Flexible schema evolution
- ✅ **Pro**: Native PostgreSQL indexing support
- ✅ **Pro**: Reduced serialization overhead
- ✅ **Pro**: Composable - views build on other views
- ❌ **Con**: Slightly more storage (but storage is cheap)
- ❌ **Con**: Need to understand JSONB operators

**Real-world impact**:
```sql
-- Base view for user
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', u.id,
    'name', u.name,
    'email', u.email,
    'avatar', u.avatar_url
) as data
FROM users u;

-- Comment view references user view's data column
CREATE VIEW v_comment AS
SELECT jsonb_build_object(
    'id', c.id,
    'text', c.text,
    -- Don't rebuild user object, use the pre-built one
    'author', (SELECT data FROM v_user WHERE id = c.author_id)
) as data
FROM comments c;

-- Post view composes from multiple views
CREATE VIEW v_post AS
SELECT jsonb_build_object(
    'id', p.id,
    'title', p.title,
    'content', p.content,
    -- Directly use pre-composed user JSONB
    'author', (SELECT data FROM v_user WHERE id = p.author_id),
    -- Aggregate pre-composed comment objects
    'comments', (
        SELECT jsonb_agg(data)
        FROM v_comment
        WHERE post_id = p.id
    )
) as data
FROM posts p;

-- FraiseQL just returns the data column - no transformation needed
```

## Why Separate Tables from Views?

**Decision**: Use prefixes to distinguish object types:
- `tb_` for tables (source data)
- `v_` for views (GraphQL queries)
- `tv_` for table views (denormalized entities)
- `fn_` for functions (mutations)

**Reasoning**:
- Clear separation of concerns
- Instantly know an object's purpose from its name
- Can have multiple views of the same table
- Easier to manage permissions and optimization

**Trade-offs**:
- ✅ **Pro**: Clear code organization
- ✅ **Pro**: Multiple API shapes from same data
- ✅ **Pro**: Easy to identify performance bottlenecks
- ❌ **Con**: More database objects to manage
- ❌ **Con**: Naming convention to learn

**Real-world impact**:
```sql
-- Source table - normalized data
CREATE TABLE tb_users (
    id UUID PRIMARY KEY,
    email TEXT UNIQUE,
    name TEXT
);

-- Simple view for lists
CREATE VIEW v_user_list AS
SELECT jsonb_build_object(
    'id', id,
    'name', name
) as data
FROM tb_users;

-- Detailed view for single user
CREATE VIEW v_user_detail AS
SELECT jsonb_build_object(
    'id', u.id,
    'name', u.name,
    'email', u.email,
    'postCount', (SELECT COUNT(*) FROM tb_posts WHERE author_id = u.id),
    'recentPosts', (SELECT jsonb_agg(...) FROM ...)
) as data
FROM tb_users u;

-- Table view for complete denormalized entity
CREATE TABLE tv_user AS
SELECT
    u.id,
    u.tenant_id,
    jsonb_build_object(/* complete user data with all relations */) as data
FROM tb_users u;
```

## Why Use Functions for Mutations?

**Decision**: All mutations are PostgreSQL functions (`fn_`), not direct table writes.

**Reasoning**:
- Functions encapsulate business logic at the database level
- Atomic operations with proper transaction handling
- Can return complex results (created entity + side effects)
- Built-in validation and error handling

**Trade-offs**:
- ✅ **Pro**: Guaranteed data consistency
- ✅ **Pro**: Complex business logic in transactions
- ✅ **Pro**: Reusable across different APIs
- ✅ **Pro**: Can trigger events/notifications
- ❌ **Con**: Business logic in database (some prefer application layer)
- ❌ **Con**: PostgreSQL-specific PL/pgSQL to learn

**Real-world impact**:
```sql
-- Mutation function with business logic
CREATE FUNCTION fn_create_post(
    p_title TEXT,
    p_content TEXT,
    p_author_id UUID
) RETURNS JSONB AS $$
DECLARE
    v_post_id UUID;
    v_result JSONB;
BEGIN
    -- Validation
    IF LENGTH(p_title) < 3 THEN
        RAISE EXCEPTION 'Title too short';
    END IF;

    -- Create post
    INSERT INTO tb_posts (title, content, author_id)
    VALUES (p_title, p_content, p_author_id)
    RETURNING id INTO v_post_id;

    -- Update user stats
    UPDATE tb_users
    SET post_count = post_count + 1
    WHERE id = p_author_id;

    -- Return complete result using pre-composed views
    SELECT jsonb_build_object(
        'post', (SELECT data FROM v_post WHERE id = v_post_id),
        'user', (SELECT data FROM v_user WHERE id = p_author_id)
    ) INTO v_result;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

## Why Table Views for Caching?

**Decision**: Use table views (`tv_`) as materialized, denormalized entities for extreme performance.

**Reasoning**:
- Pre-compute expensive joins and aggregations
- Serve complex queries from a single table scan
- Trade storage (cheap) for computation (expensive)
- Enable sub-millisecond response times

**Trade-offs**:
- ✅ **Pro**: 50-100x performance improvement
- ✅ **Pro**: Predictable query performance
- ✅ **Pro**: Reduced database CPU usage
- ❌ **Con**: 3-5x more storage per entity
- ❌ **Con**: Need to manage data synchronization
- ❌ **Con**: Initial setup complexity

**Real-world impact**:
```sql
-- Instead of complex joins at query time
SELECT /* complex 5-table join with aggregations */

-- Serve from pre-computed table view
SELECT data FROM tv_user WHERE id = $1;
-- Returns in < 1ms with complete user data
```

## Why CQRS Pattern?

**Decision**: Separate write models (normalized tables) from read models (denormalized views).

**Reasoning**:
- Optimize reads and writes independently
- Most applications are read-heavy (90%+ reads)
- Can scale read and write paths differently
- Matches how developers think about APIs

**Trade-offs**:
- ✅ **Pro**: Optimal performance for both reads and writes
- ✅ **Pro**: Clear separation of concerns
- ✅ **Pro**: Can evolve read/write models independently
- ❌ **Con**: More complex than simple CRUD
- ❌ **Con**: Eventual consistency considerations

**Real-world impact**:
- Writes go to normalized tables with full constraints
- Reads come from optimized views/table views
- Can handle 100,000+ reads/second from table views
- Writes maintain full ACID guarantees

## Why Python Over TypeScript?

**Decision**: Built FraiseQL in Python rather than Node.js/TypeScript.

**Reasoning**:
- Python has excellent PostgreSQL support (asyncpg, psycopg3)
- Strong typing with Python 3.10+ and mypy
- Rich ecosystem for data processing and analytics
- Simpler async model than JavaScript
- Better integration with data science tools

**Trade-offs**:
- ✅ **Pro**: Excellent PostgreSQL drivers
- ✅ **Pro**: Clean async/await without callback hell
- ✅ **Pro**: Type hints provide IDE support
- ✅ **Pro**: Great for data-heavy applications
- ❌ **Con**: Node.js has larger GraphQL ecosystem
- ❌ **Con**: Some developers prefer TypeScript

**Real-world impact**:
```python
# FraiseQL leverages views' data columns
from fraiseql import FraiseQL

app = FraiseQL(database_url="postgresql://...")

# GraphQL query
query = """
    query GetPost($id: ID!) {
        post(id: $id) {
            title
            author {
                name
                email
            }
            comments {
                text
                author {
                    name
                }
            }
        }
    }
"""

# FraiseQL executes: SELECT data FROM v_post WHERE id = $1
# The data column already contains the complete nested structure
# No additional queries, no joins, no transformations
# Just return the pre-composed JSONB from the view
```

## Summary

These architectural decisions work together to create a unique approach to GraphQL APIs:

1. **PostgreSQL-first** gives us powerful optimization capabilities
2. **Views over ORMs** eliminates abstraction overhead
3. **JSONB everywhere** provides perfect GraphQL alignment
4. **View composition** - views build on other views' data columns
5. **Clear naming conventions** keep code organized
6. **Functions for mutations** ensure data integrity
7. **Table views** enable extreme performance
8. **CQRS pattern** optimizes both reads and writes
9. **Python** provides clean, typed, async code

The key insight: **Every view returns a `data` column with pre-composed JSONB**. Complex views don't rebuild everything from scratch - they compose from simpler views' data columns. This creates a hierarchy of reusable, optimized data structures that map directly to your GraphQL schema.

Ready to see these decisions in action? Check out our [Blog API Tutorial](/tutorials/blog-api) or learn about [Pagination Patterns](/advanced/pagination) to see how these architectural choices benefit real applications.
