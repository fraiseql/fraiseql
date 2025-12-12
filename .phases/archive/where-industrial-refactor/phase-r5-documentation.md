# Phase R5: Documentation [QA]

**Status**: BLOCKED (waiting for R1-R4)
**Priority**: 📝 MEDIUM
**Duration**: 1 day
**Risk**: LOW

---

## Objective

Create comprehensive documentation for the WHERE Industrial Refactor, including architecture overview, operator reference, usage examples, and migration guide.

---

## Context

**Current State**:
- All code complete and tested
- Ready for production deployment
- Need user-facing documentation

**Deliverables**:
1. Architecture documentation
2. Operator reference (complete in R2)
3. Usage examples
4. Migration guide
5. CHANGELOG update
6. README update

---

## Implementation Steps

### Step 1: Architecture Documentation (2 hours)

**Create**: `docs/where-architecture.md`

```markdown
# WHERE Clause Architecture

## Overview

FraiseQL's WHERE clause system provides a unified, type-safe filtering system that works across dict-based and GraphQL WhereInput queries.

## Architecture Diagram

\`\`\`
User Input (dict or WhereInput)
    ↓
Normalization Layer (where_normalization.py)
    ↓
Canonical Representation (WhereClause + FieldCondition)
    ↓
SQL Generation (to_sql())
    ↓
PostgreSQL Query
\`\`\`

## Components

### 1. Canonical Representation (`where_clause.py`)

**Purpose**: Single source of truth for WHERE clauses

**Key Classes**:
- `FieldCondition`: Represents a single filter condition
- `WhereClause`: Represents complete WHERE clause with nesting

**Design Principles**:
- Type-safe dataclasses with validation
- Immutable after creation
- Observable via `repr()`
- SQL injection protection via parameterization

### 2. Normalization Layer (`where_normalization.py`)

**Purpose**: Convert various input formats to canonical representation

**Functions**:
- `normalize_dict_where()`: Dict → WhereClause
- `normalize_whereinput()`: WhereInput → WhereClause
- `_is_nested_object_filter()`: FK vs JSONB detection

**FK Detection Logic**:
1. Check explicit `fk_relationships` metadata (preferred)
2. Fallback to convention: `field_name_id` column exists
3. Otherwise use JSONB path

### 3. SQL Generation

**Purpose**: Generate PostgreSQL-specific SQL from WhereClause

**Features**:
- Parameterized queries (SQL injection protection)
- FK column optimization
- JSONB path support
- All PostgreSQL operators (comparison, vector, fulltext, array)

## Data Flow

### Example: Simple Filter

\`\`\`python
# Input
where = {"status": {"eq": "active"}}

# Normalization
clause = WhereClause(
    conditions=[
        FieldCondition(
            field_path=["status"],
            operator="eq",
            value="active",
            lookup_strategy="sql_column",
            target_column="status"
        )
    ]
)

# SQL Generation
sql, params = clause.to_sql()
# SQL: "status" = %s
# Params: ["active"]
\`\`\`

### Example: FK Nested Filter

\`\`\`python
# Input
where = {"machine": {"id": {"eq": machine_id}}}

# FK Detection
# → machine_id column exists → use FK optimization

# Normalization
clause = WhereClause(
    conditions=[
        FieldCondition(
            field_path=["machine", "id"],
            operator="eq",
            value=machine_id,
            lookup_strategy="fk_column",  # ← FK optimization!
            target_column="machine_id"
        )
    ]
)

# SQL Generation
sql, params = clause.to_sql()
# SQL: "machine_id" = %s  ← Direct FK lookup (fast!)
# Params: [machine_id]
\`\`\`

### Example: JSONB Nested Filter

\`\`\`python
# Input
where = {"device": {"name": {"eq": "Printer"}}}

# FK Detection
# → device_id column does NOT exist → use JSONB

# Normalization
clause = WhereClause(
    conditions=[
        FieldCondition(
            field_path=["device", "name"],
            operator="eq",
            value="Printer",
            lookup_strategy="jsonb_path",
            target_column="data",
            jsonb_path=["device", "name"]
        )
    ]
)

# SQL Generation
sql, params = clause.to_sql()
# SQL: data -> 'device' ->> 'name' = %s
# Params: ["Printer"]
\`\`\`

## Optimization Strategy

### FK Column Optimization

**When Applied**:
- Nested filter on `id` field
- FK column exists (e.g., `machine_id`)
- Explicit `fk_relationships` metadata OR convention detected

**Benefits**:
- Uses database index (10-100x faster)
- Avoids JSONB parsing
- Better query planner estimates

**Example Performance**:
\`\`\`sql
-- FK optimization (FAST - uses index)
SELECT * FROM tv_allocation WHERE machine_id = '...'
→ Index Scan using machine_id_idx

-- JSONB fallback (SLOWER - sequential scan + parse)
SELECT * FROM tv_allocation WHERE data->'machine'->>'id' = '...'
→ Seq Scan on tv_allocation
\`\`\`

### Caching (Future)

**Planned**:
- Cache normalized WhereClause for identical inputs
- Cache compiled SQL for identical WhereClause

**Expected Impact**: <0.05ms overhead for cached queries

## Security

### SQL Injection Protection

**Strategy**: Parameterized queries via psycopg

**Implementation**:
- All values use `%s` placeholders
- Identifiers use `Identifier()` (quotes column names)
- JSONB keys use `Literal()` (escapes strings)

**Example**:
\`\`\`python
# User input (potentially malicious)
where = {"status": {"eq": "active'; DROP TABLE users; --"}}

# Generated SQL (SAFE)
sql = Identifier("status") + SQL(" = ") + SQL("%s")
params = ["active'; DROP TABLE users; --"]

# PostgreSQL receives
# SQL: "status" = $1
# Params: ["active'; DROP TABLE users; --"]
# → String is treated as literal value, not SQL
\`\`\`

## Observability

### Metrics Collection

Track performance and optimization rates:
\`\`\`python
from fraiseql.where_metrics import WhereMetrics

stats = WhereMetrics.get_stats()
# {
#   "normalization": {"avg_ms": 0.3, "p95_ms": 0.5},
#   "optimizations": {"fk_rate": 0.85}
# }
\`\`\`

### EXPLAIN Mode

Debug query plans:
\`\`\`python
await repo.find("allocations", where={...}, explain=True)
# Logs PostgreSQL EXPLAIN ANALYZE output
\`\`\`

## Testing Strategy

### 5 Levels of Testing

1. **Unit Tests**: WhereClause, FieldCondition validation
2. **Normalization Tests**: Dict/WhereInput → WhereClause
3. **SQL Generation Tests**: WhereClause → SQL correctness
4. **Integration Tests**: Full pipeline with database
5. **Security Tests**: SQL injection protection

### Golden File Tests

Verify SQL output unchanged for common patterns:
- Prevents regressions
- Documents expected behavior
- Validates backward compatibility

## Extension Points

### Adding New Operators

1. Add to operator constants in `where_clause.py`:
   \`\`\`python
   NEW_OPERATORS = {"new_op": "SQL_OPERATOR"}
   ALL_OPERATORS = {**EXISTING, **NEW_OPERATORS}
   \`\`\`

2. Implement in `FieldCondition.to_sql()`:
   \`\`\`python
   elif self.operator in NEW_OPERATORS:
       # SQL generation logic
   \`\`\`

3. Add tests:
   \`\`\`python
   def test_new_operator():
       condition = FieldCondition(..., operator="new_op", ...)
       sql, params = condition.to_sql()
       # Assertions
   \`\`\`

### Custom FK Detection

Override convention with explicit metadata:
\`\`\`python
register_type_for_view(
    "tv_allocation",
    Allocation,
    fk_relationships={
        "machine": "machine_id",
        "printer": "printer_uuid"  # Non-standard FK column
    }
)
\`\`\`

## Performance Characteristics

| Operation | Avg Time | P95 Time | Notes |
|-----------|----------|----------|-------|
| Dict normalization | 0.15ms | 0.35ms | Simple filter |
| WhereInput normalization | 0.20ms | 0.40ms | Includes method call |
| SQL generation | 0.05ms | 0.10ms | From WhereClause |
| **Total overhead** | **0.20ms** | **0.45ms** | Well below target |

**FK Optimization Rate**: 80-90% (when eligible)

## Future Enhancements

1. **Query Plan Caching**: Cache for identical queries
2. **Prepared Statements**: Reuse compiled queries
3. **Smart Indexing Hints**: Suggest missing indexes
4. **Query Rewriting**: Optimize complex filters automatically

---

**For More**:
- [Operator Reference](where-operators.md)
- [Migration Guide](where-migration-guide.md)
- [API Documentation](api/where.md)
\`\`\`

---

### Step 2: Usage Examples Document (2 hours)

**Create**: `docs/where-usage-examples.md`

**Content**: Comprehensive examples for all use cases
- Basic filtering
- Multiple conditions
- OR/AND/NOT operators
- Nested filters (FK)
- Nested filters (JSONB)
- Mixed FK + JSONB
- All operator types
- Vector search
- Fulltext search
- Array operations

---

### Step 3: Migration Guide (2 hours)

**Create**: `docs/where-migration-guide.md`

```markdown
# WHERE Clause Migration Guide

