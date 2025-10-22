# Plan: Documentation Alignment with Exclusive Rust Pipeline

**Context**: FraiseQL has transitioned to an exclusive Rust-first pipeline architecture. Documentation written for the old multi-mode execution system (NORMAL, PASSTHROUGH, TURBO, JSON_PASSTHROUGH) needs to be updated or removed.

**Agent Task**: Review all documentation files and either UPDATE (align with Rust pipeline), REMOVE (deprecated content), or CREATE (new content needed).

---

## 📊 Architecture Change Summary

### OLD Architecture (Multi-Mode Execution)
```
┌──────────────┐
│  PostgreSQL  │
└──────┬───────┘
       │
  ┌────▼─────────────────────────────┐
  │   Mode Detection & Selection     │
  │  - NORMAL (Python transform)     │
  │  - PASSTHROUGH (JSON direct)     │
  │  - TURBO (TurboRouter)           │
  │  - JSON_PASSTHROUGH (Rust)       │
  └────┬─────────────────────────────┘
       │
  ┌────▼──────────────────────┐
  │   Conditional Logic       │
  │   - Python fallbacks      │
  │   - Rust optimization     │
  │   - Mode switching        │
  └────┬──────────────────────┘
       │
  ┌────▼────────┐
  │   GraphQL   │
  └─────────────┘
```

### NEW Architecture (Exclusive Rust Pipeline)
```
┌──────────────┐
│  PostgreSQL  │  ← JSONB queries
└──────┬───────┘
       │
  ┌────▼─────────────────────────────┐
  │  Rust Pipeline (fraiseql-rs)     │
  │  - build_graphql_response()      │
  │  - snake_case → camelCase        │
  │  - __typename injection          │
  │  - Field projection              │
  │  - UTF-8 bytes output            │
  └────┬─────────────────────────────┘
       │
  ┌────▼────────┐
  │   GraphQL   │  ← RustResponseBytes → HTTP
  └─────────────┘
```

**Key Changes**:
- ✅ **Single execution path**: All queries go through Rust pipeline
- ❌ **No mode detection**: No ExecutionMode enum, no conditional logic
- ❌ **No Python transformation**: All transformation in Rust
- ❌ **No passthrough detection**: Everything is "passthrough" now (to Rust)
- ✅ **Simplified API**: One function `build_graphql_response()` does everything

---

## 🎯 Documentation Categories

### 1. DELETE - Deprecated Content (13 files)
Files describing old architecture that no longer exists

### 2. UPDATE - Needs Rust Pipeline Alignment (35 files)
Files with correct concepts but wrong implementation details

### 3. KEEP - Already Accurate (30+ files)
Files that don't mention execution modes or are still accurate

### 4. CREATE - Missing Content (5 files)
New documentation needed for Rust-first architecture

---

## 📋 Detailed File-by-File Plan

## Category 1: DELETE - Deprecated Content

### 1.1 Rust Development/Planning Docs (DELETE 6 files)

These were planning documents for the Rust pipeline transition. Now that it's complete, they're historical artifacts.

```bash
# DELETE these files:
docs/rust/RUST_FIRST_SIMPLIFICATION.md         # Planning doc for transition
docs/rust/RUST_INTERFACE_FIX_NEEDED.md        # Old issues, now resolved
docs/rust/RUST_CLEANUP_SUMMARY.md             # Cleanup notes from transition
docs/rust/RUST_FIRST_CACHING_STRATEGY.md      # Old caching approach, superseded
docs/development/CLEANUP_AND_CLARIFICATION_PLAN.md  # Old cleanup tasks
docs/development/DOCUMENTATION_CLEANUP_GUIDE.md     # Meta-doc, no longer needed
```

**Reason**: These docs describe the *process* of moving to Rust pipeline. Users don't need this history - they need current architecture docs.

---

### 1.2 Theoretical/Vision Docs (DELETE 4 files)

Documents about future architectures that are either implemented or superseded.

```bash
# DELETE these files:
docs/strategic/THEORETICAL_OPTIMAL_ARCHITECTURE.md  # Was planning, now reality
docs/strategic/PATTERNS_TO_IMPLEMENT.md             # Patterns now implemented
docs/strategic/UPDATE_PERFORMANCE_CLAIMS.md         # Task list, completed
docs/strategic/FRAISEQL_POST_MIGRATION_ROADMAP.md   # Post-migration tasks done
```

**Reason**: These were roadmap/planning documents. The Rust pipeline is now production reality, not a future plan.

---

### 1.3 Old Architecture Decision Records (DELETE 3 files)

ADRs for decisions about multi-mode execution, now obsolete.

```bash
# DELETE these files:
docs/architecture/decisions/001_graphql_mutation_response_initial_plan.md  # Old mutation approach
docs/architecture/decisions/003_dual_path_cdc_pattern.md                   # Dual-path superseded
docs/architecture/decisions/004_dual_path_implementation_examples.md        # Examples for dual-path
```

**Reason**: These ADRs describe dual-path architecture (Python + Rust). Now we have single-path (Rust only).

**KEEP**:
- `002_ultra_direct_mutation_path.md` - Still relevant
- `005_simplified_single_source_cdc.md` - Current approach
- `003_unified_audit_table.md` - Still accurate

---

## Category 2: UPDATE - Needs Alignment

### 2.1 Core Documentation (UPDATE 8 files)

#### File: `docs/core/configuration.md`
**Issues**:
- Mentions `ExecutionMode` configuration
- References `execution_mode_priority` config option
- Describes Python fallback settings

**Updates Needed**:
```markdown
# DELETE these sections:
- "Execution Mode Priority" section
- "Python Transformation Fallback" section
- Any `execution_mode` config examples

# UPDATE these sections:
## Rust Pipeline Configuration

FraiseQL uses an exclusive Rust pipeline for all query execution.

### Configuration Options:
- `rust_transformation`: Always enabled (cannot be disabled)
- `field_projection`: Enable Rust-based field filtering (default: true)
- `schema_registry`: Enable schema-based transformation (default: true)

### Example:
```python
config = FraiseQLConfig(
    # Rust pipeline is always active
    field_projection=True,  # Optional: disable for debugging
)
```
```

---

#### File: `docs/core/fraiseql-philosophy.md`
**Issues**:
- May reference "smart execution mode detection"
- Describes conditional Python/Rust paths

