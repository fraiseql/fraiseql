# DDL Generation Guide: Creating Table-Backed Views

**Status:** Phase 9.5
**Last Updated:** January 24, 2026

---

## Prerequisites

**Required Knowledge:**
- SQL fundamentals (SELECT, JOIN, WHERE clauses)
- View concepts (database views, materialized views)
- FraiseQL schema definition and view selection
- Performance implications of different view strategies
- Index design and query optimization basics
- Database schema design patterns

**Required Software:**
- FraiseQL v2.0.0-alpha.1 or later
- FraiseQL CLI (for DDL generation commands)
- PostgreSQL 14+, MySQL 8.0+, SQLite 3.x, or SQL Server 2019+
- SQL client tool (psql, mysql, sqlite3, sqlcmd)
- Python 3.10+ or TypeScript 4.5+ (optional, for SDK-based generation)
- A text editor for SQL scripts

**Required Infrastructure:**
- Access to your target database (PostgreSQL, MySQL, SQLite, SQL Server)
- Database user with DDL creation permissions (CREATE TABLE, CREATE VIEW)
- Schema already deployed or accessible
- FraiseQL schema.json file for your application
- Sufficient disk space for materialized views (if applicable)

**Optional but Recommended:**
- View Selection Guide documentation for decision making
- Database performance monitoring tools
- Version control for tracking DDL changes
- Migration tools (Flyway, Liquibase)
- Schema visualization tools
- Query performance analysis tools (EXPLAIN ANALYZE)

**Time Estimate:** 15-30 minutes for DDL generation, 30-60 minutes for view validation and testing

## Overview

This guide explains how to use FraiseQL's DDL generation tools to create SQL for table-backed views (`tv_*` and `ta_*`).

### What This Guide Does

- Shows how to generate ready-to-run SQL for table-backed views
- Explains when to use each generation tool (Python, TypeScript, CLI)
- Provides working examples for all platforms
- Links to decision-making guides

### What This Guide Does NOT Do

- ❌ Make optimization decisions for you
- ❌ Automatically create views without your approval
- ❌ Modify your existing schema
- ❌ Deploy anything to your database

**Philosophy:** This is an **implementation tool**, not a decision-making tool. Use [Phase 9.4 View Selection Guide](../architecture/database/view-selection-guide.md) to decide whether you need table-backed views. Once you've decided, use these tools to generate the SQL.

---

## Quick Start

### I'm using Python

```python
from fraiseql_tools.views import generate_tv_ddl, load_schema

# Load your schema
schema = load_schema("schema.json")

# Generate DDL for User → tv_user_profile
ddl = generate_tv_ddl(
    schema=schema,
    entity="User",
    view="tv_user_profile",
    refresh_strategy="trigger-based"
)

# Save to file
with open("tv_user_profile.sql", "w") as f:
    f.write(ddl)

print("Generated tv_user_profile.sql")
```

### I'm using TypeScript

```typescript
import { generateTvDdl, loadSchema } from "@fraiseql/tools/views";
import { writeFileSync } from "fs";

// Load your schema
const schema = loadSchema("schema.json");

// Generate DDL for User → tv_user_profile
const ddl = generateTvDdl({
    schema,
    entity: "User",
    view: "tv_user_profile",
    refreshStrategy: "trigger-based"
});

// Save to file
writeFileSync("tv_user_profile.sql", ddl);

console.log("Generated tv_user_profile.sql");
```

### I'm using the CLI

```bash
fraiseql generate-views \
  --schema schema.json \
  --entity User \
  --view tv_user_profile \
  --output tv_user_profile.sql
```

---

## When to Use This Guide

**Use this guide when:**
- ✅ You've read Phase 9.4 View Selection Guide
- ✅ You've decided to use a table-backed view (tv_* or ta_*)
- ✅ You want to generate the DDL automatically
- ✅ You want to review the SQL before deploying

**Skip this guide if:**
- ❌ You haven't decided whether to use table-backed views yet (read Phase 9.4 first)
- ❌ You want to write the SQL manually
- ❌ You're still evaluating performance

---

## Parameters Explained

All generation functions take similar parameters:

| Parameter | Type | Required | Default | Purpose |
|-----------|------|----------|---------|---------|
| `schema` | dict/object | Yes | - | Loaded `schema.json` |
| `entity` | str | Yes | - | Entity name (e.g., "User", "Order") |
| `view` | str | Yes | - | View name (e.g., "tv_user_profile", "ta_orders") |
| `refresh_strategy` | str | No | "trigger-based" | "trigger-based" or "scheduled" |
| `include_composition_views` | bool | No | true | Include helper views for nested relationships |
| `include_monitoring_functions` | bool | No | true | Include staleness tracking functions |

