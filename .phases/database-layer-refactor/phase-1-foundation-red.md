# Phase 1: Foundation & Test Infrastructure

**Phase:** RED (Write Failing Tests First)
**Duration:** 6-8 hours
**Risk:** Low

---

## Objective

**TDD Phase RED:** Establish foundation and write comprehensive failing tests BEFORE implementing.

Create:
- Base directory structure
- Interface definitions
- Comprehensive test suite (FAILING)
- Performance benchmarks baseline
- Dependency injection framework

**Success Criteria:** Tests written and FAIL because implementations don't exist yet

---

## Step 1: Establish Performance Baseline (1 hour)

**CRITICAL:** Measure current performance before any changes.

### 1.1 Create Benchmark Suite

Create `tests/benchmarks/test_repository_performance.py`:

```python
"""Performance benchmarks for FraiseQLRepository."""

import pytest
import time
from statistics import mean, stdev

from fraiseql.db import FraiseQLRepository


class TestRepositoryPerformance:
    """Benchmark repository operations."""

    @pytest.mark.benchmark
    async def test_find_simple_performance(self, class_db_pool, benchmark_db):
        """Benchmark simple find() operation."""
        repo = FraiseQLRepository(class_db_pool)

        # Setup test data
        await benchmark_db.execute("""
            CREATE TABLE IF NOT EXISTS bench_users (
                id SERIAL PRIMARY KEY,
                data JSONB
            );
            INSERT INTO bench_users (data)
            SELECT jsonb_build_object('name', 'User' || i, 'age', i)
            FROM generate_series(1, 100) i;
        """)

        # Warm up
        await repo.find("bench_users", limit=10)

        # Benchmark
        times = []
        for _ in range(1000):
            start = time.perf_counter()
            await repo.find("bench_users", limit=10)
            times.append((time.perf_counter() - start) * 1000)  # ms

        avg_time = mean(times)
        std_dev = stdev(times)

        # Record baseline
        print(f"\\nBASELINE find() simple: {avg_time:.3f}ms ± {std_dev:.3f}ms")
        assert avg_time < 5.0, f"Performance degraded: {avg_time}ms > 5.0ms"

    @pytest.mark.benchmark
    async def test_find_with_where_performance(self, class_db_pool, benchmark_db):
        """Benchmark find() with complex WHERE."""
        repo = FraiseQLRepository(class_db_pool)

        # Setup
        await benchmark_db.execute("""
            CREATE TABLE IF NOT EXISTS bench_orders (
                id SERIAL PRIMARY KEY,
                data JSONB
            );
            INSERT INTO bench_orders (data)
            SELECT jsonb_build_object(
                'customer_id', (random() * 100)::int,
                'total', (random() * 1000)::numeric(10,2),
                'status', CASE WHEN random() > 0.5 THEN 'active' ELSE 'completed' END
            )
            FROM generate_series(1, 1000) i;
        """)

        # Benchmark
        times = []
        for _ in range(100):
            start = time.perf_counter()
            await repo.find(
                "bench_orders",
                where={"status": {"eq": "active"}, "total": {"gte": 100}},
                limit=20
            )
            times.append((time.perf_counter() - start) * 1000)

        avg_time = mean(times)
        print(f"\\nBASELINE find() with WHERE: {avg_time:.3f}ms")
        assert avg_time < 10.0

    @pytest.mark.benchmark
    async def test_find_one_performance(self, class_db_pool, benchmark_db):
        """Benchmark find_one() operation."""
        # Similar pattern...

    @pytest.mark.benchmark
    async def test_count_performance(self, class_db_pool, benchmark_db):
        """Benchmark count() operation."""
        # Similar pattern...

    @pytest.mark.benchmark
    async def test_aggregate_performance(self, class_db_pool, benchmark_db):
        """Benchmark aggregate() operation."""
        # Similar pattern...
```

### 1.2 Run Baseline Benchmarks

```bash
# Run benchmarks and save baseline
uv run pytest tests/benchmarks/test_repository_performance.py -v --benchmark-only \
    > baseline_performance.txt

# Review baseline
cat baseline_performance.txt
```

**Record these numbers - they are your success criteria!**

---

## Step 2: Create Directory Structure (30 min)

