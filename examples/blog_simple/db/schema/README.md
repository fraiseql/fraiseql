# Blog Simple - Schema Organization

**Project Size**: **S (Small)** - 2-digit flat structure

This schema follows FraiseQL's confiture-style DDL organization with **2-digit numbered prefixes** for deterministic file ordering.

---

## File Structure

```
schema/
├── 00_extensions.sql    # PostgreSQL extensions (uuid-ossp, pg_trgm)
├── 10_tables.sql         # Base tables (tb_user, tb_post, tb_comment, tb_tag)
├── 20_indexes.sql        # Performance indexes
├── 30_functions.sql      # Helper functions (slug generation)
├── 40_triggers.sql       # Automated triggers (updated_at, auto-slugs)
├── 50_security.sql       # Row Level Security policies
└── 90_permissions.sql    # Database permissions
```

**Total: 7 files** - Clean, flat structure suitable for small projects

---

## Numbering System

### Why 2-digit prefixes?

For small projects (<20 files), 2-digit prefixes provide:
- ✅ Clear execution order
- ✅ Simple to navigate
- ✅ Easy to maintain
- ✅ Room to add files (gaps at 10, 20, 30...)

### Execution Order

```
00_  Extensions (CREATE EXTENSION)
10_  Tables (CREATE TABLE)
20_  Indexes (CREATE INDEX)
30_  Functions (CREATE FUNCTION)
40_  Triggers (CREATE TRIGGER)
50_  Security (ALTER TABLE ... ENABLE RLS, CREATE POLICY)
90_  Permissions (GRANT)
```

Dependencies are explicit: tables before indexes, functions before triggers, etc.

---

## Loading Schema

### Option 1: Using confiture (recommended)

```bash
# Build complete schema from these files
confiture build --from schema --to my_database

# Or specific environment
confiture build --env production
```

### Option 2: Manual concatenation

```bash
# Files are loaded in alphabetical order
cat schema/*.sql | psql -d my_database
```

### Option 3: Individual files

```bash
psql -d my_database -f schema/00_extensions.sql
psql -d my_database -f schema/10_tables.sql
psql -d my_database -f schema/20_indexes.sql
# ... etc
```

---

## Schema Features

### Trinity Pattern

All tables use FraiseQL's Trinity Pattern for optimal performance:

```sql
CREATE TABLE tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Internal (fast INT joins)
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,     -- Public API (secure UUID)
    identifier TEXT UNIQUE NOT NULL,                       -- Human-readable (username)
    -- ... other columns
);
```

### Automatic Features

- **Auto-generated slugs**: Posts and tags automatically generate slugs from title/name
- **Timestamp tracking**: `updated_at` automatically maintained
- **Published tracking**: `published_at` set when status changes to 'published'
- **Row Level Security**: Example RLS policies for multi-tenant security

---

## When to Refactor

Consider upgrading to **M (Medium, 3-digit)** structure when:
- ❌ You have more than ~15-20 files
- ❌ Multiple related tables need grouping (users, posts, orders, etc.)
- ❌ Functions and views become numerous

Example refactoring to M structure:
```
schema/
├── 000_common/
│   └── 001_extensions.sql
├── 010_tables/
│   ├── 011_user.sql
│   ├── 012_post.sql
│   ├── 013_comment.sql
│   └── 014_tag.sql
├── 020_indexes/
│   └── 021_indexes.sql
└── 030_functions/
    └── 031_functions.sql
```

---

## See Also

- **[DDL Organization Guide](../../../../docs/core/ddl-organization.md)** - Full size classification (XS/S/M/L/XL)
- **[confiture Documentation](https://github.com/fraiseql/confiture)** - Schema build tool
- **[FraiseQL Migrations](../../../../docs/core/migrations.md)** - Migration workflow

---

**Size Classification**: S (Small) - 2-digit flat structure
**FraiseQL Version**: 0.11.5+