### Entity

The entity name must match a type in your `schema.json`:

```python
# Your schema.json has:
{
  "types": [
    {"name": "User", ...},
    {"name": "Post", ...}
  ]
}

# Generate for User
ddl = generate_tv_ddl(schema, entity="User", view="tv_user_profile")
```

### View Name

The view name must start with the appropriate prefix:

- **`tv_*`** - Table-backed JSON view (for GraphQL)
  - Example: `tv_user_profile`, `tv_order_summary`
  - Used with JSON plane queries

- **`ta_*`** - Table-backed Arrow view (for analytics)
  - Example: `ta_orders`, `ta_user_events`
  - Used with Arrow Flight queries

### Refresh Strategy

Choose based on your workload:

| Strategy | Best For | Overhead | Latency |
|----------|----------|----------|---------|
| **trigger-based** | High-change data, low tolerance for stale data | Medium (per-row) | <100ms |
| **scheduled** | Batch processes, can tolerate stale data | Low (batched) | 1-60 minutes |

**Examples:**

```python
# Real-time user profile updates
ddl = generate_tv_ddl(
    schema,
    entity="User",
    view="tv_user_profile",
    refresh_strategy="trigger-based"  # Updates immediately
)

# Nightly order analytics
ddl = generate_ta_ddl(
    schema,
    entity="Order",
    view="ta_orders",
    refresh_strategy="scheduled"  # Updates once per night
)
```

See [Performance Testing Guide](./view-selection-performance-testing.md) for help choosing.

### Include Composition Views

When `include_composition_views=True` (default), the generator creates helper views for nested relationships:

```sql
-- Helper views (automatically created)
CREATE VIEW v_user_posts_composed AS ...
CREATE VIEW v_posts_comments_composed AS ...

-- Main table-backed view
CREATE TABLE tv_user_profile (
    id TEXT PRIMARY KEY,
    data JSONB,  -- Contains nested posts + comments
    ...
);
```

Set to `False` if you're managing composition views manually:

```python
ddl = generate_tv_ddl(
    schema,
    entity="User",
    view="tv_user_profile",
    include_composition_views=False  # You'll create these manually
)
```

### Include Monitoring Functions

When `include_monitoring_functions=True` (default), the generator adds functions to track staleness:

```sql
-- Monitoring functions (automatically created)
CREATE FUNCTION tv_user_profile_staleness() AS ...
CREATE FUNCTION tv_user_profile_row_count() AS ...
```

These are useful for production monitoring. Set to `False` if you're managing monitoring separately:

```python
ddl = generate_tv_ddl(
    schema,
    entity="User",
    view="tv_user_profile",
    include_monitoring_functions=False
)
```

---

## Output Structure

Generated DDL contains 6 sections:

```sql
-- 1. Composition Helper Views (if include_composition_views=True)
-- These pre-compose nested relationships into JSONB format
CREATE VIEW v_user_posts_composed AS
  SELECT user_id, jsonb_agg(...) as posts FROM posts GROUP BY user_id;

-- 2. Physical Table
-- The actual table that stores materialized data
CREATE TABLE tv_user_profile (
    id TEXT NOT NULL PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (id) REFERENCES tb_user(id) ON DELETE CASCADE
);

-- 3. Indexes
-- Optimizes common queries
CREATE INDEX idx_tv_user_profile_data_gin ON tv_user_profile USING GIN(data);
CREATE INDEX idx_tv_user_profile_updated ON tv_user_profile(updated_at);

-- 4. Refresh Function
-- Maintains the table based on source data
CREATE FUNCTION refresh_tv_user_profile() AS $$
  INSERT INTO tv_user_profile (id, data, updated_at)
  SELECT u.id, jsonb_build_object(...), NOW()
  FROM tb_user u
  ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data, updated_at = NOW();
$$ LANGUAGE SQL;

-- 5. Refresh Trigger (if trigger-based strategy)
-- Automatically calls refresh function on source changes
CREATE TRIGGER trg_tv_user_profile_refresh
  AFTER INSERT OR UPDATE ON tb_user
  FOR EACH ROW
  EXECUTE FUNCTION refresh_tv_user_profile();

-- 6. Monitoring Functions (if include_monitoring_functions=True)
-- Track view health and staleness
CREATE FUNCTION tv_user_profile_staleness() AS ...
CREATE FUNCTION tv_user_profile_row_count() AS ...
```

---

## Detailed Usage by Language

### Python

**Installation:**

```bash
pip install fraiseql-tools
```

**Basic Usage:**

