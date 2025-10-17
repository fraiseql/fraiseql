# FraiseQL v1 - Pattern Updates Summary

**Date**: 2025-10-16
**Status**: ✅ **FINALIZED - Trinity + Functions are DEFAULT**

---

## 🎯 Final Naming Convention (DEFAULT)

### **Command Side (tb_*)**
```sql
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,           -- Internal primary key (fast joins)
    fk_organisation INT NOT NULL           -- Foreign key (fast joins)
        REFERENCES tb_organisation(pk_organisation),
    id UUID DEFAULT gen_random_uuid()      -- Public API identifier
        UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,       -- Human-readable (username, slug)
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL
);
```

### **Query Side (tv_*)**
```sql
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,                   -- Just UUID! Clean GraphQL
    identifier TEXT UNIQUE NOT NULL,
    data JSONB NOT NULL
);
```

### **Naming Rules**
- `pk_*` = SERIAL PRIMARY KEY (internal, fast INT joins)
- `fk_*` = INT FOREIGN KEY (references pk_*)
- `id` = UUID (public API, exposed in GraphQL)
- `identifier` = TEXT (human-readable: username, slug, etc.)

---

## 📚 Documentation Updates

### **1. V1_ADVANCED_PATTERNS.md** ✅ **UPDATED**
**Status**: Complete rewrite with correct naming
**Location**: `/home/lionel/code/fraiseql/V1_ADVANCED_PATTERNS.md`

**Key changes**:
- Trinity pattern now uses `pk_*` for SERIAL PRIMARY KEY
- Foreign keys now `fk_*` referencing `pk_*`
- Public ID is just `id` (UUID) in GraphQL
- Complete examples with organisation → user → post hierarchy
- Mutations as functions pattern included
- Both patterns marked as **DEFAULT** for v1

---

### **2. V1_COMPONENT_PRDS.md** - Updates Needed

**PRD 2: Repository Pattern**

Add this section after line 350:

```markdown
### **Trinity Identifier Support**

The QueryRepository supports trinity identifiers out of the box:

```python
class QueryRepository:
    async def find_one(
        self,
        view: str,
        id: UUID | None = None,            # By public UUID
        identifier: str | None = None       # By human identifier
    ) -> dict | None:
        """Find by UUID or identifier"""
        if id:
            where = "id = $1"
            param = id
        elif identifier:
            where = "identifier = $1"
            param = identifier
        else:
            raise ValueError("Must provide id or identifier")

        result = await self.db.fetchrow(
            f"SELECT data FROM {view} WHERE {where}",
            param
        )
        return result["data"] if result else None

# Usage
@query
async def user(
    info,
    id: UUID | None = None,
    identifier: str | None = None
) -> User | None:
    repo = QueryRepository(info.context["db"])
    if id:
        return await repo.find_one("tv_user", id=id)
    elif identifier:
        return await repo.find_one("tv_user", identifier=identifier)
```

### **Mutations as Functions (DEFAULT)**

All mutations should be implemented as PostgreSQL functions:

```python
@mutation
async def create_user(
    info,
    organisation: str,  # Organisation identifier
    identifier: str,    # Username
    name: str,
    email: str
) -> User:
    """Create user (business logic in database)"""
    db = info.context["db"]

    # Call database function (contains all logic)
    id = await db.fetchval(
        "SELECT fn_create_user($1, $2, $3, $4)",
        organisation, identifier, name, email
    )

    # Read from query side
    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=id)
```

See **V1_ADVANCED_PATTERNS.md** for complete pattern details.
```

---

### **3. FRAISEQL_V1_BLUEPRINT.md** - Updates Needed

Add this section after "Database Objects" (around line 48):

```markdown
## Core Database Patterns (DEFAULT)

### Trinity Identifiers
FraiseQL v1 uses a trinity identifier pattern by default:

**Command Side**:
```sql
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,           -- Fast internal joins
    fk_organisation INT NOT NULL,         -- Fast foreign keys
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,  -- Public API
    identifier TEXT UNIQUE NOT NULL,      -- Human-readable
    ...
);
```

**Query Side**:
```sql
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,                  -- Clean GraphQL API
    identifier TEXT UNIQUE NOT NULL,
    data JSONB NOT NULL
);
```

**Benefits**:
- Fast database joins (SERIAL integers, ~10x faster than UUID)
- Secure public API (UUID doesn't expose count)
- Human-friendly URLs (identifier/slug)
- Clean GraphQL schema (just "id")

### Mutations as Functions
All mutations are PostgreSQL functions containing business logic:

```sql
CREATE FUNCTION fn_create_user(
    p_organisation_identifier TEXT,
    p_identifier TEXT,
    p_name TEXT,
    p_email TEXT
) RETURNS UUID AS $$
DECLARE
    v_fk_organisation INT;
    v_id UUID;