**Updates Needed**:
```markdown
# UPDATE section: "Performance Philosophy"
OLD: "FraiseQL intelligently selects execution modes..."
NEW: "FraiseQL uses a single Rust pipeline for all queries..."

# ADD section: "Rust-First Philosophy"
FraiseQL delegates all post-database operations to Rust:
- PostgreSQL returns JSONB
- Rust transforms snake_case → camelCase
- Rust injects __typename
- Rust outputs UTF-8 bytes
- Zero Python string operations
```

---

#### File: `docs/core/queries-and-mutations.md`
**Issues**:
- Examples may show execution mode selection
- May reference Python transformation

**Updates Needed**:
```markdown
# UPDATE all query examples to show Rust pipeline:

## Query Execution

All queries execute through the Rust pipeline automatically:

```python
@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    # Returns RustResponseBytes (already camelCase, with __typename)
    return await repo.find("v_user")
```

Response format (automatically generated by Rust):
```json
{
  "data": {
    "users": [
      {
        "__typename": "User",
        "id": "123",
        "firstName": "John",  // ← Rust converts snake_case
        "createdAt": "2025-01-01T00:00:00Z"
      }
    ]
  }
}
```
```

---

#### File: `docs/core/database-api.md`
**Issues**:
- Repository methods may show execution mode parameters
- May reference `find_raw_json()` vs `find()` distinction

**Updates Needed**:
```markdown
# UPDATE FraiseQLRepository API docs:

## Repository Methods

### find(source, where=None, **kwargs)
Executes query and returns RustResponseBytes with GraphQL response.

**Always uses Rust pipeline** - no mode selection needed.

```python
# All these return RustResponseBytes:
users = await repo.find("v_user")
user = await repo.find_one("v_user", id=123)
filtered = await repo.find("v_user", where={"age": {"gt": 18}})
```

# DELETE these sections:
- "Execution Mode Selection"
- "find_raw_json() for passthrough mode"
- Any `mode="..."` parameter documentation
```

---

#### File: `docs/reference/config.md`
**Issues**:
- Complete config reference including deprecated options

**Updates Needed**:
```markdown
# DELETE these config options from documentation:
- execution_mode_priority
- enable_python_fallback
- passthrough_detection_enabled
- json_passthrough_mode
- intelligent_mode_switching

# UPDATE to show only Rust pipeline configs:
## Configuration Reference

### Rust Pipeline (Always Active)
- `field_projection: bool = True` - Enable Rust field filtering
- `schema_registry: bool = True` - Enable schema-based transformation

### APQ (Automatic Persisted Queries)
- `apq_enabled: bool = True`
- `apq_storage_backend: str = "memory"` # or "postgresql"

### Database
- `database_url: str` - PostgreSQL connection string
- `pool_size: int = 10`

# Add migration note:
## Migrating from v0.11.5
If you used execution mode configuration in v0.11.5, remove these settings:
- `execution_mode_priority` → No longer used (Rust pipeline always active)
- `enable_python_fallback` → No longer needed (no fallback)
```

---

#### File: `docs/reference/database.md`
**Issues**:
- Repository API reference with mode parameters

**Updates Needed**:
```markdown
# UPDATE method signatures:

## FraiseQLRepository

### find(source, where=None, **kwargs)
Execute query and return RustResponseBytes.

**Parameters:**
- `source: str` - View name (e.g., "v_user")
- `where: dict` - WHERE clause filters (optional)
- `**kwargs` - Additional filters

**Returns:** `RustResponseBytes` - Pre-serialized GraphQL response

**Example:**
```python
result = await repo.find("v_user", where={"active": {"eq": True}})
# result is RustResponseBytes, sent directly to HTTP response
```

# DELETE:
- Any `mode` parameter documentation
- Any `execution_mode` examples
```

---

#### File: `docs/production/deployment.md`
**Issues**:
- May recommend execution mode tuning for production

**Updates Needed**:
```markdown
# DELETE sections:
- "Execution Mode Optimization"
- "Mode Priority Configuration for Production"
- "When to use NORMAL vs PASSTHROUGH"

# ADD section:
## Rust Pipeline in Production

FraiseQL uses an exclusive Rust pipeline in all environments.

### Performance Characteristics:
- **0.5-5ms** response time (PostgreSQL → HTTP)
- **Zero Python overhead** for JSON transformation
- **Automatic camelCase** conversion
- **Built-in __typename** injection

### No Configuration Needed:
The Rust pipeline is always active and optimized. There are no mode switches or detection logic to configure.

### Monitoring:
Monitor Rust pipeline performance via:
```python
from fraiseql.monitoring import get_metrics

metrics = get_metrics()
print(f"Rust transform time: {metrics['rust_transform_avg_ms']}ms")
```
```

---

#### File: `docs/production/monitoring.md`
**Issues**:
- Metrics for different execution modes

**Updates Needed**:
```markdown
# UPDATE metrics section:

## Available Metrics

### Rust Pipeline Metrics:
- `rust_transform_count` - Total transformations
- `rust_transform_avg_ms` - Average transform time
- `rust_transform_p95_ms` - 95th percentile
- `rust_bytes_processed` - Total bytes transformed

# DELETE:
- `execution_mode_distribution` metric
- `normal_mode_count`, `passthrough_mode_count` metrics
- Any mode-switching metrics
```

---

### 2.2 Performance Documentation (UPDATE 6 files)

#### File: `docs/performance/PERFORMANCE_GUIDE.md`
**Issues**:
- Performance comparison tables showing different modes
- Optimization strategies for mode selection

**Updates Needed**:
```markdown
# REWRITE performance tables:

## FraiseQL Performance (Rust Pipeline)

| Operation | Response Time | Notes |
|-----------|---------------|-------|
| Simple Query | 0.5-2ms | PostgreSQL → Rust → HTTP |
| Complex Query | 2-10ms | Includes nested relations |
| With Field Projection | 1-5ms | Rust filters fields |

### OLD COMPARISON TABLE (DELETE):
| Mode | Time | Use When |
| NORMAL | 25-100ms | Python processing needed |
| PASSTHROUGH | 1-10ms | No custom logic |
| TURBO | 0.5-5ms | Cached queries |

### NEW SECTION:
## Performance Optimization

All queries use the Rust pipeline automatically. To optimize:

1. **Database Query** - Fastest improvement area
   - Use proper indexes
   - Optimize JSONB queries
   - Use materialized views (tv_*)

2. **Field Projection** - Let Rust filter fields
   ```graphql
   query {
     users {
       id          # Only these fields
       firstName   # are extracted by Rust
     }
   }
   ```

3. **APQ** - Enable Automatic Persisted Queries
   - 85-95% cache hit rate
   - Eliminates query parsing overhead
```

