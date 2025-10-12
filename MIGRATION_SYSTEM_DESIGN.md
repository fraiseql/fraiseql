# FraiseQL Migration System Design

**Date**: October 11, 2025
**Status**: Design Proposal
**Based on**: printoptim_backend db/ structure
**Reference**: ROADMAP_V1.md Phase 1 Priority 1

---

## Executive Summary

Design a **build-from-scratch** migration system for FraiseQL that maintains three synchronized representations of database state:

1. **Source DDL files** (history-free, organized hierarchy)
2. **Migration files** (incremental ALTER statements for production)
3. **Auto-population system** (fresh DB ← production data)

Plus a **fourth migration strategy** for zero-downtime production migrations:

4. **Schema-to-Schema Migration** (production [old] → pristine [new] via COPY/FDW)

**Key Philosophy**: The `db/` directory is the **single source of truth**, organized by domain and always buildable from scratch.

---

## Inspiration: printoptim_backend Architecture

### Proven Structure (750+ SQL files, <1s rebuild)

```
db/
├── 0_schema/                    # Source of truth (DDL)
│   ├── 00_common/               # Extensions, types, utilities
│   ├── 01_write_side/           # CQRS write models
│   ├── 02_query_side/           # CQRS read models (views)
│   ├── 03_functions/            # Stored procedures
│   ├── 04_turbo_router/         # Performance layer
│   └── 05_lazy_caching/         # Cache tables
├── 1_seed_common/               # Reference data (shared)
├── 2_seed_backend/              # Dev seed data
├── 3_seed_frontend/             # Frontend-specific seeds
├── 5_refresh_mv/                # Materialized view refresh
├── 7_grant/                     # Permissions
├── 99_finalize/                 # Cleanup
├── database_local.sql           # Generated (753 files)
├── database_production.sql      # Generated (548 files)
└── .schema_version.json         # Version tracking
```

### Key Insights

1. **Numbered directories** enforce execution order
2. **Environment-specific builds** from same source
3. **Python rebuilder** concatenates files deterministically
4. **Hash-based change detection** (SHA256 of all files)
5. **Template caching** for fast remote deployment (2-3s vs 80s)

---

## FraiseQL Adaptation

### Directory Structure

```
project_root/
├── db/
│   ├── schema/                  # Source DDL (build from scratch)
│   │   ├── 00_common/
│   │   │   ├── 000_extensions.sql
│   │   │   ├── 001_types.sql
│   │   │   └── 002_domains.sql
│   │   ├── 10_tables/
│   │   │   ├── users.sql
│   │   │   ├── posts.sql
│   │   │   └── comments.sql
│   │   ├── 20_views/
│   │   │   └── user_stats.sql
│   │   ├── 30_functions/
│   │   │   ├── create_user.sql
│   │   │   └── update_post.sql
│   │   ├── 40_indexes/
│   │   │   └── performance.sql
│   │   └── 50_permissions/
│   │       └── grants.sql
│   │
│   ├── migrations/              # Incremental changes (ALTER)
│   │   ├── 001_initial_schema.py
│   │   ├── 002_add_user_email_index.py
│   │   ├── 003_rename_post_title.py
│   │   └── .migration_state.json
│   │
│   ├── seeds/                   # Optional seed data
│   │   ├── common/              # Reference data
│   │   └── dev/                 # Development data
│   │
│   ├── environments/            # Environment-specific config
│   │   ├── local.yaml
│   │   ├── test.yaml
│   │   ├── staging.yaml
│   │   └── production.yaml
│   │
│   └── generated/               # Build artifacts (gitignored)
│       ├── schema_local.sql
│       ├── schema_production.sql
│       └── .checksums.json
│
├── scripts/
│   └── db/
│       ├── build_schema.py      # Schema rebuilder
│       ├── migrate.py           # Migration runner
│       └── sync_from_prod.py   # Data population tool
│
└── src/fraiseql/migration/
    ├── builder.py               # Schema builder
    ├── migrator.py              # Migration executor
    ├── diff.py                  # Schema diff detector
    └── syncer.py                # Production sync
```