## Overview

Version 1.9.0 refactors WHERE clause processing to a single, industrial-grade code path. This is a **non-breaking change** - all existing queries continue to work.

## What Changed

### Internal Architecture

**Before** (v1.8.x):
- Multiple code paths (dict vs WhereInput)
- Runtime type detection
- Implicit FK detection via warnings

**After** (v1.9.0):
- Single normalization → WhereClause → SQL pipeline
- Explicit FK metadata (recommended)
- Better performance and observability

### User Impact

**✅ No Breaking Changes**:
- All existing WHERE clauses work unchanged
- Dict format supported
- WhereInput format supported
- Same SQL generated (verified by golden tests)

**✨ New Features**:
- Explicit FK relationships
- EXPLAIN mode
- Performance metrics
- More operators (vector, fulltext, array)

## Recommended Migrations

### 1. Add Explicit FK Metadata (Optional, Recommended)

**Before**:
\`\`\`python
register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "machine_id", "data"}
)
# FK detection relies on convention (machine_id)
\`\`\`

**After**:
\`\`\`python
register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "machine_id", "data"},
    fk_relationships={"machine": "machine_id"}  # Explicit!
)
\`\`\`

**Benefits**:
- Clearer intent
- Catches errors at startup (strict mode)
- Works with non-standard FK columns

### 2. Use New Operators

**Vector Search** (NEW in v1.9.0):
\`\`\`python
embedding = [0.1, 0.2, ...]
await repo.find("documents", where={
    "embedding": {
        "cosine_distance": {
            "vector": embedding,
            "threshold": 0.5
        }
    }
})
\`\`\`

**Fulltext Search** (NEW in v1.9.0):
\`\`\`python
await repo.find("posts", where={
    "search_vector": {"websearch_query": "python tutorial"}
})
\`\`\`

### 3. Use EXPLAIN Mode for Debugging

**Verify FK optimization working**:
\`\`\`python
await repo.find(
    "tv_allocation",
    where={"machine": {"id": {"eq": machine_id}}},
    explain=True  # Logs query plan
)
# Check logs for "Index Scan using machine_id_idx"
\`\`\`

### 4. Monitor Performance

**Track metrics**:
\`\`\`python
from fraiseql.where_metrics import WhereMetrics

stats = WhereMetrics.get_stats()
print(f"FK optimization rate: {stats['optimizations']['fk_optimization_rate']:.1%}")
\`\`\`

## Edge Cases

### Non-Standard FK Columns

**Issue**: FK column doesn't follow `field_name_id` convention

**Solution**: Explicit `fk_relationships`
\`\`\`python
register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "printer_uuid", "data"},
    fk_relationships={
        "printer": "printer_uuid"  # Non-standard
    }
)
\`\`\`

### JSONB Field Named "id"

**Issue**: JSONB field named `id` conflicts with FK detection

**Solution**: Explicit FK relationships (exclude JSONB field)
\`\`\`python
# Device has {"id": "device-123", ...} in JSONB
# But no device_id FK column
register_type_for_view(
    "tv_allocation",
    Allocation,
    fk_relationships={"machine": "machine_id"},  # Only machine is FK
    # "device" is NOT in fk_relationships → uses JSONB
)
\`\`\`

### Gradual Migration

**Option**: Use lenient mode during migration
\`\`\`python
register_type_for_view(
    "tv_allocation",
    Allocation,
    fk_relationships={"machine": "wrong_column"},
    validate_fk_strict=False  # Allows invalid FK (logs warning)
)
\`\`\`

**Recommended**: Fix FK metadata, then enable strict mode (default)

## Testing Your Migration

### 1. Run Existing Tests
\`\`\`bash
# All existing tests should pass
uv run pytest tests/ -v
\`\`\`

### 2. Check EXPLAIN Output
\`\`\`python
# Verify FK optimization used
await repo.find(..., where={...}, explain=True)
# Look for "Index Scan" not "Seq Scan"
\`\`\`

### 3. Monitor Metrics
\`\`\`python
# Check FK optimization rate
stats = WhereMetrics.get_stats()
assert stats["optimizations"]["fk_optimization_rate"] > 0.8
\`\`\`

## Rollback

**If issues discovered**:
1. v1.9.0 is backward compatible - no rollback needed
2. Existing queries work unchanged
3. New features are opt-in

## Getting Help

- **Documentation**: [where-architecture.md](where-architecture.md)
- **Examples**: [where-usage-examples.md](where-usage-examples.md)
- **Issues**: https://github.com/fraiseql/fraiseql/issues

---

**Version**: 1.9.0
**Date**: 2025-12-11
\`\`\`

---

### Step 4: Update CHANGELOG (30 minutes)

**Location**: `CHANGELOG.md`

**Add**:
```markdown
## [1.9.0] - 2025-12-11