BEGIN
    -- Validation, transaction, sync all in one function
    SELECT pk_organisation INTO v_fk_organisation
    FROM tb_organisation WHERE identifier = p_organisation_identifier;

    INSERT INTO tb_user (fk_organisation, identifier, name, email)
    VALUES (v_fk_organisation, p_identifier, p_name, p_email)
    RETURNING id INTO v_id;

    PERFORM fn_sync_tv_user(v_id);
    RETURN v_id;
END;
$$ LANGUAGE plpgsql;
```

Python becomes trivial:
```python
@mutation
async def create_user(info, organisation: str, identifier: str, name: str, email: str):
    id = await db.fetchval("SELECT fn_create_user($1, $2, $3, $4)", ...)
    return await repo.find_one("tv_user", id=id)
```

**Benefits**:
- Business logic reusable (psql, cron, triggers)
- Automatic transactions
- Testable in SQL
- Single round-trip

See **V1_ADVANCED_PATTERNS.md** for complete details.
```

Update "Database Conventions" section (around line 48):
```markdown
- `tb_*` = Tables (command side, normalized)
  - `pk_*` = SERIAL PRIMARY KEY (fast internal joins)
  - `fk_*` = INT FOREIGN KEY (fast foreign key references)
  - `id` = UUID (public API identifier)
  - `identifier` = TEXT (human-readable slug/username)
- `tv_*` = Table views (query side, denormalized JSONB)
  - `id` = UUID PRIMARY KEY (public)
  - `identifier` = TEXT (human-readable)
  - `data` = JSONB (denormalized data)
- `fn_sync_tv_*` = Sync functions (explicit, no triggers)
- `fn_create_*` = Mutation functions (INSERT + sync)
- `fn_update_*` = Mutation functions (UPDATE + sync)
- `fn_delete_*` = Mutation functions (DELETE + cascade)
```

---

### **4. V1_DOCUMENTATION_PLAN.md** - Updates Needed

Update the Quick Start section (around line 150) with trinity + functions pattern:

```markdown
## Step 2: Database Setup

```sql
-- Command side with trinity identifiers
CREATE TABLE tb_organisation (
    pk_organisation SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,
    fk_organisation INT NOT NULL REFERENCES tb_organisation(pk_organisation),
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL
);

-- Query side (clean!)
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    identifier TEXT UNIQUE NOT NULL,
    data JSONB NOT NULL
);

-- Sync function
CREATE FUNCTION fn_sync_tv_user(p_id UUID) RETURNS void AS $$
BEGIN
    INSERT INTO tv_user (id, identifier, data)
    SELECT
        u.id,
        u.identifier,
        jsonb_build_object(
            'id', u.id::text,
            'identifier', u.identifier,
            'name', u.name,
            'email', u.email,
            'organisation', (
                SELECT jsonb_build_object(
                    'id', o.id::text,
                    'identifier', o.identifier,
                    'name', o.name
                )
                FROM tb_organisation o
                WHERE o.pk_organisation = u.fk_organisation
            )
        )
    FROM tb_user u
    WHERE u.id = p_id
    ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data;
END;
$$ LANGUAGE plpgsql;

-- Mutation function
CREATE FUNCTION fn_create_user(
    p_organisation_identifier TEXT,
    p_identifier TEXT,
    p_name TEXT,
    p_email TEXT
) RETURNS UUID AS $$
DECLARE
    v_fk_organisation INT;
    v_id UUID;
BEGIN
    SELECT pk_organisation INTO v_fk_organisation
    FROM tb_organisation WHERE identifier = p_organisation_identifier;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Organisation not found';
    END IF;

    INSERT INTO tb_user (fk_organisation, identifier, name, email)
    VALUES (v_fk_organisation, p_identifier, p_name, p_email)
    RETURNING id INTO v_id;

    PERFORM fn_sync_tv_user(v_id);
    RETURN v_id;