---

## Migration Strategy Comparison

### When to Use Each Approach

| Strategy | Use Case | Downtime | Complexity | Rollback |
|----------|----------|----------|------------|----------|
| **1. Build from Scratch** | New environment, dev setup | N/A | Low | N/A |
| **2. In-Place Migration (ALTER)** | Simple schema changes, single column | Seconds | Medium | Via down() |
| **3. Production Sync (data copy)** | Populate dev from prod | Minutes | Low | N/A |
| **4. Schema-to-Schema (FDW/COPY)** | Complex migrations, zero downtime | 0-5 sec | High | Full DB restore |

### Decision Tree

```
Need to change production schema?
│
├─ YES → Is it a simple change (add column, index)?
│   │
│   ├─ YES → Use Strategy 2 (In-Place ALTER migration)
│   │         fraiseql db migrate up
│   │
│   └─ NO → Complex change (rename, restructure, multiple deps)?
│             Use Strategy 4 (Schema-to-Schema FDW migration)
│             fraiseql db migrate schema-to-schema --strategy fdw
│
└─ NO → Need fresh database?
    │
    ├─ Empty DB → Use Strategy 1 (Build from scratch)
    │              fraiseql db build --env production
    │
    └─ With data → Use Strategy 3 (Production sync)
                   fraiseql db sync --from production
```

---

## Three-Medium Workflow

### Medium 1: Source DDL Files (schema/)

**Purpose**: History-free, always reflects current desired state

**Example: Change a column name**

**Before** (`db/schema/10_tables/users.sql`):
```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    full_name TEXT,  -- OLD NAME
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

**After** (`db/schema/10_tables/users.sql`):
```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    display_name TEXT,  -- NEW NAME (just update DDL)
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

**AI Assistance**: Edit DDL file directly (no history preserved here)

---

### Medium 2: Migration Files (migrations/)

**Purpose**: Incremental changes for existing production databases

**Auto-generated** (or manually written):

```python
# db/migrations/003_rename_user_full_name.py
from fraiseql.migration import Migration

class RenameUserFullName(Migration):
    """Rename users.full_name to users.display_name"""

    def up(self):
        self.execute("""
            ALTER TABLE users
            RENAME COLUMN full_name TO display_name;
        """)

    def down(self):
        self.execute("""
            ALTER TABLE users
            RENAME COLUMN display_name TO full_name;
        """)

    # Optional: Data migration
    def data_migration(self):
        pass
```

**AI Assistance**: Generate migration from schema diff or write manually

---

### Medium 3: Production Data Sync (sync_from_prod.py)

**Purpose**: Populate fresh development DB from production

**Workflow**:
```bash
# 1. Build fresh local schema from source
fraiseql db build --env local

# 2. Sync production data (respects privacy)
fraiseql db sync --from production --exclude users.email
```

**Features**:
- Schema-aware data transfer
- Column mapping (old → new names)
- PII anonymization
- Incremental sync support

---

## Medium 4: Schema-to-Schema Migration (COPY/FDW)

**Purpose**: Zero-downtime migration from old production schema to fresh pristine schema

**When to Use**:
- Complex schema changes (multiple dependent migrations)
- High-risk migrations (want atomic cutover)
- Performance-critical systems (minimize downtime)
- Schema divergence issues (ensure pristine state)

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ PRODUCTION DATABASE (OLD SCHEMA)                             │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ Database: myapp_production                               │ │
│ │ Schema: v1.5.3 (older)                                  │ │
│ │ Tables: users, posts, comments (old structure)          │ │
│ │ Data: Live production data                              │ │
│ └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ FDW Connection
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ NEW DATABASE (PRISTINE SCHEMA)                               │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ Database: myapp_production_new                          │ │
│ │ Schema: v1.6.0 (built from db/schema/)                 │ │
│ │ Tables: users, posts, comments (new structure)          │ │
│ │ FDW: myapp_production_old (foreign data wrapper)        │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                               │
│ MIGRATION SCRIPT:                                            │
│ INSERT INTO users (id, username, display_name, ...)         │
│ SELECT id, username, full_name, ... FROM old_users;         │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ Atomic Swap
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ CUTOVER (pg_rename_database or DNS switch)                  │
│ myapp_production → myapp_production_old_backup              │
│ myapp_production_new → myapp_production                     │
└─────────────────────────────────────────────────────────────┘
```

### Implementation: Two Strategies

#### **Strategy A: COPY (Direct DB Copy)**

**Pros**: Simple, fast, no external dependencies
**Cons**: Requires exclusive lock during cutover

```sql
-- 1. Build pristine schema in new database
CREATE DATABASE myapp_production_new;
-- Apply: db/generated/schema_production.sql