```bash
# Create main directory
mkdir -p src/fraiseql/db

# Create subdirectories
mkdir -p src/fraiseql/db/core
mkdir -p src/fraiseql/db/registry
mkdir -p src/fraiseql/db/query_builder
mkdir -p src/fraiseql/db/where
mkdir -p src/fraiseql/db/utils

# Create __init__.py files
touch src/fraiseql/db/__init__.py
touch src/fraiseql/db/core/__init__.py
touch src/fraiseql/db/registry/__init__.py
touch src/fraiseql/db/query_builder/__init__.py
touch src/fraiseql/db/where/__init__.py
touch src/fraiseql/db/utils/__init__.py
```

---

## Step 3: Define Base Interfaces (2 hours)

### 3.1 Create `src/fraiseql/db/core/interfaces.py`

```python
"""Base interfaces for database layer components."""

from abc import ABC, abstractmethod
from typing import Any, Optional, Protocol
from collections.abc import Mapping

from psycopg.sql import Composable
from psycopg_pool import AsyncConnectionPool


class QueryBuilder(Protocol):
    """Protocol for query builders."""

    def build_query(
        self,
        view_name: str,
        **kwargs: Any
    ) -> tuple[Composable, list[Any]]:
        """Build SQL query and parameters.

        Args:
            view_name: Database view/table name
            **kwargs: Query parameters (where, limit, offset, etc.)

        Returns:
            Tuple of (SQL query, parameters)
        """
        ...


class ConnectionManager(Protocol):
    """Protocol for connection management."""

    async def execute_query(
        self,
        query: Composable,
        params: list[Any]
    ) -> list[dict[str, Any]]:
        """Execute query and return results."""
        ...

    async def execute_in_transaction(
        self,
        operations: list[tuple[Composable, list[Any]]]
    ) -> list[list[dict[str, Any]]]:
        """Execute multiple operations in a transaction."""
        ...


class TypeRegistry(Protocol):
    """Protocol for type registration."""

    def register_type(
        self,
        view_name: str,
        type_class: type,
        **metadata: Any
    ) -> None:
        """Register a type for a view."""
        ...

    def get_type(self, view_name: str) -> Optional[type]:
        """Get registered type for a view."""
        ...


class WhereClauseBuilder(Protocol):
    """Protocol for WHERE clause building."""

    def build_where(
        self,
        view_name: str,
        where: Any,
        **kwargs: Any
    ) -> tuple[list[Any], list[Any]]:
        """Build WHERE clause SQL.

        Args:
            view_name: Database view name
            where: WHERE condition (dict or WhereInput object)
            **kwargs: Additional context

        Returns:
            Tuple of (WHERE clauses, parameters)
        """
        ...
```

### 3.2 Create `src/fraiseql/db/core/query.py`

```python
"""Database query dataclass."""

from dataclasses import dataclass
from typing import Any
from collections.abc import Mapping

from psycopg.sql import Composable, SQL


@dataclass
class DatabaseQuery:
    """Encapsulates a SQL query, parameters, and fetch flag.

    This is a direct copy from db.py - keep it simple and unchanged.
    """

    statement: Composable | SQL
    params: Mapping[str, object]
    fetch_result: bool = True
```

---

## Step 4: Write Failing Tests (3-4 hours)

### 4.1 Type Registry Tests

Create `tests/unit/db/registry/test_type_registry.py`:

```python
"""Tests for type registry."""

import pytest
from fraiseql.db.registry import TypeRegistry, register_type_for_view


class TestTypeRegistry:
    """Test type registration system."""

    def test_register_type_simple(self):
        """Test simple type registration."""
        # This will FAIL - implementation doesn't exist yet
        registry = TypeRegistry()

        class User:
            id: str
            name: str

        registry.register_type("v_user", User)

        assert registry.get_type("v_user") is User

    def test_register_type_with_metadata(self):
        """Test type registration with metadata."""
        registry = TypeRegistry()

        class User:
            id: str
            name: str

        registry.register_type(
            "v_user",
            User,
            table_columns={"id", "data"},
            has_jsonb_data=True,
            jsonb_column="data"
        )

        metadata = registry.get_metadata("v_user")
        assert metadata["has_jsonb_data"] is True
        assert metadata["jsonb_column"] == "data"

    def test_register_type_with_fk_relationships(self):
        """Test FK relationship registration."""
        # FAIL - not implemented yet
        pass

    def test_get_type_not_found(self):
        """Test getting unregistered type."""
        registry = TypeRegistry()
        assert registry.get_type("unknown") is None
```