---

#### File: `docs/performance/index.md`
**Issues**:
- Performance overview mentioning mode optimization

**Updates Needed**:
```markdown
# UPDATE performance overview:

## How FraiseQL Achieves High Performance

### 1. Rust-First Pipeline
**PostgreSQL → Rust → HTTP** with zero Python overhead.

All string operations (camelCase conversion, __typename injection, JSON wrapping) happen in Rust at native speed.

### 2. JSONB in PostgreSQL
Store pre-computed GraphQL responses as JSONB in transform tables (tv_*).

### 3. Automatic Persisted Queries
Cache query hashes for instant lookup (85-95% hit rate).

### 4. No ORM
Direct PostgreSQL queries, no object mapping overhead.

# DELETE:
- "Intelligent Mode Selection" section
- "When to use which mode" guide
- Mode comparison charts
```

---

#### File: `docs/performance/apq-optimization-guide.md`
**Issues**:
- May reference APQ working with different execution modes

**Updates Needed**:
```markdown
# UPDATE APQ + Rust pipeline integration:

## APQ with Rust Pipeline

APQ (Automatic Persisted Queries) works seamlessly with the Rust pipeline:

1. **Query Hash Lookup** (0.1ms) - Check if query hash exists
2. **Cache Hit** → Rust pipeline processes cached query
3. **Cache Miss** → Store new query, then Rust pipeline

### Performance:
- **With APQ + Rust**: 0.5-2ms total
- **Without APQ**: 5-25ms (includes GraphQL parsing)

No execution mode configuration needed - APQ works with the exclusive Rust pipeline automatically.
```

---

#### File: `docs/performance/caching.md`
**Issues**:
- Caching strategies per execution mode

**Updates Needed**:
```markdown
# DELETE:
- "Caching in NORMAL mode"
- "Caching in PASSTHROUGH mode"

# UPDATE:
## Caching Strategy

FraiseQL has three caching layers (all compatible with Rust pipeline):

### 1. PostgreSQL Transform Tables (tv_*)
Pre-computed JSONB responses in database.
```sql
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL  -- Pre-computed GraphQL response
);
```

### 2. APQ Hash Cache
Query hash → Query string mapping.

### 3. PostgreSQL UNLOGGED Tables
Fast in-database caching (optional).

All layers work with the Rust pipeline automatically.
```

---

### 2.3 Tutorial Documentation (UPDATE 3 files)

#### File: `docs/tutorials/beginner-path.md`
**Issues**:
- May introduce execution modes to beginners

**Updates Needed**:
```markdown
# SIMPLIFY architecture explanation:

## How FraiseQL Works

1. You define a GraphQL query
2. PostgreSQL returns JSONB data
3. Rust transforms it to GraphQL format
4. Response sent to client

That's it! No mode selection, no configuration needed.

# DELETE:
- "Understanding Execution Modes" section
- "Choosing the right mode" section
```

---

#### File: `docs/tutorials/blog-api.md`
**Issues**:
- Tutorial code may show execution mode configuration

**Updates Needed**:
```markdown
# REMOVE from all code examples:
- Any `mode="..."` parameters
- Any execution mode configuration
- Any mode detection comments

# ENSURE code examples work with RustResponseBytes:
```python
@fraiseql.query
async def posts(info) -> list[Post]:
    repo = info.context["repo"]
    # Returns RustResponseBytes - works automatically with GraphQL
    return await repo.find("v_post")
```
```

---

#### File: `docs/tutorials/production-deployment.md`
**Issues**:
- Production deployment guide with mode tuning

**Updates Needed**:
```markdown
# DELETE sections:
- "Execution Mode Configuration for Production"
- "Tuning Mode Priority"
- "Monitoring Mode Distribution"

# ADD:
## Production Configuration

Minimal configuration needed:

```python
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL"),
    apq_enabled=True,  # Enable APQ for production
    pool_size=20,      # Connection pool size
)
```

The Rust pipeline is always active and production-optimized.
```

---

### 2.4 Rust Documentation (UPDATE 2 files, DELETE 4 files)

#### File: `docs/rust/RUST_FIRST_PIPELINE.md`
**Decision**: **UPDATE** (make it the primary Rust docs)

**Current Status**: This doc describes the vision. Update to describe current reality.

**Updates Needed**:
```markdown
# CHANGE title:
OLD: "Rust-First Pipeline: Vision"
NEW: "Rust Pipeline Architecture"

# UPDATE intro:
This document describes FraiseQL's exclusive Rust pipeline architecture.

## Current Implementation

FraiseQL uses a single Rust pipeline for ALL query execution:

```
PostgreSQL → Rust → HTTP
   (JSONB)    (fraiseql-rs)   (bytes)
```

### Core Function: build_graphql_response()

All queries use this single Rust function:

```rust
pub fn build_graphql_response(
    json_strings: Vec<String>,
    field_name: String,
    type_name: Option<String>,
    field_paths: Option<Vec<Vec<String>>>,
) -> Vec<u8>
```

**What it does:**
1. Concatenate JSON strings into array
2. Wrap in GraphQL response structure
3. Transform snake_case → camelCase
4. Inject __typename
5. Filter fields (if field_paths provided)
6. Return UTF-8 bytes ready for HTTP

# DELETE:
- "Phase 1", "Phase 2" planning sections
- "Future work" sections
- Any "TODO" items
```

---

#### File: `docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md`
**Decision**: **UPDATE** (convert from implementation guide to usage guide)

**Updates Needed**:
```markdown
# CHANGE title:
OLD: "Rust Pipeline Implementation Guide"
NEW: "Rust Pipeline Usage Guide"

# REWRITE as user-facing documentation:

## Using the Rust Pipeline

The Rust pipeline is always active. This guide explains how it works internally so you can optimize your queries.

### How Your Query Flows Through Rust

1. **PostgreSQL Query** - Returns JSONB rows
```sql
SELECT jsonb_build_object('id', id, 'first_name', first_name)
FROM tb_user;
```

2. **Rust Transformation** - Automatic processing
- Concatenate rows: `[{...}, {...}]`
- Wrap: `{"data":{"users":[...]}}`
- Transform: `first_name → firstName`
- Add: `"__typename":"User"`

3. **HTTP Response** - UTF-8 bytes sent directly

### Optimization Tips

**Use transform tables (tv_*)** for pre-computed JSONB:
```sql
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB  -- Pre-built, Rust just wraps it
);
```

**Enable field projection** in queries:
```graphql
query {
  users {
    id         # Rust extracts only
    firstName  # these two fields
  }
}
```

# DELETE:
- TDD cycle sections
- Implementation steps
- Testing sections (move to contributor docs if needed)
```