-- 2. Copy data (supports transformations)
-- db/migrations/schema_to_schema/v1.5.3_to_v1.6.0.sql
BEGIN;

-- Copy with column mapping
INSERT INTO myapp_production_new.users (id, username, display_name, created_at)
SELECT
    id,
    username,
    full_name AS display_name,  -- Column rename
    created_at
FROM dblink('dbname=myapp_production',
    'SELECT id, username, full_name, created_at FROM users')
AS old_users(id uuid, username text, full_name text, created_at timestamptz);

-- Copy posts (no changes)
INSERT INTO myapp_production_new.posts
SELECT * FROM dblink('dbname=myapp_production',
    'SELECT * FROM posts')
AS old_posts(id uuid, user_id uuid, title text, body text, created_at timestamptz);

COMMIT;

-- 3. Verify data integrity
SELECT
    (SELECT COUNT(*) FROM myapp_production.users) AS old_count,
    (SELECT COUNT(*) FROM myapp_production_new.users) AS new_count;

-- 4. Atomic cutover (requires brief downtime)
ALTER DATABASE myapp_production RENAME TO myapp_production_old_backup;
ALTER DATABASE myapp_production_new RENAME TO myapp_production;
```

#### **Strategy B: Foreign Data Wrapper (Zero Downtime)**

**Pros**: No downtime, incremental migration, easy rollback
**Cons**: Slightly more complex setup

```sql
-- 1. Build pristine schema in new database
CREATE DATABASE myapp_production_new;
-- Apply: db/generated/schema_production.sql

-- 2. Set up FDW connection to old database
CREATE EXTENSION IF NOT EXISTS postgres_fdw;

CREATE SERVER old_production_server
FOREIGN DATA WRAPPER postgres_fdw
OPTIONS (host 'localhost', dbname 'myapp_production', port '5432');

CREATE USER MAPPING FOR CURRENT_USER
SERVER old_production_server
OPTIONS (user 'myapp', password 'xxx');

-- Import foreign schema (read-only view of old database)
IMPORT FOREIGN SCHEMA public
LIMIT TO (users, posts, comments)
FROM SERVER old_production_server
INTO old_schema;

-- 3. Data migration with transformations
-- db/migrations/schema_to_schema/v1.5.3_to_v1.6.0.sql
BEGIN;

-- Migrate users with column mapping
INSERT INTO users (id, username, display_name, created_at)
SELECT
    id,
    username,
    full_name AS display_name,  -- Rename: full_name → display_name
    created_at
FROM old_schema.users;

-- Migrate posts (no transformation)
INSERT INTO posts
SELECT * FROM old_schema.posts;

-- Migrate comments with validation
INSERT INTO comments (id, post_id, user_id, content, created_at)
SELECT
    id,
    post_id,
    user_id,
    content,
    created_at
FROM old_schema.comments
WHERE post_id IN (SELECT id FROM posts);  -- Data validation

COMMIT;

-- 4. Verify counts
SELECT
    'users' AS table_name,
    (SELECT COUNT(*) FROM old_schema.users) AS old_count,
    (SELECT COUNT(*) FROM users) AS new_count;

-- 5. Zero-downtime cutover (DNS/connection pool switch)
-- Option A: Update connection strings (no database rename)
-- Option B: Use pg_bouncer database aliasing
-- Option C: Atomic database rename (brief lock)
```

### FraiseQL CLI Commands

```bash
# Generate schema-to-schema migration
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --strategy fdw