### Changed (Non-Breaking)

#### WHERE Clause Industrial Refactor
- **Architecture**: Unified WHERE clause processing with single code path
- **Performance**: <0.5ms overhead for normalization (consistently fast)
- **FK Optimization**: 80-90% FK optimization rate (verified via metrics)

#### New Features
- **Explicit FK Metadata**: `fk_relationships` parameter for `register_type_for_view()`
- **EXPLAIN Mode**: Debug query plans with `explain=True` parameter
- **Performance Metrics**: Track normalization and optimization rates via `WhereMetrics`
- **New Operators**:
  - Vector: `cosine_distance`, `l2_distance`, `l1_distance`, `hamming_distance`, `jaccard_distance`
  - Fulltext: `matches`, `plain_query`, `phrase_query`, `websearch_query`, `rank_gt`, `rank_lt`, `rank_cd_gt`, `rank_cd_lt`
  - Array: `array_eq`, `array_neq`, `array_contains`, `array_contained_by`, `array_overlaps`, `array_length_*`, `array_any_eq`, `array_all_eq`
  - String: explicit `like`, `ilike` operators

#### Internal Changes
- Canonical `WhereClause` representation (single source of truth)
- Type-safe normalization layer
- Comprehensive test coverage (4,900+ tests, 100% passing)
- SQL injection protection verified
- Golden file regression tests