---

### 2.5 Strategic Documentation (UPDATE 3 files)

#### File: `docs/strategic/V1_VISION.md`
**Decision**: **UPDATE** (note that Rust pipeline is implemented)

**Updates Needed**:
```markdown
# ADD status update at top:
> **UPDATE 2025-10**: Rust-first pipeline is now implemented in v0.11.5+.
> This document describes the original vision. See `docs/rust/RUST_FIRST_PIPELINE.md` for current architecture.

# OR alternatively:
# ARCHIVE this file to docs/historical/V1_VISION.md
```

---

#### File: `docs/strategic/VERSION_STATUS.md`
**Decision**: **UPDATE** (current version status)

**Updates Needed**:
```markdown
# UPDATE v0.11.5 description:
**v0.11.5** (Current Stable)
**Status**: Production stable, exclusive Rust pipeline
**Key Features**:
- ✅ Rust-first architecture (PostgreSQL → Rust → HTTP)
- ✅ Single execution path (no mode detection)
- ✅ 0.5-5ms response time
- ✅ Automatic camelCase conversion
- ✅ Built-in __typename injection
```

---

#### File: `docs/strategic/FRAISEQL_INDUSTRIAL_READINESS_ASSESSMENT_2025-10-20.md`
**Decision**: **UPDATE** (mark Rust pipeline as complete)

**Updates Needed**:
```markdown
# UPDATE "Already Implemented" section:
### 6. Rust-First Pipeline - COMPLETE ✅

**Status**: Production-ready, exclusive execution path
**Performance**: 0.5-5ms response time (PostgreSQL → HTTP)

**Implemented Components:**
- ✅ build_graphql_response() unified API
- ✅ Automatic snake_case → camelCase transformation
- ✅ __typename injection
- ✅ Field projection optimization
- ✅ Schema-based transformation
- ✅ Zero-copy UTF-8 byte output
- ✅ Integration with APQ cache
- ✅ RustResponseBytes FastAPI integration

**Files**: `fraiseql_rs/` (Rust crate), `src/fraiseql/core/rust_pipeline.py`

# UPDATE completion percentage:
OLD: "FraiseQL has 80% industrial readiness"
NEW: "FraiseQL has 85% industrial readiness" (Rust pipeline complete adds 5%)
```

---

### 2.6 Migration Guides (UPDATE 2 files)

#### File: `docs/migration-guides/v0.11.0.md`
**Decision**: **UPDATE** (add Rust pipeline migration notes)

**Updates Needed**:
```markdown
# ADD section:
## Rust Pipeline Migration (v0.11.5)

### Breaking Changes:
- Removed `ExecutionMode` enum
- Removed execution mode configuration options
- All queries now use Rust pipeline exclusively

### Migration Steps:

1. **Remove execution mode config:**
```python
# OLD (remove this):
config = FraiseQLConfig(
    execution_mode_priority=["turbo", "passthrough", "normal"]
)

# NEW (remove these settings):
config = FraiseQLConfig(
    database_url=...  # Just use basic config
)
```

2. **Update test assertions:**
Tests expecting dict objects will now receive `RustResponseBytes`. Update assertions to handle this:
```python
from fraiseql.core.rust_pipeline import RustResponseBytes
import json

result = await repo.find("v_user")
if isinstance(result, RustResponseBytes):
    data = json.loads(bytes(result.bytes))
```

3. **Remove mode-specific code:**
Delete any code that checks execution mode or selects between modes.
```

---

#### File: `docs/migration-guides/v0.11-to-v1.md`
**Decision**: **UPDATE** (mark as draft for future v1.0)

**Updates Needed**:
```markdown
# ADD note at top:
> **STATUS**: Draft - v1.0 not yet released.
> Current stable: v0.11.5 (with Rust pipeline)

# CLARIFY what v1.0 will change:
v1.0 will be primarily a stability/polish release on top of v0.11.5's Rust pipeline. Major breaking changes may be deferred to v2.0.
```

---

### 2.7 Advanced Topics (UPDATE 3 files)

#### File: `docs/advanced/authentication.md`
**Check for**: References to execution modes in auth context

**If found, UPDATE**:
```markdown
# Ensure auth examples don't reference modes:
```python
@fraiseql.query
@requires_permission("user:read")
async def users(info) -> list[User]:
    repo = info.context["repo"]
    # Auth works with Rust pipeline automatically
    return await repo.find("v_user")
```
```

---

#### File: `docs/advanced/multi-tenancy.md`
**Check for**: Tenant isolation with different execution modes

**If found, UPDATE**:
```markdown
# UPDATE tenant query examples:
```python
async def get_tenant_users(tenant_id: str):
    repo = get_repository()
    # Rust pipeline respects tenant isolation automatically
    return await repo.find("v_user", where={
        "tenant_id": {"eq": tenant_id}
    })
```
```

---

#### File: `docs/advanced/database-patterns.md`
**Check for**: CQRS patterns with mode selection

**If found, UPDATE**:
```markdown
# ENSURE examples show Rust pipeline flow:

## CQRS Query Side

Transform tables (tv_*) work perfectly with Rust pipeline:

```sql
-- Query side: Pre-computed JSONB
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL
);
```

```python
# Python query (Rust handles transformation)
@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    return await repo.find("tv_user")
```

Rust pipeline:
1. Reads JSONB from tv_user
2. Wraps in GraphQL response
3. Adds __typename
4. Returns bytes to HTTP
```

---

## Category 3: KEEP - Already Accurate

These files don't mention execution modes or are still accurate:

### Core Concepts (KEEP - 8 files)
```
docs/core/concepts-glossary.md          # Domain concepts, not implementation
docs/core/ddl-organization.md           # Database structure
docs/core/dependencies.md               # Dependency injection
docs/core/explicit-sync.md              # Sync pattern
docs/core/migrations.md                 # Migration system
docs/core/postgresql-extensions.md     # PostgreSQL features
docs/core/project-structure.md          # File organization
docs/core/types-and-schema.md          # GraphQL type system
```