# Preview migration plan (dry-run)
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --strategy fdw \
    --dry-run

# Execute migration
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --strategy fdw \
    --execute

# Verify data integrity
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --verify

# Cutover (atomic swap)
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --cutover

# Rollback (if issues detected)
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --rollback
```

### Migration Script Structure

```
db/migrations/schema_to_schema/
├── v1.5.3_to_v1.6.0/
│   ├── 00_setup_fdw.sql              # FDW connection setup
│   ├── 01_migrate_users.sql          # User data migration
│   ├── 02_migrate_posts.sql          # Posts migration
│   ├── 03_migrate_comments.sql       # Comments migration
│   ├── 04_verify_counts.sql          # Data integrity checks
│   ├── 05_verify_constraints.sql     # Constraint validation
│   ├── config.yaml                   # Migration configuration
│   └── rollback.sql                  # Rollback procedure
└── v1.6.0_to_v1.7.0/
    └── ...
```

### Configuration File Example

```yaml
# db/migrations/schema_to_schema/v1.5.3_to_v1.6.0/config.yaml
migration:
  name: "Rename user full_name to display_name"
  from_version: "1.5.3"
  to_version: "1.6.0"
  strategy: "fdw"  # or "copy"

source_database:
  name: myapp_production
  host: localhost
  port: 5432

target_database:
  name: myapp_production_new
  host: localhost
  port: 5432

# FDW-specific settings
fdw:
  server_name: old_production_server
  foreign_schema_name: old_schema

# Table migration mappings
tables:
  users:
    # Column mappings (old_name: new_name)
    columns:
      full_name: display_name

    # Custom transformation SQL
    transform: |
      SELECT
        id,
        username,
        full_name AS display_name,
        COALESCE(email, username || '@legacy.local') AS email,
        created_at
      FROM old_schema.users

    # Data validation
    verify:
      - "COUNT(*) matches"
      - "PRIMARY KEY id has no duplicates"
      - "NOT NULL constraints satisfied"

  posts:
    # No transformations (direct copy)
    copy_all: true

  comments:
    # Filter during migration
    where: "created_at > '2020-01-01'"

# Verification steps
verification:
  - type: count_match
    tables: [users, posts, comments]

  - type: foreign_key_integrity
    tables: [posts, comments]

  - type: custom_sql
    sql: |
      SELECT COUNT(*) = 0 AS valid
      FROM users
      WHERE display_name IS NULL;

# Cutover strategy
cutover:
  method: "database_rename"  # or "dns_switch", "connection_pool"

  # Rollback procedure
  rollback:
    enabled: true
    keep_old_database: true
    duration: "7 days"
```

---

## CLI Commands

### Build Commands (schema/)

```bash
# Build schema from source files
fraiseql db build                    # Default environment (local)
fraiseql db build --env production   # Production schema only
fraiseql db build --all              # All environments

# Validate schema integrity
fraiseql db validate

# Show schema status
fraiseql db status
```

### Migration Commands (migrations/)

```bash
# Generate migration from schema diff
fraiseql db migrate generate --name "add_user_bio"

# Apply migrations
fraiseql db migrate up                 # Apply pending migrations
fraiseql db migrate up --target 005    # Migrate to specific version
fraiseql db migrate down               # Rollback one migration
fraiseql db migrate down --target 003  # Rollback to version

# Show migration status
fraiseql db migrate status
fraiseql db migrate history

# Create empty migration
fraiseql db migrate create --name "custom_data_fix"
```

### Sync Commands (data population)

```bash
# Sync from production
fraiseql db sync --from production
fraiseql db sync --from production --tables users,posts
fraiseql db sync --from production --exclude users.password

