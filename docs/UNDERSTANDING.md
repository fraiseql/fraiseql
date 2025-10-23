# Understanding FraiseQL in 10 Minutes

## The Big Idea

FraiseQL is **database-first GraphQL**. Instead of starting with GraphQL types and then figuring out how to fetch data, you start with your database schema and let it drive your API design.

**Why this matters:** Most GraphQL APIs suffer from N+1 query problems, ORM overhead, and complex caching. FraiseQL eliminates these by composing data in PostgreSQL views, then serving it directly as JSONB.

## How It Works: The Request Journey

Every GraphQL request follows this path:

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   GraphQL   │───▶│   FastAPI   │───▶│ PostgreSQL  │───▶│    Rust     │
│   Query     │    │  Resolver   │    │   View      │    │ Transform   │
│             │    │             │    │             │    │             │
│ { users {   │    │ @query      │    │ SELECT      │    │ jsonb →     │
│   name      │    │ def users:  │    │ jsonb_build_│    │ GraphQL     │
│ } }         │    │   return db │    │ object(...) │    │ Response    │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

1. **GraphQL Query** arrives at your FastAPI server
2. **Python Resolver** calls a PostgreSQL view or function
3. **Database View** returns pre-composed JSONB data
4. **Rust Pipeline** transforms JSONB to GraphQL response

## Core Pattern: JSONB Views

The heart of FraiseQL is the **JSONB view pattern**:

```
┌─────────────┐      ┌──────────────┐      ┌─────────────┐
│  tb_user    │  →   │   v_user     │  →   │  GraphQL    │
│ (table)     │      │  (view)      │      │  Response   │
│             │      │              │      │             │
│ id: 1       │      │ SELECT       │      │ {           │
│ name: Alice │      │ jsonb_build_ │      │   "id": 1   │
│ email: a@b  │      │   object     │      │   "name":.. │
└─────────────┘      └──────────────┘      └─────────────┘
```

Your database tables store normalized data, but your views compose it into ready-to-serve JSONB objects.

### Why JSONB Views?

**The Problem:** Traditional GraphQL APIs have performance issues:
- N+1 queries when resolving nested relationships
- ORM overhead converting database rows to objects
- Complex caching strategies needed

**The Solution:** Pre-compose data in the database:
- Single query returns complete object graphs
- No ORM - direct JSONB output
- Database handles joins, aggregations, filtering
- Views are always fresh (no stale cache issues)

## Naming Conventions Explained

FraiseQL uses consistent naming to make patterns clear:

```
Database Objects:
├── tb_*    - Write Tables (normalized storage)
├── v_*     - Read Views (JSONB composition)
├── tv_*    - Table Views (denormalized projections)
└── fn_*    - Business Logic Functions (writes/updates)
```

### tb_* - Write Tables
Store your normalized data. These are regular PostgreSQL tables.

**Example:** `tb_user`
```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

**When to use:** All data storage, relationships, constraints.

### v_* - Read Views
Compose data into JSONB objects for GraphQL queries.

**Example:** `v_user`
```sql
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', id,
    'name', name,
    'email', email,
    'created_at', created_at
) as data
FROM tb_user;
```

**When to use:** Simple queries, real-time data, no heavy aggregations.

### tv_* - Table Views
Denormalized projection tables for complex data that can be efficiently updated and queried.

**Example:** `tv_user_stats`
```sql
CREATE TABLE tv_user_stats (
    user_id INT PRIMARY KEY,
    data JSONB GENERATED ALWAYS AS (
        SELECT jsonb_build_object(
            'total_posts', COUNT(p.*),
            'last_post_date', MAX(p.created_at)
        )
        FROM tb_post p WHERE p.author_id = tv_user_stats.user_id
    ) STORED
);
```

**When to use:** Complex nested data, performance-critical reads, analytics with embedded relations.

### fn_* - Business Logic Functions
Handle writes, updates, and complex business logic.

**Example:** `fn_create_user`
```sql
CREATE FUNCTION fn_create_user(user_data JSONB)
RETURNS UUID AS $$
DECLARE
    new_id UUID;