```python
from fraiseql_tools.views import (
    generate_tv_ddl,
    generate_ta_ddl,
    load_schema,
    validate_generated_ddl
)

# Load schema
schema = load_schema("schema.json")

# Generate tv_* DDL
tv_ddl = generate_tv_ddl(
    schema=schema,
    entity="User",
    view="tv_user_profile",
    refresh_strategy="trigger-based"
)

# Generate ta_* DDL
ta_ddl = generate_ta_ddl(
    schema=schema,
    entity="Order",
    view="ta_orders",
    refresh_strategy="scheduled"
)

# Validate before deploying
errors = validate_generated_ddl(tv_ddl)
if errors:
    print(f"Validation errors: {errors}")
else:
    print("Valid DDL - ready to deploy")

# Save to file
with open("views.sql", "w") as f:
    f.write(tv_ddl)
    f.write("\n\n")
    f.write(ta_ddl)
```

**Full Example:**

See [`examples/ddl-generation/python-example.py`](../../examples/ddl-generation/python-example.py)

### TypeScript

**Installation:**

```bash
npm install @fraiseql/tools
```

**Basic Usage:**

```typescript
import {
    generateTvDdl,
    generateTaDdl,
    loadSchema,
    validateGeneratedDdl
} from "@fraiseql/tools/views";
import { writeFileSync } from "fs";

// Load schema
const schema = loadSchema("schema.json");

// Generate tv_* DDL
const tvDdl = generateTvDdl({
    schema,
    entity: "User",
    view: "tv_user_profile",
    refreshStrategy: "trigger-based"
});

// Generate ta_* DDL
const taDdl = generateTaDdl({
    schema,
    entity: "Order",
    view: "ta_orders",
    refreshStrategy: "scheduled"
});

// Validate before deploying
const errors = validateGeneratedDdl(tvDdl);
if (errors.length > 0) {
    console.error("Validation errors:", errors);
} else {
    console.log("Valid DDL - ready to deploy");
}

// Save to file
writeFileSync("views.sql", tvDdl + "\n\n" + taDdl);
```

**Full Examples:**

See the [DDL Generation Examples](../../examples/ddl-generation/) directory for detailed code examples in Python

### CLI

**Installation:**

```bash
# Already included with fraiseql-cli
cargo install fraiseql-cli
```

**Basic Usage:**

```bash
# Generate trigger-based tv_*
fraiseql generate-views \
  --schema schema.json \
  --entity User \
  --view tv_user_profile \
  --output tv_user_profile.sql

# Generate scheduled ta_*
fraiseql generate-views \
  --schema schema.json \
  --entity Order \
  --view ta_orders \
  --refresh-strategy scheduled \
  --output ta_orders.sql

# Combine multiple views
cat tv_user_profile.sql ta_orders.sql > all_views.sql
```

**All Options:**

```bash
fraiseql generate-views --help
```

```
USAGE:
    fraiseql generate-views [OPTIONS] --schema <SCHEMA> --entity <ENTITY> --view <VIEW>

OPTIONS:
    --schema <PATH>
        Path to schema.json (required)

    --entity <NAME>
        Entity name (required)

    --view <NAME>
        View name (required)

    --refresh-strategy <STRATEGY>
        "trigger-based" or "scheduled" [default: trigger-based]

    --output <PATH>
        Output file [default: {view}.sql]

    --include-composition-views
        Include helper views [default: true]

    --include-monitoring
        Include monitoring functions [default: true]

    --validate
        Only validate, don't write file

    --verbose
        Show generation steps
```

**Full Example:**

See [`examples/ddl-generation/cli-example.sh`](../../examples/ddl-generation/cli-example.sh)

---

## Next Steps

### 1. Generate the DDL

Choose your preferred method (Python, TypeScript, or CLI) and generate the DDL.

### 2. Review the SQL

Read through the generated SQL carefully:

- ✅ Does the composition match your expectations?
- ✅ Are all relationships included?
- ✅ Is the refresh strategy appropriate?
- ✅ Are indexes reasonable?

### 3. Test in Staging

Run the DDL in your staging database:

```bash
psql -h staging-db -U postgres mydb < tv_user_profile.sql
```

Monitor the initial population:

```sql
-- Check row count
SELECT COUNT(*) FROM tv_user_profile;

-- Check staleness
SELECT tv_user_profile_staleness();

-- Spot-check a few rows
SELECT * FROM tv_user_profile LIMIT 5;
```

### 4. Follow Migration Checklist

Complete the full migration process in [View Selection Migration Checklist](./view-selection-migration-checklist.md).

### 5. Deploy to Production

Once staging verification passes, deploy to production.

---

## Common Patterns

### User Profile with Nested Posts