# Anonymize PII during sync
fraiseql db sync --from production --anonymize users.email,users.phone
```

---

## Version Tracking

### `.schema_version.json` (inspired by printoptim_backend)

```json
{
  "version": "2025.10.11.001",
  "hash": "a7f3d8e1c9b2...",
  "timestamp": "2025-10-11T14:30:00Z",
  "change_type": "minor",
  "migration_state": "003_rename_user_full_name",
  "environments": {
    "local": {
      "hash": "a7f3d8e1...",
      "file_count": 47,
      "last_build": "2025-10-11T14:25:00Z"
    },
    "production": {
      "hash": "a7f3d8e1...",
      "file_count": 35,
      "last_build": "2025-10-11T14:30:00Z"
    }
  }
}
```

### Migration State Tracking

```python
# Database table (created automatically)
CREATE TABLE fraiseql_migrations (
    id SERIAL PRIMARY KEY,
    version TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    applied_at TIMESTAMPTZ DEFAULT NOW(),
    rollback_sql TEXT,
    checksum TEXT NOT NULL,
    execution_time_ms INTEGER
);
```

---

## Environment Configuration

### `db/environments/production.yaml`

```yaml
environment: production
description: "Production server - schema only"

# Which schema directories to include
include:
  - schema/00_common
  - schema/10_tables
  - schema/20_views
  - schema/30_functions
  - schema/40_indexes
  - schema/50_permissions

# Which to exclude (no seed data in production)
exclude:
  - seeds/

# Connection (respects DATABASE_URL env var)
database:
  host: ${POSTGRES_HOST}
  port: ${POSTGRES_PORT:-5432}
  database: ${POSTGRES_DB}
  user: ${POSTGRES_USER}

# Migration behavior
migrations:
  auto_backup: true
  require_confirmation: true
  max_execution_time: 300  # seconds
```

### `db/environments/local.yaml`

```yaml
environment: local
description: "Local development with seed data"

include:
  - schema/
  - seeds/common/
  - seeds/dev/

exclude: []

database:
  host: localhost
  port: 5432
  database: myapp_local
  user: myapp

migrations:
  auto_backup: false
  require_confirmation: false
  max_execution_time: 60
```

---

## Implementation Phases (TDD Approach)

### Phase 1: Schema Builder (2 weeks)

**Objective**: Build `schema/` → `generated/schema_*.sql`

**RED Phase**:
```python
# tests/test_schema_builder.py
def test_build_local_schema():
    builder = SchemaBuilder(env="local")
    output = builder.build()
    assert output.exists()
    assert "CREATE TABLE users" in output.read_text()
```

**GREEN Phase**: Implement `builder.py` (minimal)

**REFACTOR Phase**: Optimize file concatenation, add hash tracking

**QA Phase**: Test with 100+ SQL files, verify deterministic output

---

### Phase 2: Migration System (3 weeks)

**Objective**: Create and apply migration files

**RED Phase**:
```python
def test_create_migration():
    migrator = Migrator()
    migration = migrator.create("add_user_bio")
    assert migration.exists()
    assert migration.name == "004_add_user_bio.py"
```

**GREEN Phase**: Implement migration creation and execution

**REFACTOR Phase**: Add rollback support, checksums, transaction handling

**QA Phase**: Test rollback scenarios, concurrent migrations

---

### Phase 3: Schema Diff Detection (2 weeks)

**Objective**: Auto-generate migrations from schema changes

**RED Phase**:
```python
def test_detect_column_rename():
    diff = SchemaDiff.from_schemas(old_schema, new_schema)
    assert diff.has_changes()
    assert "RENAME COLUMN" in diff.generate_migration()
```

**GREEN Phase**: Implement basic diff detection (tables, columns)

**REFACTOR Phase**: Advanced diff (indexes, constraints, functions)

**QA Phase**: Edge cases (type changes, multi-step migrations)

---

### Phase 4: Production Sync (2 weeks)

**Objective**: Populate fresh DB from production

**RED Phase**:
```python
def test_sync_from_production():
    syncer = ProductionSyncer(source="prod", target="local")
    syncer.sync(tables=["users"], anonymize=["email"])
    assert local_db.count("users") > 0
```

**GREEN Phase**: Basic data copy with schema awareness

**REFACTOR Phase**: Incremental sync, PII anonymization

**QA Phase**: Large datasets, schema mismatches

---

### Phase 5: Schema-to-Schema Migration (3 weeks)

**Objective**: Implement FDW/COPY-based migration for zero-downtime production migrations

**RED Phase**:
```python
def test_fdw_migration():
    migrator = SchemaToSchemaMigrator(
        source="production",
        target="production_new",
        strategy="fdw"
    )
    migrator.setup_fdw()
    migrator.migrate_data()
    assert migrator.verify_counts() == True
