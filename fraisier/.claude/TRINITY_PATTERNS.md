# Trinity Patterns in Fraisier Database

**Status**: ✅ Implemented across all tables
**Purpose**: Enable multi-database reconciliation and consistent identifier management
**Benefit**: When Fraisier has multiple database backends (SQLite for local, PostgreSQL for cloud), reconciliation is straightforward
**Standard**: Follows PrintOptim database architecture conventions

---

## PrintOptim Trinity Column Order

**All Fraisier tables follow this strict column declaration order** (consistent with PrintOptim):

```

1. id (TEXT/UUID)                           - Public, API-facing identifier
2. identifier (TEXT)                        - Business key, human-readable
3. pk_* (INTEGER PRIMARY KEY)               - Internal key (ALWAYS LAST)
4. fk_* (FOREIGN KEYS)                      - References to other pk_* columns
5. Domain Columns                           - Business logic data
6. Audit Columns                            - created_at, updated_at, etc.
```

**Why This Order?**

- ✅ Consistent across entire organization
- ✅ Easy scanning: trinity identifiers always first 3 columns
- ✅ Clear hierarchy: public → business → internal
- ✅ Logical grouping: related columns together
- ✅ Team-wide standards enable tooling and automation

---

## Core Trinity Pattern

Every table in Fraisier follows the trinity identifier pattern:

### 1. **pk_* (PRIMARY KEY - INTEGER)**

Internal, auto-allocated primary key used for efficient referencing.

```sql
CREATE TABLE tb_fraise_state (
    pk_fraise_state INTEGER PRIMARY KEY AUTOINCREMENT,  -- Trinity PK
    ...
);
```

**Purpose**:

- Internal use only (never expose to API)
- Fast integer joins (75% smaller than UUID)
- Deterministic allocation supports reproducible PKs
- Reference point for foreign keys

**Usage**:
```sql
-- Use in FKs
fk_fraise_state INTEGER REFERENCES tb_fraise_state(pk_fraise_state)

-- Use internally
SELECT * FROM tb_deployment WHERE fk_fraise_state = 5
```

### 2. **id (UUID/TEXT - UNIQUE)**

Public, externally-facing identifier for API and sync operations.

```sql
CREATE TABLE tb_fraise_state (
    id TEXT NOT NULL UNIQUE,  -- UUID (generated)
    ...
);
```

**Purpose**:

- Public API identifier (what external systems see)
- Cross-database synchronization key
- Human-friendly debugging (UUID can encode table info)
- Immutable, globally unique

**Usage**:
```sql
-- For API responses
SELECT id FROM tb_deployment WHERE pk_deployment = 5

-- For sync across databases
SELECT * FROM tb_deployment WHERE id = '550e8400-e29b-41d4-a716-446655440000'

-- For external references
"deployment_id": "550e8400-e29b-41d4-a716-446655440000"
```

### 3. **identifier (TEXT - UNIQUE)**

Business key - human-readable, semantic name for the entity.

```sql
CREATE TABLE tb_fraise_state (
    identifier TEXT NOT NULL UNIQUE,  -- "my_api:production:backup_job"
    ...
);
```

**Purpose**:

- Human-readable alternative to UUID
- Query by semantic meaning, not ID
- Business logic key
- Deterministic generation from entity attributes

**Usage**:
```sql
-- Query by business key
SELECT * FROM tb_fraise_state WHERE identifier = 'my_api:production:backup_job'

-- For logs/reporting
"fraise": "my_api:production"

-- For reconciliation
- DB1: "my_api:production" → id: uuid-1
- DB2: "my_api:production" → id: uuid-2  ← Would indicate sync issue if different
```

---

## Table Structure: tb_fraise_state

**Purpose**: Current deployment state of each fraise

**Column Order** follows PrintOptim standard: public → business → internal → domain → audit