```python
from fraiseql_tools.views import generate_tv_ddl, load_schema

schema = load_schema("schema.json")

# User profile with all posts and comments
ddl = generate_tv_ddl(
    schema,
    entity="User",
    view="tv_user_profile",
    refresh_strategy="trigger-based"
)
```

**Generated Structure:**

```json
{
  "id": "user-123",
  "name": "Alice",
  "email": "alice@example.com",
  "posts": [
    {
      "id": "post-1",
      "title": "Hello",
      "comments": [...]
    }
  ]
}
```

### Order Summary with Line Items

```python
ddl = generate_tv_ddl(
    schema,
    entity="Order",
    view="tv_order_summary",
    refresh_strategy="scheduled"
)
```

**Generated Structure:**

```json
{
  "id": "order-123",
  "customer_id": "cust-456",
  "total": 99.99,
  "status": "shipped",
  "line_items": [
    {
      "product_id": "prod-789",
      "quantity": 2,
      "price": 49.99
    }
  ]
}
```

### Analytics Table with Denormalized Columns

```python
ddl = generate_ta_ddl(
    schema,
    entity="Event",
    view="ta_events",
    refresh_strategy="scheduled"
)
```

**Generated Columns:**

- Denormalized user info (user_name, user_email)
- Pre-aggregated metrics (event_count, total_duration)
- Time-series indexes (BRIN on timestamp)

---

## Troubleshooting

### Issue: "Schema not found"

**Symptom:** `Error: Could not load schema.json`

**Solution:**
- Verify path: `ls -la schema.json`
- Use absolute path: `generate_tv_ddl(schema, ..., schema="/full/path/to/schema.json")`

### Issue: "Entity not found in schema"

**Symptom:** `Error: Entity "UserProfile" not found in schema`

**Solution:**
- Check entity names in schema: `grep '"name"' schema.json | grep -i user`
- Match exact name (case-sensitive): `entity="User"` not `entity="user"`

### Issue: "Invalid refresh strategy"

**Symptom:** `Error: Invalid refresh_strategy "real-time"`

**Solution:**
- Use only: `"trigger-based"` or `"scheduled"`
- See [Refresh Strategy](#refresh-strategy) section for details

### Issue: Generated SQL has syntax errors

**Symptom:** `SYNTAX ERROR: unexpected token`

**Solution:**
- Validate before deploying: `validate_generated_ddl(ddl)`
- Check schema.json for invalid characters
- Report issue: [GitHub Issues](https://github.com/anthropics/fraiseql/issues)

---

## Validation

All generated DDL is automatically validated for:

- ✅ Valid PostgreSQL syntax
- ✅ Proper table/view definitions
- ✅ Valid column types
- ✅ Correct index usage
- ✅ Proper refresh function structure

To validate manually:

```python
from fraiseql_tools.views import validate_generated_ddl

errors = validate_generated_ddl(ddl)
if errors:
    for error in errors:
        print(f"❌ {error}")
else:
    print("✅ Valid DDL")
```

---

## Performance Considerations

### Trigger-Based Refresh

**Best for:**
- Small tables (< 100K rows)
- High query volume (> 1000 req/sec)
- Must have <1 second data freshness

**Cost:**
- ~10-50ms per source row change
- Scales linearly with update rate

### Scheduled Refresh

**Best for:**
- Large tables (> 1M rows)
- Batch processes
- Can tolerate 1-60 minute staleness

**Cost:**
- ~100-500ms total (regardless of table size)
- Fixed schedule (daily, hourly, etc.)

See [Performance Testing Guide](./view-selection-performance-testing.md) for benchmarking.

---

## See Also

- **[Phase 9.4: View Selection Guide](../architecture/database/view-selection-guide.md)** - Decide whether to use table-backed views
- **[TV Table Pattern](../architecture/database/tv-table-pattern.md)** - Deep dive into JSON plane table-backed views
- **[TA Table Pattern](../architecture/database/ta-table-pattern.md)** - Deep dive into Arrow plane table-backed views
- **[Migration Checklist](./view-selection-migration-checklist.md)** - Step-by-step deployment workflow
- **[Performance Testing Guide](./view-selection-performance-testing.md)** - Benchmark your views
- **[Quick Reference](./view-selection-quick-reference.md)** - Quick lookup tables

---

## Questions?

For issues or questions:

1. Check [troubleshooting section](#troubleshooting)
2. Review [Phase 9.4 View Selection Guide](../architecture/database/view-selection-guide.md)
3. Open an issue: [GitHub Issues](https://github.com/anthropics/fraiseql/issues)

---

**Last Updated:** January 24, 2026
**Phase:** 9.5
**Status:** Ready for Implementation
