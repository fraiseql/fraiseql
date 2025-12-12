# Phase R4: Optimization & Cleanup [REFACTOR]

**Status**: BLOCKED (waiting for R1, R2, R3)
**Priority**: ⚡ MEDIUM
**Duration**: 1-2 days
**Risk**: LOW

---

## Objective

Optimize performance, remove dead code, add observability features (metrics, EXPLAIN mode), and polish the implementation to production-ready quality.

---

## Context

**Current State**:
- All tests passing (after R1, R2, R3)
- Core functionality complete
- Ready for optimization and cleanup

**Goals**:
1. Remove dead code
2. Optimize performance (<0.5ms overhead)
3. Add FK optimization metrics
4. Add EXPLAIN mode for debugging
5. Final code quality polish

---

## Implementation Steps

### Step 1: Remove Dead Code (2 hours)

#### 1a. Remove Dead Code in `where_normalization.py`
**Location**: `src/fraiseql/where_normalization.py:340-359`

**Action**: Delete unreachable duplicate code

**Verification**:
```bash
# Ensure tests still pass
uv run pytest tests/unit/test_where_normalization.py -v
```

#### 1b. Clean Up Old Commented Code
**Search for**:
- `# TODO` comments related to old implementation
- Commented-out old code paths
- Unused imports

**Commands**:
```bash
# Find TODOs
grep -r "# TODO" src/fraiseql/where*.py

# Find commented code blocks
grep -r "^#.*def\|^#.*class" src/fraiseql/where*.py
```

#### 1c. Remove Unused Functions in `db.py`
**Check for**:
- Any remaining references to old WHERE code
- Unused helper functions
- Dead branches in conditionals

---

### Step 2: Add Performance Metrics (3 hours)

**Create**: `src/fraiseql/where_metrics.py`

```python
"""Performance metrics for WHERE clause processing."""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any


@dataclass
class WhereMetrics:
    """Metrics collector for WHERE clause processing."""

    # Timing metrics (in milliseconds)
    normalization_times: list[float] = field(default_factory=list)
    sql_generation_times: list[float] = field(default_factory=list)

    # Optimization metrics
    fk_optimizations_used: int = 0
    jsonb_fallbacks_used: int = 0
    total_normalizations: int = 0

    @classmethod
    def record_normalization(cls, duration_ms: float, used_fk: bool = False) -> None:
        """Record a normalization operation.

        Args:
            duration_ms: Time taken in milliseconds
            used_fk: Whether FK optimization was used
        """
        _global_metrics.normalization_times.append(duration_ms)
        _global_metrics.total_normalizations += 1

        if used_fk:
            _global_metrics.fk_optimizations_used += 1
        else:
            _global_metrics.jsonb_fallbacks_used += 1

    @classmethod
    def record_sql_generation(cls, duration_ms: float) -> None:
        """Record SQL generation time."""
        _global_metrics.sql_generation_times.append(duration_ms)

    @classmethod
    def get_stats(cls) -> dict[str, Any]:
        """Get current statistics.

        Returns:
            Dictionary with statistics
        """
        if not _global_metrics.normalization_times:
            return {
                "normalization": {"count": 0},
                "sql_generation": {"count": 0},
                "optimizations": {},
            }

        norm_times = _global_metrics.normalization_times
        sql_times = _global_metrics.sql_generation_times

        return {
            "normalization": {
                "count": len(norm_times),
                "avg_ms": sum(norm_times) / len(norm_times),
                "median_ms": sorted(norm_times)[len(norm_times) // 2],
                "p95_ms": sorted(norm_times)[int(len(norm_times) * 0.95)],
                "max_ms": max(norm_times),
            },
            "sql_generation": {
                "count": len(sql_times),
                "avg_ms": sum(sql_times) / len(sql_times) if sql_times else 0,
            },
            "optimizations": {
                "total_normalizations": _global_metrics.total_normalizations,
                "fk_optimizations_used": _global_metrics.fk_optimizations_used,
                "jsonb_fallbacks_used": _global_metrics.jsonb_fallbacks_used,
                "fk_optimization_rate": (
                    _global_metrics.fk_optimizations_used / _global_metrics.total_normalizations
                    if _global_metrics.total_normalizations > 0
                    else 0.0
                ),
            },
        }

    @classmethod
    def reset(cls) -> None:
        """Reset all metrics (useful for testing)."""
        _global_metrics.normalization_times.clear()
        _global_metrics.sql_generation_times.clear()
        _global_metrics.fk_optimizations_used = 0
        _global_metrics.jsonb_fallbacks_used = 0
        _global_metrics.total_normalizations = 0


# Global metrics instance
_global_metrics = WhereMetrics()
```