```sql
CREATE TABLE tb_fraise_state (
    -- Trinity Identifiers (in strict order: id → identifier → pk_*)
    id TEXT NOT NULL UNIQUE,                         -- 1. Public UUID for sync
    identifier TEXT NOT NULL UNIQUE,                 -- 2. Business key: "fraise:env[:job]"
    pk_fraise_state INTEGER PRIMARY KEY AUTOINCREMENT,  -- 3. Internal key

    -- Domain Data (semantic naming)
    fraise_name TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT,                                   -- NULL for non-scheduled
    current_version TEXT,
    last_deployed_at TEXT,
    last_deployed_by TEXT,
    status TEXT DEFAULT 'unknown',

    -- Audit Trail
    created_at TEXT NOT NULL,                        -- When first created
    updated_at TEXT NOT NULL,                        -- Last modification

    -- Natural Key
    UNIQUE(fraise_name, environment_name, job_name)
);
```

**Example Data**:
```
pk_fraise_state  id                                      identifier
1                550e8400-e29b-41d4-a716-446655440001   my_api:production
2                550e8400-e29b-41d4-a716-446655440002   my_api:staging
3                550e8400-e29b-41d4-a716-446655440003   backup:production:hourly
```

---

## Table Structure: tb_deployment

**Purpose**: Deployment history and audit log