```

**GREEN Phase**: Implement FDW setup and data migration

**REFACTOR Phase**:
- Add column mapping support
- Implement verification checks
- Add rollback procedures
- Support incremental migration

**QA Phase**:
- Test with large datasets (1M+ rows)
- Verify zero-downtime cutover
- Test rollback scenarios
- Benchmark migration speed

---

### Phase 6: CLI Integration (1 week)

**Objective**: Expose all features via `fraiseql db` commands

**RED Phase**:
```python
def test_cli_build():
    result = runner.invoke(cli, ["db", "build", "--env", "local"])
    assert result.exit_code == 0
```

**GREEN Phase**: Wire commands to underlying implementations

**REFACTOR Phase**: Rich output, progress bars, error handling

**QA Phase**: User acceptance testing

---

## AI-Friendly Workflow

### Scenario: Rename a column

**Step 1**: Developer says: *"Rename users.full_name to users.display_name"*

**AI Actions**:
```bash
# 1. Update source DDL (Medium 1)
#    Edit: db/schema/10_tables/users.sql
#    Change: full_name -> display_name

# 2. Generate migration (Medium 2)
fraiseql db migrate generate --name "rename_user_full_name"
#    Auto-detects schema diff
#    Creates: db/migrations/003_rename_user_full_name.py
#    Contains: ALTER TABLE users RENAME COLUMN...

# 3. Apply migration to dev
fraiseql db migrate up

# 4. Developer reviews, commits both:
#    - db/schema/10_tables/users.sql (new state)
#    - db/migrations/003_rename_user_full_name.py (migration)
```

**Production Deploy**:
```bash
# Pulls latest code
git pull

# Applies migration (preserves data)
fraiseql db migrate up --env production

# Template is automatically updated for next fast deployment
```

**New Developer Onboarding**:
```bash
# Build fresh DB from source
fraiseql db build --env local

