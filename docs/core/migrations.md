# Database Migrations

> **Manage your database schema with confidence using FraiseQL's integrated migration system**

FraiseQL provides a robust migration management system through the `fraiseql migrate` CLI, making it easy to evolve your database schema over time while maintaining consistency across development, staging, and production environments.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Migration Commands](#migration-commands)
- [Migration File Structure](#migration-file-structure)
- [Best Practices](#migration-best-practices)
- [CQRS Migrations](#cqrs-migrations)
- [Production Deployment](#production-deployment)
- [Troubleshooting](#troubleshooting)

---

## Overview

### Why Migrations?

Database migrations allow you to:

- **Version control** your database schema alongside your code
- **Collaborate** with team members without schema conflicts
- **Deploy** confidently knowing the database state is predictable
- **Roll back** changes if something goes wrong
- **Document** schema changes over time

### FraiseQL's Approach

FraiseQL's migration system is powered by **confiture** (https://github.com/fraiseql/confiture):

- **Simple**: SQL-based migrations (no complex DSL to learn)
- **Integrated**: Built into the `fraiseql` CLI
- **Safe**: Track applied migrations to prevent duplicates
- **Flexible**: Works with any PostgreSQL schema

---

## Quick Start

### Initialize Migrations

```bash
# Navigate to your project
cd my-fraiseql-project

# Initialize migration system
fraiseql migrate init

# This creates:
# - migrations/ directory
# - migrations/README.md with instructions
```

### Create Your First Migration

```bash
# Create a new migration
fraiseql migrate create initial_schema

# This creates:
# - migrations/001_initial_schema.sql
```

### Write the Migration

Edit `migrations/001_initial_schema.sql`:

```sql
-- Migration 001: Initial schema

-- Users table
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL UNIQUE,
    full_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Posts table
CREATE TABLE tb_post (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    author_id UUID NOT NULL REFERENCES tb_user(id),
    published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Apply the Migration

```bash
# Apply pending migrations
fraiseql migrate up

# Output:
# ✓ Running migration: 001_initial_schema.sql
# ✓ Migration completed successfully
```

---

## Migration Commands

### `fraiseql migrate init`

Initialize the migration system in your project.

```bash
fraiseql migrate init

# Creates:
# - migrations/ directory
# - migrations/README.md
```

**Options:**
- `--path PATH`: Custom migrations directory (default: `./migrations`)

### `fraiseql migrate create <name>`

Create a new migration file.

```bash
fraiseql migrate create add_comments_table

# Creates: migrations/002_add_comments_table.sql
```

**Naming conventions:**
- Use descriptive names: `add_comments_table`, `add_email_index`
- Use snake_case
- Be specific: `add_user_bio_column` not `update_users`

### `fraiseql migrate up`

Apply all pending migrations.

```bash
fraiseql migrate up

# Apply all pending migrations
```

**Options:**
- `--steps N`: Apply only N migrations
- `--dry-run`: Show what would be applied without running

```bash
# Apply next 2 migrations only
fraiseql migrate up --steps 2

# Preview migrations without applying
fraiseql migrate up --dry-run
```

### `fraiseql migrate down`

Roll back the last migration.

```bash
fraiseql migrate down

# Rolls back the most recent migration
```

**Options:**
- `--steps N`: Roll back N migrations
- `--force`: Skip confirmation prompt

```bash
# Roll back last 2 migrations
fraiseql migrate down --steps 2

# Roll back without confirmation (dangerous!)
fraiseql migrate down --force
```

**⚠️ Warning**: Only use `down` in development. In production, prefer forward-only migrations.

### `fraiseql migrate status`

Show migration status.

```bash
fraiseql migrate status

# Output:
# Migration Status:
#   ✓ 001_initial_schema.sql (applied 2024-01-15 10:30:00)
#   ✓ 002_add_comments_table.sql (applied 2024-01-16 14:20:00)
#   ○ 003_add_indexes.sql (pending)
```

---

## Migration File Structure

### Basic Structure

```sql
-- Migration XXX: Description of what this migration does
--
-- Author: Your Name
-- Date: 2024-01-15
--
-- This migration adds support for user profiles with bio and avatar.

-- Create table
CREATE TABLE tb_user_profile (
    user_id UUID PRIMARY KEY REFERENCES tb_user(id) ON DELETE CASCADE,
    bio TEXT,
    avatar_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add index
CREATE INDEX idx_user_profile_user ON tb_user_profile(user_id);

-- Add initial data (if needed)
INSERT INTO tb_user_profile (user_id, bio)
SELECT id, 'Default bio'
FROM tb_user
WHERE created_at < NOW() - INTERVAL '1 day';
```

### Migration Best Practices

1. **One purpose per migration**
   ```sql
   -- ✅ Good: Focused on one change
   -- Migration 005: Add email verification

   ALTER TABLE tb_user ADD COLUMN email_verified BOOLEAN DEFAULT FALSE;
   CREATE INDEX idx_user_email_verified ON tb_user(email_verified);
   ```

   ```sql
   -- ❌ Bad: Multiple unrelated changes
   -- Migration 005: Various updates

   ALTER TABLE tb_user ADD COLUMN email_verified BOOLEAN;
   CREATE TABLE tb_settings (...);  -- Unrelated!
   ALTER TABLE tb_post ADD COLUMN views INTEGER;  -- Also unrelated!
   ```

2. **Include rollback comments**
   ```sql
   -- Migration 010: Add post categories

   CREATE TABLE tb_category (
       id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
       name TEXT NOT NULL UNIQUE
   );

   -- Rollback:
   -- DROP TABLE tb_category;
   ```

3. **Handle existing data**
   ```sql
   -- Migration 015: Make email required

   -- First, ensure all existing users have emails
   UPDATE tb_user SET email = username || '@example.com'
   WHERE email IS NULL;

   -- Now make it NOT NULL
   ALTER TABLE tb_user ALTER COLUMN email SET NOT NULL;
   ```

---

## CQRS Migrations

When using FraiseQL's CQRS pattern, your migrations will include both command (`tb_*`) and query (`tv_*`) tables.

### Example: Adding a CQRS Entity

```sql
-- Migration 020: Add comments with CQRS pattern

-- ============================================================================
-- COMMAND SIDE: Normalized table for writes
-- ============================================================================

CREATE TABLE tb_comment (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES tb_user(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_comment_post ON tb_comment(post_id);
CREATE INDEX idx_comment_author ON tb_comment(author_id);

-- ============================================================================
-- QUERY SIDE: Denormalized table for reads
-- ============================================================================

CREATE TABLE tv_comment (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,  -- Contains comment + author info
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- GIN index for fast JSONB queries
CREATE INDEX idx_tv_comment_data ON tv_comment USING GIN(data);

-- ============================================================================
-- SYNC TRACKING (optional but recommended)
-- ============================================================================

-- Track when each entity was last synced
CREATE TABLE sync_history (
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (entity_type, entity_id)
);

CREATE INDEX idx_sync_history_synced ON sync_history(synced_at DESC);
```

### Initial Data Sync

After creating `tv_*` tables, you'll need to perform an initial sync:

```python
# In your application startup
from your_app.sync import EntitySync

@app.on_event("startup")
async def initial_sync():
    sync = EntitySync(db_pool)

    # Sync all existing data to query side
    await sync.sync_all_comments()
    logger.info("Initial comment sync complete")
```

---

## Production Deployment

### Safe Production Migrations

1. **Always test migrations first**
   ```bash
   # Test in development
   fraiseql migrate up --dry-run

   # Apply in development
   fraiseql migrate up

   # Verify application works
   ./test_suite.sh
   ```

2. **Use transactions**
   ```sql
   -- Migration 030: Update post status

   BEGIN;

   ALTER TABLE tb_post ADD COLUMN status TEXT DEFAULT 'draft';
   UPDATE tb_post SET status = CASE
       WHEN published THEN 'published'
       ELSE 'draft'
   END;
   ALTER TABLE tb_post DROP COLUMN published;

   COMMIT;
   ```

3. **Avoid long-running migrations during peak hours**
   ```sql
   -- ❌ Bad: Locks table during heavy read load
   CREATE INDEX CONCURRENTLY idx_post_created ON tb_post(created_at);

   -- ✅ Better: Create index concurrently (doesn't lock)
   CREATE INDEX CONCURRENTLY idx_post_created ON tb_post(created_at);
   ```

4. **Have a rollback plan**
   ```bash
   # Before applying migration
   pg_dump -U user -d database > backup_before_migration.sql

   # Apply migration
   fraiseql migrate up

   # If something goes wrong
   psql -U user -d database < backup_before_migration.sql
   ```

### Deployment Process

```bash
#!/bin/bash
# deploy.sh - Safe production deployment

set -e  # Exit on error

echo "1. Creating database backup..."
pg_dump -U $DB_USER -d $DB_NAME > backup_$(date +%Y%m%d_%H%M%S).sql

echo "2. Running migrations..."
fraiseql migrate up

echo "3. Verifying database state..."
fraiseql migrate status

echo "4. Running application tests..."
./test_suite.sh

echo "✓ Deployment complete!"
```

---

## Troubleshooting

### Migration Already Applied

**Problem**: Migration file modified after being applied.

```bash
fraiseql migrate up
# Error: Migration 003_add_indexes.sql checksum mismatch
```

**Solution**: Don't modify applied migrations. Create a new migration instead:

```bash
fraiseql migrate create fix_indexes
```

### Migration Failed Midway

**Problem**: Migration partially applied then failed.

```sql
-- Migration 040: Multiple operations

ALTER TABLE tb_user ADD COLUMN phone TEXT;  -- ✓ Applied
CREATE INDEX idx_user_phone ON tb_user(phone);  -- ✓ Applied
ALTER TABLE tb_post ADD COLUMN invalid_column INVALID_TYPE;  -- ✗ Failed
```

**Solution**:

1. Check what was applied:
   ```bash
   psql -U user -d database -c "\d tb_user"
   ```

2. Manually fix:
   ```sql
   -- Remove partially applied changes
   ALTER TABLE tb_user DROP COLUMN phone;
   DROP INDEX idx_user_phone;
   ```

3. Fix migration file and reapply:
   ```bash
   fraiseql migrate up
   ```

### Migration Tracking Out of Sync

**Problem**: Migration tracking table and actual schema don't match.

**Solution**: Reset migration tracking (⚠️ dangerous):

```sql
-- Check what migrations are tracked
SELECT * FROM fraiseql_migrations ORDER BY applied_at;

-- If needed, manually mark migration as applied
INSERT INTO fraiseql_migrations (version, applied_at)
VALUES ('003_add_indexes', NOW());
```

---

## Advanced Patterns

### Data Migrations

When you need to migrate large amounts of data:

```sql
-- Migration 050: Migrate user preferences

-- Create new table
CREATE TABLE tb_user_preferences (
    user_id UUID PRIMARY KEY REFERENCES tb_user(id),
    preferences JSONB NOT NULL DEFAULT '{}'
);

-- Migrate data in batches (for large datasets)
DO $$
DECLARE
    batch_size INTEGER := 1000;
    offset_val INTEGER := 0;
    rows_affected INTEGER;
BEGIN
    LOOP
        INSERT INTO tb_user_preferences (user_id, preferences)
        SELECT id, jsonb_build_object('theme', 'light', 'language', 'en')
        FROM tb_user
        ORDER BY id
        LIMIT batch_size OFFSET offset_val;

        GET DIAGNOSTICS rows_affected = ROW_COUNT;
        EXIT WHEN rows_affected = 0;

        offset_val := offset_val + batch_size;
        RAISE NOTICE 'Migrated % users', offset_val;
    END LOOP;
END $$;
```

### Zero-Downtime Migrations

For critical production systems:

```sql
-- Step 1: Add new column (nullable)
ALTER TABLE tb_user ADD COLUMN new_email TEXT;

-- Step 2: Backfill data (in batches, over time)
-- (Done by application or background job)

-- Step 3: Make column required (in next migration, after backfill)
ALTER TABLE tb_user ALTER COLUMN new_email SET NOT NULL;

-- Step 4: Drop old column (in yet another migration)
ALTER TABLE tb_user DROP COLUMN old_email;
```

---

## Integration with FraiseQL Features

### CASCADE Rules

When you create foreign keys, consider CASCADE implications:

```sql
-- Migration 060: Add comments with CASCADE

CREATE TABLE tb_comment (
    id UUID PRIMARY KEY,
    post_id UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
    -- ☝️ When post deleted, comments are automatically deleted
    author_id UUID NOT NULL REFERENCES tb_user(id) ON DELETE SET NULL
    -- ☝️ When user deleted, comments remain but author_id becomes NULL
);
```

FraiseQL's auto-CASCADE will detect these relationships and set up cache invalidation rules automatically.

### IVM Setup

After migrations that add tb_/tv_ pairs, update your IVM setup:

```python
# In application startup
from fraiseql.ivm import setup_auto_ivm

@app.on_event("startup")
async def setup_ivm():
    # Analyze schema and setup IVM
    recommendation = await setup_auto_ivm(db_pool, verbose=True)

    # Apply recommended SQL
    async with db_pool.connection() as conn:
        await conn.execute(recommendation.setup_sql)
```

---

## See Also

- Complete CQRS Example (../../examples/complete_cqrs_blog/)
- [CASCADE Best Practices](../guides/cascade-best-practices.md)
- [Explicit Sync Guide](./explicit-sync.md)
- [Database Patterns](../advanced/database-patterns.md)
- [confiture on GitHub](https://github.com/fraiseql/confiture) - Migration library

---

## Summary

FraiseQL's migration system provides:

✅ **Simple** SQL-based migrations
✅ **Safe** tracking of applied changes
✅ **Integrated** with the `fraiseql` CLI
✅ **Production-ready** deployment patterns

**Next Steps**:
1. Initialize migrations: `fraiseql migrate init`
2. Create your first migration: `fraiseql migrate create initial_schema`
3. Apply migrations: `fraiseql migrate up`
4. See the [Complete CQRS Example](../../examples/complete_cqrs_blog/) for a full working demo

---

**Last Updated**: 2025-10-11
**FraiseQL Version**: 0.1.0+
