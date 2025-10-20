# Core Concepts Glossary

Beginner-friendly explanations of FraiseQL's key concepts. Each concept includes a simple definition, real-world analogy, and why it matters.

## CQRS (Command Query Responsibility Segregation)

### What is CQRS?
**CQRS separates reading data from writing data** - like having separate lines for ordering food vs. picking it up.

### Simple Analogy
Imagine a restaurant:
- **Commands** (writes): You tell the kitchen "I want a burger" (create order)
- **Queries** (reads): You ask "Is my burger ready?" (check status)

### CQRS Flow Diagram
```
User Action → Command (Write) → tb_* Table → Sync → tv_* Table → Query (Read) → User
     ↓              ↓              ↓              ↓              ↓              ↓
  "Create Post"  INSERT INTO    tb_post      fn_sync_tv_post   tv_post      SELECT data
                 tb_post                                      FROM tv_post
```

In FraiseQL:
- **Commands**: Write to `tb_*` tables (like placing an order)
- **Queries**: Read from `v_*` or `tv_*` views (like checking order status)

### Why It Matters
- **Performance**: Reading and writing have different needs
- **Scalability**: Can optimize reads and writes separately
- **Simplicity**: Clear separation of concerns

### Example
```sql
-- Command (write): Create user
INSERT INTO tb_user (id, data) VALUES ($1, $2);

-- Query (read): Get user info
SELECT data FROM v_user WHERE id = $1;
```

## JSONB Views (v_* and tv_*)

### What are JSONB Views?
**Views that package your data as JSONB objects** for GraphQL - like pre-packaged meal kits vs. cooking from scratch.

### Simple Analogy
- **Raw ingredients** (`tb_*` tables): Individual columns (id, name, email, etc.)
- **Meal kit** (`v_*` views): Everything packaged as JSONB ready to serve

### View Types
- **`v_*` views**: Real-time, compute JSONB on-the-fly
- **`tv_*` tables**: Pre-computed JSONB for faster reads

### Why It Matters
- **Performance**: Skip Python object creation
- **Flexibility**: Easy to add/change fields without schema changes
- **GraphQL-ready**: Direct JSONB → GraphQL mapping

### Example
```sql
-- Raw table
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    name TEXT,
    email TEXT
);

-- JSONB view
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) AS data
FROM tb_user;
```

## Trinity Identifiers

### What is the Trinity Pattern?
**Three types of identifiers per entity** - like having a house number, street address, and nickname.

### The Three Identifiers
1. **`pk_*` (Primary Key)**: Internal database ID (fast joins)
2. **`id` (UUID)**: Public API identifier (secure)
3. **`identifier` (Slug)**: Human-readable name (SEO-friendly)

### Simple Analogy
- **House number** (`pk_*`): `123` - fast for computers
- **Full address** (`id`): `550e8400-e29b-41d4-a716-446655440000` - unique everywhere
- **Nickname** (`identifier`): `johns-house` - easy for humans

### Why It Matters
- **Performance**: Fast joins with integers, secure APIs with UUIDs
- **SEO**: Human-readable URLs with slugs
- **Security**: Never expose internal IDs to users

### Naming Convention Table

| Identifier Type | Column Name | Data Type | Purpose | Exposed in API? |
|----------------|-------------|-----------|---------|-----------------|
| **Primary Key** | `pk_post` | `INTEGER` | Fast database joins | ❌ Never |
| **Public ID** | `id` | `UUID` | Secure API identifier | ✅ Always |
| **Human Slug** | `identifier` | `TEXT` | SEO-friendly URLs | ✅ Sometimes |

### Example
```sql
CREATE TABLE tb_post (
    pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Fast joins
    id UUID DEFAULT gen_random_uuid(),                         -- API security
    identifier TEXT UNIQUE,                                    -- SEO-friendly
    title TEXT NOT NULL
);
```

## Database-First Architecture

### What is Database-First?
**Design your API starting from the database** - like building a house from the foundation up, not decorating first.

### Simple Analogy
- **Traditional**: Start with API design, then figure out database
- **Database-first**: Start with data model, then build API on top

### Visual Comparison: ORM vs Database-First

**Traditional ORM Approach:**
```
API Layer (Python) → ORM Objects → SQL Generation → Database
    ↓                      ↓              ↓              ↓
Flask/Django         SQLAlchemy      Generated SQL    Tables
```

**FraiseQL Database-First:**
```
Database Functions → JSONB Views → Direct JSONB → GraphQL API
    ↓                      ↓              ↓              ↓
PostgreSQL Logic      v_*/tv_*       No Python       Strawberry
Stored Procedures     Views          Objects         Types
```

### Why It Matters
- **Performance**: Database does what it does best
- **Consistency**: Single source of truth
- **Maintainability**: Business logic lives in reusable functions