### 4.2 Query Builder Tests

Create `tests/unit/db/query_builder/test_find_builder.py`:

```python
"""Tests for find() query builder."""

import pytest
from fraiseql.db.query_builder import FindQueryBuilder


class TestFindQueryBuilder:
    """Test find() query building."""

    def test_simple_find(self):
        """Test simple find() query."""
        # This will FAIL
        builder = FindQueryBuilder()

        query, params = builder.build_query(
            view_name="v_user",
            limit=10
        )

        assert "SELECT" in str(query)
        assert "v_user" in str(query)
        assert "LIMIT" in str(query)

    def test_find_with_where(self):
        """Test find() with WHERE clause."""
        builder = FindQueryBuilder()

        query, params = builder.build_query(
            view_name="v_user",
            where={"name": {"eq": "John"}},
            limit=10
        )

        assert "WHERE" in str(query)
        assert len(params) > 0

    def test_find_with_order_by(self):
        """Test find() with ORDER BY."""
        # FAIL - not implemented
        pass

    def test_find_with_offset(self):
        """Test find() with OFFSET."""
        # FAIL - not implemented
        pass
```

### 4.3 Connection Manager Tests

Create `tests/unit/db/core/test_connection_manager.py`:

```python
"""Tests for connection manager."""

import pytest
from fraiseql.db.core import ConnectionManager


class TestConnectionManager:
    """Test connection management."""

    async def test_execute_query(self, mock_pool):
        """Test query execution."""
        # FAIL - not implemented
        manager = ConnectionManager(mock_pool)

        results = await manager.execute_query(
            SQL("SELECT * FROM users"),
            []
        )

        assert isinstance(results, list)

    async def test_execute_in_transaction(self, mock_pool):
        """Test transaction execution."""
        # FAIL - not implemented
        pass

    async def test_connection_error_handling(self, mock_pool):
        """Test connection error handling."""
        # FAIL - not implemented
        pass
```

### 4.4 WHERE Builder Tests

Create `tests/unit/db/where/test_where_builder.py`:

```python
"""Tests for WHERE clause builder."""

import pytest
from fraiseql.db.where import WhereClauseBuilder


class TestWhereClauseBuilder:
    """Test WHERE clause building."""

    def test_dict_where_simple(self):
        """Test simple dict WHERE."""
        # FAIL - not implemented
        builder = WhereClauseBuilder()

        clauses, params = builder.build_where(
            view_name="v_user",
            where={"name": {"eq": "John"}}
        )

        assert len(clauses) > 0
        assert len(params) > 0

    def test_dict_where_nested(self):
        """Test nested dict WHERE."""
        # FAIL - not implemented
        pass

    def test_whereinput_where(self):
        """Test WhereInput object WHERE."""
        # FAIL - not implemented
        pass
```

---

## Step 5: Verify Tests Fail (30 min)

```bash
# Run all new tests - they should ALL FAIL
uv run pytest tests/unit/db/ -v

# Expected output: All tests fail with ImportError or AttributeError
# This is CORRECT for RED phase
```

**Success Criteria:** All tests fail because classes don't exist yet.

---

## Verification Commands

```bash
# Verify directory structure
tree src/fraiseql/db/

# Verify tests exist and fail
uv run pytest tests/unit/db/ -v

# Verify baseline benchmarks recorded
cat baseline_performance.txt

# Verify no regressions in existing tests
uv run pytest tests/unit/ -v --ignore=tests/unit/db/
uv run pytest tests/integration/ -v
```

---

## Acceptance Criteria

- [ ] Directory structure created
- [ ] Base interfaces defined
- [ ] DatabaseQuery dataclass copied
- [ ] Performance baseline established and recorded
- [ ] Comprehensive test suite written (30+ tests)
- [ ] All new tests FAIL (expected - RED phase)
- [ ] All existing tests still PASS
- [ ] No changes to src/fraiseql/db.py yet

---

## DO NOT

- ❌ Implement any functionality (this is RED phase)
- ❌ Modify existing db.py file
- ❌ Fix failing tests (they should fail)
- ❌ Skip performance baseline

---

## Next Phase

Once foundation is established and all tests fail:
→ **Phase 2:** Type Registry & Metadata (implement to make those tests pass)