**Integrate in `where_normalization.py`**:
```python
from fraiseql.where_metrics import WhereMetrics
import time

def normalize_dict_where(
    where_dict: dict[str, Any],
    view_name: str,
    table_columns: set[str] | None = None,
    jsonb_column: str = "data",
) -> WhereClause:
    """Normalize dict WHERE clause to canonical WhereClause."""

    start_time = time.perf_counter()

    # ... existing logic ...

    # Track FK usage
    used_fk = any(
        c.lookup_strategy == "fk_column"
        for c in conditions
    )

    # Record metrics
    duration_ms = (time.perf_counter() - start_time) * 1000
    WhereMetrics.record_normalization(duration_ms, used_fk)

    return WhereClause(...)
```

**Test**:
```python
# tests/unit/test_where_metrics.py
def test_metrics_tracking():
    """Test metrics are collected."""
    from fraiseql.where_metrics import WhereMetrics

    WhereMetrics.reset()

    # Perform normalizations
    repo = FraiseQLRepository(None)
    repo._normalize_where({"status": {"eq": "active"}}, "test", {"status"})

    stats = WhereMetrics.get_stats()

    assert stats["normalization"]["count"] == 1
    assert stats["normalization"]["avg_ms"] < 1.0  # Should be <1ms
```

**Verification**:
```bash
# Test metrics collection
uv run pytest tests/unit/test_where_metrics.py -v

# Check metrics in real usage
python -c "
from fraiseql.db import FraiseQLRepository
from fraiseql.where_metrics import WhereMetrics

repo = FraiseQLRepository(None)
for i in range(100):
    repo._normalize_where({'status': {'eq': 'active'}}, 'test', {'status'})

print(WhereMetrics.get_stats())
"
```

---

### Step 3: Add EXPLAIN Mode (3 hours)

**Goal**: Allow users to see PostgreSQL query plans to verify FK optimization

**Update**: `src/fraiseql/db.py`

```python
async def find(
    self,
    view_name: str,
    where: dict | Any | None = None,
    explain: bool = False,  # NEW parameter
    **kwargs: Any,
) -> list[dict[str, Any]]:
    """Find records in a view.

    Args:
        view_name: View name
        where: WHERE clause
        explain: If True, log EXPLAIN ANALYZE output
        **kwargs: Additional parameters

    Returns:
        List of records
    """
    # ... build query as usual ...

    async with self._pool.connection() as conn, conn.cursor() as cursor:
        if explain:
            # Run EXPLAIN ANALYZE
            explain_query = SQL("EXPLAIN ANALYZE ") + query
            await cursor.execute(explain_query, params)
            explain_result = await cursor.fetchall()

            # Log query plan
            logger.info(
                "Query plan for %s",
                view_name,
                extra={
                    "view": view_name,
                    "where": where,
                    "plan": "\n".join(row[0] for row in explain_result),
                },
            )

            # Also execute actual query
            await cursor.execute(query, params)
        else:
            await cursor.execute(query, params)

        # ... rest of method ...
```