### Database Patterns (KEEP - 3 files)
```
docs/database/DATABASE_LEVEL_CACHING.md    # Database caching concepts
docs/database/TABLE_NAMING_CONVENTIONS.md   # Naming patterns
docs/database/ltree-index-optimization.md   # Index optimization
```

### Enterprise Features (KEEP - 3 files)
```
docs/enterprise/ENTERPRISE.md                    # Enterprise overview
docs/enterprise/RBAC_POSTGRESQL_ASSESSMENT.md    # RBAC design
docs/fraiseql_enterprise_gap_analysis.md         # Gap analysis
```

### Architecture Decisions (KEEP - 3 files)
```
docs/architecture/decisions/README.md                        # ADR index
docs/architecture/decisions/002_ultra_direct_mutation_path.md # Still relevant
docs/architecture/decisions/005_simplified_single_source_cdc.md # Current approach
docs/architecture/decisions/003_unified_audit_table.md        # Audit design
```

### Advanced Topics (KEEP - 4 files)
```
docs/advanced/bounded-contexts.md       # DDD concepts
docs/advanced/event-sourcing.md         # Event sourcing pattern
docs/advanced/llm-integration.md        # LLM features
docs/case-studies/                      # Case studies
```

### Reference Docs (KEEP - 3 files)
```
docs/reference/cli.md                   # CLI commands
docs/reference/decorators.md            # Decorator reference
docs/TESTING_CHECKLIST.md              # Testing guide
```

### Other (KEEP - 6+ files)
```
docs/README.md                          # Docs index
docs/quickstart.md                      # Quick start (check for mode refs)
docs/FAKE_DATA_GENERATOR_DESIGN.md      # Generator design
docs/development/NEW_USER_CONFUSIONS.md # UX issues to address
docs/performance/cascade-invalidation.md # Cache invalidation
docs/performance/coordinate_performance_guide.md # Coordinate perf
```

---

## Category 4: CREATE - Missing Content

### 4.1 CREATE: `docs/rust/README.md`
**Purpose**: Overview of Rust pipeline for users

**Content**:
```markdown
# FraiseQL Rust Pipeline

FraiseQL uses an exclusive Rust pipeline for all query execution, achieving 0.5-5ms response times.

## Architecture

```
PostgreSQL → Rust (fraiseql-rs) → HTTP
  (JSONB)      Transformation      (bytes)
```

## How It Works

1. **PostgreSQL** returns JSONB data
2. **Rust** transforms it:
   - snake_case → camelCase
   - Inject __typename
   - Wrap in GraphQL response structure
   - Filter fields (optional)
3. **HTTP** receives UTF-8 bytes

## Key Documents

- [Pipeline Architecture](RUST_FIRST_PIPELINE.md) - Technical details
- [Usage Guide](RUST_PIPELINE_IMPLEMENTATION_GUIDE.md) - How to optimize queries
- [Field Projection](RUST_FIELD_PROJECTION.md) - Performance optimization

## For Contributors

The Rust code lives in `fraiseql_rs/` directory. See [Contributing Guide](../../CONTRIBUTING.md) for development setup.
```

---

### 4.2 CREATE: `docs/migration-guides/multi-mode-to-rust-pipeline.md`
**Purpose**: Help v0.11.4 and earlier users migrate to exclusive Rust pipeline

**Content**:
```markdown
# Migration Guide: Multi-Mode Execution → Exclusive Rust Pipeline

**Affects**: Users upgrading from v0.11.4 or earlier to v0.11.5+

## What Changed

FraiseQL v0.11.5+ uses an **exclusive Rust pipeline** for all queries. The old multi-mode execution system (NORMAL, PASSTHROUGH, TURBO) has been removed.

### Before (v0.11.4):
```
Query → Mode Detection → NORMAL | PASSTHROUGH | TURBO → Response
```

### After (v0.11.5+):
```
Query → Rust Pipeline → Response
```

## Breaking Changes

### 1. Removed: ExecutionMode Enum
```python
# ❌ OLD (no longer works):
from fraiseql.execution import ExecutionMode

config = FraiseQLConfig(
    execution_mode_priority=[
        ExecutionMode.TURBO,
        ExecutionMode.PASSTHROUGH,
        ExecutionMode.NORMAL,
    ]
)

# ✅ NEW (remove execution mode config):
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL"),
    # Rust pipeline always active, no configuration needed
)
```

### 2. Removed: Mode-Specific Configuration
These configuration options no longer exist:
- `execution_mode_priority`
- `enable_python_fallback`
- `passthrough_detection_enabled`
- `json_passthrough_mode`
- `intelligent_mode_switching`

### 3. Changed: Repository Return Type
Repository methods now return `RustResponseBytes` instead of Python dicts in most cases.

```python
# For GraphQL queries (most common):
result = await repo.find("v_user")
# result is RustResponseBytes - works directly with GraphQL

# For direct access (e.g., in tests):
import json
from fraiseql.core.rust_pipeline import RustResponseBytes

result = await repo.find("v_user")
if isinstance(result, RustResponseBytes):
    data = json.loads(bytes(result.bytes))
    users = data["data"]["users"]
```

## Migration Steps

### Step 1: Update Configuration
Remove all execution mode configuration:

```python
# Remove these lines from your config:
- execution_mode_priority=[...]
- enable_python_fallback=...
- Any other mode-related config
```

### Step 2: Update Tests
If you have tests asserting on return types, update them:

```python
# OLD:
async def test_get_users():
    users = await repo.find("v_user")
    assert isinstance(users, list)
    assert users[0]["first_name"] == "John"

# NEW:
async def test_get_users():
    from fraiseql.core.rust_pipeline import RustResponseBytes
    import json

    result = await repo.find("v_user")
    if isinstance(result, RustResponseBytes):
        data = json.loads(bytes(result.bytes))
        users = data["data"]["users"]
    else:
        users = result

    assert isinstance(users, list)
    assert users[0]["firstName"] == "John"  # Note: camelCase now
```

### Step 3: Remove Mode-Specific Code
Delete any code that:
- Checks `ExecutionMode`
- Selects between execution modes
- Implements Python transformation fallbacks

```python
# ❌ DELETE code like this:
if mode == ExecutionMode.NORMAL:
    result = transform_to_camel_case(result)