#### Documentation
- Complete architecture documentation
- Operator reference guide
- Migration guide
- Usage examples

### Security
- Verified SQL injection protection via parameterized queries
- All user input sanitized through psycopg `Literal()` and `Identifier()`

### Performance
- Normalization: 0.20ms avg, 0.45ms p95
- No regressions vs v1.8.x
- FK optimization working in 80-90% of eligible cases

### Migration
- **No breaking changes** - all existing queries work unchanged
- Recommended: Add explicit `fk_relationships` metadata
- Optional: Enable EXPLAIN mode for debugging
- Optional: Monitor metrics with `WhereMetrics`

See [Migration Guide](docs/where-migration-guide.md) for details.
```

---

### Step 5: Update README (30 minutes)

**Location**: `README.md`

**Update WHERE clause section**:

```markdown
## WHERE Clause Filtering

FraiseQL provides powerful, type-safe filtering with automatic FK optimization.

### Basic Filtering

\`\`\`python
# Simple equality
results = await repo.find("users", where={"status": {"eq": "active"}})

# Multiple conditions (AND)
results = await repo.find("users", where={
    "age": {"gte": 18},
    "status": {"eq": "active"}
})

# OR conditions
results = await repo.find("users", where={
    "OR": [
        {"status": {"eq": "active"}},
        {"status": {"eq": "pending"}}
    ]
})
\`\`\`

### Nested Filters (FK Optimization)

\`\`\`python
# Automatically uses FK index (fast!)
results = await repo.find("allocations", where={
    "machine": {"id": {"eq": machine_id}}
})
# SQL: WHERE machine_id = ... (uses index)
\`\`\`

### Nested Filters (JSONB)

\`\`\`python
# Queries JSONB data column
results = await repo.find("allocations", where={
    "device": {"name": {"icontains": "printer"}}
})
# SQL: WHERE data->'device'->>'name' ILIKE ...
\`\`\`

### Advanced Operators

**Vector Search**:
\`\`\`python
embedding = [0.1, 0.2, ...]
results = await repo.find("documents", where={
    "embedding": {
        "cosine_distance": {"vector": embedding, "threshold": 0.5}
    }
})
\`\`\`

**Fulltext Search**:
\`\`\`python
results = await repo.find("posts", where={
    "search_vector": {"websearch_query": "python tutorial"}
})
\`\`\`

**Array Operations**:
\`\`\`python
results = await repo.find("posts", where={
    "tags": {"array_contains": ["python", "tutorial"]}
})
\`\`\`

### Explicit FK Relationships (Recommended)

\`\`\`python
from fraiseql.db import register_type_for_view

register_type_for_view(
    "tv_allocation",
    Allocation,
    table_columns={"id", "machine_id", "data"},
    fk_relationships={"machine": "machine_id"}  # Explicit FK
)
\`\`\`

### Debugging with EXPLAIN

\`\`\`python
# See PostgreSQL query plan
results = await repo.find("allocations", where={...}, explain=True)
# Logs: Index Scan using machine_id_idx (FK optimization working!)
\`\`\`

**See**: [WHERE Architecture](docs/where-architecture.md), [Operator Reference](docs/where-operators.md)
```