# Optionally sync production data
fraiseql db sync --from production --anonymize
```

---

## Key Design Decisions

### 1. **Build-from-Scratch First**
- `schema/` files are **always** the source of truth
- Migrations are derived, not primary
- New developers: build from `schema/`, not replay migrations

### 2. **Deterministic Builds**
- Numbered directories enforce order
- SHA256 hash of all files detects changes
- Parallel environment support (local vs production)

### 3. **Migration Safety**
- Automatic backups before applying
- Rollback support (down migrations)
- Checksum validation prevents tampering
- Transaction wrapping (all-or-nothing)

### 4. **Production-Ready**
- Template caching (printoptim_backend proven: 2-3s deploys)
- Schema validation before migration
- Dry-run mode
- Execution time tracking

### 5. **Developer Experience**
- Single command to rebuild: `fraiseql db build`
- Auto-generate migrations: `fraiseql db migrate generate`
- Rich CLI output (progress, errors)
- Documentation generated from DDL comments

---

## Success Metrics

**Technical**:
- ✅ Build 100+ SQL files in <1s
- ✅ Detect schema changes automatically
- ✅ Zero-downtime migrations (Blue/Green pattern)
- ✅ Rollback capability (down migrations)

**Developer Experience**:
- ✅ New dev onboarding: `fraiseql db build` (one command)
- ✅ Production deploy: `fraiseql db migrate up` (one command)
- ✅ AI-assisted migration generation (90% accuracy)

**Production**:
- ✅ Template caching reduces deploy time 20-30x
- ✅ Migration history tracked in database
- ✅ Automatic backups before changes
- ✅ Environment-specific builds (local, test, staging, prod)

---

## Comparison: Alembic vs FraiseQL Migration System

| Feature | Alembic | FraiseQL (Proposed) |
|---------|---------|---------------------|
| Source of truth | Migrations | `schema/` DDL files |
| Fresh DB setup | Replay all migrations | Build from `schema/` |
| Auto-detection | Limited (SQLAlchemy models) | Full SQL diff |
| Environments | Single alembic.ini | Multi-environment YAML |
| Production sync | Manual | Built-in `db sync` |
| Template caching | No | Yes (30x faster deploys) |

---

## Next Steps

1. **Review this design** with team/community
2. **Create GitHub issue** for Phase 1 (Schema Builder)
3. **Write detailed specs** for each phase
4. **Begin Phase 1 TDD cycles** (RED → GREEN → REFACTOR → QA)
5. **Update ROADMAP_V1.md** with detailed timeline

---

## Open Questions

1. **Migration file format**: Python (like Alembic) or pure SQL?
   - **Recommendation**: Python for flexibility (data migrations, conditional logic)

2. **Schema diff algorithm**: AST parsing or pg_dump comparison?
   - **Recommendation**: Hybrid (parse DDL + pg_dump for validation)

3. **Blue/Green deployments**: Built-in or separate tool?
   - **Recommendation**: Separate guide, leverage template caching

4. **Distributed systems**: Multi-database migration coordination?
   - **Recommendation**: v1.1 feature (use existing tools initially)

---

**Last Updated**: October 11, 2025
**Author**: Lionel Hamayon + Claude
**Status**: ✅ Ready for Phase 1 Implementation

---

## Complete Production Migration Example

### Scenario: Rename users.full_name → users.display_name

**Production Context**:
- 10M users in production
- 24/7 uptime requirement
- Zero-downtime mandatory

### Strategy Decision

**Option A: In-Place Migration** (Simple ALTER)
```bash
# For low-traffic apps or acceptable brief lock
fraiseql db migrate generate --name "rename_user_full_name"
fraiseql db migrate up --env production
# Downtime: 5-30 seconds (table lock during ALTER)
```

**Option B: Schema-to-Schema** (Zero Downtime) ✅
```bash
# For high-traffic production systems
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --strategy fdw \
    --execute
# Downtime: 0 seconds (atomic cutover)
```

### Step-by-Step: Schema-to-Schema Approach

#### **1. Update Source Schema** (local development)

```bash
# Edit db/schema/10_tables/users.sql
# Change: full_name TEXT → display_name TEXT

# Commit changes
git add db/schema/10_tables/users.sql
git commit -m "Rename users.full_name to display_name"
git push
```

#### **2. Generate Schema-to-Schema Migration**

```bash
# On production server
cd /srv/myapp
git pull

# Build new pristine schema
fraiseql db build --env production --output /tmp/schema_new.sql

# Generate migration plan
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --strategy fdw \
    --generate

# Creates: db/migrations/schema_to_schema/v1.5.3_to_v1.6.0/
```

#### **3. Review Generated Migration**

```yaml
# db/migrations/schema_to_schema/v1.5.3_to_v1.6.0/config.yaml
migration:
  name: "Rename user full_name to display_name"
  from_version: "1.5.3"
  to_version: "1.6.0"
  strategy: "fdw"

tables:
  users:
    columns:
      full_name: display_name  # Auto-detected column mapping

    transform: |
      INSERT INTO users (id, username, display_name, created_at)
      SELECT
        id,
        username,
        full_name AS display_name,
        created_at
      FROM old_schema.users

    verify:
      - "COUNT(*) matches"
      - "PRIMARY KEY id has no duplicates"
```

#### **4. Dry-Run Verification**

```bash
# Test migration plan (no changes)
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --strategy fdw \
    --dry-run

# Output:
# ✅ Will create database: myapp_production_new
# ✅ Will setup FDW connection to myapp_production
# ✅ Will migrate 10,000,000 users
# ✅ Will migrate 50,000,000 posts
# ✅ Will migrate 200,000,000 comments
# ⏱️  Estimated time: 15-20 minutes
# 📊 Estimated disk space: 120GB
```

#### **5. Execute Migration**

```bash
# Create new database + migrate data
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --strategy fdw \
    --execute