### Example
```sql
-- Business logic in database
CREATE FUNCTION create_user(user_data JSONB)
RETURNS UUID AS $$
    -- Validation, defaults, relationships all here
    INSERT INTO tb_user (data)
    VALUES (user_data || '{"created_at": now()}')
    RETURNING id;
$$ LANGUAGE sql;

-- Simple API call
@app.mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    user_id = await db.execute_function("create_user", {"user_data": input})
    return await db.find_one("v_user", where={"id": user_id})
```

## Transform Tables (tv_*)

### What are Transform Tables?
**Pre-computed JSONB tables for instant queries** - like having frozen meals ready in your freezer.

### Simple Analogy
- **Cook from scratch** (`v_*` views): Prepare meal when ordered
- **Frozen meal** (`tv_*` tables): Pre-cooked, just heat and serve

### How They Work
1. **Sync functions** keep `tv_*` tables updated from `tb_*` tables
2. **Queries read** from `tv_*` tables (fast!)
3. **Rust transformer** converts JSONB to GraphQL instantly

### Why It Matters
- **Speed**: 40x faster than computing JSONB on-the-fly
- **Consistency**: Pre-computed ensures identical results
- **Scalability**: Great for high-traffic reads

### Example
```sql
-- Transform table (pre-computed)
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL  -- Pre-built JSONB
);

-- Sync function (keeps tv_* updated)
CREATE FUNCTION fn_sync_tv_user(user_id UUID)
RETURNS void AS $$
    INSERT INTO tv_user (id, data)
    SELECT id, jsonb_build_object(...) FROM tb_user WHERE id = user_id
    ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data;
$$ LANGUAGE sql;
```

## Putting It All Together

### Complete Flow Example
```sql
-- 1. Command (CQRS write)
INSERT INTO tb_post (id, title, content)
VALUES (gen_random_uuid(), 'Hello World', 'Content...');

-- 2. Sync (update transform table)
SELECT fn_sync_tv_post(post_id);

-- 3. Query (CQRS read from transform table)
SELECT data FROM tv_post WHERE id = $1;
-- Returns pre-computed JSONB instantly!
```

### Why This Architecture Works
- **CQRS**: Separates concerns for better performance
- **Trinity**: Fast joins, secure APIs, human-friendly URLs
- **JSONB**: Flexible data without schema migrations
- **Database-first**: Business logic where it belongs
- **Transform tables**: Pre-compute for speed

## Quick Reference Table

| Concept | Purpose | Example Pattern |
|---------|---------|-----------------|
| **CQRS** | Separate reads/writes | `tb_*` (write) ↔ `v_*`/`tv_*` (read) |
| **JSONB Views** | GraphQL-ready data | `v_user`, `tv_post` |
| **Trinity IDs** | Multi-purpose identifiers | `pk_*`, `id`, `identifier` |
| **Database-First** | Logic in PostgreSQL | Functions over application code |
| **Transform Tables** | Pre-computed speed | `tv_*` with sync functions |

## Progressive Learning Path

### Level 1: Basic Understanding (Start Here)
**Focus**: What each concept does
- CQRS = Separate reads from writes
- JSONB Views = Pre-packaged data for GraphQL
- Trinity = Three types of IDs per entity
- Database-First = Logic lives in PostgreSQL

### Level 2: Simple Implementation
**Focus**: Basic usage patterns
```sql
-- Simple CQRS
CREATE TABLE tb_user (id UUID, name TEXT);
CREATE VIEW v_user AS SELECT id, jsonb_build_object('id', id, 'name', name) AS data FROM tb_user;
```

### Level 3: Advanced Patterns
**Focus**: Performance optimization
```sql
-- Trinity + CQRS + JSONB
CREATE TABLE tb_post (
    pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid(),
    identifier TEXT UNIQUE,
    data JSONB
);
CREATE TABLE tv_post (id UUID PRIMARY KEY, data JSONB);
```

### Level 4: Production Architecture
**Focus**: Enterprise patterns
- Multi-tenant isolation
- Explicit sync functions
- Rust transformation
- Performance monitoring

## Real-World Analogies

### CQRS Like a Bank
- **Teller Window** (Commands): Deposit/withdraw money (changes state)
- **ATM Screen** (Queries): Check balance (reads state)

### JSONB Like a Swiss Army Knife
- **Traditional DB**: One tool per job (separate columns)
- **JSONB**: Multi-tool that adapts (flexible data structure)

### Trinity Like Address Systems
- **GPS Coordinates** (`pk_*`): Precise but not human-friendly
- **Full Address** (`id`): Globally unique but complex
- **Nickname** (`identifier`): Easy to remember and share

## Next Steps

- **[Database API](../api-reference/database.md)**: How to use these concepts in code
- **[DDL Organization](ddl-organization.md)**: How to structure your database
- **[Quickstart](../../quickstart.md)**: See concepts in action
- **[Examples](../../examples/)**: Real implementations</content>
</xai:function_call">Write file to docs/core/concepts-glossary.md