BEGIN
    INSERT INTO tb_user (name, email)
    VALUES (user_data->>'name', user_data->>'email')
    RETURNING id INTO new_id;

    RETURN new_id;
END;
$$ LANGUAGE plpgsql;
```

**When to use:** All write operations, validation, business rules.

## Trinity Identifiers

FraiseQL uses **three types of identifiers** per entity for different purposes:

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│    pk_*     │  │     id      │  │ identifier  │
│ (internal)  │  │  (public)   │  │   (human)   │
├─────────────┤  ┌─────────────┤  ┌─────────────┤
│ Fast joins  │  │ API access  │  │ SEO/URLs    │
│ Never shown │  │ UUID        │  │ Readable    │
│ Auto-inc    │  │ External    │  │ Nullable    │
└─────────────┘  └─────────────┘  └─────────────┘
```

- **pk_***: Internal primary keys for fast database joins (never exposed in API)
- **id**: Public UUID identifiers for GraphQL queries and external references
- **identifier**: Human-readable slugs for URLs and user interfaces (nullable)

## The CQRS Pattern

FraiseQL implements **Command Query Responsibility Segregation**:

```
┌─────────────────────────────────────┐
│         GraphQL API                 │
├──────────────────┬──────────────────┤
│   QUERIES        │   MUTATIONS      │
│   (Reads)        │   (Writes)       │
├──────────────────┼──────────────────┤
│  v_* views       │  fn_* functions  │
│  tv_* tables     │  tb_* tables     │
└──────────────────┴──────────────────┘
```

**Queries** (reads) use views for fast, fresh data.
**Mutations** (writes) use functions for business logic and data integrity.

## Development Workflow

Here's how you build with FraiseQL:

```
1. Design Domain          2. Create Tables          3. Create Views
   What data?             (tb_* tables)             (v_* views)
   What relationships?                              JSONB composition

4. Define Types           5. Write Resolvers        6. Test API
   Python classes         @query/@mutation          GraphQL queries
   Match view structure   Call views/functions      Verify responses
```

### Step-by-Step Example

**Goal:** Build a user management API

1. **Design:** Users have name, email, posts
2. **Tables:** `tb_user`, `tb_post` with foreign keys
3. **Views:** `v_user` (single user), `v_users` (list with post counts)
4. **Types:** `User` class matching `v_user` JSONB structure
5. **Resolvers:** `@query def user(id): return db.v_user(id)`
6. **Test:** Query `{ user(id: "123") { name email } }`

## Performance Patterns

Different query patterns optimized for different use cases:

**Performance Decision Tree:**
```
Need fast response?
├── Yes → Use tv_* table view (0.05ms)
└── No  → Need fresh data?
    ├── Yes → Use v_* view (real-time)
    └── No  → Use tv_* table view (denormalized)
```

**Response Time Comparison:**
```
Query Type      | Response Time | Use Case
───────────────|──────────────|─────────────────────
tv_* table view | 0.05-0.5ms   | Dashboard, analytics
v_* view        | 1-5ms        | Real-time data
Complex JOIN    | 50-200ms     | Traditional ORM
```

## When to Use What

Decision tree for choosing patterns:

```
Need to read data?
├── Simple query, real-time data → v_* view
├── Complex nested data → tv_* table view
└── Performance-critical analytics → tv_* table view
```

## Next Steps

Now that you understand the patterns:

- **[5-Minute Quickstart](quickstart.md)** - Get a working API immediately
- **[First Hour Guide](FIRST_HOUR.md)** - Progressive tutorial from zero to production
- **[Core Concepts](core/concepts-glossary.md)** - Deep dive into each pattern
- **[Quick Reference](reference/quick-reference.md)** - Complete cheatsheet and examples

**Ready to code?** Start with the [quickstart](quickstart.md) to see it in action.