# Output (real-time progress):
# [14:30:00] Creating database myapp_production_new...
# [14:30:05] Building schema from db/schema/...
# [14:30:10] Setting up FDW connection...
# [14:30:15] Migrating users... (0/10M)
# [14:35:20] Migrating users... (10M/10M) ✅
# [14:35:25] Migrating posts... (0/50M)
# [14:48:10] Migrating posts... (50M/50M) ✅
# [14:48:15] Migrating comments... (0/200M)
# [15:10:30] Migrating comments... (200M/200M) ✅
# [15:10:35] Verifying data integrity...
# [15:11:00] ✅ All verification checks passed
```

#### **6. Verification**

```bash
# Automated verification (already done during migration)
# Manual spot checks:
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --verify

# Output:
# ✅ Table counts match:
#    users: 10,000,000 (old) = 10,000,000 (new)
#    posts: 50,000,000 (old) = 50,000,000 (new)
#    comments: 200,000,000 (old) = 200,000,000 (new)
#
# ✅ Foreign key integrity verified
# ✅ Custom validation passed:
#    - No NULL display_name values
#    - All user IDs preserved
```

#### **7. Cutover (Zero Downtime)**

```bash
# Update connection pool to point to new database
# Option A: pg_bouncer database alias switch
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --cutover \
    --method pgbouncer

# Option B: Database rename (5 second lock)
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_new \
    --cutover \
    --method database_rename

# Output:
# [15:15:00] Pausing connection pool...
# [15:15:01] Renaming databases:
#            myapp_production → myapp_production_old_backup
#            myapp_production_new → myapp_production
# [15:15:02] Resuming connection pool...
# [15:15:03] ✅ Cutover complete (3 seconds downtime)
```

#### **8. Monitoring + Rollback Plan**

```bash
# Monitor new database
watch -n 1 'psql -c "SELECT COUNT(*) FROM pg_stat_activity WHERE datname = '\''myapp_production'\''"'

# If issues detected (within 7 days):
fraiseql db migrate schema-to-schema \
    --from production \
    --to production_old_backup \
    --rollback

# Output:
# [15:20:00] Rolling back to myapp_production_old_backup...
# [15:20:01] Renaming databases:
#            myapp_production → myapp_production_failed
#            myapp_production_old_backup → myapp_production
# [15:20:02] ✅ Rollback complete
```

### Timeline Summary

| Phase | Duration | Downtime | Notes |
|-------|----------|----------|-------|
| Schema update (dev) | 5 min | N/A | Edit DDL, commit |
| Generate migration | 2 min | N/A | Auto-detect changes |
| Dry-run verification | 1 min | N/A | Validate plan |
| Execute migration | 40 min | 0 | Background copy via FDW |
| Verification | 1 min | 0 | Automated checks |
| Cutover | 3 sec | 3 sec | Atomic database rename |
| **TOTAL** | **49 min** | **3 sec** | vs 30 sec ALTER lock |

---

## Timeline Summary

| Phase | Duration | Key Deliverables | Target Date |
|-------|----------|------------------|-------------|
| **Phase 1: Schema Builder** | 2 weeks | Build from schema/, hash tracking | Oct 25, 2025 |
| **Phase 2: In-Place Migrations** | 3 weeks | ALTER migrations, rollback | Nov 15, 2025 |
| **Phase 3: Schema Diff** | 2 weeks | Auto-detect changes, generate migrations | Nov 29, 2025 |
| **Phase 4: Production Sync** | 2 weeks | Data copy, anonymization | Dec 13, 2025 |
| **Phase 5: Schema-to-Schema** | 3 weeks | FDW/COPY, zero-downtime | Jan 3, 2026 |
| **Phase 6: CLI Integration** | 1 week | fraiseql db commands | Jan 10, 2026 |

**Total Estimated Time**: 13 weeks (~3 months)

**Target Release**: **January 10, 2026** (integrated with FraiseQL v1.0)

---

**Last Updated**: October 11, 2025
**Author**: Lionel Hamayon + Claude
**Status**: ✅ Ready for Phase 1 Implementation

---

**Let's build the best migration system for GraphQL-first PostgreSQL apps.** 🚀