elif mode == ExecutionMode.PASSTHROUGH:
    result = raw_json_result

# ✅ Rust pipeline handles everything automatically
```

### Step 4: Verify Field Names
Rust pipeline **always** converts snake_case → camelCase:

```python
# OLD (might have varied by mode):
user["first_name"]  # snake_case in some modes

# NEW (always camelCase):
user["firstName"]  # Always camelCase now
```

## Performance Impact

**Good news**: You'll see performance improvements!

### Before (v0.11.4):
- NORMAL mode: 25-100ms
- PASSTHROUGH mode: 5-25ms
- TURBO mode: 1-10ms

### After (v0.11.5+):
- All queries: **0.5-5ms** (Rust pipeline)

## Troubleshooting

### "ExecutionMode not found"
**Solution**: Remove all `ExecutionMode` imports and usage.

### "Expected dict, got RustResponseBytes"
**Solution**: Update test assertions to handle `RustResponseBytes` (see Step 2 above).

### "Field 'first_name' not found"
**Solution**: Use camelCase field names (`firstName` instead of `first_name`).

## Questions?

- See [Rust Pipeline Architecture](../rust/RUST_FIRST_PIPELINE.md)
- Open an issue: https://github.com/fraiseql/fraiseql/issues
```

---

### 4.3 CREATE: `docs/core/rust-pipeline-integration.md`
**Purpose**: Explain how Python code integrates with Rust pipeline

**Content**:
```markdown
# Python ↔ Rust Integration

This guide explains how FraiseQL's Python code integrates with the Rust pipeline.

## Overview

FraiseQL is primarily a Python framework (GraphQL schema, resolvers, database queries) with a **Rust performance layer** for JSON transformation.

```
┌─────────────────────────────────────────────┐
│           Python Layer                      │
│  - GraphQL schema definition                │
│  - Query resolvers                          │
│  - Database queries (PostgreSQL)            │
│  - Business logic                           │
└──────────────────┬──────────────────────────┘
                   │
                   │ JSONB strings
                   ▼
┌─────────────────────────────────────────────┐
│           Rust Layer (fraiseql-rs)          │
│  - JSON concatenation                       │
│  - GraphQL response wrapping                │
│  - snake_case → camelCase                   │
│  - __typename injection                     │
│  - Field projection                         │
└──────────────────┬──────────────────────────┘
                   │
                   │ UTF-8 bytes
                   ▼
                FastAPI → HTTP

## The Boundary

### What Python Does:
- Define GraphQL types and queries
- Execute PostgreSQL queries
- Collect JSONB strings from database

### What Rust Does:
- Transform JSONB to GraphQL JSON
- Convert field names to camelCase
- Inject __typename
- Output UTF-8 bytes for HTTP

## Code Example

### Python Side:
```python
# 1. Define GraphQL type
@fraiseql.type(sql_source="v_user")
class User:
    id: str
    first_name: str  # Python uses snake_case
    created_at: datetime

# 2. Define query resolver
@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]

    # 3. Execute PostgreSQL query (returns JSONB)
    # Rust pipeline handles transformation automatically
    return await repo.find("v_user")
```

### Under the Hood:
```python
# In FraiseQLRepository.find():
async def find(self, source: str):
    # 1. Execute PostgreSQL query
    rows = await conn.fetch(f"SELECT data FROM {source}")

    # 2. Extract JSONB strings
    json_strings = [row["data"] for row in rows]

    # 3. Call Rust pipeline
    import fraiseql_rs

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=json_strings,
        field_name="users",
        type_name="User",
        field_paths=None,
    )

    # 4. Return RustResponseBytes (FastAPI sends as HTTP response)
    return RustResponseBytes(response_bytes)
```

### Rust Side (fraiseql_rs crate):
```rust
#[pyfunction]
pub fn build_graphql_response(
    json_strings: Vec<String>,
    field_name: String,
    type_name: Option<String>,
    field_paths: Option<Vec<Vec<String>>>,
) -> PyResult<Vec<u8>> {
    // 1. Concatenate JSON strings
    let array = format!("[{}]", json_strings.join(","));

    // 2. Wrap in GraphQL response
    let response = format!(
        r#"{{"data":{{"{}":{}}}}}"#,
        field_name, array
    );

    // 3. Transform to camelCase + inject __typename
    let transformed = transform_json(&response, type_name);

    // 4. Return UTF-8 bytes
    Ok(transformed.into_bytes())
}
```

## Performance Benefits

By delegating to Rust:
- **7-10x faster** JSON transformation
- **Zero Python overhead** for string operations
- **Direct UTF-8 bytes** to HTTP (no Python serialization)

## Type Safety

The Python/Rust boundary is type-safe via PyO3:
- Python `list[str]` → Rust `Vec<String>`
- Python `Optional[str]` → Rust `Option<String>`
- Rust `Vec<u8>` → Python `bytes`

## Debugging

### Enable Rust Logs:
```bash
RUST_LOG=fraiseql_rs=debug python app.py
```

### Inspect Rust Output:
```python
from fraiseql.core.rust_pipeline import RustResponseBytes
import json

result = await repo.find("v_user")
if isinstance(result, RustResponseBytes):
    # Convert bytes to string for inspection
    json_str = result.bytes.decode('utf-8')
    print(json_str)  # See what Rust produced

    # Parse to verify structure
    data = json.loads(json_str)
    print(json.dumps(data, indent=2))
```

## Contributing to Rust Code

The Rust code lives in `fraiseql_rs/` directory:

```
fraiseql_rs/
├── Cargo.toml           # Rust dependencies
├── src/
│   ├── lib.rs          # Main entry point
│   ├── transform.rs    # CamelCase transformation
│   ├── typename.rs     # __typename injection
│   └── response.rs     # GraphQL response building
└── tests/              # Rust tests
```

See [Contributing Guide](../CONTRIBUTING.md) for Rust development setup.
```

---

### 4.4 CREATE: `docs/performance/rust-pipeline-optimization.md`
**Purpose**: Performance optimization guide focused on Rust pipeline

**Content**:
```markdown
# Rust Pipeline Performance Optimization

How to get the best performance from FraiseQL's Rust pipeline.

## Performance Characteristics

The Rust pipeline is **already optimized** and provides 0.5-5ms response times out of the box. However, you can improve end-to-end performance with these strategies.

## 1. Optimize Database Queries (Biggest Impact)

