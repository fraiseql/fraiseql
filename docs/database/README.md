# PostgreSQL Patterns - The FraiseQL Way™

**The authoritative index for FraiseQL's opinionated PostgreSQL architecture.**

FraiseQL is radically database-first. Everything happens in PostgreSQL: business logic, validation, authorization, caching, and transformations. This document indexes all our opinionated patterns and links to detailed guides.

---

## 🎯 Core Philosophy

> **"If PostgreSQL can do it, PostgreSQL should do it."**

FraiseQL treats PostgreSQL as:
- ✅ **Application server** - Business logic lives in PL/pgSQL functions
- ✅ **Data layer** - JSONB views compose database to API
- ✅ **Security boundary** - Row-Level Security enforces authorization
- ✅ **Type system** - PostgreSQL types map directly to GraphQL
- ✅ **Performance layer** - Materialized views, indexes, and caching

**Why?**
- Single source of truth (no ORM drift)
- Maximum performance (no network round-trips)
- Type safety end-to-end (DB → GraphQL → TypeScript)
- Easier testing (functions testable in pure SQL)
- Better observability (one query log, not scattered app logs)

**See**: [FraiseQL Philosophy](../core/fraiseql-philosophy.md) for complete rationale.

---

## 📋 Quick Decision Matrix

| If you want to... | Use this pattern | Documentation |
|-------------------|------------------|---------------|
| Name tables/views/functions | **Trinity Pattern** (`tb_`, `v_`, `tv_`, `fn_`) | [Table Naming](#1-table-naming-conventions) |
| Structure primary keys | **Trinity Identifiers** (id/identifier/uuid) | [Trinity Identifiers](#2-trinity-identifiers) |
| Expose data to GraphQL | **JSONB Views** (`v_*`) | [View Strategies](#3-view-strategies) |
| Write mutations | **mutation_response + Status Strings** | [Mutation Requirements](#4-mutation-patterns) |
| Handle errors | **Status strings** (`validation:`) | [Error Handling](#5-error-handling) |
| Validate input | **PL/pgSQL validation** in functions | [Validation Patterns](#6-validation-patterns) |
| Control access | **Row-Level Security (RLS)** | [Security & RLS](#7-security-patterns) |
| Improve performance | **Materialized views + indexes** | [Performance](#8-performance-patterns) |
| Cache frequently accessed data | **Database-level caching** | [Caching](#9-caching-patterns) |
| Version schema | **Sequential migrations** (no down migrations) | [Migrations](#10-migrations) |

---

## 1. Table Naming Conventions

### The Trinity Pattern

FraiseQL uses **strict naming prefixes** for clarity and performance:

```
tb_*   - Base tables (source of truth, normalized)
v_*    - Virtual views (simple entities, no foreign keys)
tv_*   - Physical table views (entities with foreign keys, denormalized)
fn_*   - Functions (business logic, mutations)
```

**Example**:
```sql
-- Base table (normalized)
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid(),
    name TEXT,
    email TEXT
);

CREATE TABLE tb_post (
    pk_post SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid(),
    fk_user INT REFERENCES tb_user(pk_user),
    title TEXT
);

-- Virtual view (simple entity, no FKs)
CREATE VIEW v_user AS
SELECT pk_user, id, jsonb_build_object('id', id, 'name', name) AS data
FROM tb_user;

-- Physical table view (entity with FK, denormalized)
CREATE TABLE tv_post (
    pk_post TEXT PRIMARY KEY,
    id UUID,
    fk_user INT,      -- FK lineage
    user_id UUID,     -- FK filtering
    data JSONB        -- Includes embedded author from v_user
);

-- Business logic function
CREATE FUNCTION fn_create_post(...) RETURNS mutation_response;
```

**Why prefixes?**
- ✅ Clear separation of concerns (normalized vs denormalized)
- ✅ Easier auto-discovery (FraiseQL scans `v_*` and `tv_*` views)
- ✅ TVIEW-ready (automatic cascade propagation)
- ✅ Prevents naming conflicts
- ✅ Better organization at scale

**Documentation**:
- **[Table Naming Conventions](table-naming-conventions.md)** - Complete reference
- **[Trinity Pattern Philosophy](../core/trinity-pattern.md)** - Architectural rationale

---

## 2. Trinity Identifiers

### Three-Tier ID System

Every entity has **three identifiers** for different use cases:

```sql
CREATE TABLE tb_user (
    id          BIGSERIAL PRIMARY KEY,           -- Internal (foreign keys, joins)
    identifier  TEXT UNIQUE NOT NULL,            -- External (URLs, APIs, UX)
    uuid        UUID UNIQUE NOT NULL DEFAULT gen_random_uuid()  -- Global (federation, sync)
);
```

**When to use each**:
- **`id` (BIGSERIAL)**: Internal references, joins, foreign keys, performance-critical queries
- **`identifier` (TEXT)**: User-facing identifiers, URLs, API parameters, UX
- **`uuid` (UUID)**: Cross-database sync, federation, external systems

**Example**:
```
Internal query:  SELECT * FROM tb_post WHERE author_id = 12345;
API endpoint:    GET /posts/my-first-post-abc123
Federation:      Sync entity with UUID a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11
```

**Documentation**:
- **[Trinity Identifiers](trinity-identifiers.md)** - Complete guide with examples
- **[Database Patterns](../advanced/database-patterns.md)** - Advanced ID patterns

---

## 3. View Strategies

### Virtual Views (`v_*`) vs Physical Table Views (`tv_*`)

FraiseQL uses **two types of views with identical structure** but different storage and use cases:

### The Decision Rule: Do You Have Foreign Keys?

**Simple rule**:
- **No foreign keys** → Use `v_*` (virtual view)
- **Has foreign keys** → Use `tv_*` (physical table view)

**Why?** Foreign keys require denormalized data from joins, and `tv_*` enables efficient cascade updates through the upcoming **TVIEW extension**.

---

#### Virtual Views (`v_*`) - Simple Entities (No Relations)

**When to use**: Standalone entities with no foreign keys.

**Required structure**:
```sql
CREATE VIEW v_user AS
SELECT
    pk_user,              -- External primary key (TEXT)
    id,                   -- UUID for GraphQL filtering
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM tb_user
WHERE deleted_at IS NULL;
```

**Characteristics**:
- ✅ **Virtual** - computed on every query
- ✅ Always up-to-date (reflects current base table state)
- ✅ No storage cost
- ✅ Simple entities without foreign keys
- ✅ Structure: `(pk_{entity}, id, data JSONB)`
- ✅ FraiseQL scans these automatically for GraphQL
- ⚠️ Don't use for entities with foreign keys (use `tv_*` instead)

---

#### Physical Table Views (`tv_*`) - Entities with Relations

**When to use**: Entities with foreign keys that need denormalized data from joins.

**Required structure** (optimized for TVIEW extension):
```sql
CREATE TABLE tv_post (
    pk_post TEXT PRIMARY KEY,      -- External primary key
    id UUID NOT NULL,               -- UUID for GraphQL filtering
    fk_user INT NOT NULL,           -- FK lineage (for TVIEW propagation)
    user_id UUID NOT NULL,          -- FK UUID (for FraiseQL filtering)
    data JSONB NOT NULL             -- Denormalized entity with nested author
);

-- Populated/updated explicitly in mutations
CREATE FUNCTION fn_create_post(...) AS $$
DECLARE
    new_pk TEXT;
    author_uuid UUID;
BEGIN
    -- Insert into base table
    INSERT INTO tb_post (pk_post, fk_user, title, content)
    VALUES (generate_pk(), input_author_id, input_title, input_content)
    RETURNING pk_post INTO new_pk;

    -- Get author UUID
    SELECT id INTO author_uuid FROM tb_user WHERE pk_user = input_author_id;

    -- Update physical table view with denormalized data
    INSERT INTO tv_post (pk_post, id, fk_user, user_id, data)
    VALUES (
        new_pk,
        gen_random_uuid(),
        input_author_id,          -- Integer FK for lineage
        author_uuid,              -- UUID FK for filtering
        jsonb_build_object(
            'id', new_pk,
            'title', input_title,
            'author', (SELECT data FROM v_user WHERE pk_user = input_author_id)
        )
    );
END;
$$;
```

**Characteristics**:
- ✅ **Physical table** - data stored on disk
- ✅ **Required structure**: `(pk_{entity}, id, fk_{relation} INT, {relation}_id UUID, data JSONB)`
- ✅ Updated explicitly in mutation functions
- ✅ Fast queries (pre-computed, indexed)
- ✅ Denormalized data from joins (author embedded in post)
- ✅ **TVIEW-ready**: FK columns enable automatic cascade propagation
- ⚠️ Must be kept in sync manually (until TVIEW extension)

**Why both `fk_user` (INT) and `user_id` (UUID)?**
1. **`fk_user` (INT)**: Used by TVIEW extension for efficient cascade propagation
   - "Find all posts WHERE fk_user = 123" (integer comparison, fast)
   - Enables automatic update propagation when user changes
2. **`user_id` (UUID)**: Used by FraiseQL for GraphQL filtering
   - `posts(where: {user_id: "uuid-here"})` in GraphQL queries
   - Matches GraphQL ID type

---

### Column Requirements (TVIEW-Aligned)

**All views/table views MUST include**:

```sql
(
    pk_{entity} TEXT PRIMARY KEY,   -- External primary key
    id UUID NOT NULL,                -- UUID for GraphQL filtering

    -- For entities with foreign keys (tv_* only):
    fk_{relation} INT,               -- Integer FK (TVIEW lineage)
    {relation}_id UUID,              -- UUID FK (FraiseQL filtering)

    data JSONB NOT NULL              -- Entity payload
)
```

**Example comparison**:

```sql
-- Simple entity (no FKs) → v_*
CREATE VIEW v_tag AS
SELECT
    pk_tag,        -- External PK
    id,            -- UUID
    data           -- Payload
FROM tb_tag;

-- Entity with FKs → tv_*
CREATE TABLE tv_post (
    pk_post TEXT PRIMARY KEY,
    id UUID,
    fk_user INT,       -- ← FK present
    user_id UUID,      -- ← FK UUID
    data JSONB
);
```

**Documentation**:
- **[View Strategies](view-strategies.md)** - Complete guide with performance notes
- **[Database-Level Caching](database-level-caching.md)** - Materialized view patterns

---

## 4. Mutation Patterns

### PostgreSQL Function Requirements

All FraiseQL mutations use PostgreSQL functions returning `mutation_response`:

```sql
CREATE TYPE mutation_response AS (
    status          TEXT,      -- "created", "validation:", "not_found:user"
    message         TEXT,      -- Human-readable message
    entity_id       TEXT,      -- Optional entity identifier
    entity_type     TEXT,      -- GraphQL type name (for __typename)
    entity          JSONB,     -- Entity data
    updated_fields  TEXT[],    -- Changed fields (for optimistic updates)
    cascade         JSONB,     -- Side effects (for cache updates)
    metadata        JSONB      -- Additional context
);
```

**Example**:
```sql
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    new_user_id TEXT;
BEGIN
    -- Validation
    IF input_data->>'email' IS NULL THEN
        result.status := 'validation:';
        result.message := 'Email is required';
        RETURN result;
    END IF;

    -- Create user
    INSERT INTO tb_user (identifier, name, email)
    VALUES (
        generate_identifier('user'),
        input_data->>'name',
        input_data->>'email'
    )
    RETURNING identifier INTO new_user_id;

    -- Success response
    result.status := 'created';
    result.message := 'User created successfully';
    result.entity_id := new_user_id;
    result.entity_type := 'User';
    result.entity := (SELECT data FROM v_user WHERE identifier = new_user_id);

    RETURN result;
END;
$$ LANGUAGE plpgsql;
```

**Documentation**:
- **[Mutation SQL Requirements](../guides/mutation-sql-requirements.md)** - Complete reference
- **[Status Strings](../mutations/status-strings.md)** - Status taxonomy
- **[CASCADE Architecture](../mutations/cascade-architecture.md)** - Side effects

---

## 5. Error Handling

### Status String Pattern

FraiseQL uses **status strings** that automatically generate structured errors:

```sql
-- Validation error
status := 'validation:';

-- Not found
status := 'not_found:user';

-- Conflict
status := 'conflict:duplicate_email';

-- Permission denied
status := 'forbidden:insufficient_role';
```

**Automatic transformation**:
```
PostgreSQL:  status = "validation:"
             message = "Email format invalid"

GraphQL:     errors = [{
               "code": 422,
               "identifier": "validation",
               "message": "Email format invalid",
               "details": null
             }]
```

**Status prefixes and HTTP codes**:
- `failed:*` → 422 (Unprocessable Entity)
- `not_found:*` → 404 (Not Found)
- `conflict:*` → 409 (Conflict)
- `unauthorized:*` → 401 (Unauthorized)
- `forbidden:*` → 403 (Forbidden)
- `timeout:*` → 408 (Request Timeout)
- `noop:*` → 422 (No changes made)

**Documentation**:
- **[Error Handling Patterns](../guides/error-handling-patterns.md)** - Deep dive
- **[Status Strings Reference](../mutations/status-strings.md)** - Complete taxonomy

---

## 6. Validation Patterns

### Database-Side Validation

**All validation happens in PostgreSQL functions**, not application code.

**Pattern 1: Simple Validation**
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    title TEXT;
BEGIN
    title := input_data->>'title';

    -- Validation
    IF title IS NULL OR length(trim(title)) = 0 THEN
        result.status := 'validation:';
        result.message := 'Title is required';
        RETURN result;
    END IF;

    IF length(title) > 200 THEN
        result.status := 'validation:';
        result.message := 'Title must be 200 characters or less';
        RETURN result;
    END IF;

    -- Continue with creation...
END;
$$ LANGUAGE plpgsql;
```

**Pattern 2: Multi-Field Validation with Explicit Errors**
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    validation_errors JSONB := '[]'::JSONB;
BEGIN
    -- Collect all validation errors
    IF input_data->>'title' IS NULL THEN
        validation_errors := validation_errors || jsonb_build_object(
            'code', 422,
            'identifier', 'title_required',
            'message', 'Title is required',
            'details', jsonb_build_object('field', 'title')
        );
    END IF;

    IF input_data->>'author_id' IS NULL THEN
        validation_errors := validation_errors || jsonb_build_object(
            'code', 422,
            'identifier', 'author_required',
            'message', 'Author is required',
            'details', jsonb_build_object('field', 'author_id')
        );
    END IF;

    -- Return all errors at once
    IF jsonb_array_length(validation_errors) > 0 THEN
        result.status := 'validation:';
        result.message := 'Validation failed';
        result.metadata := jsonb_build_object('errors', validation_errors);
        RETURN result;
    END IF;

    -- Continue with creation...
END;
$$ LANGUAGE plpgsql;
```

**Why database-side validation?**
- ✅ Single source of truth
- ✅ Can't be bypassed
- ✅ Enforced across all clients (web, mobile, CLI)
- ✅ Atomic with data changes
- ✅ Better error messages (knows data context)

**Documentation**:
- **[Error Handling Patterns](../guides/error-handling-patterns.md)** - Validation section
- **[Mutation SQL Requirements](../guides/mutation-sql-requirements.md)** - Complete examples

---

## 7. Security Patterns

### Row-Level Security (RLS)

**All authorization happens via PostgreSQL RLS**, not application middleware.

**Basic RLS Setup**:
```sql
-- Enable RLS on table
ALTER TABLE tb_post ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own posts
CREATE POLICY select_own_posts ON tb_post
    FOR SELECT
    USING (author_id = current_user_id());

-- Policy: Users can only update their own posts
CREATE POLICY update_own_posts ON tb_post
    FOR UPDATE
    USING (author_id = current_user_id())
    WITH CHECK (author_id = current_user_id());
```

**Advanced: Multi-Tenant RLS**:
```sql
-- Policy: Users can only see data in their tenant
CREATE POLICY tenant_isolation ON tb_post
    FOR ALL
    USING (tenant_id = current_tenant_id());
```

**Set user context** (from FraiseQL middleware):
```python
# Set user context before query
await db.execute(
    "SELECT set_config('app.user_id', $1, false)",
    str(user_id)
)

# Then execute query - RLS enforced automatically
result = await db.fetch("SELECT * FROM tb_post")
```

**Documentation**:
- **[RBAC & RLS Patterns](../enterprise/rbac-postgresql-assessment.md)** - Complete guide
- **[Multi-Tenancy](../advanced/multi-tenancy.md)** - Tenant isolation patterns

---

## 8. Performance Patterns

### Indexing Strategy

**Index trinity identifiers**:
```sql
CREATE INDEX idx_user_id ON tb_user(id);              -- Already has (PRIMARY KEY)
CREATE INDEX idx_user_identifier ON tb_user(identifier);  -- Already has (UNIQUE)
CREATE INDEX idx_user_uuid ON tb_user(uuid);          -- Already has (UNIQUE)
```

**Index foreign keys**:
```sql
CREATE INDEX idx_post_author_id ON tb_post(author_id);
CREATE INDEX idx_comment_post_id ON tb_comment(post_id);
CREATE INDEX idx_comment_author_id ON tb_comment(author_id);
```

**Composite indexes for common queries**:
```sql
-- Posts by status and author
CREATE INDEX idx_post_status_author ON tb_post(status, author_id);

-- Active posts ordered by date
CREATE INDEX idx_post_active_date ON tb_post(created_at DESC)
    WHERE deleted_at IS NULL;
```

**JSONB indexes**:
```sql
-- GIN index for JSONB operators
CREATE INDEX idx_post_metadata ON tb_post USING GIN(metadata);

-- Specific JSONB field index
CREATE INDEX idx_post_tags ON tb_post USING GIN((metadata->'tags'));
```

### Materialized Views

For expensive aggregations, use materialized views:

```sql
-- Expensive view
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT
    u.id,
    u.identifier,
    COUNT(DISTINCT p.id) AS post_count,
    COUNT(DISTINCT c.id) AS comment_count,
    AVG(p.view_count) AS avg_post_views
FROM tb_user u
LEFT JOIN tb_post p ON p.author_id = u.id
LEFT JOIN tb_comment c ON c.author_id = u.id
GROUP BY u.id;

-- Index the materialized view
CREATE UNIQUE INDEX idx_mv_user_stats_id ON mv_user_stats(id);

-- Refresh strategy (manual)
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats;

-- Or refresh on schedule (pg_cron)
SELECT cron.schedule('refresh-user-stats', '0 * * * *',
    $$REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats$$
);
```

**Documentation**:
- **[Performance Guide](../performance/performance-guide.md)** - Complete optimization guide
- **[Database-Level Caching](database-level-caching.md)** - Materialized view patterns

---

## 9. Caching Patterns

### Database-Level Caching

FraiseQL supports **three caching strategies**:

**1. Denormalized Fields** (Best for simple cases):
```sql
ALTER TABLE tb_user ADD COLUMN post_count INTEGER DEFAULT 0;

-- Update via trigger or explicit in mutation
UPDATE tb_user SET post_count = post_count + 1 WHERE id = author_id;
```

**2. Materialized Views** (Best for complex aggregations):
```sql
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT user_id, COUNT(*) as post_count, ...
FROM tb_post
GROUP BY user_id;
```

**3. Dedicated Cache Tables** (Best for external data):
```sql
CREATE TABLE cache_api_responses (
    key TEXT PRIMARY KEY,
    value JSONB,
    expires_at TIMESTAMPTZ
);
```

**Documentation**:
- **[Database-Level Caching](database-level-caching.md)** - Complete caching guide

---

## 10. Migrations

### Migration Best Practices

**FraiseQL philosophy on migrations**:
- ✅ Sequential only (no down migrations)
- ✅ Idempotent (can run multiple times)
- ✅ Versioned (numbered files: 001_, 002_, ...)
- ✅ Tested in transaction (rollback on error)

**Example migration structure**:
```
migrations/
├── 001_initial_schema.sql
├── 002_add_user_tables.sql
├── 003_add_post_tables.sql
├── 004_add_mutation_helpers.sql
└── 005_add_indexes.sql
```

**Migration template**:
```sql
-- Migration: 003_add_post_tables.sql
BEGIN;

-- Create table
CREATE TABLE IF NOT EXISTS tb_post (
    id BIGSERIAL PRIMARY KEY,
    identifier TEXT UNIQUE NOT NULL,
    uuid UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT,
    author_id BIGINT REFERENCES tb_user(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

-- Create view
CREATE OR REPLACE VIEW v_post AS
SELECT id, identifier, jsonb_build_object(
    'id', identifier,
    'title', title,
    'content', content,
    'author_id', author_id,
    'created_at', created_at
) AS data
FROM tb_post
WHERE deleted_at IS NULL;

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_post_author_id ON tb_post(author_id);
CREATE INDEX IF NOT EXISTS idx_post_created_at ON tb_post(created_at DESC);

COMMIT;
```

**Documentation**:
- **[Migrations Guide](migrations.md)** - Complete migration patterns

---

## 11. Anti-Patterns (What NOT to Do)

### ❌ Don't Use Triggers

**Why avoid triggers?**
- Hidden complexity (debugging nightmares)
- Performance issues (implicit N+1 queries)
- Breaking RLS (SECURITY DEFINER triggers bypass RLS)
- Cascade complexity (trigger chains)

**Use explicit logic instead**:
```sql
-- ❌ BAD: Trigger updates count
CREATE TRIGGER update_post_count ...

-- ✅ GOOD: Explicit update in mutation
CREATE OR REPLACE FUNCTION fn_create_post(input_data JSONB)
RETURNS mutation_response AS $$
BEGIN
    -- Create post
    INSERT INTO tb_post ...;

    -- Explicitly update count
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = author_id;

    RETURN result;
END;
$$;
```

**Documentation**:
- **[Avoid Triggers](avoid-triggers.md)** - Complete rationale

### ❌ Don't Use ORMs

**FraiseQL is ORM-free by design.**

**Why no ORMs?**
- Impedance mismatch (forcing OOP onto relational)
- N+1 query problems
- Loss of PostgreSQL features
- Performance degradation
- Type drift (DB schema ≠ ORM models)

**Use raw SQL + typed views instead**:
```python
# ❌ BAD: ORM query
users = await User.objects.filter(status="active").prefetch_related("posts")

# ✅ GOOD: Direct SQL via view
users = await db.fetch("SELECT data FROM v_user WHERE status = 'active'")
```

### ❌ Don't Store Business Logic in Python

**Keep business logic in PostgreSQL functions.**

```python
# ❌ BAD: Validation in Python
def create_user(name, email):
    if not name:
        raise ValueError("Name required")
    if not email or "@" not in email:
        raise ValueError("Invalid email")
    # Can be bypassed!
```

```sql
-- ✅ GOOD: Validation in PostgreSQL
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSONB)
RETURNS mutation_response AS $$
BEGIN
    IF input_data->>'name' IS NULL THEN
        result.status := 'validation:';
        result.message := 'Name is required';
        RETURN result;
    END IF;
    -- Cannot be bypassed!
END;
$$;
```

---

## 12. Quick Reference

### Common Patterns Cheat Sheet

```sql
-- Trinity table structure
CREATE TABLE tb_{entity} (
    id BIGSERIAL PRIMARY KEY,
    identifier TEXT UNIQUE NOT NULL,
    uuid UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    -- entity fields here
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ  -- Soft delete
);

-- JSONB view
CREATE VIEW v_{entity} AS
SELECT id, identifier, jsonb_build_object(
    'id', identifier,
    -- map fields to GraphQL shape
) AS data
FROM tb_{entity}
WHERE deleted_at IS NULL;

-- Mutation function
CREATE OR REPLACE FUNCTION fn_create_{entity}(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
BEGIN
    -- Validation
    IF input_data->>'field' IS NULL THEN
        result.status := 'validation:';
        result.message := 'Field is required';
        RETURN result;
    END IF;

    -- Create entity
    INSERT INTO tb_{entity} (...) VALUES (...);

    -- Success
    result.status := 'created';
    result.message := '{Entity} created successfully';
    result.entity := (SELECT data FROM v_{entity} WHERE ...);
    RETURN result;
END;
$$ LANGUAGE plpgsql;

-- RLS policy
ALTER TABLE tb_{entity} ENABLE ROW LEVEL SECURITY;
CREATE POLICY select_policy ON tb_{entity}
    FOR SELECT
    USING (author_id = current_user_id());
```

---

## 📖 Complete Documentation Index

### Core Database Patterns
- [Table Naming Conventions](table-naming-conventions.md) - Trinity pattern reference
- [Trinity Identifiers](trinity-identifiers.md) - Three-tier ID system
- [View Strategies](view-strategies.md) - JSONB vs table views
- [Trinity Pattern Philosophy](../core/trinity-pattern.md) - Architectural rationale
- [PostgreSQL Extensions](../core/postgresql-extensions.md) - Required extensions

### Mutation & Error Handling
- [Mutation SQL Requirements](../guides/mutation-sql-requirements.md) - Complete function guide
- [Error Handling Patterns](../guides/error-handling-patterns.md) - Error handling deep dive
- [Status Strings Reference](../mutations/status-strings.md) - Status taxonomy
- [CASCADE Architecture](../mutations/cascade-architecture.md) - Side effects & cache updates

### Performance & Caching
- [Database-Level Caching](database-level-caching.md) - Caching strategies
- [Performance Guide](../performance/performance-guide.md) - Complete optimization guide
- [Coordinate Performance](../performance/coordinate-performance-guide.md) - Geospatial optimization

### Security
- [RBAC & RLS Patterns](../enterprise/rbac-postgresql-assessment.md) - Authorization guide
- [Multi-Tenancy](../advanced/multi-tenancy.md) - Tenant isolation

### Migrations & Operations
- [Migrations Guide](migrations.md) - Migration best practices
- [Avoid Triggers](avoid-triggers.md) - Why we don't use triggers

### Advanced Topics
- [Advanced Database Patterns](../advanced/database-patterns.md) - Advanced patterns
- [Database API Reference](../core/database-api.md) - Connection and query APIs

---

## 🎓 Learning Path

**New to FraiseQL's database patterns?** Follow this path:

1. **Start**: [FraiseQL Philosophy](../core/fraiseql-philosophy.md) - Understand "why"
2. **Basics**: [Table Naming Conventions](table-naming-conventions.md) - Learn the trinity pattern
3. **IDs**: [Trinity Identifiers](trinity-identifiers.md) - Understand the three-tier ID system
4. **Data**: [View Strategies](view-strategies.md) - Learn how to expose data
5. **Mutations**: [Mutation SQL Requirements](../guides/mutation-sql-requirements.md) - Write your first mutation
6. **Errors**: [Error Handling Patterns](../guides/error-handling-patterns.md) - Handle errors properly
7. **Security**: [RBAC & RLS](../enterprise/rbac-postgresql-assessment.md) - Add authorization
8. **Performance**: [Performance Guide](../performance/performance-guide.md) - Optimize queries

---

**The FraiseQL Way™**: PostgreSQL-first, type-safe, and opinionated. One way to do it, and it's the right way.