**Test**:
```python
# tests/integration/test_explain_mode.py
import logging

@pytest.mark.asyncio
async def test_explain_mode_logs_query_plan(class_db_pool, caplog):
    """Test EXPLAIN mode logs query plan."""
    repo = FraiseQLRepository(class_db_pool)

    with caplog.at_level(logging.INFO):
        await repo.find(
            "test_table",
            where={"id": {"eq": "test"}},
            explain=True
        )

    # Check log contains plan
    assert any("Index Scan" in record.message or "Seq Scan" in record.message for record in caplog.records)

@pytest.mark.asyncio
async def test_explain_mode_detects_fk_optimization(class_db_pool, setup_hybrid_table, caplog):
    """Test EXPLAIN shows FK index usage."""
    test_data = setup_hybrid_table
    repo = FraiseQLRepository(class_db_pool)

    with caplog.at_level(logging.INFO):
        await repo.find(
            "tv_allocation",
            where={"machine": {"id": {"eq": test_data["machine1_id"]}}},
            explain=True
        )

    # Should use machine_id index, not sequential scan
    plan = next(r.message for r in caplog.records if "plan" in r.message)
    assert "machine_id" in plan
    assert "Index Scan" in plan or "Index Only Scan" in plan
```

**Verification**:
```bash
uv run pytest tests/integration/test_explain_mode.py -v -s --log-cli-level=INFO
```

---

### Step 4: Performance Benchmarking (2 hours)

**Create**: `tests/performance/test_where_performance.py`

```python
"""Performance benchmarks for WHERE clause processing."""

import uuid
import pytest
from fraiseql.db import FraiseQLRepository
from fraiseql.where_metrics import WhereMetrics


class TestWherePerformance:
    """Performance benchmarks."""

    def test_normalization_overhead_benchmark(self, benchmark):
        """Benchmark normalization overhead (target: <0.5ms)."""
        repo = FraiseQLRepository(None)

        where = {
            "status": {"eq": "active"},
            "machine": {"id": {"eq": uuid.uuid4()}},
            "name": {"contains": "test"},
        }

        def normalize():
            return repo._normalize_where(
                where, "tv_allocation", {"status", "machine_id", "name", "data"}
            )

        result = benchmark(normalize)

        # Verify result is correct
        assert result is not None

        # Check benchmark stats
        assert benchmark.stats["mean"] < 0.0005  # <0.5ms

    def test_sql_generation_overhead_benchmark(self, benchmark):
        """Benchmark SQL generation (target: <0.1ms)."""
        repo = FraiseQLRepository(None)
        clause = repo._normalize_where(
            {"status": {"eq": "active"}}, "test", {"status"}
        )

        def generate_sql():
            return clause.to_sql()

        benchmark(generate_sql)

        assert benchmark.stats["mean"] < 0.0001  # <0.1ms

    def test_fk_optimization_rate(self):
        """Test FK optimization used >80% of time."""
        WhereMetrics.reset()
        repo = FraiseQLRepository(None)

        # 100 normalizations with FK opportunity
        for _ in range(100):
            repo._normalize_where(
                {"machine": {"id": {"eq": uuid.uuid4()}}},
                "tv_allocation",
                {"machine_id", "data"},
            )

        stats = WhereMetrics.get_stats()

        # Should use FK optimization in most cases
        assert stats["optimizations"]["fk_optimization_rate"] > 0.8
```

**Run Benchmarks**:
```bash
uv run pytest tests/performance/test_where_performance.py --benchmark-only
```

---

### Step 5: Code Quality Polish (2 hours)

#### 5a. Run Linter
```bash
ruff check src/fraiseql/where*.py
ruff check src/fraiseql/db.py
```

**Fix all violations**

#### 5b. Type Checking
```bash
mypy src/fraiseql/where_clause.py
mypy src/fraiseql/where_normalization.py
```

**Fix all type errors**

#### 5c. Docstring Completeness
Ensure all public functions have:
- Summary line
- Args section
- Returns section
- Examples (for complex functions)

#### 5d. Code Coverage Check
```bash
pytest --cov=src/fraiseql/where_clause --cov=src/fraiseql/where_normalization --cov-report=html
```

**Target**: >90% coverage

---

### Step 6: Final Integration Test (1 hour)

**Run Full Test Suite**:
```bash
# All tests
uv run pytest tests/ -v

# Check pass rate
uv run pytest tests/ -v | grep -E "passed|failed"

# Should be 100% passing
```