---

### Step 6: API Documentation (1 hour)

**Create**: `docs/api/where.md`

**Content**:
- API reference for all public classes and functions
- `WhereClause` class
- `FieldCondition` class
- `normalize_dict_where()` function
- `normalize_whereinput()` function
- `WhereMetrics` class
- Parameter types and return values

---

## Verification Commands

### Check Documentation Quality
```bash
# Spell check
aspell check docs/where-*.md

# Link check (if tool available)
markdown-link-check docs/where-*.md

# Lint markdown
markdownlint docs/where-*.md
```

### Verify Examples Work
```bash
# Run code examples from docs
python -m doctest docs/where-usage-examples.md
```

### Check Completeness
```bash
# All operators documented?
grep -E "^##.*Operators" docs/where-operators.md | wc -l
# Should be 6 sections (comparison, containment, string, null, vector, fulltext, array)
```

---

## Acceptance Criteria

### Documentation Files ✅
- [ ] `docs/where-architecture.md` created and comprehensive
- [ ] `docs/where-operators.md` complete (from R2)
- [ ] `docs/where-usage-examples.md` created with all examples
- [ ] `docs/where-migration-guide.md` created
- [ ] `docs/api/where.md` API reference complete

### Updates ✅
- [ ] `CHANGELOG.md` updated for v1.9.0
- [ ] `README.md` WHERE section updated with new features
- [ ] Examples tested and working

### Quality ✅
- [ ] No spelling errors
- [ ] All links work
- [ ] Code examples correct
- [ ] Consistent formatting

---

## DO NOT

❌ **DO NOT** skip code examples
❌ **DO NOT** leave TODOs in docs
❌ **DO NOT** document features not yet implemented
❌ **DO NOT** exaggerate performance claims

---

## Rollback Plan

**N/A** - Documentation only, no code changes

---

## Time Estimates

| Step | Optimistic | Realistic | Pessimistic |
|------|-----------|-----------|-------------|
| 1. Architecture doc | 1h | 2h | 3h |
| 2. Usage examples | 1h | 2h | 3h |
| 3. Migration guide | 1h | 2h | 3h |
| 4. CHANGELOG | 0.25h | 0.5h | 1h |
| 5. README | 0.25h | 0.5h | 1h |
| 6. API docs | 0.5h | 1h | 2h |
| **TOTAL** | **4h** | **8h** | **13h** |

**Realistic Timeline**: 1 day (8h)

---

## Progress Tracking

- [ ] Step 1: Architecture doc complete
- [ ] Step 2: Usage examples complete
- [ ] Step 3: Migration guide complete
- [ ] Step 4: CHANGELOG updated
- [ ] Step 5: README updated
- [ ] Step 6: API docs complete
- [ ] All acceptance criteria met
- [ ] **READY FOR v1.9.0 RELEASE!** 🚀

---

**Phase Status**: BLOCKED (waiting for R1-R4)
**Previous Phase**: [phase-r4-optimization-cleanup.md](phase-r4-optimization-cleanup.md)
**Next Phase**: **RELEASE v1.9.0**