END;
$$ LANGUAGE plpgsql;
```

## Step 5: Write Mutations

```python
@mutation
async def create_user(
    info,
    organisation: str,  # Organisation identifier (human-friendly!)
    identifier: str,    # Username
    name: str,
    email: str
) -> User:
    """Create user (business logic in database)"""
    db = info.context["db"]

    # Call database function - that's it!
    id = await db.fetchval(
        "SELECT fn_create_user($1, $2, $3, $4)",
        organisation, identifier, name, email
    )

    repo = QueryRepository(db)
    return await repo.find_one("tv_user", id=id)
```

## Step 8: Test Mutation

```graphql
mutation {
  createUser(
    organisation: "acme-corp",      # Human-friendly!
    identifier: "alice",
    name: "Alice",
    email: "alice@example.com"
  ) {
    id
    identifier
    name
    organisation {
      identifier
      name
    }
  }
}
```

**Response time**: ~0.6ms ⚡ (single database call!)
```

---

## 🎯 Configuration Updates

Update FraiseQLConfig with trinity + functions defaults:

```python
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Trinity identifiers (DEFAULT in v1)
    trinity_identifiers=True,
    primary_key_prefix="pk_",          # pk_user, pk_post
    foreign_key_prefix="fk_",          # fk_organisation, fk_user
    public_id_column="id",             # UUID (GraphQL)
    identifier_column="identifier",    # Human-readable

    # Mutations as functions (DEFAULT in v1)
    mutations_as_functions=True,
    mutation_function_prefix="fn_",
    sync_function_prefix="fn_sync_tv_",

    # CQRS naming
    command_table_prefix="tb_",
    query_view_prefix="tv_",
    jsonb_column="data",
)
```

---

## 📋 Migration Guide

For projects migrating to v1 patterns:

### **1. Update Schema**
```sql
-- Add trinity identifiers to existing tables
ALTER TABLE tb_user RENAME COLUMN user_id TO pk_user;
ALTER TABLE tb_user
    ADD COLUMN id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    ADD COLUMN identifier TEXT UNIQUE NOT NULL;

ALTER TABLE tb_post
    RENAME COLUMN user_id TO fk_user;
```

### **2. Create Database Functions**
```bash
# Use CLI to generate
fraiseql codegen functions --all

# Or write manually following pattern in V1_ADVANCED_PATTERNS.md
```

### **3. Update Python Code**
```python
# Before
@mutation
async def create_user(info, name, email):
    user_id = await db.execute("INSERT INTO tb_user...")
    await sync_tv_user(db, user_id)
    return await repo.find_one("tv_user", pk_user=user_id)

# After (v1 pattern)
@mutation
async def create_user(info, identifier, name, email):
    id = await db.fetchval("SELECT fn_create_user($1, $2, $3)", identifier, name, email)
    return await repo.find_one("tv_user", id=id)
```

---

## ✅ Implementation Checklist

- [x] V1_ADVANCED_PATTERNS.md - Complete rewrite with correct naming
- [ ] V1_COMPONENT_PRDS.md - Add trinity + functions to PRD 2
- [ ] FRAISEQL_V1_BLUEPRINT.md - Add "Core Patterns" section
- [ ] V1_DOCUMENTATION_PLAN.md - Update Quick Start with trinity
- [ ] Create example migrations showing full pattern
- [ ] Update all code examples in docs
- [ ] Generate SQL templates for codegen
- [ ] Write tests for trinity pattern
- [ ] Write tests for mutation functions

---

## 🎓 Why This Matters (Interview Talking Points)

**Trinity Identifiers**:
- "I implemented a trinity identifier pattern: SERIAL for fast joins, UUID for secure APIs, and slugs for user-friendly URLs"
- "This gave us 10x faster joins while maintaining API security"
- "The pattern balances performance, security, and user experience"

**Mutations as Functions**:
- "I moved all business logic into PostgreSQL functions for reusability"
- "This made mutations testable in SQL without needing the Python app"
- "Functions provide automatic transactions and can be called from any client"
- "Python becomes a thin wrapper - just 3 lines per mutation"

**Combined Impact**:
- "Shows database-first thinking, not ORM-centric"
- "Demonstrates understanding of stored procedures"
- "Proves I can balance trade-offs (complexity vs performance)"
- "Perfect for discussing with database-savvy interviewers"

---

## 📖 Reference

**Primary Source**: `V1_ADVANCED_PATTERNS.md`
- Complete examples
- Full SQL schemas
- Python integration
- Testing strategies
- CLI codegen support

**This document**: Quick reference for updating other docs

---

**Status**: ✅ Patterns finalized and documented
**Next**: Apply updates to remaining docs and create migration templates