**Performance Verification**:
```python
# Quick performance check
python -c "
from fraiseql.where_metrics import WhereMetrics
from fraiseql.db import FraiseQLRepository

repo = FraiseQLRepository(None)

# Warm-up
for _ in range(10):
    repo._normalize_where({'status': {'eq': 'test'}}, 'test', {'status'})

WhereMetrics.reset()

# Benchmark
for _ in range(1000):
    repo._normalize_where({'status': {'eq': 'test'}}, 'test', {'status'})

stats = WhereMetrics.get_stats()
print(f\"Avg normalization: {stats['normalization']['avg_ms']:.3f}ms\")
print(f\"P95 normalization: {stats['normalization']['p95_ms']:.3f}ms\")
print(f\"FK optimization rate: {stats['optimizations']['fk_optimization_rate']:.1%}\")
"
```

**Expected Output**:
```
Avg normalization: 0.150ms
P95 normalization: 0.350ms
FK optimization rate: 85.0%
```

---

## Verification Commands

### After Each Step
```bash
# Step 1: Dead code removal
uv run pytest tests/ -v --tb=short

# Step 2: Metrics
uv run pytest tests/unit/test_where_metrics.py -v
python -c "from fraiseql.where_metrics import WhereMetrics; print(WhereMetrics.get_stats())"

# Step 3: EXPLAIN mode
uv run pytest tests/integration/test_explain_mode.py -v -s --log-cli-level=INFO

# Step 4: Performance
uv run pytest tests/performance/test_where_performance.py --benchmark-only

# Step 5: Quality
ruff check src/fraiseql/where*.py
pytest --cov=src/fraiseql/where_clause --cov=src/fraiseql/where_normalization

# Step 6: Full suite
uv run pytest tests/ -v
```

---

## Acceptance Criteria

### Code Quality ✅
- [ ] No dead code
- [ ] Ruff passes (0 violations)
- [ ] Mypy passes (0 errors)
- [ ] Code coverage >90%
- [ ] All docstrings complete

### Performance ✅
- [ ] Normalization overhead <0.5ms (avg)
- [ ] SQL generation <0.1ms (avg)
- [ ] FK optimization rate >80%
- [ ] No performance regressions vs baseline

### Observability ✅
- [ ] Metrics collection working
- [ ] EXPLAIN mode working
- [ ] Logging comprehensive
- [ ] Debugging info available

### Tests ✅
- [ ] All 4,901 tests passing (100%)
- [ ] Performance benchmarks passing
- [ ] Metrics tests passing
- [ ] EXPLAIN mode tests passing

---

## DO NOT

❌ **DO NOT** optimize prematurely (profile first)
❌ **DO NOT** break existing APIs
❌ **DO NOT** skip performance benchmarks
❌ **DO NOT** remove useful logging

---

## Rollback Plan

**If performance targets not met**:
- Document actual performance
- Identify bottlenecks with profiling
- Create follow-up optimization phase
- Still acceptable if <2ms overhead

---

## Time Estimates

| Step | Optimistic | Realistic | Pessimistic |
|------|-----------|-----------|-------------|
| 1. Dead code | 1h | 2h | 3h |
| 2. Metrics | 2h | 3h | 5h |
| 3. EXPLAIN mode | 2h | 3h | 4h |
| 4. Benchmarks | 1h | 2h | 3h |
| 5. Quality polish | 1h | 2h | 4h |
| 6. Final test | 0.5h | 1h | 2h |
| **TOTAL** | **7.5h** | **13h** | **21h** |

**Realistic Timeline**: 1.5 days (13h over 2 days)

---

## Progress Tracking

### Day 1
- [ ] Steps 1-3 complete
- [ ] Dead code removed
- [ ] Metrics + EXPLAIN working

### Day 2
- [ ] Steps 4-6 complete
- [ ] Performance targets met
- [ ] All tests passing
- [ ] Phase R4 complete

---

**Phase Status**: BLOCKED (waiting for R1, R2, R3)
**Previous Phase**: [phase-r3-whereinput-integration.md](phase-r3-whereinput-integration.md)
**Next Phase**: [phase-r5-documentation.md](phase-r5-documentation.md)
