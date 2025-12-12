# Phase 7: Performance Optimization [REFACTOR]

## Objective

Add caching and optimizations to make WHERE normalization near-zero overhead.

## Context

Normalization adds a processing step before SQL generation. While this is necessary for correctness, we can optimize to minimize overhead:

- Cache normalized WhereInput objects
- Optimize SQL generation
- Add query plan analysis
- Performance benchmarks

Target: <0.5ms normalization overhead for typical queries.

## Files to Create

- `tests/performance/test_where_performance.py` - Performance benchmarks

## Files to Modify

- `src/fraiseql/where_clause.py` - Add caching
- `src/fraiseql/sql/graphql_where_generator.py` - Cache normalization

## Implementation Steps

### Step 1: Add Caching to WhereInput Objects

```python
# In graphql_where_generator.py

def create_graphql_where_input(cls: type, name: str | None = None) -> type:
    """Generate WhereInput with caching."""

    # ... existing code ...

    def _to_whereinput_dict(self) -> dict[str, Any]:
        """Convert to dict with caching."""
        # Check cache
        if hasattr(self, "_cached_dict"):
            return self._cached_dict

        # ... existing conversion logic ...

        # Cache result (WhereInput objects are typically immutable)
        self._cached_dict = result
        return result

    WhereInputClass._to_whereinput_dict = _to_whereinput_dict

    return WhereInputClass
```

### Step 2: Add __hash__ and __eq__ to WhereClause

```python
# In where_clause.py

@dataclass(frozen=True)  # Make immutable for hashing
class FieldCondition:
    """..."""
    # Already hashable since frozen

@dataclass
class WhereClause:
    """..."""

    def __hash__(self):
        """Hash for caching."""
        # Hash based on conditions
        condition_hash = tuple(
            (tuple(c.field_path), c.operator, c.value, c.lookup_strategy)
            for c in self.conditions
        )
        nested_hash = tuple(hash(n) for n in self.nested_clauses)
        not_hash = hash(self.not_clause) if self.not_clause else None

        return hash((condition_hash, self.logical_op, nested_hash, not_hash))

    def __eq__(self, other):
        """Equality for caching."""
        if not isinstance(other, WhereClause):
            return False

        return (
            self.conditions == other.conditions
            and self.logical_op == other.logical_op
            and self.nested_clauses == other.nested_clauses
            and self.not_clause == other.not_clause
        )
```

### Step 3: Cache SQL Generation

```python
# In where_clause.py

@dataclass
class WhereClause:
    """..."""

    def to_sql(self) -> tuple[Composed | None, list[Any]]:
        """Generate SQL with caching."""
        # Check cache
        cache_key = hash(self)
        if hasattr(self, "_sql_cache") and cache_key in self._sql_cache:
            return self._sql_cache[cache_key]

        # Generate SQL
        sql, params = self._generate_sql()

        # Cache result
        if not hasattr(self, "_sql_cache"):
            self._sql_cache = {}
        self._sql_cache[cache_key] = (sql, params)

        return sql, params

    def _generate_sql(self) -> tuple[Composed | None, list[Any]]:
        """Internal SQL generation (uncached)."""
        # ... existing to_sql() logic ...
```

### Step 4: Add Performance Benchmarks

Create `tests/performance/test_where_performance.py`:

```python
"""Performance benchmarks for WHERE clause processing."""

import uuid
import time
from statistics import mean, stdev

import pytest

from fraiseql.db import FraiseQLRepository
from fraiseql.sql import create_graphql_where_input, UUIDFilter
from tests.regression.test_nested_filter_id_field import Allocation, Machine


class TestWherePerformance:
    """Performance benchmarks for WHERE normalization and SQL generation."""

    def test_dict_normalization_performance(self):
        """Benchmark dict WHERE normalization."""
        where = {"machine": {"id": {"eq": uuid.uuid4()}}}
        repo = FraiseQLRepository(None)
        table_columns = {"machine_id", "data"}

        # Warmup
        for _ in range(10):
            repo._normalize_where(where, "tv_allocation", table_columns)

        # Benchmark
        times = []
        for _ in range(1000):
            start = time.perf_counter()
            repo._normalize_where(where, "tv_allocation", table_columns)
            times.append((time.perf_counter() - start) * 1000)  # ms

        avg_time = mean(times)
        std_time = stdev(times)

        print(f"\nDict normalization: {avg_time:.3f}ms ± {std_time:.3f}ms")
        assert avg_time < 0.5, f"Dict normalization too slow: {avg_time:.3f}ms"

    def test_whereinput_normalization_performance(self):
        """Benchmark WhereInput normalization."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        where_input = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=uuid.uuid4()))
        )

        repo = FraiseQLRepository(None)
        table_columns = {"machine_id", "data"}

        # Warmup
        for _ in range(10):
            repo._normalize_where(where_input, "tv_allocation", table_columns)

        # Benchmark
        times = []
        for _ in range(1000):
            start = time.perf_counter()
            repo._normalize_where(where_input, "tv_allocation", table_columns)
            times.append((time.perf_counter() - start) * 1000)

        avg_time = mean(times)
        std_time = stdev(times)

        print(f"\nWhereInput normalization: {avg_time:.3f}ms ± {std_time:.3f}ms")
        assert avg_time < 0.5, f"WhereInput normalization too slow: {avg_time:.3f}ms"

    def test_sql_generation_performance(self):
        """Benchmark SQL generation from WhereClause."""
        from fraiseql.where_clause import WhereClause, FieldCondition

        clause = WhereClause(
            conditions=[
                FieldCondition(
                    field_path=["machine", "id"],
                    operator="eq",
                    value=uuid.uuid4(),
                    lookup_strategy="fk_column",
                    target_column="machine_id",
                )
            ]
        )

        # Warmup
        for _ in range(10):
            clause.to_sql()

        # Benchmark
        times = []
        for _ in range(1000):
            start = time.perf_counter()
            clause.to_sql()
            times.append((time.perf_counter() - start) * 1000)

        avg_time = mean(times)
        std_time = stdev(times)

        print(f"\nSQL generation: {avg_time:.3f}ms ± {std_time:.3f}ms")
        assert avg_time < 0.2, f"SQL generation too slow: {avg_time:.3f}ms"

    def test_cache_effectiveness(self):
        """Test cache hit rates."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        machine_id = uuid.uuid4()
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(id=UUIDFilter(eq=machine_id))
        )

        # First call (cache miss)
        start = time.perf_counter()
        dict1 = where_input._to_whereinput_dict()
        first_time = (time.perf_counter() - start) * 1000

        # Second call (cache hit)
        start = time.perf_counter()
        dict2 = where_input._to_whereinput_dict()
        second_time = (time.perf_counter() - start) * 1000

        print(f"\nFirst call: {first_time:.3f}ms")
        print(f"Second call (cached): {second_time:.3f}ms")
        print(f"Speedup: {first_time / second_time:.1f}x")

        # Cache should be significantly faster
        assert second_time < first_time / 10, "Cache not effective"
```

### Step 5: Add Query EXPLAIN Mode (Observability)

**IMPORTANT: Help users verify FK optimization is working**

Add to `src/fraiseql/db.py`:

```python
class FraiseQLRepository:
    """..."""

    async def find(
        self,
        view_name: str,
        select: list[str] | None = None,
        explain: bool = False,  # NEW
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Find records with optional EXPLAIN mode.

        Args:
            explain: If True, log EXPLAIN ANALYZE output for debugging

        Example:
            # Debug query performance
            await repo.find(
                "tv_allocation",
                where={"machine": {"id": {"eq": machine_id}}},
                explain=True
            )
            # Logs show: Index Scan using machine_id_idx ✅
        """
        # ... build query ...

        if explain:
            # Run EXPLAIN ANALYZE
            explain_query = SQL("EXPLAIN (ANALYZE, BUFFERS, VERBOSE) ") + query
            async with self.pool.connection() as conn:
                async with conn.cursor() as cursor:
                    await cursor.execute(explain_query, self._where_params or [])
                    plan = await cursor.fetchall()

                    # Log query plan
                    logger.info(
                        f"Query plan for {view_name}",
                        extra={
                            "view": view_name,
                            "where": kwargs.get("where"),
                            "plan": "\n".join(row[0] for row in plan),
                        }
                    )

                    # Check for common issues
                    plan_text = "\n".join(row[0] for row in plan)

                    if "Seq Scan" in plan_text and "machine_id" in plan_text:
                        logger.warning(
                            f"Sequential scan on {view_name} - FK optimization may not be working"
                        )

                    if "Index Scan" in plan_text or "Index Only Scan" in plan_text:
                        logger.info(f"Index scan detected - FK optimization working ✅")

        # ... execute actual query ...
```

### Step 6: Add Performance Metrics Collection

**IMPORTANT: Track normalization performance and optimization rates**

Create `src/fraiseql/where_metrics.py`:

```python
"""Performance metrics for WHERE clause processing."""

import time
from dataclasses import dataclass, field
from typing import ClassVar


@dataclass
class WhereMetrics:
    """Global metrics for WHERE clause processing."""

    # Timing metrics (milliseconds)
    normalization_times_ms: ClassVar[list[float]] = []
    sql_generation_times_ms: ClassVar[list[float]] = []

    # Optimization metrics
    fk_optimization_count: ClassVar[int] = 0
    jsonb_fallback_count: ClassVar[int] = 0
    cache_hit_count: ClassVar[int] = 0
    cache_miss_count: ClassVar[int] = 0

    @classmethod
    def record_normalization(cls, duration_ms: float, used_fk: bool = False):
        """Record a normalization operation."""
        cls.normalization_times_ms.append(duration_ms)
        if used_fk:
            cls.fk_optimization_count += 1
        else:
            cls.jsonb_fallback_count += 1

    @classmethod
    def record_sql_generation(cls, duration_ms: float, from_cache: bool = False):
        """Record SQL generation operation."""
        cls.sql_generation_times_ms.append(duration_ms)
        if from_cache:
            cls.cache_hit_count += 1
        else:
            cls.cache_miss_count += 1

    @classmethod
    def get_stats(cls) -> dict:
        """Get performance statistics."""
        from statistics import mean, median

        return {
            "normalization": {
                "count": len(cls.normalization_times_ms),
                "avg_ms": mean(cls.normalization_times_ms) if cls.normalization_times_ms else 0,
                "median_ms": median(cls.normalization_times_ms) if cls.normalization_times_ms else 0,
                "p95_ms": sorted(cls.normalization_times_ms)[int(len(cls.normalization_times_ms) * 0.95)] if cls.normalization_times_ms else 0,
            },
            "sql_generation": {
                "count": len(cls.sql_generation_times_ms),
                "avg_ms": mean(cls.sql_generation_times_ms) if cls.sql_generation_times_ms else 0,
                "median_ms": median(cls.sql_generation_times_ms) if cls.sql_generation_times_ms else 0,
            },
            "optimizations": {
                "fk_count": cls.fk_optimization_count,
                "jsonb_count": cls.jsonb_fallback_count,
                "fk_rate": cls.fk_optimization_count / (cls.fk_optimization_count + cls.jsonb_fallback_count)
                    if (cls.fk_optimization_count + cls.jsonb_fallback_count) > 0 else 0,
            },
            "cache": {
                "hit_count": cls.cache_hit_count,
                "miss_count": cls.cache_miss_count,
                "hit_rate": cls.cache_hit_count / (cls.cache_hit_count + cls.cache_miss_count)
                    if (cls.cache_hit_count + cls.cache_miss_count) > 0 else 0,
            },
        }

    @classmethod
    def reset(cls):
        """Reset all metrics (useful for testing)."""
        cls.normalization_times_ms.clear()
        cls.sql_generation_times_ms.clear()
        cls.fk_optimization_count = 0
        cls.jsonb_fallback_count = 0
        cls.cache_hit_count = 0
        cls.cache_miss_count = 0
```

Integrate into normalization:

```python
# In where_normalization.py

from fraiseql.where_metrics import WhereMetrics

def normalize_dict_where(...) -> WhereClause:
    """..."""
    start = time.perf_counter()

    # ... normalization logic ...

    # Track metrics
    duration_ms = (time.perf_counter() - start) * 1000
    used_fk = any(c.lookup_strategy == "fk_column" for c in conditions)
    WhereMetrics.record_normalization(duration_ms, used_fk)

    return clause
```

### Step 7: Optimize Hot Paths

Profile and optimize:

```python
# Use cProfile to find hot spots
python -m cProfile -s cumulative -m pytest tests/performance/test_where_performance.py

# Optimize based on results
# Common optimizations:
# - Reduce __dict__ lookups
# - Cache metadata lookups
# - Reuse SQL objects
```

## Verification Commands

```bash
# Run performance benchmarks
uv run pytest tests/performance/test_where_performance.py -v -s

# Test EXPLAIN mode
uv run pytest tests/regression/test_nested_filter_id_field.py::test_whereinput_nested_filter -v -s --log-cli-level=INFO
# Look for "Index scan detected - FK optimization working ✅"

# Check metrics
python -c "
from fraiseql.where_metrics import WhereMetrics
# Run some queries...
print(WhereMetrics.get_stats())
"

# Profile normalization
uv run python -m cProfile -s cumulative -m pytest tests/performance/ -v

# Compare before/after (if baseline exists)
# Store baseline: pytest-benchmark or similar

# Verify no regressions
uv run pytest tests/ -v
```

## Acceptance Criteria

- [ ] Caching added to WhereInput._to_whereinput_dict()
- [ ] WhereClause.to_sql() cached
- [ ] **EXPLAIN mode added to find() method**
- [ ] **Performance metrics collection implemented**
- [ ] Performance benchmarks pass (<0.5ms normalization)
- [ ] Cache effectiveness >10x speedup for repeated queries
- [ ] FK optimization verified via EXPLAIN mode
- [ ] Metrics show FK optimization rate >80% (when applicable)
- [ ] No memory leaks from caching
- [ ] All tests pass
- [ ] Profile shows no obvious hot spots

## Notes

Performance targets:
- Dict normalization: <0.5ms
- WhereInput normalization: <0.5ms (first call), <0.05ms (cached)
- SQL generation: <0.2ms

These are **negligible** compared to:
- Database query time: 1-100ms
- Network latency: 1-50ms
- GraphQL parsing: 0.5-2ms

The refactor should have **no measurable impact** on end-to-end performance.

## Next Phase

**Phase 8:** Documentation and migration guide.