The Rust pipeline is fast (< 1ms), but database queries can take 1-100ms+ depending on complexity.

### Use Transform Tables (tv_*)
Pre-compute JSONB responses in the database:

```sql
-- Slow: Compute JSONB on every query
SELECT jsonb_build_object(
    'id', u.id,
    'first_name', u.first_name,
    'posts', (SELECT jsonb_agg(...) FROM posts WHERE user_id = u.id)
) FROM tb_user u;
-- Takes: 10-50ms for complex queries

-- Fast: Pre-computed JSONB in transform table
SELECT data FROM tv_user WHERE id = $1;
-- Takes: 0.5-2ms (just index lookup!)
```

**Impact**: 5-50x faster database queries

### Index Properly
```sql
-- Index JSONB paths used in WHERE clauses
CREATE INDEX idx_user_email ON tv_user ((data->>'email'));

-- Index foreign keys
CREATE INDEX idx_post_user_id ON tb_post (fk_user);
```

## 2. Enable Field Projection

Let Rust filter only requested fields:

```graphql
# Client requests only these fields:
query {
  users {
    id
    firstName
  }
}
```

Rust pipeline will extract only `id` and `firstName` from the full JSONB, ignoring other fields.

**Configuration:**
```python
config = FraiseQLConfig(
    field_projection=True,  # Enable field filtering (default)
)
```

**Impact**: 20-40% faster transformation for large objects with many fields

## 3. Use Automatic Persisted Queries (APQ)

Enable APQ to cache query parsing:

```python
config = FraiseQLConfig(
    apq_enabled=True,
    apq_storage_backend="postgresql",  # or "memory"
)
```

**Benefits:**
- 85-95% cache hit rate in production
- Eliminates GraphQL parsing overhead
- Reduces bandwidth (send hash instead of full query)

**Impact**: 5-20ms saved per query

## 4. Minimize JSONB Size

Smaller JSONB = faster Rust transformation:

### Don't Include Unnecessary Data
```sql
-- ❌ Bad: Include everything
SELECT jsonb_build_object(
    'id', id,
    'first_name', first_name,
    'email', email,
    'bio', bio,  -- 1MB+ text field!
    'preferences', preferences,  -- Large JSON
    ...
) FROM tb_user;

-- ✅ Good: Only include what GraphQL needs
SELECT jsonb_build_object(
    'id', id,
    'first_name', first_name,
    'email', email
) FROM tb_user;
```

**Impact**: 2-5x faster for large objects

### Use Separate Queries for Large Fields
```graphql
# Main query: small fields
query {
  users {
    id
    firstName
  }
}

# Separate query when needed: large fields
query {
  user(id: "123") {
    bio
    preferences
  }
}
```

## 5. Batch Queries with DataLoader (if needed)

For N+1 query problems, use DataLoader pattern:

```python
from fraiseql.utils import DataLoader

user_loader = DataLoader(load_fn=batch_load_users)

# Batches multiple user lookups into single query
users = await asyncio.gather(*[
    user_loader.load(id) for id in user_ids
])
```

## 6. Monitor Rust Performance

Track Rust pipeline metrics:

```python
from fraiseql.monitoring import get_metrics

metrics = get_metrics()
print(f"Rust transform avg: {metrics['rust_transform_avg_ms']}ms")
print(f"Rust transform p95: {metrics['rust_transform_p95_ms']}ms")
```

**Normal values:**
- Simple objects: 0.1-0.5ms
- Complex nested: 0.5-2ms
- Large arrays: 1-5ms

**If higher:** Check JSONB size or field projection settings

## 7. PostgreSQL Configuration

Optimize PostgreSQL for JSONB queries:

```sql
-- postgresql.conf
shared_buffers = 4GB          -- 25% of RAM
effective_cache_size = 12GB   -- 75% of RAM
work_mem = 64MB               -- For complex queries
```

## Performance Checklist

- [ ] Use transform tables (tv_*) for complex queries
- [ ] Index JSONB paths used in WHERE clauses
- [ ] Enable field projection (default: enabled)
- [ ] Enable APQ for production
- [ ] Minimize JSONB size (only include needed fields)
- [ ] Use DataLoader for N+1 queries
- [ ] Monitor Rust pipeline metrics
- [ ] Optimize PostgreSQL configuration

## Benchmarking

Measure end-to-end performance:

```python
import time

start = time.time()
result = await repo.find("v_user")
duration = time.time() - start
print(f"Total time: {duration*1000:.2f}ms")
```

**Target times:**
- Simple query: < 5ms
- Complex query with joins: < 25ms
- With APQ cache hit: < 2ms

## Advanced: Custom Rust Transformations

For very specialized needs, you can extend fraiseql-rs. See [Contributing Guide](../../CONTRIBUTING.md).

## Summary

The Rust pipeline itself is already optimized. Focus your optimization efforts on:
1. **Database query speed** (biggest impact)
2. **APQ caching** (easiest win)
3. **JSONB size** (if working with large objects)
```

---

### 4.5 UPDATE (not CREATE): `README.md`
**Purpose**: Update main README to reflect Rust-first architecture

**Updates Needed**:
```markdown
# UPDATE "Architecture" section:

## 🏗️ Architecture

FraiseQL's **Rust-first** architecture delivers exceptional performance through unified execution:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   GraphQL       │ →  │   PostgreSQL     │ →  │   Rust          │
│   Request       │    │   JSONB Query    │    │   Transform     │
│                 │    │   (0.05-0.5ms)  │    │   (0.5ms)       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                 ↓
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   APQ Hash      │ →  │   Storage        │ →  │   HTTP          │
│   (SHA-256)     │    │   Backend        │    │   Response      │
│                 │    │   Memory/PG      │    │   (0.5-2ms)     │
└─────────────────┘    └──────────────────┘    └─────────────────┘
    Optional Cache         FraiseQL Cache         Instant Response
```

### **Key Innovations**
1. **Exclusive Rust Pipeline**: PostgreSQL → Rust → HTTP (no Python overhead)
2. **Rust Field Projection**: 7-10x faster JSON transformation than Python
3. **Transform Tables**: `tv_*` tables with generated JSONB for instant queries
4. **APQ Storage Abstraction**: Pluggable backends (Memory/PostgreSQL) for query hash storage
5. **Zero-Copy Path**: Sub-millisecond responses with zero Python serialization

# DELETE outdated sections:
- "Multi-mode execution" section
- "Intelligent mode detection" section

# UPDATE performance tables to show single pipeline:
| Optimization Stack | Response Time | Use Case |
|-------------------|---------------|----------|
| **Rust Pipeline + APQ** | **0.5-2ms** | Production applications |
| **Rust Pipeline only** | **1-5ms** | Development & testing |
```