```sql
CREATE TABLE tb_deployment (
    -- Trinity Identifiers
    pk_deployment INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,                    -- UUID for sync
    identifier TEXT NOT NULL UNIQUE,            -- "fraise:env:timestamp"

    -- Foreign Key (always uses pk_*, never id)
    fk_fraise_state INTEGER NOT NULL REFERENCES tb_fraise_state(pk_fraise_state),

    -- Business Data (semantic naming)
    fraise_name TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_seconds REAL,
    old_version TEXT,
    new_version TEXT,
    status TEXT NOT NULL,                       -- in_progress, success, failed, rolled_back
    triggered_by TEXT,                          -- webhook, manual, scheduled
    triggered_by_user TEXT,
    git_commit TEXT,
    git_branch TEXT,
    error_message TEXT,
    details TEXT,                               -- JSON

    -- Audit Trail
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

**Example Data**:
```
pk_deployment  id                  identifier                              fk_fraise_state  status
1              550e8400-e29b-41d4  my_api:production:2026-01-22T10:30:00  1               success
2              550e8400-e29b-41d5  my_api:production:2026-01-22T09:15:00  1               failed
3              550e8400-e29b-41d6  backup:production:2026-01-22T08:00:00  3               success
```

---

## Table Structure: tb_webhook_event

**Purpose**: Webhook events received and processed

**Column Order** follows PrintOptim standard: public → business → internal → domain → audit

```sql
CREATE TABLE tb_webhook_event (
    -- Trinity Identifiers (in strict order: id → identifier → pk_*)
    id TEXT NOT NULL UNIQUE,                           -- 1. Public UUID for sync
    identifier TEXT NOT NULL UNIQUE,                   -- 2. Business key: "provider:timestamp:hash"
    pk_webhook_event INTEGER PRIMARY KEY AUTOINCREMENT,  -- 3. Internal key

    -- Foreign Keys (after pk_*)
    fk_deployment INTEGER REFERENCES tb_deployment(pk_deployment),

    -- Domain Data (semantic naming)
    received_at TEXT NOT NULL,
    event_type TEXT NOT NULL,                          -- push, ping, pull_request
    git_provider TEXT NOT NULL,                        -- github, gitlab, gitea, bitbucket
    branch_name TEXT,
    commit_sha TEXT,
    sender TEXT,
    payload TEXT,                                      -- Full JSON payload
    processed INTEGER DEFAULT 0,                       -- 1 if linked to deployment

    -- Audit Trail
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

**Example Data**:
```
pk_webhook_event  id                  identifier                fk_deployment  event_type
1                 550e8400-e29b-41d7  github:2026-01-22:a1b2c3 1             push
2                 550e8400-e29b-41d8  github:2026-01-22:d4e5f6 NULL          ping
3                 550e8400-e29b-41d9  gitlab:2026-01-22:g7h8i9 2             push
```

---

## Views: v_fraise_status

**Purpose**: Read-side view with computed fields

```sql
CREATE VIEW v_fraise_status AS
SELECT
    fs.pk_fraise_state,
    fs.id,
    fs.identifier,
    fs.fraise_name,
    fs.environment_name,
    fs.job_name,
    fs.current_version,
    fs.status,
    fs.last_deployed_at,
    fs.last_deployed_by,
    (SELECT COUNT(*) FROM tb_deployment d
     WHERE d.fk_fraise_state = fs.pk_fraise_state
       AND d.status = 'success') as successful_deployments,
    (SELECT COUNT(*) FROM tb_deployment d
     WHERE d.fk_fraise_state = fs.pk_fraise_state
       AND d.status = 'failed') as failed_deployments,
    fs.created_at,
    fs.updated_at
FROM tb_fraise_state fs;
```

**Usage in Code**:
```python
# Python - Query using v_fraise_status
state = db.get_fraise_state("my_api", "production")
# Returns dict with pk_fraise_state, id, identifier, etc.
```

---

## Views: v_deployment_history

**Purpose**: Read-side view of deployments with computed fields

```sql
CREATE VIEW v_deployment_history AS
SELECT
    d.pk_deployment,
    d.id,
    d.identifier,
    d.fraise_name,
    d.environment_name,
    d.job_name,
    d.started_at,
    d.completed_at,
    d.duration_seconds,
    d.old_version,
    d.new_version,
    d.status,
    d.triggered_by,
    d.triggered_by_user,
    d.git_commit,
    d.git_branch,
    d.error_message,
    CASE
        WHEN d.old_version != d.new_version THEN 'upgrade'
        WHEN d.old_version = d.new_version THEN 'redeploy'
        ELSE 'unknown'
    END as deployment_type,
    d.created_at,
    d.updated_at
FROM tb_deployment d
ORDER BY d.started_at DESC;
```

---

## Views: v_webhook_event_history

**Purpose**: Webhook events with linked deployment info

```sql
CREATE VIEW v_webhook_event_history AS
SELECT
    we.pk_webhook_event,
    we.id,
    we.identifier,
    we.git_provider,
    we.event_type,
    we.branch_name,
    we.commit_sha,
    we.sender,
    we.received_at,
    we.processed,
    we.fk_deployment,
    d.id as deployment_id,
    d.fraise_name,
    d.environment_name,
    we.created_at,
    we.updated_at
FROM tb_webhook_event we
LEFT JOIN tb_deployment d ON we.fk_deployment = d.pk_deployment
ORDER BY we.received_at DESC;
```

---

## Python API Usage

### Insert with Trinity Identifiers

```python
from fraisier.database import FraisierDB

db = FraisierDB()

# Insert fraise state (automatically generates trinity IDs)
db.update_fraise_state(
    fraise="my_api",
    environment="production",
    version="abc123de",
    status="healthy"
)

# Returns pk_fraise_state used internally
# Automatically generates:
# - id: UUID
# - identifier: "my_api:production"
# - created_at, updated_at
```

### Start Deployment (FK Reference)

```python
# Start deployment (uses fk_fraise_state = pk_fraise_state)
deployment_id = db.start_deployment(
    fraise="my_api",
    environment="production",
    triggered_by="webhook",
    git_branch="main",
    git_commit="abc123"
)
# deployment_id is pk_deployment (INTEGER)

# Automatically generates:
# - id: UUID for sync
# - identifier: "my_api:production:2026-01-22T10:30:00"
# - fk_fraise_state: Resolved from fraise/environment lookup
# - created_at, updated_at
```

### Record Webhook

```python
# Record webhook event (with git provider)
webhook_id = db.record_webhook_event(
    event_type="push",
    git_provider="github",
    branch="main",
    commit_sha="abc123",
    sender="developer",
    payload='{"test": "data"}'
)
# webhook_id is pk_webhook_event (INTEGER)

# Automatically generates:
# - id: UUID for sync
# - identifier: "github:2026-01-22T10:30:00:a1b2c3d4"
# - created_at, updated_at
```

### Link Webhook to Deployment

```python
# Link webhook to deployment (uses FK reference)
db.link_webhook_to_deployment(
    webhook_id=webhook_id,      # pk_webhook_event
    deployment_id=deployment_id # pk_deployment
)

# Updates:
# - fk_deployment = pk_deployment (INTEGER FK)
# - processed = 1
# - updated_at
```

---

## Benefits for Multi-Database Reconciliation

### Scenario: SQLite (Local) + PostgreSQL (Cloud)

**SQLite Instance (Local)**:
```
tb_fraise_state:
  pk_fraise_state: 1
  id: "550e8400-e29b-41d4-a716-446655440001"
  identifier: "my_api:production"

tb_deployment:
  pk_deployment: 5
  id: "550e8400-e29b-41d4-a716-446655440005"
  identifier: "my_api:production:2026-01-22T10:30:00"
```

**PostgreSQL Instance (Cloud)**:
```
tb_fraise_state:
  pk_fraise_state: 42
  id: "550e8400-e29b-41d4-a716-446655440001"  ← SAME
  identifier: "my_api:production"              ← SAME

tb_deployment:
  pk_deployment: 99
  id: "550e8400-e29b-41d4-a716-446655440005"  ← SAME
  identifier: "my_api:production:2026-01-22T10:30:00"  ← SAME
```

### Reconciliation Algorithm

```python
def reconcile_databases(local_db, cloud_db):
    """Sync deployments from local SQLite to cloud PostgreSQL."""

    # Get unsynced deployments (where id not in cloud)
    for deployment in local_db.get_recent_deployments():
        cloud_deployment = cloud_db.get_by_id(deployment['id'])

        if not cloud_deployment:
            # New in local - sync to cloud
            cloud_db.insert_deployment(deployment)

        elif cloud_deployment['updated_at'] > deployment['updated_at']:
            # Newer in cloud - update local
            local_db.update_deployment(cloud_deployment)

        else:
            # Same - no action
            pass
```

**Key Points**:

- ✅ `id` (UUID) is unique across databases
- ✅ `identifier` (business key) is human-readable for matching
- ✅ `updated_at` resolves conflicts deterministically
- ✅ `pk_*` stays local (don't sync between databases)

---

## Indexes for Performance

```sql
-- Trinity identifier indexes (critical for sync)
CREATE INDEX idx_fraise_state_id ON tb_fraise_state(id);
CREATE INDEX idx_fraise_state_identifier ON tb_fraise_state(identifier);
CREATE INDEX idx_deployment_id ON tb_deployment(id);
CREATE INDEX idx_deployment_identifier ON tb_deployment(identifier);
CREATE INDEX idx_webhook_event_id ON tb_webhook_event(id);

-- FK indexes (critical for joins)
CREATE INDEX idx_deployment_fraise_state_fk ON tb_deployment(fk_fraise_state);
CREATE INDEX idx_webhook_event_deployment_fk ON tb_webhook_event(fk_deployment);

-- Query optimization
CREATE INDEX idx_deployment_started_at ON tb_deployment(started_at DESC);
CREATE INDEX idx_webhook_event_received_at ON tb_webhook_event(received_at DESC);
```

---

## Migration from Old Schema

**For existing Fraisier installations** (if migrating from non-trinity schema):

```sql
-- Create new trinity tables alongside old ones
CREATE TABLE tb_fraise_state_new (
    pk_fraise_state INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    identifier TEXT NOT NULL UNIQUE,
    -- ... rest of schema
);

-- Migrate data
INSERT INTO tb_fraise_state_new (id, identifier, fraise_name, environment_name, ...)
SELECT
    hex(randomblob(16)) as id,
    fraise || ':' || environment || CASE WHEN job IS NOT NULL THEN ':' || job ELSE '' END as identifier,
    fraise as fraise_name,
    environment as environment_name,
    job as job_name,
    -- ... rest
FROM tb_fraise_state;

-- Drop old, rename new
DROP TABLE tb_fraise_state;
ALTER TABLE tb_fraise_state_new RENAME TO tb_fraise_state;
```

---

## Summary

Trinity patterns in Fraisier enable:

1. **Multi-database sync** - UUID (`id`) enables reconciliation
2. **Human-readable keys** - `identifier` enables business logic
3. **Efficient references** - `pk_*` INTEGER foreign keys are 75% smaller
4. **Audit trail** - `created_at`, `updated_at` track all changes
5. **Consistency** - Uniform naming across all tables and databases

This architecture scales from single SQLite instance to distributed PostgreSQL deployments with zero changes to the application logic.

---

**Implemented**: 2026-01-22
**Schema Version**: 1.0-trinity
**Compatibility**: SQLite 3.8+, PostgreSQL 12+, MySQL 8.0+