---

## 📋 Implementation Checklist

### Phase 1: DELETE Deprecated Docs (13 files)
```bash
# Rust planning docs (6 files)
rm docs/rust/RUST_FIRST_SIMPLIFICATION.md
rm docs/rust/RUST_INTERFACE_FIX_NEEDED.md
rm docs/rust/RUST_CLEANUP_SUMMARY.md
rm docs/rust/RUST_FIRST_CACHING_STRATEGY.md
rm docs/development/CLEANUP_AND_CLARIFICATION_PLAN.md
rm docs/development/DOCUMENTATION_CLEANUP_GUIDE.md

# Theoretical/vision docs (4 files)
rm docs/strategic/THEORETICAL_OPTIMAL_ARCHITECTURE.md
rm docs/strategic/PATTERNS_TO_IMPLEMENT.md
rm docs/strategic/UPDATE_PERFORMANCE_CLAIMS.md
rm docs/strategic/FRAISEQL_POST_MIGRATION_ROADMAP.md

# Old ADRs (3 files)
rm docs/architecture/decisions/001_graphql_mutation_response_initial_plan.md
rm docs/architecture/decisions/003_dual_path_cdc_pattern.md
rm docs/architecture/decisions/004_dual_path_implementation_examples.md
```

### Phase 2: CREATE New Docs (4 files)
```bash
# Create Rust docs overview
touch docs/rust/README.md

# Create migration guide
touch docs/migration-guides/multi-mode-to-rust-pipeline.md

# Create integration guide
touch docs/core/rust-pipeline-integration.md

# Create optimization guide
touch docs/performance/rust-pipeline-optimization.md
```

Write content as specified in "Category 4: CREATE" above.

### Phase 3: UPDATE Core Docs (Priority Order)

#### High Priority (Do First):
1. `README.md` - Main repository README
2. `docs/core/configuration.md` - Config reference
3. `docs/reference/config.md` - API config reference
4. `docs/migration-guides/v0.11.0.md` - Migration guide

#### Medium Priority:
5. `docs/performance/PERFORMANCE_GUIDE.md` - Performance guide
6. `docs/performance/index.md` - Performance overview
7. `docs/core/queries-and-mutations.md` - Query examples
8. `docs/core/database-api.md` - Repository API
9. `docs/rust/RUST_FIRST_PIPELINE.md` - Main Rust docs
10. `docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md` - Rust usage

#### Lower Priority:
11. `docs/production/deployment.md`
12. `docs/production/monitoring.md`
13. `docs/tutorials/beginner-path.md`
14. `docs/tutorials/blog-api.md`
15. `docs/tutorials/production-deployment.md`
16. `docs/performance/apq-optimization-guide.md`
17. `docs/performance/caching.md`
18. `docs/strategic/VERSION_STATUS.md`
19. `docs/strategic/FRAISEQL_INDUSTRIAL_READINESS_ASSESSMENT_2025-10-20.md`
20. `docs/migration-guides/v0.11-to-v1.md`

### Phase 4: Quick Checks (Skim for Issues)
For each file in "Category 3: KEEP", quickly search for:
```bash
# Search for execution mode references
grep -i "execution.mode\|ExecutionMode\|NORMAL.*mode\|PASSTHROUGH.*mode" docs/advanced/*.md

# If found, update that file
```

### Phase 5: Validation
```bash
# Verify all links work
make docs-linkcheck  # or equivalent

# Build documentation
make docs  # or equivalent

# Manually review key docs in browser
```

---

## 🎯 Success Criteria

### Documentation Should:
- [ ] **Never mention** ExecutionMode enum
- [ ] **Never mention** NORMAL, PASSTHROUGH modes (TURBO is obsolete too)
- [ ] **Always show** Rust pipeline as the only execution path
- [ ] **Accurately describe** RustResponseBytes return type
- [ ] **Include** migration guide from multi-mode to Rust pipeline
- [ ] **Show** correct field names (camelCase in responses)
- [ ] **Explain** how Python integrates with Rust
- [ ] **Provide** optimization guide for Rust pipeline

### Common Phrases to Search & Replace:
```bash
# Search for these patterns and update:
"execution mode"              → "Rust pipeline"
"mode detection"              → DELETE or rephrase
"NORMAL mode"                 → DELETE
"PASSTHROUGH mode"            → DELETE
"Python transformation"       → "Rust transformation"
"mode priority"               → DELETE
"intelligent mode switching"  → DELETE
```

---

## ⚠️ Critical Notes for Agent

1. **Don't delete CQRS documentation** - CQRS (Command Query Responsibility Segregation) is still core to FraiseQL. Only remove execution mode references.

2. **Trinity identifiers are still valid** - The Trinity pattern (pk_*, fk_*, id, identifier) is still part of the architecture. Don't remove this.

3. **Transform tables (tv_*) are still important** - These are more important than ever with the Rust pipeline. Ensure docs explain how they work WITH Rust.

4. **Preserve examples** - When updating examples, keep the same functionality but remove mode references.

5. **Migration guide is critical** - The multi-mode to Rust pipeline migration guide is the most important new doc. Be thorough.

6. **README.md is user-facing** - Changes to README.md should be marketing-friendly, not just technical. Emphasize benefits (speed, simplicity).

---

## 🚀 Execution Order (Recommended)

1. ✅ **Phase 1 first** - DELETE deprecated docs (safest, clear value)
2. ✅ **Phase 2 second** - CREATE new essential docs (migration guide is urgent)
3. ✅ **Phase 3.1** - UPDATE high priority docs (impacts most users)
4. ✅ **Phase 3.2** - UPDATE medium priority docs
5. ✅ **Phase 3.3** - UPDATE lower priority docs
6. ✅ **Phase 4** - Quick check of KEEP files (find any missed references)
7. ✅ **Phase 5** - Validation (ensure docs build and links work)

---

**Agent**: Work through this plan phase-by-phase. After each file update, verify the new content is accurate by cross-referencing with the actual code in `src/fraiseql/core/rust_pipeline.py` and `fraiseql_rs/`. Report any ambiguities or questions.
